//! Offline FFmpeg effects. Their inputs are temporary float WAV files.
use super::*;
use sonitrelix_audio_io::ffmpeg;

#[derive(Clone, Copy, Debug)]
pub enum FilteredEffect {
    NoiseReduction(f32),
    DeEsser(f32),
    DeHum(f32),
    Tempo(f32),
    Ducking(f32),
}

pub struct Sidechain {
    pub timeline: Timeline,
    /// Starting frame on the controlling track's timeline after rate conversion.
    pub start: u64,
}

impl FilteredEffect {
    pub fn validate(self) -> io::Result<()> {
        let valid = match self {
            Self::NoiseReduction(db) => within(db, 1., 30.),
            Self::DeEsser(percent) | Self::Ducking(percent) => within(percent, 0., 100.),
            Self::DeHum(hz) => hz == 50. || hz == 60.,
            Self::Tempo(percent) => within(percent, 50., 200.),
        };
        if valid {
            Ok(())
        } else {
            Err(invalid("Effect parameters outside allowed range"))
        }
    }

    fn graph(self, frames: u64, rate: u32, noise_floor: f64) -> String {
        match self {
            Self::NoiseReduction(db) => {
                // afftdn uses a three-hop FFT window and emits audio two hops late.
                // Flush the delayed end, then trim the delay so vocals stay aligned.
                let hop = u64::from(rate / 80);
                let delay = 2 * hop;
                let end = frames + delay;
                let pad = end.div_ceil(hop) * hop - frames;
                format!(
                    "apad=pad_len={pad},afftdn=nr={db}:nf={noise_floor}:tn=0:gs=4,atrim=start_sample={delay}:end_sample={end},asetpts=PTS-STARTPTS"
                )
            }
            Self::DeEsser(percent) => format!("deesser=i={}:m=0.5", percent / 100.),
            Self::DeHum(hz) => [hz, hz * 2., hz * 3.]
                .map(|f| format!("bandreject=f={f}:t=h:w=4"))
                .join(","),
            Self::Tempo(100.) => "anull".into(),
            Self::Tempo(percent) => format!("atempo={}", percent / 100.),
            Self::Ducking(percent) => format!(
                "[1:a]apad,atrim=end_sample={frames}[voice];[0:a][voice]sidechaincompress=threshold=0.0316:ratio=6:attack=15:release=280:mix={}[out]",
                percent / 100.
            ),
        }
    }
}

/// Estimate the recording's floor from quiet, nonzero 50 ms windows.
/// Ignore digital gaps and use a histogram so memory is constant for long takes.
struct NoiseFloor {
    windows: [u64; 81],
    energy: f64,
    count: usize,
    window_samples: usize,
}
impl NoiseFloor {
    fn new(rate: u32, channels: usize) -> Self {
        Self {
            windows: [0; 81],
            energy: 0.,
            count: 0,
            window_samples: rate as usize / 20 * channels,
        }
    }
    fn add(&mut self, samples: &[f32]) {
        for &sample in samples {
            self.energy += f64::from(sample).powi(2);
            self.count += 1;
            if self.count == self.window_samples {
                self.finish_window();
            }
        }
    }
    fn finish_window(&mut self) {
        if self.count > 0 && self.energy > 0. {
            let rms_db = 10. * (self.energy / self.count as f64).log10();
            if (-80. ..=0.).contains(&rms_db) {
                self.windows[(rms_db + 80.).round() as usize] += 1;
            }
        }
        self.energy = 0.;
        self.count = 0;
    }
    fn estimate(mut self) -> f64 {
        self.finish_window();
        let total: u64 = self.windows.iter().sum();
        // A single transient in otherwise digital silence is not a noise floor.
        if total < 3 {
            return -65.;
        }
        let mut rank = (total - 1) / 5;
        for (bin, count) in self.windows.into_iter().enumerate() {
            if rank < count {
                return (bin as f64 - 76.).clamp(-75., -24.);
            }
            rank -= count;
        }
        -65.
    }
}

