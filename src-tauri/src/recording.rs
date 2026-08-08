use std::{
  path::PathBuf,
  sync::{
    atomic::{AtomicU64, Ordering},
    Mutex, RwLock,
  },
  time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, State};

use crate::windows;

const RECORDING_STATE_EVENT: &str = "recording://state";
const RECORDING_ERROR_EVENT: &str = "recording://error";
// The stub finalize keeps the `Stopping` state on screen long enough for the
// pill's finishing affordance to be exercised before real muxing exists.
const FINALIZE_DELAY_MS: u64 = 250;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RecordingStatus {
  #[default]
  Idle,
  Starting,
  Recording,
  Paused,
  Stopping,
}

impl RecordingStatus {
  const fn label(self) -> &'static str {
    match self {
      Self::Idle => "idle",
      Self::Starting => "starting",
      Self::Recording => "recording",
      Self::Paused => "paused",
      Self::Stopping => "stopping",
    }
  }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RecordingMode {
  Screen,
  Region,
  Window,
  Camera,
  Audio,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct Region {
  pub position: LogicalPosition<f64>,
  pub size: LogicalSize<f64>,
}

/// Options assembled by the recording bar from the source and input stores.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartRecordingOptions {
  pub mode: RecordingMode,
  #[serde(default)]
  pub monitor_id: Option<u32>,
  #[serde(default)]
  pub window_id: Option<u32>,
  #[serde(default)]
  pub region: Option<Region>,
  #[serde(default)]
  pub show_cursor: bool,
  #[serde(default)]
  pub system_audio: bool,
  #[serde(default)]
  pub microphone_id: Option<String>,
  #[serde(default)]
  pub camera_id: Option<String>,
}

/// Epoch-millisecond timestamps are stamped by Rust so every window - including
/// ones that reload or join late - derives the same elapsed time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingSnapshot {
  pub status: RecordingStatus,
  pub mode: Option<RecordingMode>,
  pub started_at_ms: Option<u64>,
  pub accumulated_ms: u64,
  pub paused_at_ms: Option<u64>,
}

/// Handles owned by the capture pipeline. Empty until real capture lands.
#[derive(Debug, Default)]
struct CaptureHandles {
  excluded_window_labels: Vec<String>,
  output_path: Option<PathBuf>,
}

#[derive(Default)]
pub struct RecordingState {
  snapshot: RwLock<RecordingSnapshot>,
  handles: Mutex<Option<CaptureHandles>>,
  generation: AtomicU64,
}

impl RecordingState {
  /// Claims the current start attempt, invalidating any in-flight one.
  fn begin_start(&self) -> u64 {
    self
      .generation
      .fetch_add(1, Ordering::SeqCst)
      .wrapping_add(1)
  }

  fn is_current(&self, generation: u64) -> bool {
    self.generation.load(Ordering::SeqCst) == generation
  }

  /// Invalidates the in-flight start so a late confirmation is ignored.
  fn cancel(&self) {
    self.generation.fetch_add(1, Ordering::SeqCst);
  }
}

fn now_ms() -> u64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .ok()
    .and_then(|elapsed| u64::try_from(elapsed.as_millis()).ok())
    .unwrap_or_default()
}

fn state(app: &AppHandle) -> State<'_, RecordingState> {
  app.state::<RecordingState>()
}

pub fn snapshot(app: &AppHandle) -> RecordingSnapshot {
  app
    .try_state::<RecordingState>()
    .map(|state| {
      *state
        .snapshot
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
    })
    .unwrap_or_default()
}

/// Whether the app is outside a recording. Window commands that would re-show
/// hidden chrome consult this before doing anything.
pub fn is_idle(app: &AppHandle) -> bool {
  snapshot(app).status == RecordingStatus::Idle
}

