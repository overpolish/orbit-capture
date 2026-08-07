#[cfg(target_os = "macos")]
use std::collections::HashSet;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionKind {
  Accessibility,
  ScreenRecording,
  Camera,
  Microphone,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionStatus {
  pub can_request: bool,
  pub granted: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionSnapshot {
  pub accessibility: PermissionStatus,
  pub screen_recording: PermissionStatus,
  pub camera: PermissionStatus,
  pub microphone: PermissionStatus,
}

impl PermissionSnapshot {
  pub fn unavailable() -> Self {
    let status = PermissionStatus {
      can_request: true,
      granted: false,
    };

    Self {
      accessibility: status,
      screen_recording: status,
      camera: status,
      microphone: status,
    }
  }

  pub fn granted() -> Self {
    let status = PermissionStatus {
      can_request: false,
      granted: true,
    };

    Self {
      accessibility: status,
      screen_recording: status,
      camera: status,
      microphone: status,
    }
  }

  pub fn has_required_recording_permissions(&self) -> bool {
    self.accessibility.granted && self.screen_recording.granted
  }

  pub fn missing(&self, required: &[PermissionKind]) -> Vec<PermissionKind> {
    required
      .iter()
      .copied()
      .filter(|permission| !self.status(*permission).granted)
      .collect()
  }

  #[cfg(target_os = "macos")]
  pub fn with_request_state(mut self, requested: &HashSet<PermissionKind>) -> Self {
    for permission in requested {
      self.status_mut(*permission).can_request = false;
    }
    self
  }

  fn status(&self, permission: PermissionKind) -> PermissionStatus {
    match permission {
      PermissionKind::Accessibility => self.accessibility,
      PermissionKind::ScreenRecording => self.screen_recording,
      PermissionKind::Camera => self.camera,
      PermissionKind::Microphone => self.microphone,
    }
  }

  #[cfg(target_os = "macos")]
  fn status_mut(&mut self, permission: PermissionKind) -> &mut PermissionStatus {
    match permission {
      PermissionKind::Accessibility => &mut self.accessibility,
      PermissionKind::ScreenRecording => &mut self.screen_recording,
      PermissionKind::Camera => &mut self.camera,
      PermissionKind::Microphone => &mut self.microphone,
    }
  }
}
