// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::atomic::AtomicU32;
use std::time::Duration;

use super::*;

/// An NV12 frame of the right shape, standing in for a captured one.
///
/// The picture is drawn rather than left blank on purpose. A fresh
/// `cv::PixelBuf` is whatever the allocator last had - sometimes zeroes,
/// sometimes rubbish - and neither tells you anything afterwards: zeroed YUV
/// decodes to exactly the flat green that a *corrupt* frame region also
/// decodes to, so a blank source could never distinguish the two. Instead
/// luma is a top-to-bottom ramp, which makes each band of the image
/// identifiable by its brightness alone, with a little per-column, per-tick
/// dither so the encoder has real work to do and a frozen frame shows up.
/// Chroma is left neutral, so the movie decodes grey and any green is damage.
fn synthetic_frame(width: u32, height: u32, source_ns: i64, wall: Instant) -> Frame {
  let mut buf = cv::PixelBuf::new(
    width as usize,
    height as usize,
    cv::PixelFormat::_420V,
    None,
  )
  .expect("a pixel buffer");

  let tick = (source_ns / MS) as usize;
  // Locked by hand rather than with `base_address_lock`, whose guard borrows
  // the buffer mutably for as long as it lives and so leaves no way to ask
  // the buffer about its own planes while it is held.
  // SAFETY: the matching unlock is at the end of this block, and nothing in
  // between can return early or panic past it - `plane_*` are pure reads and
  // the writes are bounded by the sizes they report.
  unsafe { buf.lock_base_addr(cv::pixel_buffer::LockFlags::DEFAULT) }
    .result()
    .expect("the pixel buffer locks");
  {
    let (rows, columns) = (buf.plane_height(0), buf.plane_width(0));
    let stride = buf.plane_bytes_per_row(0);
    let luma = buf.plane_base_address(0).cast_mut();
    // A bar that slides down the ramp, so the picture moves without the
    // ramp itself moving - the bands have to keep their brightness for the
    // bottom-of-frame check below to mean anything. Whole rows are filled at
    // a time because a per-pixel loop here is fast enough in a release build
    // and far too slow in the debug build the tests actually run in: it
    // starves the producer thread and shortens the recording.
    let bar = (tick * 6) % rows.max(1);
    for row in 0..rows {
      // SAFETY: the buffer is locked, and the row is within the plane's own
      // height and stride as CoreVideo reports them.
      let line = unsafe { std::slice::from_raw_parts_mut(luma.add(row * stride), columns) };
      line.fill(if row.abs_diff(bar) < 16 {
        235
      } else {
        16 + (200 * row / rows.max(1)) as u8
      });
    }

    let (rows, columns) = (buf.plane_height(1), buf.plane_width(1));
    let stride = buf.plane_bytes_per_row(1);
    let chroma = buf.plane_base_address(1).cast_mut();
    for row in 0..rows {
      // SAFETY: as above; the interleaved plane is two bytes per pixel.
      let line = unsafe { std::slice::from_raw_parts_mut(chroma.add(row * stride), columns * 2) };
      line.fill(128);
    }
  }
  // SAFETY: pairs with the lock above.
  unsafe { buf.unlock_lock_base_addr(cv::pixel_buffer::LockFlags::DEFAULT) }
    .result()
    .expect("the pixel buffer unlocks");

  Frame {
    buf,
    source_ns,
    wall,
  }
}

const SOURCE_EPOCH: i64 = 1_000_000_000_000;
const MS: i64 = 1_000_000;

const FRAME_MS: u64 = 33;
static WIDTH: AtomicU32 = AtomicU32::new(640);
static HEIGHT: AtomicU32 = AtomicU32::new(480);

#[test]
fn trims_microphone_preroll_at_whole_pcm_frames() {
  let captured_at = Instant::now();
  let microphone = MicrophoneBuffer {
    captured_at,
    samples: (0..40).map(|sample| sample as f32).collect(),
  };
  let trimmed = microphone_buffer_from_origin(
    microphone,
    captured_at + Duration::from_millis(5),
    MicrophoneFormat {
      channels: 2,
      sample_rate: 1_000,
    },
  )
  .expect("the buffer overlaps time zero");

  assert_eq!(trimmed.captured_at, captured_at + Duration::from_millis(5));
  assert_eq!(
    trimmed.samples,
    (10..40).map(|sample| sample as f32).collect::<Vec<_>>()
  );
}