/// The span of the current recording run, i.e. time since the last resume.
fn open_span_ms(snapshot: &RecordingSnapshot, now: u64) -> u64 {
  match snapshot.started_at_ms {
    Some(started_at_ms) if snapshot.status == RecordingStatus::Recording => {
      now.saturating_sub(started_at_ms)
    }
    _ => 0,
  }
}

/// Validates the transition and, only if it is legal, applies it. Pure so the
/// full transition table can be unit tested without an app handle.
fn apply_transition(
  snapshot: &mut RecordingSnapshot,
  to: RecordingStatus,
  mode: Option<RecordingMode>,
  now: u64,
) -> Result<(), String> {
  match (snapshot.status, to) {
    (RecordingStatus::Idle, RecordingStatus::Starting) => {
      *snapshot = RecordingSnapshot {
        status: RecordingStatus::Starting,
        mode,
        ..RecordingSnapshot::default()
      };
    }
    (RecordingStatus::Starting, RecordingStatus::Recording) => {
      snapshot.status = RecordingStatus::Recording;
      snapshot.started_at_ms = Some(now);
      snapshot.paused_at_ms = None;
    }
    (RecordingStatus::Recording, RecordingStatus::Paused) => {
      snapshot.accumulated_ms = snapshot
        .accumulated_ms
        .saturating_add(open_span_ms(snapshot, now));
      snapshot.status = RecordingStatus::Paused;
      snapshot.started_at_ms = None;
      snapshot.paused_at_ms = Some(now);
    }
    (RecordingStatus::Paused, RecordingStatus::Recording) => {
      snapshot.status = RecordingStatus::Recording;
      snapshot.started_at_ms = Some(now);
      snapshot.paused_at_ms = None;
    }
    (RecordingStatus::Recording | RecordingStatus::Paused, RecordingStatus::Stopping) => {
      snapshot.accumulated_ms = snapshot
        .accumulated_ms
        .saturating_add(open_span_ms(snapshot, now));
      snapshot.status = RecordingStatus::Stopping;
      snapshot.started_at_ms = None;
      snapshot.paused_at_ms = None;
    }
    (RecordingStatus::Starting | RecordingStatus::Stopping, RecordingStatus::Idle) => {
      *snapshot = RecordingSnapshot::default();
    }
    (from, to) => {
      return Err(format!(
        "A recording cannot move from {} to {}",
        from.label(),
        to.label()
      ))
    }
  }

  Ok(())
}

