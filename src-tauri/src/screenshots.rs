#[cfg(target_os = "macos")]
mod platform;
#[cfg(target_os = "windows")]
mod platform_windows;

use std::collections::HashMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use chrono::{Local, NaiveDateTime};
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder};
use quantette::deps::palette::{cast::from_component_slice, Srgb};
use quantette::{ImageRef, Pipeline, QuantizeMethod};
use serde::Deserialize;
use tauri::{image::Image, AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::recording::Region;

/// A captured still: straight (non-premultiplied) RGBA8, packed rows, top down.
/// That is what both the clipboard and the PNG encoder want.
pub struct CapturedImage {
  pub rgba: Vec<u8>,
  pub width: u32,
  pub height: u32,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(
  rename_all = "camelCase",
  rename_all_fields = "camelCase",
  tag = "kind"
)]
pub enum ScreenshotTarget {
  Screen { monitor_id: u32 },
  Window { window_id: u32 },
  Region { monitor_id: u32, region: Region },
}

/// A capture rectangle in physical device pixels, relative to its monitor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaptureRect {
  pub x: u32,
  pub y: u32,
  pub width: u32,
  pub height: u32,
}

/// Converts a logical, monitor-local region into physical device pixels.
///
/// The two platforms disagree about units - ScreenCaptureKit's source rect is
/// in points, xcap's capture region is in physical pixels - so everything is
/// normalised to physical here, exactly once, and the macOS caller divides back
/// down. Edges are rounded before the size is derived from them, so the
/// rectangle can never disagree with its own corners by a pixel, and the result
/// is clamped to the monitor because xcap errors on an out-of-bounds region
/// rather than trimming it.
pub fn physical_capture_rect(
  region: Region,
  scale: f64,
  monitor_width: u32,
  monitor_height: u32,
) -> Option<CaptureRect> {
  let edges = [
    region.position.x,
    region.position.y,
    region.size.width,
    region.size.height,
    scale,
  ];
  if !edges.iter().all(|edge| edge.is_finite()) || scale <= 0.0 {
    return None;
  }

  let monitor_width = f64::from(monitor_width);
  let monitor_height = f64::from(monitor_height);
  let left = (region.position.x * scale)
    .round()
    .clamp(0.0, monitor_width);
  let top = (region.position.y * scale)
    .round()
    .clamp(0.0, monitor_height);
  let right = ((region.position.x + region.size.width) * scale)
    .round()
    .clamp(0.0, monitor_width);
  let bottom = ((region.position.y + region.size.height) * scale)
    .round()
    .clamp(0.0, monitor_height);

  if right <= left || bottom <= top {
    return None;
  }

  Some(CaptureRect {
    x: left as u32,
    y: top as u32,
    width: (right - left) as u32,
    height: (bottom - top) as u32,
  })
}

/// The naming macOS's own `screencapture` uses, which is the least surprising
/// thing to find sitting on a Desktop.
pub fn screenshot_file_stem(captured_at: NaiveDateTime) -> String {
  captured_at
    .format("Orbit Capture %Y-%m-%d at %H.%M.%S")
    .to_string()
}

/// Appends " (2)", " (3)" and so on until the name is free, as both platforms'
/// file managers do. `exists` is injected so the walk can be tested without
/// touching a disk.
pub fn unique_path(
  directory: &Path,
  stem: &str,
  extension: &str,
  exists: &dyn Fn(&Path) -> bool,
) -> PathBuf {
  let mut candidate = directory.join(format!("{stem}.{extension}"));
  let mut suffix = 1_u32;

  while exists(&candidate) {
    suffix += 1;
    candidate = directory.join(format!("{stem} ({suffix}).{extension}"));
  }

  candidate
}

/// Where a still goes when it is not going to the clipboard. Both are the
/// platform's own screenshot destination.
pub fn screenshot_directory(app: &AppHandle) -> Result<PathBuf, String> {
  let path = app.path();

  #[cfg(target_os = "macos")]
  let directory = path.desktop_dir().map_err(|error| error.to_string())?;

  #[cfg(not(target_os = "macos"))]
  let directory = path
    .picture_dir()
    .map_err(|error| error.to_string())?
    .join("Screenshots");

  Ok(directory)
}