fn write_input(
    timeline: Timeline,
    start: u64,
    count: u64,
    path: &Path,
    cancel: &AtomicBool,
    measure_noise: bool,
) -> io::Result<Option<f64>> {
    let info = timeline.info();
    let mut reader = TimelineReader::new(timeline);
    let start = reader.seek_frame(start)?;
    let available = count.min(info.frames.saturating_sub(start));
    let mut writer = WavWriter::create_float(path, info.sample_rate, info.channels)?;
    let channels = usize::from(info.channels);
    let mut noise = measure_noise.then(|| NoiseFloor::new(info.sample_rate, channels));
    let mut block = vec![0.; 8192 * channels];
    let mut remaining = available;
    while remaining > 0 {
        if cancel.load(Ordering::Relaxed) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Effect cancelled",
            ));
        }
        let n = remaining.min(8192) as usize;
        let n = reader.read_frames(&mut block[..n * channels])?;
        if n == 0 {
            return Err(io::Error::other("Effect input ended early"));
        }
        writer.write_frames(&block[..n * channels])?;
        if let Some(noise) = &mut noise {
            noise.add(&block[..n * channels]);
        }
        remaining -= n as u64;
    }
    if available == 0 {
        writer.write_frames(&[0., 0.][..channels])?;
    }
    writer.finish()?;
    Ok(noise.map(NoiseFloor::estimate))
}

