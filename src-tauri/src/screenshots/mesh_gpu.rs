// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::{mpsc, OnceLock};

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use super::mesh::MeshGradientPoint;

const MAX_POINTS: usize = 4;
const SHADER: &str = include_str!("mesh.wgsl");

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct MeshUniforms {
  dimensions: [u32; 2],
  point_count: u32,
  seed: u32,
  warp_percent: f32,
  _padding: [f32; 3],
  base_color: [f32; 4],
  points: [[f32; 8]; MAX_POINTS],
  colors: [[f32; 4]; MAX_POINTS],
}

struct Renderer {
  device: wgpu::Device,
  pipeline: wgpu::ComputePipeline,
  queue: wgpu::Queue,
}

static RENDERER: OnceLock<Result<Renderer, String>> = OnceLock::new();

fn renderer() -> Result<&'static Renderer, String> {
  RENDERER
    .get_or_init(|| pollster::block_on(Renderer::new()))
    .as_ref()
    .map_err(Clone::clone)
}

impl Renderer {
  async fn new() -> Result<Self, String> {
    let instance = wgpu::Instance::default();
    let adapter = instance
      .request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
      })
      .await
      .map_err(|error| {
        format!("A graphics adapter is required to render mesh backgrounds: {error}")
      })?;
    let (device, queue) = adapter
      .request_device(&wgpu::DeviceDescriptor {
        label: Some("Orbit Capture mesh renderer"),
        ..Default::default()
      })
      .await
      .map_err(|error| format!("The graphics device could not be opened: {error}"))?;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
      label: Some("Orbit Capture mesh shader"),
      source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
      label: Some("Orbit Capture mesh pipeline"),
      layout: None,
      module: &shader,
      entry_point: Some("main"),
      compilation_options: Default::default(),
      cache: None,
    });
    Ok(Self {
      device,
      pipeline,
      queue,
    })
  }

  fn render(&self, width: u32, height: u32, uniforms: &MeshUniforms) -> Result<Vec<u8>, String> {
    let uniform_buffer = self
      .device
      .create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Orbit Capture mesh parameters"),
        contents: bytemuck::bytes_of(uniforms),
        usage: wgpu::BufferUsages::UNIFORM,
      });
    let texture = self.device.create_texture(&wgpu::TextureDescriptor {
      label: Some("Orbit Capture mesh output"),
      size: wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
      },
      mip_level_count: 1,
      sample_count: 1,
      dimension: wgpu::TextureDimension::D2,
      format: wgpu::TextureFormat::Rgba8Unorm,
      usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
      view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
      label: Some("Orbit Capture mesh bindings"),
      layout: &self.pipeline.get_bind_group_layout(0),
      entries: &[
        wgpu::BindGroupEntry {
          binding: 0,
          resource: uniform_buffer.as_entire_binding(),
        },
        wgpu::BindGroupEntry {
          binding: 1,
          resource: wgpu::BindingResource::TextureView(&view),
        },
      ],
    });
    let unpadded_bytes_per_row = width * 4;
    let bytes_per_row = unpadded_bytes_per_row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
      * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let output = self.device.create_buffer(&wgpu::BufferDescriptor {
      label: Some("Orbit Capture mesh readback"),
      size: u64::from(bytes_per_row) * u64::from(height),
      usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
      mapped_at_creation: false,
    });
    let mut encoder = self
      .device
      .create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Orbit Capture mesh commands"),
      });
    {
      let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
        label: Some("Orbit Capture mesh pass"),
        timestamp_writes: None,
      });
      pass.set_pipeline(&self.pipeline);
      pass.set_bind_group(0, &bind_group, &[]);
      pass.dispatch_workgroups(width.div_ceil(16), height.div_ceil(16), 1);
    }
    encoder.copy_texture_to_buffer(
      texture.as_image_copy(),
      wgpu::TexelCopyBufferInfo {
        buffer: &output,
        layout: wgpu::TexelCopyBufferLayout {
          offset: 0,
          bytes_per_row: Some(bytes_per_row),
          rows_per_image: Some(height),
        },
      },
      texture.size(),
    );
    self.queue.submit([encoder.finish()]);
    let slice = output.slice(..);
    let (sender, receiver) = mpsc::sync_channel(1);
    slice.map_async(wgpu::MapMode::Read, move |result| {
      let _ = sender.send(result);
    });
    self
      .device
      .poll(wgpu::PollType::wait_indefinitely())
      .map_err(|error| error.to_string())?;
    receiver
      .recv()
      .map_err(|error| error.to_string())?
      .map_err(|error| error.to_string())?;
    let mapped = slice
      .get_mapped_range()
      .map_err(|error| error.to_string())?;
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for row in mapped.chunks_exact(bytes_per_row as usize) {
      pixels.extend_from_slice(&row[..unpadded_bytes_per_row as usize]);
    }
    drop(mapped);
    output.unmap();
    Ok(pixels)
  }
}