/// The largest palette an 8-bit indexed PNG can carry.
const MAX_PALETTE: usize = 256;

fn is_opaque(rgba: &[u8]) -> bool {
  rgba.chunks_exact(4).all(|pixel| pixel[3] == u8::MAX)
}

fn rgb_from_rgba(rgba: &[u8]) -> Vec<u8> {
  let mut rgb = Vec::with_capacity(rgba.len() / 4 * 3);
  for pixel in rgba.chunks_exact(4) {
    rgb.extend_from_slice(&pixel[..3]);
  }
  rgb
}

/// An exact palette, for a still that already fits inside 256 colours.
///
/// A terminal, a solid window or a flat UI is bit-exact this way, rather than
/// being handed to a quantizer that can only approximate it. Anything richer
/// bails on the 257th colour, which for a real screenshot happens within the
/// first handful of pixels, so this costs nothing in the common case.
fn exact_palette(rgb: &[u8]) -> Option<(Vec<[u8; 3]>, Vec<u8>)> {
  let mut palette: Vec<[u8; 3]> = Vec::new();
  let mut index_of: HashMap<[u8; 3], u8> = HashMap::new();
  let mut indices = Vec::with_capacity(rgb.len() / 3);

  for pixel in rgb.chunks_exact(3) {
    let colour = [pixel[0], pixel[1], pixel[2]];
    let index = match index_of.get(&colour) {
      Some(index) => *index,
      None => {
        if palette.len() >= MAX_PALETTE {
          return None;
        }
        let index = u8::try_from(palette.len()).ok()?;
        palette.push(colour);
        index_of.insert(colour, index);
        index
      }
    };
    indices.push(index);
  }

  Some((palette, indices))
}

/// Reduces the still to a 256-colour palette.
///
/// k-means with Floyd-Steinberg dithering: the dithering is what keeps
/// wallpapers and photos from banding, and k-means is both markedly more
/// faithful than Wu and, run in parallel, faster than encoding the image
/// losslessly would have been.
fn quantized_palette(rgb: &[u8], width: u32, height: u32) -> Option<(Vec<[u8; 3]>, Vec<u8>)> {
  let colours = from_component_slice::<Srgb<u8>>(rgb);
  let image = ImageRef::new(width, height, colours).ok()?;
  let (palette, indices) = Pipeline::new()
    .quantize_method(QuantizeMethod::kmeans())
    .parallel(true)
    .input_image(image)
    .output_srgb8_indexed_image()
    .into_parts();

  Some((
    palette
      .into_iter()
      .map(|colour| [colour.red, colour.green, colour.blue])
      .collect(),
    indices,
  ))
}

fn encode_indexed_png(
  palette: &[[u8; 3]],
  indices: &[u8],
  width: u32,
  height: u32,
) -> Result<Vec<u8>, String> {
  let mut png = Vec::new();
  {
    let mut encoder = png::Encoder::new(Cursor::new(&mut png), width, height);
    encoder.set_color(png::ColorType::Indexed);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::High);
    encoder.set_palette(palette.iter().flatten().copied().collect::<Vec<u8>>());
    let mut writer = encoder.write_header().map_err(|error| error.to_string())?;
    writer
      .write_image_data(indices)
      .map_err(|error| error.to_string())?;
  }

  Ok(png)
}

fn encode_truecolor_png(image: &CapturedImage) -> Result<Vec<u8>, String> {
  let mut png = Vec::new();
  PngEncoder::new_with_quality(
    Cursor::new(&mut png),
    CompressionType::Default,
    FilterType::Sub,
  )
  .write_image(
    &image.rgba,
    image.width,
    image.height,
    ExtendedColorType::Rgba8,
  )
  .map_err(|error| error.to_string())?;

  Ok(png)
}