#[test]
fn drops_a_microphone_buffer_wholly_before_video() {
  let captured_at = Instant::now();
  let microphone = MicrophoneBuffer {
    captured_at,
    samples: vec![0.0; 40],
  };

  assert!(microphone_buffer_from_origin(
    microphone,
    captured_at + Duration::from_millis(30),
    MicrophoneFormat {
      channels: 2,
      sample_rate: 1_000,
    },
  )
  .is_none());
}

#[test]
fn records_into_a_quicktime_movie_that_flushes_a_fragment_every_two_seconds() {
  let container = Container::quicktime_fragmented();
  assert_eq!(container.format.extension(), "mov");
  let interval = container.fragment_interval.expect("a fragment interval");
  assert!((interval.as_secs() - 2.0).abs() < f64::EPSILON);
}

/// The container names the file, and the name is what every reader afterwards
/// believes. These two are written in different modules - one macOS-only, one
/// not - so nothing but this holds them together.
#[test]
fn names_the_working_file_after_the_container_it_is() {
  let name = crate::recording::encoding::temp_file_name(
    chrono::NaiveDate::from_ymd_opt(2026, 8, 8)
      .unwrap()
      .and_hms_milli_opt(14, 32, 5, 250)
      .unwrap(),
  );
  assert!(name.ends_with(&format!(
    ".{}",
    Container::quicktime_fragmented().format.extension()
  )));
}

/// What the app records to. Named for what these tests are asking about it,
/// and taken from production rather than rebuilt, so the evidence below is
/// always evidence about the shipping container.
fn fragmented_qt() -> Container {
  Container::quicktime_fragmented()
}

/// The default for every test that is asking about timing rather than about
/// containers: whatever the app itself records to.
fn record(name: &str, pause: Option<(u64, u64)>, end_at_ms: u64) -> Outcome {
  record_at(
    name,
    pause,
    end_at_ms,
    640,
    480,
    Container::quicktime_fragmented(),
  )
}

/// Plays a recording out at real speed on another thread, because the
/// writer's real-time input paces itself against the wall clock: fed any
/// faster it reports itself busy and nothing is ever appended.
fn play(
  commands: mpsc::Sender<Command>,
  base: Instant,
  pause: Option<(u64, u64)>,
  end_at_ms: u64,
) -> std::thread::JoinHandle<()> {
  std::thread::spawn(move || {
    let mut at = 0;
    // Which side of the pause the producer is on. A latch rather than a
    // boolean, because a boolean that goes back to "running" at the resume
    // reads as "has not paused yet" on the very next frame and pauses all
    // over again - and a timeline told to resume repeatedly adds the same
    // span to its paused total every time, which drags every later frame
    // back onto the pause instant.
    let (mut sent_pause, mut sent_resume) = (false, false);
    while at < end_at_ms {
      if let Some((pause_at, resume_at)) = pause {
        // Tested by crossing rather than equality: the catch-up below can
        // step over any particular millisecond.
        if !sent_pause && at >= pause_at {
          sent_pause = true;
          let _ = commands.send(Command::Pause {
            at: base + Duration::from_millis(pause_at),
          });
        }
        if sent_pause && !sent_resume && at >= resume_at {
          sent_resume = true;
          let _ = commands.send(Command::Resume {
            at: base + Duration::from_millis(resume_at),
          });
        }
      }
      if commands
        .send(Command::Frame(synthetic_frame(
          WIDTH.load(Ordering::Relaxed),
          HEIGHT.load(Ordering::Relaxed),
          SOURCE_EPOCH + at as i64 * MS,
          Instant::now(),
        )))
        .is_err()
      {
        return;
      }

      // Paced against `base` rather than by sleeping a fixed step, so that
      // however long building a frame takes, this timeline keeps step with
      // the wall clock the writer and the stop signal are both on. A fixed
      // step lets a slow producer fall behind unboundedly and end the
      // recording early, which reads afterwards exactly like the encoder
      // having dropped everything.
      at += FRAME_MS;
      let target = base + Duration::from_millis(at);
      match target.checked_duration_since(Instant::now()) {
        Some(remaining) => std::thread::sleep(remaining),
        // Behind: give up the frames that should already have gone out.
        None => at = at.max(base.elapsed().as_millis() as u64 / FRAME_MS * FRAME_MS),
      }
    }
  })
}