pub fn render_filtered(
    timeline: Timeline,
    effect: FilteredEffect,
    sidechain: Option<Sidechain>,
    output: &Path,
    cancel: &AtomicBool,
    progress: &AtomicU32,
) -> io::Result<WavInfo> {
    effect.validate()?;
    let exe = ffmpeg::installed().ok_or_else(|| io::Error::other(
        "This effect requires FFmpeg. Place ffmpeg.exe beside Sonitrelix.exe or install it on PATH.",
    ))?;
    let info = timeline.info();
    if info.frames == 0 || matches!(effect, FilteredEffect::Ducking(_)) != sidechain.is_some() {
        return Err(invalid(
            "The effect needs audio and a valid controlling track",
        ));
    }
    let input = output.with_extension("source.tmp.wav");
    let voice = output.with_extension("voice.tmp.wav");
    let result = (|| {
        let noise_floor = write_input(
            timeline,
            0,
            info.frames,
            &input,
            cancel,
            matches!(effect, FilteredEffect::NoiseReduction(_)),
        )?
        .unwrap_or(-65.);
        let mut inputs = vec![input.as_path()];
        if let Some(sidechain) = sidechain {
            write_input(
                sidechain.timeline,
                sidechain.start,
                info.frames,
                &voice,
                cancel,
                false,
            )?;
            inputs.push(&voice);
        }
        progress.store(50, Ordering::Relaxed);
        let rendered = ffmpeg::filter_wav(
            &exe,
            &inputs,
            output,
            &effect.graph(info.frames, info.sample_rate, noise_floor),
            cancel,
        )?;
        if rendered.sample_rate != info.sample_rate
            || rendered.channels != info.channels
            || (!matches!(effect, FilteredEffect::Tempo(_)) && rendered.frames != info.frames)
        {
            return Err(io::Error::other(
                "FFmpeg changed the effect's required audio format or duration",
            ));
        }
        progress.store(100, Ordering::Relaxed);
        Ok(rendered)
    })();
    let _ = std::fs::remove_file(input);
    let _ = std::fs::remove_file(voice);
    if result.is_err() {
        let _ = std::fs::remove_file(output);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use sonitrelix_audio_io::WavReader;

    fn rms(samples: &[f32]) -> f64 {
        (samples.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>() / samples.len() as f64).sqrt()
    }
    fn read_window(path: &Path, start: u64, frames: usize) -> Vec<f32> {
        let mut reader = WavReader::open(path).unwrap();
        assert_eq!(reader.seek_frame(start).unwrap(), start);
        let mut samples = vec![0.; frames];
        for block in samples.chunks_mut(8192) {
            assert_eq!(reader.read_frames(block).unwrap(), block.len());
        }
        samples
    }

    #[test]
    #[ignore = "Requires FFmpeg with afftdn and deesser"]
    fn ffmpeg_noise_reduction_and_deesser_measurably_change_audio() {
        let root = std::env::temp_dir();
        let dir = root.join(format!("sonitrelix-noise-{}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        let rate = 48000;
        let stop = AtomicBool::new(false);
        let progress = AtomicU32::new(0);
        let sine = |i: usize, frequency: f64| {
            (i as f64 * std::f64::consts::TAU * frequency / f64::from(rate)).sin() as f32
        };
        for (case, amplitude) in [("normal", 0.02), ("loud", 0.06)] {
            let mut rng = 13_u32;
            let samples: Vec<_> = (0..6 * rate as usize)
                .map(|i| {
                    rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                    let noise = (2. * (rng >> 8) as f32 / 16777216. - 1.) * amplitude;
                    // Model an imported gap, then steady noise, then a foreground tone.
                    if i < rate as usize / 2 {
                        0.
                    } else if (2 * rate as usize..4 * rate as usize).contains(&i) {
                        noise + 0.14 * sine(i, 440.)
                    } else {
                        noise
                    }
                })
                .collect();
            let source = dir.join(format!("{case}.wav"));
            let mut writer = WavWriter::create_float(&source, rate, 1).unwrap();
            writer.write_frames(&samples).unwrap();
            let info = writer.finish().unwrap();
            let output = dir.join(format!("{case}-filtered.wav"));
            let rendered = render_filtered(
                Timeline::single(source, info).unwrap(),
                FilteredEffect::NoiseReduction(12.),
                None,
                &output,
                &stop,
                &progress,
            )
            .unwrap();
            assert_eq!(rendered, info);
            let quiet_start = rate as usize;
            let quieter = read_window(&output, quiet_start as u64, rate as usize);
            let attenuation = 20.
                * (rms(&samples[quiet_start..quiet_start + rate as usize]) / rms(&quieter)).log10();
            assert!(
                attenuation >= 8.,
                "{case} noise attenuation: {attenuation:.2} dB"
            );
            let foreground_start = 3 * rate as usize;
            let foreground = read_window(&output, foreground_start as u64, rate as usize);
            let project_tone = |data: &[f32]| {
                data.iter()
                    .enumerate()
                    .map(|(i, x)| f64::from(*x * sine(foreground_start + i, 440.)))
                    .sum::<f64>()
            };
            let ratio = project_tone(&foreground)
                / project_tone(&samples[foreground_start..foreground_start + rate as usize]);
            assert!(
                (0.9..=1.1).contains(&ratio),
                "{case} foreground gain: {ratio}"
            );
            println!("{case}: {attenuation:.2} dB of noise reduction; foreground gain {ratio:.3}");
        }
        let samples: Vec<_> = (0..4 * rate as usize)
            .map(|i| {
                let base = 0.12 * sine(i, 250.);
                if (i / (rate as usize / 4)) % 4 == 1 {
                    base + 0.42 * sine(i, 7200.)
                } else {
                    base
                }
            })
            .collect();
        let source = dir.join("sibilant.wav");
        let mut writer = WavWriter::create_float(&source, rate, 1).unwrap();
        writer.write_frames(&samples).unwrap();
        let info = writer.finish().unwrap();
        let output = dir.join("deessed.wav");
        assert_eq!(
            render_filtered(
                Timeline::single(source, info).unwrap(),
                FilteredEffect::DeEsser(35.),
                None,
                &output,
                &stop,
                &progress
            )
            .unwrap(),
            info
        );
        for (offset, expected) in [(60000, 0.2..0.7), (72000, 0.9..1.1)] {
            let gain = rms(&read_window(&output, offset as u64, 6000))
                / rms(&samples[offset..offset + 6000]);
            assert!(
                expected.contains(&gain),
                "de-esser gain at {offset}: {gain}"
            );
        }
        assert!(dir.starts_with(root));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    #[ignore = "Requires FFmpeg with its standard audio filters"]
    fn ffmpeg_effects_keep_format_change_tempo_and_duck_only_under_voice() {
        let root = std::env::temp_dir();
        let dir = root.join(format!("sonitrelix-filtered-{}-á", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        let source = dir.join("backing.wav");
        let rate = 48000;
        let mut writer = WavWriter::create_float(&source, rate, 1).unwrap();
        let frames = rate as usize * 3;
        let samples: Vec<_> = (0..frames)
            .map(|i| 0.2 * (i as f32 * std::f32::consts::TAU * 60. / rate as f32).sin())
            .collect();
        writer.write_frames(&samples).unwrap();
        let info = writer.finish().unwrap();
        let input = Timeline::single(source.clone(), info).unwrap();
        let cancel = AtomicBool::new(false);
        let progress = AtomicU32::new(0);
        let impulse = dir.join("impulse.wav");
        let mut impulse_writer = WavWriter::create_float(&impulse, rate, 1).unwrap();
        let mut samples = vec![0.; 12347];
        samples[12000] = 0.8;
        impulse_writer.write_frames(&samples).unwrap();
        let impulse_info = impulse_writer.finish().unwrap();
        let impulse_rendered = dir.join("aligned.wav");
        let rendered = render_filtered(
            Timeline::single(impulse, impulse_info).unwrap(),
            FilteredEffect::NoiseReduction(12.),
            None,
            &impulse_rendered,
            &cancel,
            &progress,
        )
        .unwrap();
        assert_eq!(rendered.frames, impulse_info.frames);
        let mut aligned = WavReader::open(&impulse_rendered).unwrap();
        let mut tail = [0.; 347];
        aligned.seek_frame(12000).unwrap();
        assert_eq!(aligned.read_frames(&mut tail).unwrap(), 347);
        assert!(tail[0].abs() > 0.4);
        assert!(tail[1..].iter().all(|x| x.abs() < 0.4));
        drop(aligned);
        for (name, effect) in [
            ("denoise", FilteredEffect::NoiseReduction(12.)),
            ("deesser", FilteredEffect::DeEsser(35.)),
            ("dehum", FilteredEffect::DeHum(60.)),
            ("tempo", FilteredEffect::Tempo(150.)),
        ] {
            let out = dir.join(format!("{name}.wav"));
            let rendered =
                render_filtered(input.clone(), effect, None, &out, &cancel, &progress).unwrap();
            assert_eq!(rendered.channels, 1);
            assert_eq!(rendered.sample_rate, rate);
            if name == "tempo" {
                assert!((94000..=98000).contains(&rendered.frames));
            } else {
                assert_eq!(rendered.frames, info.frames);
            }
            if name == "dehum" {
                let mut reader = WavReader::open(&out).unwrap();
                reader.seek_frame(96000).unwrap();
                let mut block = [0.; 4000];
                reader.read_frames(&mut block).unwrap();
                assert!(block.iter().all(|&value| value.abs() < 0.02));
            }
        }
        let voice_path = dir.join("voice.wav");
        let mut voice = WavWriter::create_float(&voice_path, rate, 2).unwrap();
        let mut samples = vec![0.; rate as usize * 2];
        samples.extend(vec![0.5; rate as usize * 2]);
        voice.write_frames(&samples).unwrap();
        let voice_info = voice.finish().unwrap();
        let voice = Timeline::single(voice_path, voice_info).unwrap();
        let out = dir.join("duck.wav");
        let rendered = render_filtered(
            input,
            FilteredEffect::Ducking(100.),
            Some(Sidechain {
                timeline: voice,
                start: u64::from(rate / 2),
            }),
            &out,
            &cancel,
            &progress,
        )
        .unwrap();
        assert_eq!(rendered, info);
        let mut ducked = WavReader::open(&out).unwrap();
        let mut block = [0.; 2400];
        for (frame, min, max) in [
            (10000, 0.18, 0.21),
            (48000, 0.0, 0.08),
            (130000, 0.18, 0.21),
        ] {
            ducked.seek_frame(frame).unwrap();
            ducked.read_frames(&mut block).unwrap();
            let peak = block.iter().map(|x| x.abs()).fold(0., f32::max);
            assert!((min..=max).contains(&peak), "{frame}: {peak}");
        }
        drop(ducked);
        assert!(dir.starts_with(root));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