/// Encodes a still as PNG bytes.
///
/// Deliberately one function, so the compression backend can be swapped
/// without capture or saving noticing. A screen capture is almost always fully
/// opaque, so the alpha channel is dropped and the image is written as an
/// 8-bit indexed PNG - that indexing is where the size comes from, not the
/// quantization on its own. A capture that is not opaque keeps every channel
/// and is written losslessly instead, because the palette path has no alpha.
///
/// Measured on a synthetic 3456x2234 desktop: 4.57 MB originally, 2.78 MB
/// losslessly, 0.57 MB here, in ~390ms.
pub fn encode_png(image: &CapturedImage) -> Result<Vec<u8>, String> {
  if !is_opaque(&image.rgba) {
    return encode_truecolor_png(image);
  }

  let rgb = rgb_from_rgba(&image.rgba);
  let (palette, indices) = exact_palette(&rgb)
    .or_else(|| quantized_palette(&rgb, image.width, image.height))
    .ok_or_else(|| "The capture could not be reduced to a palette".to_owned())?;

  encode_indexed_png(&palette, &indices, image.width, image.height)
}

async fn capture(
  app: &AppHandle,
  target: ScreenshotTarget,
  show_cursor: bool,
) -> Result<CapturedImage, String> {
  let _ = app;

  #[cfg(target_os = "macos")]
  {
    tauri::async_runtime::spawn_blocking(move || platform::capture_blocking(target, show_cursor))
      .await
      .map_err(|error| error.to_string())?
  }

  #[cfg(target_os = "windows")]
  {
    tauri::async_runtime::spawn_blocking(move || platform_windows::capture(target, show_cursor))
      .await
      .map_err(|error| error.to_string())?
  }

  #[cfg(not(any(target_os = "macos", target_os = "windows")))]
  {
    let _ = (target, show_cursor);
    Err("Screenshots are not available on this platform".to_owned())
  }
}

/// Captures a still and either copies it or saves it, returning the path it was
/// written to when it went to disk.
#[tauri::command]
pub async fn capture_still(
  app: AppHandle,
  target: ScreenshotTarget,
  show_cursor: bool,
  to_clipboard: bool,
) -> Result<Option<PathBuf>, String> {
  let image = capture(&app, target, show_cursor).await?;

  if to_clipboard {
    // The clipboard takes the raw pixels, so there is nothing to encode.
    app
      .clipboard()
      .write_image(&Image::new(&image.rgba, image.width, image.height))
      .map_err(|error| error.to_string())?;

    return Ok(None);
  }

  // With the clipboard off, the export window takes over: the user names the
  // file and picks where it goes, so nothing is written here.
  crate::exports::present_screenshot(
    &app,
    image,
    screenshot_file_stem(Local::now().naive_local()),
  )?;

  Ok(None)
}

#[cfg(test)]
mod tests {
  use std::collections::HashSet;

  use chrono::NaiveDate;
  use tauri::{LogicalPosition, LogicalSize};

  use super::*;

  fn region(x: f64, y: f64, width: f64, height: f64) -> Region {
    Region {
      position: LogicalPosition::new(x, y),
      size: LogicalSize::new(width, height),
    }
  }

  #[test]
  fn passes_a_region_through_unchanged_at_one_times_scale() {
    let rect = physical_capture_rect(region(10.0, 20.0, 300.0, 200.0), 1.0, 1920, 1080).unwrap();
    assert_eq!(
      rect,
      CaptureRect {
        x: 10,
        y: 20,
        width: 300,
        height: 200
      }
    );
  }

  #[test]
  fn doubles_a_region_on_a_retina_monitor() {
    let rect = physical_capture_rect(region(10.0, 20.0, 300.0, 200.0), 2.0, 3840, 2160).unwrap();
    assert_eq!(
      rect,
      CaptureRect {
        x: 20,
        y: 40,
        width: 600,
        height: 400
      }
    );
  }

  #[test]
  fn rounds_the_edges_rather_than_the_size() {
    // Rounding the size independently would give 226 here, leaving the right
    // edge a pixel away from where the corner says it is.
    let rect = physical_capture_rect(region(10.4, 0.0, 150.3, 10.0), 1.5, 1920, 1080).unwrap();
    assert_eq!(rect.x, 16);
    assert_eq!(rect.width, 225);
    assert_eq!(rect.x + rect.width, 241);
  }