struct Outcome {
  appended: u64,
  duration_ms: u64,
  /// Reported because it is the counter that catches the interesting
  /// failure. A wedged encoder never refuses a frame - it is simply never
  /// ready for another one, for the whole rest of the recording - so the
  /// movie comes out truncated while `rejected` stays at zero.
  not_ready: u64,
  rejected: u64,
  result: Result<FinalizeInfo, String>,
}

impl Outcome {
  fn report(&self, label: &str) {
    println!(
      "{label}: appended={} not_ready={} rejected={} duration={}ms result={:?}",
      self.appended,
      self.not_ready,
      self.rejected,
      self.duration_ms,
      self.result.as_ref().err()
    );
  }

  /// Asserts the movie is as long as the material fed to it, give or take
  /// the slack of a real-time run. This is what a silent truncation trips.
  fn assert_lasts(&self, expected_ms: u64) {
    assert!(self.result.is_ok(), "{:?}", self.result.as_ref().err());
    assert!(
      self.duration_ms.abs_diff(expected_ms) < 500,
      "the movie is {}ms long, but {expected_ms}ms of frames were fed to it",
      self.duration_ms
    );
  }
}

/// Where the encoder tests leave their movies, so `ffprobe` can be pointed
/// at them afterwards.
fn test_movie(name: &str, container: Container) -> PathBuf {
  let directory = std::env::temp_dir().join("orbit-capture-tests");
  std::fs::create_dir_all(&directory).unwrap();
  directory.join(format!("{name}.{}", container.format.extension()))
}

fn record_at(
  name: &str,
  pause: Option<(u64, u64)>,
  end_at_ms: u64,
  width: u32,
  height: u32,
  container: Container,
) -> Outcome {
  let path = test_movie(name, container);
  let _ = std::fs::remove_file(&path);

  WIDTH.store(width, Ordering::Relaxed);
  HEIGHT.store(height, Ordering::Relaxed);
  let stats = Arc::new(CaptureStats::default());
  let writer = Writer::new(WriterConfig {
    path,
    width,
    height,
    fps: 30,
    system_audio: false,
    microphone_format: None,
    stats: Arc::clone(&stats),
    on_failure: Box::new(|reason| println!("failure reported: {reason}")),
    container,
  })
  .expect("a writer");
  let base = writer.base;
  let (commands, inbox) = mpsc::channel();
  let (first_frame, _first_framed) = mpsc::channel();
  let (reply, replies) = mpsc::channel();

  let producer = play(commands.clone(), base, pause, end_at_ms);
  std::thread::spawn(move || {
    std::thread::sleep(Duration::from_millis(end_at_ms + 200));
    let _ = commands.send(Command::Stop {
      at: Instant::now(),
      reply,
    });
  });

  writer.run(&inbox, &first_frame);
  let result = replies.recv().expect("a reply");
  producer.join().unwrap();

  Outcome {
    appended: stats.appended.load(Ordering::Relaxed),
    duration_ms: result.as_ref().map_or(0, |info| info.duration_ms),
    not_ready: stats.not_ready.load(Ordering::Relaxed),
    rejected: stats.rejected.load(Ordering::Relaxed),
    result,
  }
}

#[test]
#[ignore = "drives a real encoder at real speed; run with --ignored"]
fn writes_a_movie_without_a_pause() {
  let outcome = record("plain", None, 3_000);
  outcome.report("plain");
  outcome.assert_lasts(3_000);
  assert_eq!(outcome.rejected, 0);
}

#[test]
#[ignore = "drives a real encoder at real speed; run with --ignored"]
fn writes_a_movie_across_a_pause() {
  let outcome = record("paused", Some((990, 3_960)), 6_000);
  outcome.report("paused");
  outcome.assert_lasts(3_030);
  assert_eq!(outcome.rejected, 0, "frames were rejected after the resume");
}

