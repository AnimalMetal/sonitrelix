//! Offline, bounded-memory effects. Source timelines are immutable; results are new float WAVs.
mod filtered;
pub use filtered::{FilteredEffect, Sidechain, render_filtered};
use sonitrelix_audio_io::{Timeline, TimelineReader, WavInfo, WavWriter};
use std::{
    io,
    path::Path,
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
};

#[derive(Clone, Copy, Debug)]
pub enum Effect {
    Amplify(f32),
    Normalize(f32),
    FadeIn,
    FadeOut,
    Reverse,
    Silence,
    Eq {
        low: f32,
        mid: f32,
        high: f32,
    },
    Compressor {
        threshold: f32,
        ratio: f32,
        makeup: f32,
    },
    Gate {
        threshold: f32,
        attack_ms: f32,
        release_ms: f32,
    },
    Reverb {
        wet: f32,
        decay: f32,
        tail: f32,
    },
}
fn invalid(s: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, s)
}
fn db(v: f32) -> f32 {
    10f32.powf(v / 20.)
}
fn within(v: f32, a: f32, b: f32) -> bool {
    v.is_finite() && (a..=b).contains(&v)
}
impl Effect {
    pub fn validate(self) -> io::Result<()> {
        let valid = match self {
            Self::Amplify(v) => within(v, -60., 24.),
            Self::Normalize(v) => within(v, -60., 0.),
            Self::Eq { low, mid, high } => [low, mid, high].iter().all(|v| within(*v, -24., 24.)),
            Self::Compressor {
                threshold,
                ratio,
                makeup,
            } => within(threshold, -80., 0.) && within(ratio, 1., 20.) && within(makeup, 0., 24.),
            Self::Gate {
                threshold,
                attack_ms,
                release_ms,
            } => {
                within(threshold, -100., 0.)
                    && within(attack_ms, 0.1, 500.)
                    && within(release_ms, 1., 5000.)
            }
            Self::Reverb { wet, decay, tail } => {
                within(wet, 0., 100.) && within(decay, 0.1, 10.) && within(tail, 0., 10.)
            }
            _ => true,
        };
        if valid {
            Ok(())
        } else {
            Err(invalid("Effect parameters outside allowed range"))
        }
    }
}
struct Delay {
    data: Vec<f32>,
    pos: usize,
    feedback: f32,
}
impl Delay {
    fn tick(&mut self, input: f32) -> f32 {
        let out = self.data[self.pos];
        self.data[self.pos] = input + out * self.feedback;
        self.pos = (self.pos + 1) % self.data.len();
        out
    }
}
pub struct Processor {
    effect: Effect,
    frames: u64,
    position: u64,
    channels: usize,
    rate: f32,
    low: Vec<f32>,
    upper: Vec<f32>,
    envelope: f32,
    gain: f32,
    delays: Vec<Vec<Delay>>,
}
impl Processor {
    pub fn new(effect: Effect, rate: u32, channels: u16, frames: u64) -> io::Result<Self> {
        effect.validate()?;
        if !(8000..=192000).contains(&rate) || ![1, 2].contains(&channels) || frames == 0 {
            return Err(invalid("Empty or invalid effect format"));
        }
        let delays = if let Effect::Reverb { decay, .. } = effect {
            (0..channels)
                .map(|c| {
                    [0.0297, 0.0371, 0.0411, 0.0437]
                        .iter()
                        .map(|seconds| {
                            let n =
                                ((*seconds + f32::from(c) * 0.0013) * rate as f32).round() as usize;
                            Delay {
                                data: vec![0.; n.max(1)],
                                pos: 0,
                                feedback: 10f32.powf(-3. * n as f32 / (rate as f32 * decay)),
                            }
                        })
                        .collect()
                })
                .collect()
        } else {
            vec![]
        };
        Ok(Self {
            effect,
            frames,
            position: 0,
            channels: channels as usize,
            rate: rate as f32,
            low: vec![0.; channels as usize],
            upper: vec![0.; channels as usize],
            envelope: 0.,
            gain: 0.,
            delays,
        })
    }
    pub fn process(&mut self, samples: &mut [f32]) {
        let low_a = 1. - (-std::f32::consts::TAU * 200. / self.rate).exp();
        let high_a = 1. - (-std::f32::consts::TAU * 4000. / self.rate).exp();
        for frame in samples.chunks_exact_mut(self.channels) {
            let peak = frame.iter().map(|v| v.abs()).fold(0., f32::max);
            let linked_gain = match self.effect {
                Effect::Compressor {
                    threshold,
                    ratio,
                    makeup,
                } => {
                    let coef =
                        (-1. / (self.rate * if peak > self.envelope { 0.005 } else { 0.1 })).exp();
                    self.envelope = coef * self.envelope + (1. - coef) * peak;
                    let over = (20. * self.envelope.max(1e-12).log10() - threshold).max(0.);
                    db(makeup - over * (1. - 1. / ratio))
                }
                Effect::Gate {
                    threshold,
                    attack_ms,
                    release_ms,
                } => {
                    let open = peak >= db(threshold);
                    let coef = (-1.
                        / (self.rate * if open { attack_ms } else { release_ms } * 0.001))
                        .exp();
                    self.gain = coef * self.gain + (1. - coef) * if open { 1. } else { 0. };
                    self.gain
                }
                _ => 1.,
            };
            for (c, sample) in frame.iter_mut().enumerate() {
                let v = if sample.is_finite() { *sample } else { 0. };
                *sample = match self.effect {
                    Effect::Amplify(g) => v * db(g),
                    Effect::FadeIn => {
                        v * self.position as f32 / self.frames.saturating_sub(1).max(1) as f32
                    }
                    Effect::FadeOut => {
                        v * (1.
                            - self.position.min(self.frames - 1) as f32
                                / self.frames.saturating_sub(1).max(1) as f32)
                    }
                    Effect::Silence => 0.,
                    Effect::Eq { low, mid, high } => {
                        self.low[c] += low_a * (v - self.low[c]);
                        self.upper[c] += high_a * (v - self.upper[c]);
                        self.low[c] * db(low)
                            + (self.upper[c] - self.low[c]) * db(mid)
                            + (v - self.upper[c]) * db(high)
                    }
                    Effect::Compressor { .. } | Effect::Gate { .. } => v * linked_gain,
                    Effect::Reverb { wet, .. } => {
                        let reverberated =
                            self.delays[c].iter_mut().map(|d| d.tick(v)).sum::<f32>() * 0.25;
                        v * (1. - wet * 0.01) + reverberated * wet * 0.01
                    }
                    _ => v,
                };
                if !sample.is_finite() {
                    *sample = 0.;
                }
            }
            self.position += 1;
        }
    }
}
pub fn render(
    timeline: Timeline,
    effect: Effect,
    path: &Path,
    cancel: &AtomicBool,
    progress: &AtomicU32,
) -> io::Result<WavInfo> {
    effect.validate()?;
    let info = timeline.info();
    let mut processor = Processor::new(effect, info.sample_rate, info.channels, info.frames)?;
    let tail = if let Effect::Reverb { tail, .. } = effect {
        (tail * info.sample_rate as f32).round() as u64
    } else {
        0
    };
    let total = info
        .frames
        .checked_add(tail)
        .ok_or_else(|| invalid("Effect length overflow"))?;
    if total.saturating_mul(u64::from(info.channels) * 4) > u64::from(u32::MAX) - 50 {
        return Err(invalid("Effect output exceeds WAV size limit"));
    }
    let mut reader = TimelineReader::new(timeline);
    let mut buffer = vec![0.; 8192 * usize::from(info.channels)];
    let mut normalization = None;
    // Normalize scans before creating output; all effects keep memory bounded.
    if let Effect::Normalize(target) = effect {
        let mut peak = 0f32;
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(io::Error::new(io::ErrorKind::Interrupted, "Cancelled"));
            }
            let n = reader.read_frames(&mut buffer)?;
            if n == 0 {
                break;
            }
            peak = buffer[..n * usize::from(info.channels)]
                .iter()
                .map(|v| v.abs())
                .fold(peak, f32::max);
        }
        normalization = Some(if peak > 0. {
            f64::from(db(target)) / f64::from(peak)
        } else {
            1.
        });
        reader.seek_frame(0)?;
    }
    let mut writer = WavWriter::create_float(path, info.sample_rate, info.channels)?;
    let result = (|| {
        let mut position = 0u64;
        while position < total {
            if cancel.load(Ordering::Relaxed) {
                return Err(io::Error::new(io::ErrorKind::Interrupted, "Cancelled"));
            }
            let n = (total - position).min(8192) as usize;
            let block = &mut buffer[..n * usize::from(info.channels)];
            if position >= info.frames {
                block.fill(0.);
            } else {
                let count = (info.frames - position).min(n as u64) as usize;
                if matches!(effect, Effect::Reverse) {
                    reader.seek_frame(info.frames - position - count as u64)?;
                }
                let read = reader.read_frames(&mut block[..count * usize::from(info.channels)])?;
                if read != count {
                    return Err(invalid("Unexpected end of effect source"));
                }
                if matches!(effect, Effect::Reverse) {
                    for a in 0..count / 2 {
                        for c in 0..usize::from(info.channels) {
                            block.swap(
                                a * usize::from(info.channels) + c,
                                (count - 1 - a) * usize::from(info.channels) + c,
                            );
                        }
                    }
                }
                block[count * usize::from(info.channels)..].fill(0.);
            }
            if let Some(gain) = normalization {
                for v in block.iter_mut() {
                    *v = (f64::from(*v) * gain) as f32;
                }
            } else {
                processor.process(block);
            }
            writer.write_frames(block)?;
            position += n as u64;
            progress.store((position * 100 / total) as u32, Ordering::Relaxed);
        }
        writer.finish()
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(path);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gain_fades_eq_and_linked_dynamics() {
        let mut b = [0.5; 4];
        Processor::new(Effect::Amplify(-6.0206), 48000, 2, 2)
            .unwrap()
            .process(&mut b);
        assert!((b[0] - 0.25).abs() < 0.0001);
        let mut b = [1.; 4];
        Processor::new(Effect::FadeIn, 48000, 1, 4)
            .unwrap()
            .process(&mut b);
        assert_eq!(b[0], 0.);
        assert_eq!(b[3], 1.);
        let mut b = [1.; 4];
        Processor::new(Effect::FadeOut, 48000, 1, 4)
            .unwrap()
            .process(&mut b);
        assert_eq!(b[0], 1.);
        assert_eq!(b[3], 0.);
        let mut b = [0.5, -0.2, 0.1, 0.];
        let expected = b;
        Processor::new(
            Effect::Eq {
                low: 0.,
                mid: 0.,
                high: 0.,
            },
            48000,
            2,
            2,
        )
        .unwrap()
        .process(&mut b);
        for (a, b) in b.iter().zip(expected) {
            assert!((*a - b).abs() < 1e-6);
        }
        let bass: Vec<_> = (0..4800)
            .map(|n| (n as f32 * std::f32::consts::TAU / 480.).sin() * 0.1)
            .collect();
        let mut boosted = bass.clone();
        Processor::new(
            Effect::Eq {
                low: 12.,
                mid: 0.,
                high: 0.,
            },
            48000,
            1,
            4800,
        )
        .unwrap()
        .process(&mut boosted);
        assert!(
            boosted[2400..].iter().map(|v| v * v).sum::<f32>()
                > 3. * bass[2400..].iter().map(|v| v * v).sum::<f32>()
        );
        let mut b = vec![0.8; 48000 * 2];
        Processor::new(
            Effect::Compressor {
                threshold: -20.,
                ratio: 4.,
                makeup: 0.,
            },
            48000,
            2,
            48000,
        )
        .unwrap()
        .process(&mut b);
        assert!(b[b.len() - 1] < 0.2);
        assert_eq!(b[b.len() - 1], b[b.len() - 2]);
        let mut b = vec![0.001; 48000];
        Processor::new(
            Effect::Gate {
                threshold: -40.,
                attack_ms: 5.,
                release_ms: 100.,
            },
            48000,
            1,
            48000,
        )
        .unwrap()
        .process(&mut b);
        assert!(b.iter().all(|v| *v == 0.));
    }
    #[test]
    fn reverb_tail_and_block_continuity() {
        let effect = Effect::Reverb {
            wet: 100.,
            decay: 1.,
            tail: 1.,
        };
        let mut whole = vec![0.; 96000];
        whole[0] = 1.;
        let mut split = whole.clone();
        Processor::new(effect, 48000, 1, 48000)
            .unwrap()
            .process(&mut whole);
        let mut p = Processor::new(effect, 48000, 1, 48000).unwrap();
        for b in split.chunks_mut(127) {
            p.process(b);
        }
        assert_eq!(whole, split);
        assert!(whole[1000..].iter().any(|v| v.abs() > 0.01));
        assert!(whole.iter().all(|v| v.is_finite()));
        assert!(
            Effect::Reverb {
                wet: f32::NAN,
                decay: 1.,
                tail: 1.
            }
            .validate()
            .is_err()
        );
    }
    #[test]
    fn render_reverse_normalize_and_cancel_preserve_source() {
        use sonitrelix_audio_io::WavReader;
        let dir = std::env::temp_dir().join(format!("sonitrelix-fx-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("source.wav");
        let mut w = WavWriter::create_float(&src, 48000, 2).unwrap();
        w.write_frames(&[0.1, 0.2, 0.3, 0.4, 0.5, 0.25]).unwrap();
        let info = w.finish().unwrap();
        let original = std::fs::read(&src).unwrap();
        let timeline = Timeline::single(src.clone(), info).unwrap();
        let stop = AtomicBool::new(false);
        let progress = AtomicU32::new(0);
        let out = dir.join("reverse.wav");
        render(timeline.clone(), Effect::Reverse, &out, &stop, &progress).unwrap();
        let mut r = WavReader::open(&out).unwrap();
        let mut samples = [0.; 6];
        r.read_frames(&mut samples).unwrap();
        assert_eq!(samples, [0.5, 0.25, 0.3, 0.4, 0.1, 0.2]);
        drop(r);
        let out = dir.join("norm.wav");
        render(
            timeline.clone(),
            Effect::Normalize(0.),
            &out,
            &stop,
            &progress,
        )
        .unwrap();
        let mut r = WavReader::open(&out).unwrap();
        r.read_frames(&mut samples).unwrap();
        assert_eq!(samples[4], 1.);
        drop(r);
        let out = dir.join("silence.wav");
        render(timeline.clone(), Effect::Silence, &out, &stop, &progress).unwrap();
        let mut reader = WavReader::open(&out).unwrap();
        assert_eq!(reader.info.frames, info.frames);
        reader.read_frames(&mut samples).unwrap();
        assert_eq!(samples, [0.; 6]);
        drop(reader);
        stop.store(true, Ordering::Relaxed);
        assert!(
            render(
                timeline,
                Effect::Silence,
                &dir.join("cancel.wav"),
                &stop,
                &progress
            )
            .is_err()
        );
        assert!(!dir.join("cancel.wav").exists());
        assert_eq!(std::fs::read(src).unwrap(), original);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
