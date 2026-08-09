// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use cidre::{av, cm};

/// What shape of file the writer produces, and how often - if ever - it
/// flushes a movie fragment to disk.
#[derive(Clone, Copy)]
pub(in crate::recording::platform) struct Container {
  pub(in crate::recording::platform) format: ContainerFormat,
  /// `None` leaves the index to be written in one go at `finish_writing`, so
  /// the file is worthless until then. `Some` asks for a self-contained
  /// fragment this often, which is what would make a half-written recording
  /// playable - at the cost documented in `Writer::new`.
  pub(in crate::recording::platform) fragment_interval: Option<cm::Time>,
}

/// Named rather than held as an `&'static av::FileType` because the config
/// travels to the writer thread, and Objective-C strings are not `Send`. The
/// real file type is fetched once the config has arrived.
#[derive(Clone, Copy)]
pub(in crate::recording::platform) enum ContainerFormat {
  /// Reachable only from the encoder tests now. It is what recordings used to
  /// be written as, and the tests keep it around as the control the fragmented
  /// QuickTime container is measured against.
  #[cfg(test)]
  Mp4,
  /// What recordings are written as. Movie fragments are a QuickTime feature
  /// that .mp4 merely borrows, and only QuickTime survives being fragmented -
  /// see [`Container::quicktime_fragmented`].
  QuickTime,
}

impl ContainerFormat {
  pub(in crate::recording::platform) fn file_type(self) -> &'static av::FileType {
    match self {
      #[cfg(test)]
      Self::Mp4 => av::FileType::mp4(),
      Self::QuickTime => av::FileType::qt(),
    }
  }

  /// AVFoundation infers nothing from the URL, but everything that opens the
  /// file afterwards reads the name, so the two have to agree. Production
  /// spells the working file's name out in `encoding::temp_file_name` instead,
  /// because that name is built on every platform and this type is macOS's;
  /// `names_the_working_file_after_the_container_it_is` holds the two together.
  #[cfg(test)]
  pub(in crate::recording::platform) fn extension(self) -> &'static str {
    match self {
      #[cfg(test)]
      Self::Mp4 => "mp4",
      Self::QuickTime => "mov",
    }
  }
}

/// How often a fragment is flushed. Two seconds is the most a crash can cost,
/// and short enough that the overhead of a fragment header never shows up
/// against a screen recording's bitrate. The timescale is the 600 QuickTime
/// has always used for durations.
const FRAGMENT_INTERVAL_SECONDS: f64 = 2.0;
const FRAGMENT_TIMESCALE: i32 = 600;

impl Container {
  /// What recordings are written as: a QuickTime movie that stamps a
  /// self-contained fragment every two seconds.
  ///
  /// The point is that a recording is worth something before it is finished.
  /// An unfragmented file has its index written in one go at
  /// `finish_writing`, so a crash - or a kill, or a panic - leaves a corpse
  /// with no `moov` atom at all, which no player and no repair tool can make
  /// anything of. Fragmented, the same interruption leaves a movie that probes
  /// and decodes cleanly up to the last flushed fragment.
  ///
  /// The container has to be QuickTime. Fragmenting an .mp4 through this
  /// pipeline puts the writer into a failed state - sometimes at the first
  /// fragment boundary after a resume, so every later frame is refused, and
  /// sometimes only at `finish_writing`, which loses the whole recording. The
  /// ignored encoder tests at the foot of this file hold both halves of that
  /// evidence: the fragmented QuickTime corpse of an aborted ten-second
  /// recording probes at 8.02s and decodes 243 frames without an error, while
  /// its .mp4 twin is `moov atom not found`.
  ///
  /// One caveat travels with fragments: `nb_frames` in the finished header
  /// counts only the frames of the last fragment - 61 against the 243 that
  /// actually decode - so nothing may read a frame count out of the container.
  /// Durations stay exact, which is what everything here uses anyway.
  ///
  /// The saved file is still an .mp4: see `exports::save_recording`, which
  /// stream-copies the working movie into one rather than renaming it.
  pub(in crate::recording::platform) fn quicktime_fragmented() -> Self {
    Self {
      format: ContainerFormat::QuickTime,
      fragment_interval: Some(cm::Time::with_secs(
        FRAGMENT_INTERVAL_SECONDS,
        FRAGMENT_TIMESCALE,
      )),
    }
  }

  /// The container recordings used to be written as, kept for the encoder
  /// tests that measure the fragmented one against it.
  #[cfg(test)]
  pub(in crate::recording::platform) const fn mp4() -> Self {
    Self {
      format: ContainerFormat::Mp4,
      fragment_interval: None,
    }
  }
}
