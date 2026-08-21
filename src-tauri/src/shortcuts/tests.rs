// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashSet;

use super::*;

#[test]
fn defaults_open_the_recording_bar_take_screenshots_and_recognize_text() {
  let settings = ShortcutSettings::default();
  let assigned = settings
    .bindings
    .iter()
    .filter(|binding| binding.shortcut.is_some())
    .collect::<Vec<_>>();
  assert_eq!(assigned.len(), 4);
  assert_eq!(assigned[0].action, ShortcutAction::ToggleRecordingBar);
  assert_eq!(assigned[1].action, ShortcutAction::TakeScreenshot);
  assert_eq!(
    assigned[1].shortcut.as_deref(),
    Some("CommandOrControl+Shift+Digit8")
  );
  assert_eq!(assigned[2].action, ShortcutAction::RecognizeText);
  assert_eq!(
    assigned[2].shortcut.as_deref(),
    Some("CommandOrControl+Shift+KeyT")
  );
  assert_eq!(assigned[3].action, ShortcutAction::RulerOverlay);
  assert_eq!(
    assigned[3].shortcut.as_deref(),
    Some("CommandOrControl+Shift+KeyR")
  );
}

#[test]
fn each_frontend_action_goes_to_the_window_that_performs_it() {
  assert_eq!(
    action_window(ShortcutAction::StartStopRecording).map(WindowLabel::as_str),
    Some(WindowLabel::RecordingBar.as_str())
  );
  assert_eq!(
    action_window(ShortcutAction::TakeScreenshot).map(WindowLabel::as_str),
    Some(WindowLabel::RegionSelector.as_str())
  );
}

#[test]
fn taking_a_screenshot_never_reaches_the_recording_bar() {
  assert_ne!(
    action_window(ShortcutAction::TakeScreenshot).map(WindowLabel::as_str),
    action_window(ShortcutAction::StartStopRecording).map(WindowLabel::as_str)
  );
}

#[test]
fn taking_a_screenshot_keeps_the_ruler_visible() {
  assert_eq!(
    preserved_capture_overlay(ShortcutAction::TakeScreenshot),
    Some(crate::capture_overlays::CaptureOverlay::Ruler)
  );
  assert_eq!(
    preserved_capture_overlay(ShortcutAction::RulerOverlay),
    Some(crate::capture_overlays::CaptureOverlay::Ruler)
  );
  assert_eq!(
    preserved_capture_overlay(ShortcutAction::RecognizeText),
    Some(crate::capture_overlays::CaptureOverlay::TextRecognition)
  );
}

#[test]
fn the_actions_rust_handles_alone_ask_no_window() {
  for action in [
    ShortcutAction::ToggleRecordingBar,
    ShortcutAction::PauseResumeRecording,
    ShortcutAction::RecognizeText,
    ShortcutAction::RulerOverlay,
  ] {
    assert!(action_window(action).is_none());
  }
}

#[test]
fn every_action_appears_once() {
  let settings = ShortcutSettings::default();
  let actions = settings
    .bindings
    .iter()
    .map(|binding| binding.action)
    .collect::<HashSet<_>>();
  assert_eq!(actions.len(), settings.bindings.len());
}
