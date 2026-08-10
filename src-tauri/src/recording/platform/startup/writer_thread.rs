// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::super::*;

/// The writer thread, once it has confirmed it can write.
pub(super) struct WriterThread {
  pub(super) commands: SyncSender<Command>,
  pub(super) first_frame: Receiver<Result<(), String>>,
  pub(super) worker: JoinHandle<()>,
}

pub(super) fn spawn_writer(config: WriterConfig, name: &str) -> Result<WriterThread, String> {
  let (commands, inbox) = mpsc::sync_channel(FRAME_QUEUE_DEPTH);
  let (ready, readied) = mpsc::channel();
  let (first_frame, first_framed) = mpsc::channel();
  let worker = std::thread::Builder::new()
    .name(name.to_owned())
    .spawn(move || match Writer::new(config) {
      Ok(writer) => {
        let _ = ready.send(Ok(()));
        writer.run(&inbox, &first_frame);
      }
      Err(error) => {
        let _ = ready.send(Err(error));
      }
    })
    .map_err(|error| error.to_string())?;

  match readied.recv() {
    Ok(Ok(())) => Ok(WriterThread {
      commands,
      first_frame: first_framed,
      worker,
    }),
    Ok(Err(error)) => {
      let _ = worker.join();
      Err(error)
    }
    Err(_) => {
      let _ = worker.join();
      Err("The recording's encoder could not be started".to_owned())
    }
  }
}

pub(super) fn both_first_frames(
  primary: Receiver<Result<(), String>>,
  camera: Receiver<Result<(), String>>,
) -> Receiver<Result<(), String>> {
  let (ready, first_frames) = mpsc::channel();
  std::thread::spawn(move || {
    let result = primary
      .recv()
      .unwrap_or_else(|_| Err("The primary video produced no frames".to_owned()))
      .and_then(|()| {
        camera
          .recv()
          .unwrap_or_else(|_| Err("The camera produced no frames".to_owned()))
      });
    let _ = ready.send(result);
  });
  first_frames
}
