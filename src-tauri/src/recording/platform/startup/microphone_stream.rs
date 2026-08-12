// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::super::*;
use crate::recording::monitor::RecordingMonitor;

pub(super) fn start(
  source: Option<MicrophoneSource>,
  commands: &SyncSender<Command>,
  monitor: &Arc<RecordingMonitor>,
  stats: &Arc<CaptureStats>,
) -> Result<Option<Stream>, String> {
  let Some(source) = source else {
    return Ok(None);
  };
  let sample_commands = commands.clone();
  let sample_monitor = Arc::clone(monitor);
  let sample_stats = Arc::clone(stats);
  let on_buffer = Arc::new(move |buffer: MicrophoneBuffer| {
    sample_monitor.send_microphone(&buffer.samples);
    if let Err(TrySendError::Full(_)) = sample_commands.try_send(Command::Microphone(buffer)) {
      sample_stats
        .microphone_dropped
        .fetch_add(1, Ordering::Relaxed);
    }
  });
  let error_commands = commands.clone();
  let on_error = Arc::new(move |error| {
    let _ = error_commands.send(Command::MicrophoneFailed(error));
  });
  source.start(on_buffer, on_error).map(Some)
}