  #[test]
  fn clamps_a_region_that_runs_past_the_monitor() {
    let rect =
      physical_capture_rect(region(1800.0, 1000.0, 400.0, 400.0), 1.0, 1920, 1080).unwrap();
    assert_eq!(
      rect,
      CaptureRect {
        x: 1800,
        y: 1000,
        width: 120,
        height: 80
      }
    );
  }

  #[test]
  fn clamps_a_region_that_starts_before_the_monitor() {
    let rect = physical_capture_rect(region(-50.0, -30.0, 200.0, 100.0), 1.0, 1920, 1080).unwrap();
    assert_eq!(
      rect,
      CaptureRect {
        x: 0,
        y: 0,
        width: 150,
        height: 70
      }
    );
  }

  #[test]
  fn fills_the_monitor_exactly_at_its_bounds() {
    let rect = physical_capture_rect(region(0.0, 0.0, 1920.0, 1080.0), 1.0, 1920, 1080).unwrap();
    assert_eq!(rect.width, 1920);
    assert_eq!(rect.height, 1080);
  }

  #[test]
  fn rejects_a_region_entirely_off_the_monitor() {
    assert!(physical_capture_rect(region(2000.0, 0.0, 100.0, 100.0), 1.0, 1920, 1080).is_none());
  }

  #[test]
  fn rejects_an_empty_or_nonsensical_region() {
    assert!(physical_capture_rect(region(0.0, 0.0, 0.0, 100.0), 1.0, 1920, 1080).is_none());
    assert!(physical_capture_rect(region(0.0, 0.0, 100.0, 100.0), 0.0, 1920, 1080).is_none());
    assert!(physical_capture_rect(region(f64::NAN, 0.0, 100.0, 100.0), 1.0, 1920, 1080).is_none());
  }

  /// A still with `colours` distinct colours, tiled over the whole image.
  fn palette_image(width: u32, height: u32, colours: u32, alpha: u8) -> CapturedImage {
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for pixel in 0..width * height {
      let colour = pixel % colours;
      rgba.extend_from_slice(&[
        (colour % 256) as u8,
        (colour / 256 % 256) as u8,
        (colour / 65_536 % 256) as u8,
        alpha,
      ]);
    }
    CapturedImage {
      rgba,
      width,
      height,
    }
  }