/// Mutates under the lock, drops the guard, and only then emits and touches the
/// tray. Nothing that takes another mutex may run while the guard is alive.
fn transition(
  app: &AppHandle,
  to: RecordingStatus,
  mode: Option<RecordingMode>,
) -> Result<RecordingSnapshot, String> {
  let (changed, snapshot) = {
    let state = state(app);
    let mut current = state
      .snapshot
      .write()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    let previous = *current;
    apply_transition(&mut current, to, mode, now_ms())?;
    (previous != *current, *current)
  };

  if changed {
    let _ = app.emit(RECORDING_STATE_EVENT, snapshot);

    #[cfg(desktop)]
    crate::tray::apply_recording_status(app, snapshot.status);
  }

  Ok(snapshot)
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RecordingErrorPayload {
  phase: &'static str,
  message: String,
}

fn emit_error(app: &AppHandle, phase: &'static str, message: &str) {
  eprintln!("Recording {phase} failed: {message}");
  let _ = app.emit(
    RECORDING_ERROR_EVENT,
    RecordingErrorPayload {
      phase,
      message: message.to_owned(),
    },
  );
}

fn require_status(
  app: &AppHandle,
  allowed: &[RecordingStatus],
  action: &str,
) -> Result<RecordingStatus, String> {
  let status = snapshot(app).status;
  if allowed.contains(&status) {
    Ok(status)
  } else {
    Err(format!(
      "A recording that is {} cannot {action}",
      status.label()
    ))
  }
}

fn take_handles(app: &AppHandle) -> Option<CaptureHandles> {
  state(app)
    .handles
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .take()
}

fn store_handles(app: &AppHandle, handles: CaptureHandles) {
  *state(app)
    .handles
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(handles);
}

// ---------------------------------------------------------------------------
// Capture seams. These are deliberately no-ops with the shapes the real
// pipeline needs so encoding can drop in without reshaping the state machine.
// ---------------------------------------------------------------------------

/// Defence in depth, run before anything is hidden or transitioned. The Record
/// button is already gated on a selected source, but a mode without one could
/// never produce a file.
fn validate_options(options: &StartRecordingOptions) -> Result<(), String> {
  match options.mode {
    RecordingMode::Screen | RecordingMode::Region if options.monitor_id.is_none() => {
      Err("No monitor is selected to record".to_owned())
    }
    RecordingMode::Window if options.window_id.is_none() => {
      Err("No window is selected to record".to_owned())
    }
    RecordingMode::Audio if !options.system_audio && options.microphone_id.is_none() => {
      Err("No audio source is selected to record".to_owned())
    }
    _ => Ok(()),
  }
}

fn begin_capture(
  app: &AppHandle,
  options: &StartRecordingOptions,
) -> Result<CaptureHandles, String> {
  // These configure the real encoder; the stub only proves that they arrive
  // intact.
  let _ = (
    options.camera_id.as_deref(),
    options.microphone_id.as_deref(),
    options.monitor_id,
    options.region.map(|region| (region.position, region.size)),
    options.show_cursor,
    options.system_audio,
    options.window_id,
  );

  // The real pipeline hands these labels to ScreenCaptureKit (macOS) and to the
  // Windows capture graph so the app's own overlays never land in the file.
  let excluded_window_labels = windows::CAPTURE_EXCLUDED_WINDOW_LABELS
    .iter()
    .filter(|label| app.get_webview_window(label).is_some())
    .map(|label| (*label).to_owned())
    .collect();

  Ok(CaptureHandles {
    excluded_window_labels,
    output_path: None,
  })
}

fn pause_capture(_handles: &mut CaptureHandles) {
  // Real capture suspends the encoder segment here.
}

fn resume_capture(_handles: &mut CaptureHandles) -> Result<(), String> {
  // Real resume spawns a fresh encoder segment, which can fail.
  Ok(())
}

fn finalize_capture(handles: CaptureHandles) -> Result<Option<PathBuf>, String> {
  // Real finalize muxes the segments that were captured with
  // `excluded_window_labels` kept out of frame, then returns the file's path.
  let CaptureHandles {
    excluded_window_labels,
    output_path,
  } = handles;
  let _ = excluded_window_labels;

  Ok(output_path)
}

fn discard_capture(handles: Option<CaptureHandles>) {
  // Real discard deletes the segment files written under `output_path`.
  let _ = handles.and_then(|handles| handles.output_path);
}

// ---------------------------------------------------------------------------
// Window choreography. Every call here composes existing window commands and
// runs with no recording lock held.
// ---------------------------------------------------------------------------

fn prepare_windows(app: &AppHandle, options: &StartRecordingOptions) -> Result<(), String> {
  let to_message = |error: tauri::Error| error.to_string();

  windows::hide_recording_options(app.clone()).map_err(to_message)?;
  windows::collapse_recording_source_selector(app.clone()).map_err(to_message)?;
  windows::set_recording_source_selector_visible(app.clone(), false).map_err(to_message)?;
  windows::hide_recording_bar(app).map_err(to_message)?;

  if options.mode == RecordingMode::Region {
    // The overlay stays up as the recording boundary, but must stop eating
    // clicks now that the user is no longer editing the region.
    windows::set_region_selector_passthrough(app.clone(), true).map_err(to_message)?;
  } else {
    windows::hide_region_selector(app.clone()).map_err(to_message)?;
  }

  windows::show_recording_dock(app).map_err(to_message)?;

  Ok(())
}

fn restore_windows(app: &AppHandle) {
  let _ = windows::hide_recording_dock(app);
  if windows::is_region_selector_visible(app) {
    // Interactivity is deliberately not touched here. The overlay is about to
    // be hidden, and when the bar shows it again `show_region_selector`
    // re-asserts the invariant for us.
    let _ = windows::hide_region_selector(app.clone());
  }
}

/// Shows the recording bar again. Must run after the snapshot is back to
/// `Idle` so the bar's mode-driven UI sync is no longer gated.
fn show_recording_ui(app: &AppHandle) {
  if let Err(error) = windows::show_recording_ui(app) {
    eprintln!("Could not restore the recording bar: {error}");
  }
}

// ---------------------------------------------------------------------------
// Lifecycle. Each entry point validates before it causes any side effect, and
// is callable from both the commands below and the tray menu.
// ---------------------------------------------------------------------------

pub fn start(app: &AppHandle, options: StartRecordingOptions) -> Result<(), String> {
  validate_options(&options)?;
  // A second start while `Starting` is rejected here, not merely by a
  // disabled button.
  transition(app, RecordingStatus::Starting, Some(options.mode))?;
  let generation = state(app).begin_start();

  let handles = prepare_windows(app, &options).and_then(|()| begin_capture(app, &options));
  let handles = match handles {
    Ok(handles) => handles,
    Err(error) => {
      emit_error(app, "start", &error);
      state(app).cancel();
      discard_capture(take_handles(app));
      restore_windows(app);
      let _ = transition(app, RecordingStatus::Idle, None);
      show_recording_ui(app);
      return Err(error);
    }
  };
  store_handles(app, handles);

  let app = app.clone();
  tauri::async_runtime::spawn(async move {
    // Real capture confirms its first frame here; the stub keeps the async
    // shape and the stale-start guard that goes with it.
    if !state(&app).is_current(generation) {
      return;
    }
    if let Err(error) = transition(&app, RecordingStatus::Recording, None) {
      emit_error(&app, "start", &error);
    }
  });

  Ok(())
}

pub fn pause(app: &AppHandle) -> Result<(), String> {
  transition(app, RecordingStatus::Paused, None).inspect_err(|error| {
    emit_error(app, "pause", error);
  })?;

  if let Some(handles) = state(app)
    .handles
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .as_mut()
  {
    pause_capture(handles);
  }

  Ok(())
}

pub fn resume(app: &AppHandle) -> Result<(), String> {
  require_status(app, &[RecordingStatus::Paused], "resume").inspect_err(|error| {
    emit_error(app, "resume", error);
  })?;

  let resumed = {
    let state = state(app);
    let mut handles = state
      .handles
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    handles.as_mut().map_or(Ok(()), resume_capture)
  };
  if let Err(error) = resumed {
    emit_error(app, "resume", &error);
    return Err(error);
  }

  transition(app, RecordingStatus::Recording, None).inspect_err(|error| {
    emit_error(app, "resume", error);
  })?;

  Ok(())
}

pub fn toggle_pause(app: &AppHandle) -> Result<(), String> {
  if snapshot(app).status == RecordingStatus::Paused {
    resume(app)
  } else {
    pause(app)
  }
}

pub fn stop(app: &AppHandle) -> Result<(), String> {
  transition(app, RecordingStatus::Stopping, None).inspect_err(|error| {
    emit_error(app, "stop", error);
  })?;
  state(app).cancel();

  let app = app.clone();
  // `tokio` is macOS-only in this crate, so the finalize wait uses a blocking
  // task the way the window animations do.
  tauri::async_runtime::spawn_blocking(move || {
    std::thread::sleep(Duration::from_millis(FINALIZE_DELAY_MS));

    let finalized = take_handles(&app).map_or(Ok(None), finalize_capture);
    match finalized {
      // The saved recording is surfaced to the user once capture is real.
      Ok(_path) => {}
      Err(error) => emit_error(&app, "stop", &error),
    }

    restore_windows(&app);
    if let Err(error) = transition(&app, RecordingStatus::Idle, None) {
      emit_error(&app, "stop", &error);
    }
    show_recording_ui(&app);
  });

  Ok(())
}

pub fn cancel(app: &AppHandle) -> Result<(), String> {
  let status = require_status(
    app,
    &[
      RecordingStatus::Starting,
      RecordingStatus::Recording,
      RecordingStatus::Paused,
    ],
    "be discarded",
  )?;

  if matches!(status, RecordingStatus::Recording | RecordingStatus::Paused) {
    transition(app, RecordingStatus::Stopping, None)?;
  }
  state(app).cancel();

  discard_capture(take_handles(app));
  restore_windows(app);
  transition(app, RecordingStatus::Idle, None)?;
  show_recording_ui(app);

  Ok(())
}

#[tauri::command]
pub fn get_recording_snapshot(state: State<'_, RecordingState>) -> RecordingSnapshot {
  *state
    .snapshot
    .read()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[tauri::command]
pub fn start_recording(app: AppHandle, options: StartRecordingOptions) -> Result<(), String> {
  start(&app, options)
}

#[tauri::command]
pub fn pause_recording(app: AppHandle) -> Result<(), String> {
  pause(&app)
}

#[tauri::command]
pub fn resume_recording(app: AppHandle) -> Result<(), String> {
  resume(&app)
}

#[tauri::command]
pub fn stop_recording(app: AppHandle) -> Result<(), String> {
  stop(&app)
}

#[tauri::command]
pub fn cancel_recording(app: AppHandle) -> Result<(), String> {
  cancel(&app)
}

#[cfg(test)]
mod tests {
  use super::*;

  const START: u64 = 1_000_000;

  fn starting() -> RecordingSnapshot {
    let mut snapshot = RecordingSnapshot::default();
    apply_transition(
      &mut snapshot,
      RecordingStatus::Starting,
      Some(RecordingMode::Screen),
      START,
    )
    .unwrap();
    snapshot
  }

  fn recording() -> RecordingSnapshot {
    let mut snapshot = starting();
    apply_transition(&mut snapshot, RecordingStatus::Recording, None, START).unwrap();
    snapshot
  }

  #[test]
  fn accepts_every_legal_transition() {
    let legal = [
      (RecordingStatus::Idle, RecordingStatus::Starting),
      (RecordingStatus::Starting, RecordingStatus::Recording),
      (RecordingStatus::Starting, RecordingStatus::Idle),
      (RecordingStatus::Recording, RecordingStatus::Paused),
      (RecordingStatus::Paused, RecordingStatus::Recording),
      (RecordingStatus::Recording, RecordingStatus::Stopping),
      (RecordingStatus::Paused, RecordingStatus::Stopping),
      (RecordingStatus::Stopping, RecordingStatus::Idle),
    ];

    for (from, to) in legal {
      let mut snapshot = RecordingSnapshot {
        status: from,
        ..RecordingSnapshot::default()
      };
      assert!(
        apply_transition(&mut snapshot, to, None, START).is_ok(),
        "{} to {} should be legal",
        from.label(),
        to.label()
      );
      assert_eq!(snapshot.status, to);
    }
  }

  #[test]
  fn rejects_every_illegal_transition() {
    let all = [
      RecordingStatus::Idle,
      RecordingStatus::Starting,
      RecordingStatus::Recording,
      RecordingStatus::Paused,
      RecordingStatus::Stopping,
    ];
    let legal = [
      (RecordingStatus::Idle, RecordingStatus::Starting),
      (RecordingStatus::Starting, RecordingStatus::Recording),
      (RecordingStatus::Starting, RecordingStatus::Idle),
      (RecordingStatus::Recording, RecordingStatus::Paused),
      (RecordingStatus::Paused, RecordingStatus::Recording),
      (RecordingStatus::Recording, RecordingStatus::Stopping),
      (RecordingStatus::Paused, RecordingStatus::Stopping),
      (RecordingStatus::Stopping, RecordingStatus::Idle),
    ];

    for from in all {
      for to in all {
        if legal.contains(&(from, to)) {
          continue;
        }

        let mut snapshot = RecordingSnapshot {
          status: from,
          ..RecordingSnapshot::default()
        };
        let error = apply_transition(&mut snapshot, to, None, START).unwrap_err();
        assert!(error.contains(from.label()) && error.contains(to.label()));
        assert_eq!(
          snapshot.status, from,
          "a rejected transition must not mutate the snapshot"
        );
      }
    }
  }

  #[test]
  fn rejects_a_second_start_while_starting() {
    let mut snapshot = starting();
    assert!(apply_transition(
      &mut snapshot,
      RecordingStatus::Starting,
      Some(RecordingMode::Camera),
      START
    )
    .is_err());
    assert_eq!(snapshot.mode, Some(RecordingMode::Screen));
  }

  #[test]
  fn starts_the_clock_when_recording_begins() {
    let snapshot = recording();
    assert_eq!(snapshot.started_at_ms, Some(START));
    assert_eq!(snapshot.accumulated_ms, 0);
    assert_eq!(snapshot.paused_at_ms, None);
  }

  #[test]
  fn folds_the_open_span_into_accumulated_time_on_pause() {
    let mut snapshot = recording();
    apply_transition(&mut snapshot, RecordingStatus::Paused, None, START + 5_000).unwrap();

    assert_eq!(snapshot.accumulated_ms, 5_000);
    assert_eq!(snapshot.paused_at_ms, Some(START + 5_000));
    assert_eq!(snapshot.started_at_ms, None);
  }

  #[test]
  fn resuming_restarts_the_span_without_counting_the_pause() {
    let mut snapshot = recording();
    apply_transition(&mut snapshot, RecordingStatus::Paused, None, START + 5_000).unwrap();
    apply_transition(
      &mut snapshot,
      RecordingStatus::Recording,
      None,
      START + 25_000,
    )
    .unwrap();

    assert_eq!(snapshot.accumulated_ms, 5_000);
    assert_eq!(snapshot.started_at_ms, Some(START + 25_000));
    assert_eq!(snapshot.paused_at_ms, None);

    apply_transition(
      &mut snapshot,
      RecordingStatus::Stopping,
      None,
      START + 28_000,
    )
    .unwrap();
    assert_eq!(snapshot.accumulated_ms, 8_000);
  }

  #[test]
  fn stopping_from_paused_keeps_the_frozen_elapsed_time() {
    let mut snapshot = recording();
    apply_transition(&mut snapshot, RecordingStatus::Paused, None, START + 3_000).unwrap();
    apply_transition(
      &mut snapshot,
      RecordingStatus::Stopping,
      None,
      START + 90_000,
    )
    .unwrap();

    assert_eq!(snapshot.accumulated_ms, 3_000);
  }

  #[test]
  fn returning_to_idle_clears_the_snapshot() {
    let mut snapshot = recording();
    apply_transition(
      &mut snapshot,
      RecordingStatus::Stopping,
      None,
      START + 1_000,
    )
    .unwrap();
    apply_transition(&mut snapshot, RecordingStatus::Idle, None, START + 1_250).unwrap();

    assert_eq!(snapshot, RecordingSnapshot::default());
  }

  #[test]
  fn excludes_the_pill_and_the_region_overlay_from_capture() {
    assert!(windows::CAPTURE_EXCLUDED_WINDOW_LABELS.contains(&"recording-dock"));
    assert!(windows::CAPTURE_EXCLUDED_WINDOW_LABELS.contains(&"region-selector"));
  }
}