/// The real case: a large frame, so the encoder is actually working, and a
/// long pause, so the movie's clock falls far behind the wall clock.
#[test]
#[ignore = "drives a real encoder at real speed; run with --ignored"]
fn writes_a_large_movie_across_a_long_pause() {
  let outcome = record_at(
    "paused-hd",
    Some((990, 5_940)),
    8_000,
    2560,
    1440,
    Container::mp4(),
  );
  outcome.report("paused-hd");
  outcome.assert_lasts(3_050);
  assert_eq!(outcome.rejected, 0, "frames were rejected after the resume");
}

/// A capture the size of a real display, whose height is not a multiple of
/// the 16-pixel macroblock the encoder works in. An encoder that mishandles
/// the ragged last row leaves the bottom of the picture as untouched YUV,
/// which decodes flat green; the assertion below is that it does not.
#[test]
#[ignore = "drives a real encoder at real speed; run with --ignored"]
fn writes_a_movie_at_an_unaligned_display_size() {
  let outcome = record_at(
    "hidpi",
    None,
    3_000,
    3600,
    2338,
    Container::quicktime_fragmented(),
  );
  outcome.report("hidpi");
  outcome.assert_lasts(3_000);
  assert_eq!(outcome.rejected, 0);

  // The average colour of the bottom quarter of every frame. The source ramp
  // makes that band bright grey; the failure being looked for is green,
  // which is what a zeroed - i.e. never written - YUV region decodes to.
  let path = test_movie("hidpi", Container::quicktime_fragmented());
  let decoded = std::process::Command::new("ffmpeg")
    .args(["-v", "error", "-i"])
    .arg(&path)
    .args([
      "-vf",
      "crop=3600:584:0:1754,scale=1:1,format=rgb24",
      "-f",
      "rawvideo",
      "-",
    ])
    .output()
    .expect("ffmpeg is installed");
  assert!(
    decoded.status.success(),
    "the movie did not decode: {}",
    String::from_utf8_lossy(&decoded.stderr)
  );
  assert!(
    decoded.stderr.is_empty(),
    "the decoder complained: {}",
    String::from_utf8_lossy(&decoded.stderr)
  );

  let pixels: Vec<_> = decoded.stdout.chunks_exact(3).collect();
  assert!(!pixels.is_empty(), "no frames came back out");
  println!(
    "hidpi: {} frames, bottom-quarter average rgb {:?}",
    pixels.len(),
    &pixels[..pixels.len().min(6)]
  );
  for (index, pixel) in pixels.iter().enumerate() {
    let (red, green, blue) = (pixel[0], pixel[1], pixel[2]);
    assert!(
      red > 80 && blue > 80,
      "frame {index} has a green bottom quarter: rgb({red}, {green}, {blue})"
    );
  }
}

#[test]
#[ignore = "drives a real encoder at real speed; run with --ignored"]
fn writes_a_fragmented_movie_without_a_pause() {
  let outcome = record_at("plain-frag", None, 3_000, 640, 480, fragmented_qt());
  outcome.report("plain-frag");
  outcome.assert_lasts(3_000);
  assert_eq!(outcome.rejected, 0);
}

#[test]
#[ignore = "drives a real encoder at real speed; run with --ignored"]
fn writes_a_fragmented_movie_across_a_pause() {
  let outcome = record_at(
    "paused-frag",
    Some((990, 3_960)),
    6_000,
    640,
    480,
    fragmented_qt(),
  );
  outcome.report("paused-frag");
  outcome.assert_lasts(3_030);
  assert_eq!(outcome.rejected, 0, "frames were rejected after the resume");
}

/// The case the unfragmented writer was reported to fail: a large frame and
/// a pause long enough that several fragment boundaries pass while the
/// movie's clock stands still.
#[test]
#[ignore = "drives a real encoder at real speed; run with --ignored"]
fn writes_a_large_fragmented_movie_across_a_long_pause() {
  let outcome = record_at(
    "paused-hd-frag",
    Some((990, 5_940)),
    8_000,
    2560,
    1440,
    fragmented_qt(),
  );
  outcome.report("paused-hd-frag");
  outcome.assert_lasts(3_050);
  assert_eq!(outcome.rejected, 0, "frames were rejected after the resume");
}

/// The environment variable the crash tests below use to tell a child
/// process that it is the one meant to die.
const ABANDON: &str = "ORBIT_ABANDON_RECORDING";