  /// A smooth gradient, which is far past what a 256-colour palette holds.
  fn gradient(width: u32, height: u32, alpha: u8) -> CapturedImage {
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
      for x in 0..width {
        rgba.extend_from_slice(&[(x % 256) as u8, (y % 256) as u8, 128, alpha]);
      }
    }
    CapturedImage {
      rgba,
      width,
      height,
    }
  }

  fn png_info(png: &[u8]) -> (png::ColorType, png::BitDepth, u32, u32, usize) {
    let reader = png::Decoder::new(Cursor::new(png)).read_info().unwrap();
    let info = reader.info();
    let palette = info.palette.as_ref().map_or(0, |palette| palette.len() / 3);
    (
      info.color_type,
      info.bit_depth,
      info.width,
      info.height,
      palette,
    )
  }

  #[test]
  fn writes_an_opaque_still_as_an_eight_bit_indexed_png() {
    let image = gradient(320, 200, 255);
    let png = encode_png(&image).unwrap();
    let (color, depth, width, height, palette) = png_info(&png);

    assert_eq!(color, png::ColorType::Indexed);
    assert_eq!(depth, png::BitDepth::Eight);
    assert_eq!((width, height), (320, 200));
    assert!(palette > 0 && palette <= MAX_PALETTE);
    assert!(png.len() < image.rgba.len());
  }

  #[test]
  fn keeps_a_still_that_already_fits_a_palette_bit_exact() {
    let image = palette_image(64, 64, 200, 255);
    let png = encode_png(&image).unwrap();
    let (color, _, _, _, palette) = png_info(&png);

    assert_eq!(color, png::ColorType::Indexed);
    assert_eq!(palette, 200);

    let decoded = image::load_from_memory(&png).unwrap().to_rgba8();
    assert_eq!(decoded.into_raw(), image.rgba);
  }

  #[test]
  fn stays_close_to_the_original_when_it_has_to_quantize() {
    let image = gradient(256, 256, 255);
    let png = encode_png(&image).unwrap();
    let decoded = image::load_from_memory(&png).unwrap().to_rgba8();

    assert_eq!(decoded.dimensions(), (256, 256));
    let error: u64 = decoded
      .as_raw()
      .iter()
      .zip(&image.rgba)
      .map(|(decoded, original)| u64::from(decoded.abs_diff(*original)))
      .sum();
    let mean = error as f64 / image.rgba.len() as f64;
    assert!(mean < 8.0, "mean channel error {mean} is too high");
  }

  #[test]
  fn keeps_every_channel_when_the_still_is_not_opaque() {
    let image = gradient(64, 64, 128);
    let png = encode_png(&image).unwrap();
    let (color, _, _, _, _) = png_info(&png);

    assert_eq!(color, png::ColorType::Rgba);
    let decoded = image::load_from_memory(&png).unwrap().to_rgba8();
    assert_eq!(decoded.into_raw(), image.rgba);
  }

  #[test]
  fn names_a_still_the_way_the_platform_does() {
    let captured_at = NaiveDate::from_ymd_opt(2026, 8, 8)
      .unwrap()
      .and_hms_opt(14, 32, 5)
      .unwrap();
    assert_eq!(
      screenshot_file_stem(captured_at),
      "Orbit Capture 2026-08-08 at 14.32.05"
    );
  }

  #[test]
  fn zero_pads_every_field_of_the_name() {
    let captured_at = NaiveDate::from_ymd_opt(2026, 1, 2)
      .unwrap()
      .and_hms_opt(9, 5, 3)
      .unwrap();
    assert_eq!(
      screenshot_file_stem(captured_at),
      "Orbit Capture 2026-01-02 at 09.05.03"
    );
  }

  #[test]
  fn uses_the_plain_name_when_nothing_is_in_the_way() {
    let path = unique_path(Path::new("/tmp"), "Shot", "png", &|_| false);
    assert_eq!(path, Path::new("/tmp/Shot.png"));
  }

  #[test]
  fn counts_up_past_every_name_already_taken() {
    let taken: HashSet<PathBuf> = ["/tmp/Shot.png", "/tmp/Shot (2).png", "/tmp/Shot (3).png"]
      .iter()
      .map(PathBuf::from)
      .collect();
    let path = unique_path(Path::new("/tmp"), "Shot", "png", &|candidate| {
      taken.contains(candidate)
    });
    assert_eq!(path, Path::new("/tmp/Shot (4).png"));
  }

  #[test]
  fn starts_the_suffix_at_two() {
    let taken: HashSet<PathBuf> = ["/tmp/Shot.png"].iter().map(PathBuf::from).collect();
    let path = unique_path(Path::new("/tmp"), "Shot", "png", &|candidate| {
      taken.contains(candidate)
    });
    assert_eq!(path, Path::new("/tmp/Shot (2).png"));
  }

  #[test]
  fn deserializes_every_target_the_bar_can_send() {
    let screen: ScreenshotTarget =
      serde_json::from_str(r#"{"kind":"screen","monitorId":7}"#).unwrap();
    assert!(matches!(screen, ScreenshotTarget::Screen { monitor_id: 7 }));

    let window: ScreenshotTarget =
      serde_json::from_str(r#"{"kind":"window","windowId":42}"#).unwrap();
    assert!(matches!(window, ScreenshotTarget::Window { window_id: 42 }));

    let region: ScreenshotTarget = serde_json::from_str(
      r#"{"kind":"region","monitorId":7,"region":{"position":{"x":1,"y":2},"size":{"width":3,"height":4}}}"#,
    )
    .unwrap();
    let ScreenshotTarget::Region {
      monitor_id, region, ..
    } = region
    else {
      panic!("expected a region target");
    };
    assert_eq!(monitor_id, 7);
    assert_eq!(region.size.width, 3.0);
  }
}