fn channel(value: u8) -> f32 {
  f32::from(value) / 255.0
}

fn color(value: &[u8; 4]) -> [f32; 4] {
  [channel(value[0]), channel(value[1]), channel(value[2]), 1.0]
}

pub(super) fn render(
  width: u32,
  height: u32,
  colors: &[[u8; 4]],
  points: &[MeshGradientPoint],
  seed: u32,
  warp_percent: f64,
) -> Result<image::RgbaImage, String> {
  let mut uniforms = MeshUniforms {
    dimensions: [width, height],
    point_count: points.len() as u32,
    seed,
    warp_percent: warp_percent as f32,
    _padding: [0.0; 3],
    base_color: color(colors.last().expect("a validated mesh has a base colour")),
    points: [[0.0; 8]; MAX_POINTS],
    colors: [[0.0; 4]; MAX_POINTS],
  };
  for (index, point) in points.iter().enumerate() {
    let angle = point.rotation.to_radians() as f32;
    uniforms.points[index] = [
      point.x as f32 / 100.0,
      point.y as f32 / 100.0,
      point.radius_x as f32 / 100.0,
      point.radius_y as f32 / 100.0,
      angle.cos(),
      angle.sin(),
      0.0,
      0.0,
    ];
    uniforms.colors[index] = color(&colors[index]);
  }
  let pixels = renderer()?.render(width, height, &uniforms)?;
  image::RgbaImage::from_raw(width, height, pixels)
    .ok_or_else(|| "The GPU returned invalid mesh pixels".to_owned())
}

#[cfg(test)]
mod tests {
  use std::time::Instant;

  use super::*;

  #[test]
  #[ignore = "4K GPU stress test"]
  fn renders_and_encodes_a_4k_mesh_without_noise_bloat() {
    let colors = [
      [255, 46, 129, 255],
      [34, 211, 238, 255],
      [250, 204, 21, 255],
      [99, 102, 241, 255],
      [17, 24, 39, 255],
    ];
    let points = [
      MeshGradientPoint {
        radius_x: 80.0,
        radius_y: 52.0,
        rotation: 24.0,
        x: 12.0,
        y: 18.0,
      },
      MeshGradientPoint {
        radius_x: 60.0,
        radius_y: 88.0,
        rotation: -38.0,
        x: 88.0,
        y: 14.0,
      },
      MeshGradientPoint {
        radius_x: 94.0,
        radius_y: 48.0,
        rotation: 72.0,
        x: 22.0,
        y: 90.0,
      },
      MeshGradientPoint {
        radius_x: 54.0,
        radius_y: 82.0,
        rotation: -12.0,
        x: 92.0,
        y: 84.0,
      },
    ];
    let started = Instant::now();
    let image = render(3840, 2160, &colors, &points, 42, 10.0).unwrap();
    let rendered_in = started.elapsed();
    let (width, height) = image.dimensions();
    let mut image = crate::screenshots::CapturedImage {
      height,
      rgba: image.into_raw(),
      width,
    };
    image.rgba[3] = 0;
    let encoding_started = Instant::now();
    let encoded = crate::screenshots::encoding::encode_png(&image).unwrap();
    eprintln!(
      "4K mesh: GPU render/readback {rendered_in:?}, PNG encode {:?}, {} bytes",
      encoding_started.elapsed(),
      encoded.len()
    );
    assert_eq!((image.width, image.height), (3840, 2160));
    assert!(
      encoded.len() < 3_000_000,
      "anti-banding noise made the empty mesh PNG {} bytes",
      encoded.len()
    );
  }

  #[test]
  fn mesh_contains_distinct_colour_regions() {
    let colors = [
      [255, 20, 40, 255],
      [20, 255, 60, 255],
      [30, 60, 255, 255],
      [10, 10, 10, 255],
    ];
    let points = [
      MeshGradientPoint {
        radius_x: 48.0,
        radius_y: 42.0,
        rotation: 0.0,
        x: 8.0,
        y: 12.0,
      },
      MeshGradientPoint {
        radius_x: 46.0,
        radius_y: 52.0,
        rotation: 24.0,
        x: 92.0,
        y: 18.0,
      },
      MeshGradientPoint {
        radius_x: 54.0,
        radius_y: 44.0,
        rotation: -31.0,
        x: 48.0,
        y: 94.0,
      },
    ];
    let image = render(640, 360, &colors, &points, 42, 7.0).unwrap();
    let samples = [
      image.get_pixel(48, 40).0,
      image.get_pixel(590, 54).0,
      image.get_pixel(320, 330).0,
    ];
    let channel_range = |channel: usize| {
      let minimum = samples.iter().map(|pixel| pixel[channel]).min().unwrap();
      let maximum = samples.iter().map(|pixel| pixel[channel]).max().unwrap();
      maximum - minimum
    };
    assert!(
      channel_range(0) > 80 && channel_range(1) > 80 && channel_range(2) > 80,
      "mesh samples were unexpectedly flat: {samples:?}"
    );
  }
}
