// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// The ScreenCaptureKit objects a running session keeps alive.
pub(super) struct StreamObjects {
  pub(super) _output: arc::R<ScreenOutput>,
  pub(super) queue: arc::R<dispatch::Queue>,
  pub(super) streams: Vec<arc::R<sc::Stream>>,
}

// SAFETY: `sc::Stream` already declares itself thread-safe. The queue is a
// dispatch object, which is thread-safe by construction. The output delegate's
// own state is only ever touched from the one serial queue it was registered
// with; every other thread does nothing to it but retain and release, which
// Objective-C makes atomic.
unsafe impl Send for StreamObjects {}

/// A running recording, as seen by the state machine.
pub struct CaptureSession {
  pub(super) commands: SyncSender<Command>,
  pub(super) microphone: Option<Stream>,
  pub(super) objects: StreamObjects,
  pub(super) worker: Option<JoinHandle<()>>,
}

impl CaptureSession {
  pub fn pause(&self) {
    let _ = self.commands.send(Command::Pause { at: Instant::now() });
  }

  pub fn resume(&self) -> Result<(), String> {
    self
      .commands
      .send(Command::Resume { at: Instant::now() })
      .map_err(|_| "The recording is no longer running".to_owned())
  }

  /// Finishes the movie and hands back what was written.
  ///
  /// The stop instant is taken before asking ScreenCaptureKit to stop, so the
  /// asynchronous shutdown time never lengthens the movie. Its completion is
  /// followed by a barrier on the serial output queue; only then is the writer
  /// finalized. That ordering guarantees the final audio buffers are written
  /// instead of being stranded behind `Stop`.
  pub fn stop(mut self) -> Result<FinalizeInfo, String> {
    let at = Instant::now();
    self.microphone.take();
    let (stopped, did_stop) = mpsc::channel();
    for stream in &self.objects.streams {
      let stopped = stopped.clone();
      stream.stop_with_ch(move |error| {
        let result = error.map_or_else(|| Ok(()), |error| Err(error.to_string()));
        let _ = stopped.send(result);
      });
    }
    drop(stopped);
    for _ in 0..self.objects.streams.len() {
      match did_stop.recv_timeout(FINALIZE_TIMEOUT) {
        Ok(Ok(())) => {}
        Ok(Err(error)) => eprintln!("ScreenCaptureKit reported an error while stopping: {error}"),
        Err(_) => {
          eprintln!("ScreenCaptureKit did not confirm shutdown before finalization");
          break;
        }
      }
    }
    self.objects.queue.sync_once(|| {});

    let (reply, replies) = mpsc::channel();
    self
      .commands
      .send(Command::Stop { at, reply })
      .map_err(|_| "The recording is no longer running".to_owned())?;
    let finalized = replies
      .recv_timeout(FINALIZE_TIMEOUT)
      .map_err(|_| "The recording did not finish in time".to_owned())?;
    self.join_writer();

    finalized
  }

  /// Throws the recording away. The file itself is deleted by the caller,
  /// which is the only place that knows whether it was ever wanted.
  pub fn cancel(mut self) {
    self.shutdown();
  }

  /// Stops the stream and puts the writer thread to rest. Idempotent, because
  /// `Drop` runs it again behind every other path.
  ///
  /// The cancel goes out unconditionally: after a `Stop` the writer has
  /// already returned and the send simply fails, but on every other path it is
  /// what wakes the thread up. Joining without it would wait forever on a
  /// thread blocked reading a channel this very handle still holds open.
  fn shutdown(&mut self) {
    if self.worker.is_none() {
      return;
    }

    for stream in &self.objects.streams {
      stream.stop_with_ch(|_| {});
    }
    self.microphone.take();
    let _ = self.commands.send(Command::Cancel);
    self.join_writer();
  }

  fn join_writer(&mut self) {
    if let Some(worker) = self.worker.take() {
      let _ = worker.join();
    }
  }
}

impl Drop for CaptureSession {
  fn drop(&mut self) {
    // Already done when `stop` or `cancel` ran; this is for the paths that
    // drop the handle outright, such as a start that was cancelled mid-flight.
    self.shutdown();
  }
}
