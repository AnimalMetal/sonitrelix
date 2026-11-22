//! Match three known bursts captured through the selected input/output path.
use crate::{
    Engine, Event, PlayRequest, StartGate,
    recording::{RecordRequest, Recorder},
};
use sonitrelix_audio_io::{Timeline, WavReader, WavWriter};
use std::{
    fs, io,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const RATE: u32 = 48000;
const DECIMATION: usize = 6;
const LOW_RATE: usize = RATE as usize / DECIMATION;
const POINTS: usize = 768;
const SEND_AT_MS: [usize; 3] = [350, 850, 1350];
const CAPTURE_MS: usize = 3550;

fn burst(index: usize) -> Vec<f32> {
    let mut state = 0xac19_2691u32 ^ (0x4a9d_3017 * (index as u32 + 1));
    let mut previous = 0.;
    (0..POINTS)
        .map(|point| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let bit = if state & 1 == 0 { -1. } else { 1. };
            let value = (bit - previous) * 0.055;
            previous = bit;
            let taper = (point.min(POINTS - 1 - point) as f32 / 24.).min(1.);
            value * taper
        })
        .collect()
}

fn measure(audio: &[f32]) -> io::Result<i32> {
    let low: Vec<_> = audio
        .chunks_exact(DECIMATION)
        .map(|chunk| chunk.iter().sum::<f32>() / DECIMATION as f32)
        .collect();
    let mut delays = Vec::with_capacity(3);
    for (index, &time) in SEND_AT_MS.iter().enumerate() {
        let probe = burst(index);
        let energy: f32 = probe.iter().map(|v| v * v).sum();
        let nominal = time * LOW_RATE / 1000;
        let mut score = Vec::with_capacity(2 * LOW_RATE);
        for at in nominal.saturating_sub(LOW_RATE / 20)
            ..=(nominal + 2 * LOW_RATE).min(low.len().saturating_sub(POINTS))
        {
            let (dot, level) = probe
                .iter()
                .zip(&low[at..at + POINTS])
                .fold((0., 0.), |(dot, level), (p, s)| {
                    (dot + p * s, level + s * s)
                });
            let similarity = if level < 0.000001 {
                0.
            } else {
                dot.abs() / (level * energy).sqrt()
            };
            score.push((at, similarity));
        }
        let Some(&(at, peak)) = score.iter().max_by(|a, b| a.1.total_cmp(&b.1)) else {
            continue;
        };
        let other = score
            .iter()
            .filter(|(position, _)| position.abs_diff(at) > LOW_RATE / 330)
            .map(|(_, score)| *score)
            .fold(0., f32::max);
        if peak >= 0.2 && peak >= other * 1.35 {
            delays.push((at as i32 - nominal as i32) * 1000 / LOW_RATE as i32);
        }
    }
    delays.sort_unstable();
    if delays.len() < 2 || delays.last().unwrap() - delays[0] > 8 {
        return Err(io::Error::other("No consistent calibration signal"));
    }
    Ok((-delays[delays.len() / 2]).clamp(-2000, 2000))
}

struct TemporaryAudio<'a>(&'a Path);
impl Drop for TemporaryAudio<'_> {
    fn drop(&mut self) {
        let _ = fs::remove_file(self.0);
    }
}

