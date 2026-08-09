// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

pub(super) fn waveform(
  source: &Path,
  track: &RecordingAudioTrack,
  duration_ms: u64,
) -> Result<Vec<f32>, String> {
  let mut child = Command::new(ffmpeg_path())
    .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-i"])
    .arg(source)
    .args([
      "-map",
      &format!("0:a:{}", track.stream_index),
      "-vn",
      "-ac",
      "1",
      "-ar",
      &WAVEFORM_SAMPLE_RATE.to_string(),
      "-f",
      "f32le",
      "pipe:1",
    ])
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .map_err(|error| format!("FFmpeg could not be started: {error}"))?;

  let stdout = child
    .stdout
    .take()
    .ok_or_else(|| "FFmpeg did not expose its waveform output".to_owned())?;
  let expected_samples = duration_ms
    .saturating_mul(WAVEFORM_SAMPLE_RATE)
    .div_ceil(1_000)
    .max(1);
  let mut peaks = vec![0.0_f32; WAVEFORM_POINTS];
  let mut reader = BufReader::new(stdout);
  let mut bytes = [0_u8; 16 * 1024];
  let mut remainder = Vec::with_capacity(3);
  let mut sample_index = 0_u64;

  loop {
    let read = reader.read(&mut bytes).map_err(|error| error.to_string())?;
    if read == 0 {
      break;
    }
    remainder.extend_from_slice(&bytes[..read]);
    let complete = remainder.len() / 4 * 4;
    for sample in remainder[..complete].chunks_exact(4) {
      let value = f32::from_le_bytes([sample[0], sample[1], sample[2], sample[3]]);
      let bucket = ((sample_index.saturating_mul(WAVEFORM_POINTS as u64)) / expected_samples)
        .min((WAVEFORM_POINTS - 1) as u64) as usize;
      peaks[bucket] = peaks[bucket].max(value.abs().min(1.0));
      sample_index = sample_index.saturating_add(1);
    }
    remainder.drain(..complete);
  }

  let output = child
    .wait_with_output()
    .map_err(|error| error.to_string())?;
  if !output.status.success() {
    let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    return Err(if detail.is_empty() {
      format!("FFmpeg could not read the {} waveform", track.label)
    } else {
      detail
    });
  }

  Ok(peaks)
}

pub fn prepare(
  artifact_id: u64,
  source: &Path,
  duration_ms: u64,
  tracks: &[RecordingAudioTrack],
) -> Result<RecordingPreview, String> {
  let mut prepared = Vec::with_capacity(tracks.len());
  for track in tracks {
    // Nothing is written, so a failure part-way through leaves nothing behind
    // to tidy up - only a preview the window will not show.
    prepared.push(PreparedAudioTrack {
      kind: track.kind,
      label: track.label.clone(),
      stream_index: track.stream_index,
      waveform: waveform(source, track, duration_ms)?,
    });
  }

  Ok(RecordingPreview {
    artifact_id,
    tracks: prepared,
  })
}
