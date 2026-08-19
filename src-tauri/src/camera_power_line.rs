// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Windows camera anti-flicker through the UVC power line frequency control.
//!
//! macOS removes mains flicker by pinning the camera to a PAL cadence (25/50
//! fps), which AVFoundation allows anywhere inside a format's frame rate range.
//! Media Foundation only offers the discrete frame intervals a camera's
//! descriptor lists — a C920 exposes 5/10/15/20/24/30, never 25 — so the
//! cadence route is closed there. What Windows does expose is the camera's own
//! anti-flicker: the UVC power line frequency control, which makes the camera
//! hold exposure to multiples of the mains period. Setting it is a property
//! write on the device, not on a stream, and the value persists in the camera
//! until it is unplugged; applying it just before a preview or recording opens
//! the device is enough for both.

use windows::core::{Interface, HSTRING};
use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use windows::Win32::Media::DirectShow::{IAMVideoProcAmp, VideoProcAmp_Flags_Manual};
use windows::Win32::Media::MediaFoundation::{
  IMFAttributes, MFCreateAttributes, MFCreateDeviceSource, MFShutdown, MFStartup,
  MFSTARTUP_NOSOCKET, MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
  MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
  MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK, MF_VERSION,
};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

/// `KSPROPERTY_VIDEOPROCAMP_POWERLINE_FREQUENCY`. DirectShow's public
/// `VideoProcAmpProperty` enum stops at gain; the kernel streaming set carries
/// on and `IAMVideoProcAmp` on a Media Foundation source forwards any id the
/// driver knows.
const POWER_LINE_FREQUENCY: i32 = 13;
const POWER_LINE_50_HZ: i32 = 1;
const POWER_LINE_60_HZ: i32 = 2;

/// Tells the camera behind `device_id` (its Media Foundation symbolic link)
/// which mains frequency to cancel: 50 Hz under PAL, 60 Hz otherwise.
///
/// Returns the value the camera reports afterwards, which a driver that
/// accepted the write silently may still differ from the request.
pub(crate) fn apply_power_line_frequency(device_id: &str, pal: bool) -> Result<i32, String> {
  let _com = Com::enter()?;
  let _media_foundation = MediaFoundation::start()?;
  let source = device_source(device_id)?;
  let result = set_power_line_frequency(&source, pal);
  let _ = unsafe { source.Shutdown() };
  result
}

fn set_power_line_frequency(
  source: &windows::Win32::Media::MediaFoundation::IMFMediaSource,
  pal: bool,
) -> Result<i32, String> {
  let proc_amp = source
    .cast::<IAMVideoProcAmp>()
    .map_err(|error| format!("The camera has no video processing controls: {error}"))?;
  let wanted = if pal {
    POWER_LINE_50_HZ
  } else {
    POWER_LINE_60_HZ
  };
  unsafe { proc_amp.Set(POWER_LINE_FREQUENCY, wanted, VideoProcAmp_Flags_Manual.0) }
    .map_err(|error| format!("The camera has no power line frequency control: {error}"))?;
  let mut applied = 0;
  let mut flags = 0;
  unsafe { proc_amp.Get(POWER_LINE_FREQUENCY, &mut applied, &mut flags) }
    .map_err(|error| format!("The camera did not report its power line frequency: {error}"))?;
  Ok(applied)
}

fn device_source(
  symbolic_link: &str,
) -> Result<windows::Win32::Media::MediaFoundation::IMFMediaSource, String> {
  let mut attributes: Option<IMFAttributes> = None;
  unsafe { MFCreateAttributes(&mut attributes, 2) }.map_err(|error| error.to_string())?;
  let attributes =
    attributes.ok_or_else(|| "Media Foundation returned no attributes".to_owned())?;
  unsafe {
    attributes.SetGUID(
      &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
      &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
    )
  }
  .map_err(|error| error.to_string())?;
  let link = HSTRING::from(symbolic_link);
  unsafe {
    attributes.SetString(
      &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
      &link,
    )
  }
  .map_err(|error| error.to_string())?;
  unsafe { MFCreateDeviceSource(&attributes) }
    .map_err(|error| format!("The selected camera could not be opened for configuration: {error}"))
}

/// COM for the calling thread, tolerant of a thread that already holds it in
/// another apartment (nokhwa initialises its threads apartment-threaded).
struct Com {
  owned: bool,
}

impl Com {
  fn enter() -> Result<Self, String> {
    let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    if result == RPC_E_CHANGED_MODE {
      return Ok(Self { owned: false });
    }
    result.ok().map_err(|error| error.to_string())?;
    Ok(Self { owned: true })
  }
}

impl Drop for Com {
  fn drop(&mut self) {
    if self.owned {
      unsafe { CoUninitialize() };
    }
  }
}

struct MediaFoundation;

impl MediaFoundation {
  fn start() -> Result<Self, String> {
    unsafe { MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET) }.map_err(|error| error.to_string())?;
    Ok(Self)
  }
}

impl Drop for MediaFoundation {
  fn drop(&mut self) {
    let _ = unsafe { MFShutdown() };
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  #[ignore = "requires a physical UVC camera"]
  fn sets_and_reads_back_the_power_line_frequency() {
    let cameras = nokhwa::query(nokhwa::utils::ApiBackend::Auto).unwrap();
    for camera in &cameras {
      let id = crate::recording_inputs::camera_id(camera);
      let pal = apply_power_line_frequency(&id, true);
      let ntsc = apply_power_line_frequency(&id, false);
      println!("{}: PAL -> {pal:?}, NTSC -> {ntsc:?}", camera.human_name());
    }
    assert!(cameras.iter().any(|camera| {
      apply_power_line_frequency(&crate::recording_inputs::camera_id(camera), true)
        == Ok(POWER_LINE_50_HZ)
    }));
  }
}