/// Writes for a while and then kills this process outright, leaving the file
/// exactly as a crash would. It has to be `abort` rather than an early
/// return: a test that merely stops calling `finish_writing` still unwinds,
/// and AVFoundation's own teardown would tidy up the very thing we want to
/// catch mid-flight.
fn abandon_a_recording(name: &str, container: Container, write_for_ms: u64) {
  if std::env::var_os(ABANDON).is_none() {
    // Being run as part of the ordinary ignored suite rather than by the
    // crash test. Taking the process down here would end the whole run.
    return;
  }
  let path = test_movie(name, container);
  let _ = std::fs::remove_file(&path);

  WIDTH.store(640, Ordering::Relaxed);
  HEIGHT.store(480, Ordering::Relaxed);
  let stats = Arc::new(CaptureStats::default());
  let writer = Writer::new(WriterConfig {
    path: path.clone(),
    width: 640,
    height: 480,
    fps: 30,
    system_audio: false,
    microphone_format: None,
    stats: Arc::clone(&stats),
    on_failure: Box::new(|reason| println!("failure reported: {reason}")),
    container,
  })
  .expect("a writer");
  let base = writer.base;
  let (commands, inbox) = mpsc::channel();
  let (first_frame, _first_framed) = mpsc::channel();

  // Kept feeding past the kill point so the writer is genuinely mid-recording
  // when the process dies, not idling at the end of its material.
  let _producer = play(commands, base, None, write_for_ms * 2);
  std::thread::spawn(move || writer.run(&inbox, &first_frame));

  std::thread::sleep(Duration::from_millis(write_for_ms));
  println!(
    "abandoning after {write_for_ms}ms with {} appended: {}",
    stats.appended.load(Ordering::Relaxed),
    path.display()
  );
  std::io::Write::flush(&mut std::io::stdout()).ok();
  std::process::abort();
}

/// Re-runs this very test binary for one of the abandon helpers above, so
/// the abort lands in a process of its own.
fn crash(test: &str) -> std::process::Output {
  std::process::Command::new(std::env::current_exe().expect("this test binary"))
    .args([
      test,
      "--ignored",
      "--exact",
      "--nocapture",
      "--test-threads=1",
    ])
    .env(ABANDON, "1")
    .output()
    .expect("the child test binary ran")
}

fn ffprobe(path: &std::path::Path) -> std::process::Output {
  std::process::Command::new("ffprobe")
    .args([
      "-hide_banner",
      "-show_format",
      "-show_streams",
      "-of",
      "flat",
    ])
    .arg(path)
    .output()
    .expect("ffprobe is installed")
}

fn report_abandoned(label: &str, path: &std::path::Path) {
  let size = std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
  let probe = ffprobe(path);
  println!("--- {label}: {} bytes at {}", size, path.display());
  println!("{label}: ffprobe status={}", probe.status);
  println!("{}", String::from_utf8_lossy(&probe.stdout));
  println!("{}", String::from_utf8_lossy(&probe.stderr));
}

#[test]
#[ignore = "kills a child process on purpose; run with --ignored"]
fn writes_a_fragmented_movie_then_dies() {
  abandon_a_recording("abandoned-frag", fragmented_qt(), 10_000);
}

#[test]
#[ignore = "kills a child process on purpose; run with --ignored"]
fn writes_a_plain_movie_then_dies() {
  abandon_a_recording("abandoned-plain", Container::mp4(), 10_000);
}

/// What the whole fragment question is actually about: after an unclean
/// death, is there a recording left or not?
#[test]
#[ignore = "spawns and kills child processes; run with --ignored"]
fn a_fragmented_movie_survives_a_crash_where_a_plain_one_does_not() {
  let fragmented = crash("recording::platform::tests::writes_a_fragmented_movie_then_dies");
  println!("{}", String::from_utf8_lossy(&fragmented.stdout));
  let plain = crash("recording::platform::tests::writes_a_plain_movie_then_dies");
  println!("{}", String::from_utf8_lossy(&plain.stdout));

  let frag_path = test_movie("abandoned-frag", fragmented_qt());
  let plain_path = test_movie("abandoned-plain", Container::mp4());
  report_abandoned("fragmented .mov", &frag_path);
  report_abandoned("plain .mp4", &plain_path);

  assert!(
    ffprobe(&frag_path).status.success(),
    "the abandoned fragmented movie should still be readable"
  );
}