pub fn calibrate(
    directory: &Path,
    input_id: String,
    output_id: String,
    channels: u16,
    buffer_frames: u32,
    cancelled: &AtomicBool,
) -> Result<i32, String> {
    let result = (|| -> Result<i32, Box<dyn std::error::Error>> {
        let output_path = directory.join("probe.wav");
        let input_path = directory.join("capture.wav");
        let _output_file = TemporaryAudio(&output_path);
        let _input_file = TemporaryAudio(&input_path);
        let mut writer = WavWriter::create_float(&output_path, RATE, 2)?;
        let mut audio = vec![0.; RATE as usize * 3];
        for (index, &time) in SEND_AT_MS.iter().enumerate() {
            let start = time * RATE as usize / 1000 * 2;
            for (chip, value) in burst(index).into_iter().enumerate() {
                let offset = start + chip * DECIMATION * 2;
                audio[offset..offset + DECIMATION * 2].fill(value);
            }
        }
        writer.write_frames(&audio)?;
        let info = writer.finish()?;
        let gate = Arc::new(StartGate::new());
        let mut engine = Engine::new()?;
        engine.play(PlayRequest {
            gate: Some(gate.clone()),
            timeline: Timeline::single(output_path.clone(), info)?,
            start_ms: 0,
            start_frame: Some(0),
            loop_frames: None,
            output_id,
            buffer_frames,
            paused: false,
        })?;
        let recorder = Recorder::start(RecordRequest {
            gate: Some(gate),
            path: input_path.clone(),
            input_id,
            sample_rate: RATE,
            channels,
            buffer_frames,
            monitoring_output: None,
            capture_frames: Some(0..(CAPTURE_MS as u64 * u64::from(RATE) / 1000)),
        })?;
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut failed = None;
        let captured = loop {
            if failed.is_none() {
                failed = if cancelled.load(Ordering::Acquire) {
                    Some("Calibration cancelled".into())
                } else if let Some(Event::Error { message, .. }) = engine.try_event() {
                    Some(message)
                } else if engine.snapshot().underruns > 0 {
                    Some("Calibration playback underrun".into())
                } else if Instant::now() >= deadline {
                    Some("Calibration timed out".into())
                } else {
                    None
                };
                if failed.is_some() {
                    recorder.stop();
                    engine.stop();
                }
            }
            if let Some(result) = recorder.try_result() {
                break result;
            }
            if Instant::now() >= deadline + Duration::from_secs(3) {
                return Err(failed
                    .unwrap_or_else(|| "Calibration timed out".into())
                    .into());
            }
            thread::sleep(Duration::from_millis(10));
        };
        if let Some(message) = failed {
            return Err(message.into());
        }
        let captured = captured?;
        if let Some(warning) = captured.warning {
            return Err(warning.into());
        }
        let mut reader = WavReader::open(&captured.path)?;
        let mut interleaved = vec![0.; captured.info.frames as usize * usize::from(channels)];
        for chunk in interleaved.chunks_mut(8192 * usize::from(channels)) {
            if reader.read_frames(chunk)? * usize::from(channels) != chunk.len() {
                return Err("Calibration capture ended unexpectedly".into());
            }
        }
        if channels == 1 {
            return Ok(measure(&interleaved)?);
        }
        // A stereo interface may expose the connected microphone on just one channel.
        let left: Vec<_> = interleaved.iter().copied().step_by(2).collect();
        let right: Vec<_> = interleaved.iter().skip(1).copied().step_by(2).collect();
        let offset = match (measure(&left), measure(&right)) {
            (Ok(a), Ok(b)) if a.abs_diff(b) <= 8 => (a + b) / 2,
            (Ok(a), Err(_)) | (Err(_), Ok(a)) => a,
            _ => return Err("Could not identify a consistent signal on the input channels".into()),
        };
        Ok(offset)
    })();
    result.map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifies_repeatable_delay_and_rejects_missing_signal() {
        let mut audio = vec![0.; CAPTURE_MS * RATE as usize / 1000];
        assert!(measure(&audio).is_err());
        let lag = 137 * RATE as usize / 1000 + 2;
        for (index, &time) in SEND_AT_MS.iter().enumerate() {
            let start = time * RATE as usize / 1000 + lag;
            for (chip, value) in burst(index).into_iter().enumerate() {
                let offset = start + chip * DECIMATION;
                audio[offset..offset + DECIMATION].fill(-value * 0.3);
            }
        }
        for (i, sample) in audio.iter_mut().enumerate() {
            *sample += (i % 29) as f32 * 0.0001;
        }
        assert_eq!(measure(&audio).unwrap(), -137);
    }
}
