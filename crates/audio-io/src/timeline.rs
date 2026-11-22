use super::*;
use std::path::PathBuf;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub path: PathBuf,
    pub info: WavInfo,
}

#[derive(Clone, Debug)]
pub struct Segment {
    pub source: Option<Source>,
    pub source_start: u64,
    pub frames: u64,
}
#[derive(Clone, Debug)]
pub struct Timeline {
    format: WavInfo,
    segments: Vec<Segment>,
    mix: Vec<MixTrack>,
    live_mix: Option<LiveMix>,
}
/// Track controls for the decode worker. The render thread consumes PCM only.
#[derive(Clone, Debug)]
pub struct LiveMix(std::sync::Arc<std::sync::RwLock<Vec<LiveTrack>>>);

#[derive(Clone, Copy, Debug)]
struct LiveTrack {
    audible: bool,
    levels: Option<(f32, f32)>,
}

impl LiveMix {
    pub fn new(audible: Vec<bool>) -> Self {
        Self(std::sync::Arc::new(std::sync::RwLock::new(
            audible
                .into_iter()
                .map(|audible| LiveTrack {
                    audible,
                    levels: None,
                })
                .collect(),
        )))
    }

    pub fn update(&self, audible: impl Iterator<Item = bool>) {
        let mut current = self.0.write().unwrap();
        for (value, enabled) in current.iter_mut().zip(audible) {
            value.audible = enabled;
        }
    }

    pub fn set_levels(&self, index: usize, gain: f32, pan: f32) {
        if (0. ..=16.).contains(&gain)
            && (-1. ..=1.).contains(&pan)
            && let Some(track) = self.0.write().unwrap().get_mut(index)
        {
            track.levels = Some((gain, pan));
        }
    }
}
#[derive(Clone, Debug)]
pub struct MixTrack {
    pub timeline: Timeline,
    pub gain: f32,
    pub pan: f32,
}
impl Timeline {
    pub fn new(rate: u32, channels: u16, segments: Vec<Segment>) -> io::Result<Self> {
        if !(8000..=192000).contains(&rate) || ![1, 2].contains(&channels) {
            return Err(invalid("Invalid track format"));
        }
        let mut frames = 0u64;
        for c in &segments {
            frames = frames
                .checked_add(c.frames)
                .ok_or_else(|| invalid("Timeline overflow"))?;
            if c.frames == 0 {
                return Err(invalid("Empty clip"));
            }
            if let Some(s) = &c.source {
                if s.info.sample_rate != rate
                    || s.info.channels != channels
                    || c.source_start
                        .checked_add(c.frames)
                        .is_none_or(|end| end > s.info.frames)
                {
                    return Err(invalid("Clip source format or bounds mismatch"));
                }
            } else if c.source_start != 0 {
                return Err(invalid("Silence offset"));
            }
        }
        Ok(Self {
            format: WavInfo {
                sample_rate: rate,
                channels,
                bits: 32,
                frames,
                encoding: Encoding::Float,
            },
            segments,
            mix: vec![],
            live_mix: None,
        })
    }
    pub fn single(path: PathBuf, info: WavInfo) -> io::Result<Self> {
        Self::new(
            info.sample_rate,
            info.channels,
            vec![Segment {
                source: Some(Source { path, info }),
                source_start: 0,
                frames: info.frames,
            }],
        )
    }
    pub fn info(&self) -> WavInfo {
        self.format
    }
    pub fn is_live_mix(&self) -> bool {
        self.live_mix.is_some()
    }
    /// Stereo mix at a common rate. All resampling and disk access happen in readers/workers.
    pub fn mix(rate: u32, tracks: Vec<MixTrack>) -> io::Result<Self> {
        if !(8000..=192000).contains(&rate) {
            return Err(invalid("Mix rate"));
        }
        let mut frames = 0;
        for t in &tracks {
            if !t.timeline.mix.is_empty()
                || !t.gain.is_finite()
                || !(0. ..=16.).contains(&t.gain)
                || !t.pan.is_finite()
                || !(-1. ..=1.).contains(&t.pan)
            {
                return Err(invalid("Mix gain, pan or nested mix"));
            }
            let n = (u128::from(t.timeline.info().frames) * u128::from(rate))
                .div_ceil(u128::from(t.timeline.info().sample_rate));
            frames = frames.max(u64::try_from(n).map_err(|_| invalid("Mix duration overflow"))?);
        }
        Ok(Self {
            format: WavInfo {
                sample_rate: rate,
                channels: 2,
                bits: 32,
                frames,
                encoding: Encoding::Float,
            },
            segments: vec![],
            mix: tracks,
            live_mix: None,
        })
    }
    pub fn with_live_mix(mut self, controls: LiveMix) -> io::Result<Self> {
        if self.mix.len() != controls.0.read().unwrap().len() {
            return Err(invalid("Live mix track count"));
        }
        self.live_mix = Some(controls);
        Ok(self)
    }
    pub fn slice(&self, start: u64, end: u64) -> io::Result<Self> {
        if !self.mix.is_empty() {
            return Err(invalid("Slice tracks before mixing"));
        }
        if end <= start || end > self.format.frames {
            return Err(invalid("Timeline slice bounds"));
        }
        let mut position = 0;
        let mut segments = vec![];
        for s in &self.segments {
            let a = start.max(position);
            let b = end.min(position + s.frames);
            if b > a {
                let mut part = s.clone();
                part.frames = b - a;
                if part.source.is_some() {
                    part.source_start += a - position;
                }
                segments.push(part);
            }
            position += s.frames;
        }
        Self::new(self.format.sample_rate, self.format.channels, segments)
    }
}
pub struct TimelineReader {
    timeline: Timeline,
    index: usize,
    within: u64,
    reader: Option<WavReader>,
    reader_source: Option<Source>,
    mix: Vec<Resampled>,
    mix_position: u64,
    live_tracks: Vec<LiveTrack>,
}
fn resampler(source: u32, target: u32) -> Vec<Vec<f32>> {
    if source == target {
        return vec![];
    }
    let ratio = (target as f64 / source as f64).min(1.);
    let taps = ((64. / ratio).ceil() as usize)
        .clamp(64, 1536)
        .next_multiple_of(2);
    let cutoff = 0.95 * ratio;
    (0..=512)
        .map(|phase| {
            let fraction = phase as f64 / 512.;
            let mut coefficients: Vec<_> = (0..taps)
                .map(|tap| {
                    let x = tap as f64 - (taps / 2) as f64 + 1. - fraction;
                    let sinc = if x.abs() < 1e-12 {
                        cutoff
                    } else {
                        (std::f64::consts::PI * cutoff * x).sin() / (std::f64::consts::PI * x)
                    };
                    let window = 0.5 + 0.5 * (std::f64::consts::PI * x / (taps / 2) as f64).cos();
                    (sinc * window) as f32
                })
                .collect();
            let sum = coefficients.iter().sum::<f32>();
            for v in &mut coefficients {
                *v /= sum;
            }
            coefficients
        })
        .collect()
}
struct Resampled {
    reader: Box<TimelineReader>,
    cache: Vec<f32>,
    start: u64,
    count: usize,
    left: f32,
    right: f32,
    coefficients: Vec<Vec<f32>>,
}
fn channel_levels(channels: u16, gain: f32, pan: f32) -> (f32, f32) {
    let (left, right) = if channels == 1 {
        let angle = (pan + 1.) * std::f32::consts::FRAC_PI_4;
        (angle.cos(), angle.sin())
    } else {
        ((1. - pan).min(1.), (1. + pan).min(1.))
    };
    (left * gain, right * gain)
}

impl Resampled {
    fn sample(&mut self, frame: u64) -> io::Result<[f32; 2]> {
        let info = self.reader.info();
        if frame >= info.frames {
            return Ok([0.; 2]);
        }
        if self.count == 0 || frame < self.start || frame >= self.start + self.count as u64 {
            let start =
                frame.saturating_sub(self.coefficients.first().map_or(2, |v| v.len() + 2) as u64);
            self.reader.seek_frame(start)?;
            self.start = start;
            self.count = self.reader.read_frames(&mut self.cache)?;
        }
        let i = (frame - self.start) as usize * usize::from(info.channels);
        if info.channels == 1 {
            Ok([self.cache[i] * self.left, self.cache[i] * self.right])
        } else {
            Ok([self.cache[i] * self.left, self.cache[i + 1] * self.right])
        }
    }
}
impl TimelineReader {
    pub fn new(timeline: Timeline) -> Self {
        let mix = timeline
            .mix
            .iter()
            .map(|t| {
                let (left, right) = channel_levels(t.timeline.info().channels, t.gain, t.pan);
                Resampled {
                    reader: Box::new(Self::new(t.timeline.clone())),
                    cache: vec![0.; 8192 * usize::from(t.timeline.info().channels)],
                    start: 0,
                    count: 0,
                    left,
                    right,
                    coefficients: resampler(
                        t.timeline.info().sample_rate,
                        timeline.info().sample_rate,
                    ),
                }
            })
            .collect();
        Self {
            live_tracks: vec![
                LiveTrack {
                    audible: true,
                    levels: None
                };
                timeline.mix.len()
            ],
            timeline,
            index: 0,
            within: 0,
            reader: None,
            reader_source: None,
            mix,
            mix_position: 0,
        }
    }
    pub fn info(&self) -> WavInfo {
        self.timeline.info()
    }
    pub fn seek_frame(&mut self, frame: u64) -> io::Result<u64> {
        let target = frame.min(self.timeline.format.frames);
        self.mix_position = target;
        // Resampler caches are keyed by absolute source frames and stay valid after a seek.
        let mut n = target;
        self.index = 0;
        while self.index < self.timeline.segments.len()
            && n >= self.timeline.segments[self.index].frames
        {
            n -= self.timeline.segments[self.index].frames;
            self.index += 1;
        }
        self.within = n;
        self.reader = None;
        self.reader_source = None;
        Ok(target)
    }
    pub fn read_frames(&mut self, out: &mut [f32]) -> io::Result<usize> {
        if !self.mix.is_empty() {
            let n = (out.len() / 2).min(8192).min(
                self.timeline
                    .info()
                    .frames
                    .saturating_sub(self.mix_position) as usize,
            );
            out[..n * 2].fill(0.);
            let rate = self.timeline.info().sample_rate;
            if let Some(controls) = &self.timeline.live_mix {
                self.live_tracks
                    .copy_from_slice(&controls.0.read().unwrap());
            }
            for (track, state) in self.mix.iter_mut().zip(&self.live_tracks) {
                if !state.audible {
                    continue;
                }
                if let Some((gain, pan)) = state.levels {
                    (track.left, track.right) =
                        channel_levels(track.reader.info().channels, gain, pan);
                }
                let source_rate = track.reader.info().sample_rate;
                for i in 0..n {
                    let numerator =
                        u128::from(self.mix_position + i as u64) * u128::from(source_rate);
                    let frame = (numerator / u128::from(rate)) as u64;
                    if frame >= track.reader.info().frames {
                        continue;
                    }
                    let fraction = (numerator % u128::from(rate)) as f32 / rate as f32;
                    if track.coefficients.is_empty() {
                        let v = track.sample(frame)?;
                        for c in 0..2 {
                            out[i * 2 + c] += v[c];
                        }
                    } else {
                        let phase = fraction * 512.;
                        let table = phase.floor() as usize;
                        let blend = phase - table as f32;
                        let taps = track.coefficients[table].len();
                        for tap in 0..taps {
                            let coefficient = track.coefficients[table][tap] * (1. - blend)
                                + track.coefficients[table + 1][tap] * blend;
                            let index = (i128::from(frame) + tap as i128 - (taps / 2) as i128 + 1)
                                .clamp(0, i128::from(track.reader.info().frames) - 1)
                                as u64;
                            let v = track.sample(index)?;
                            for c in 0..2 {
                                out[i * 2 + c] += v[c] * coefficient;
                            }
                        }
                    }
                }
            }
            self.mix_position += n as u64;
            return Ok(n);
        }
        let channels = usize::from(self.timeline.format.channels);
        let requested = (out.len() / channels).min(8192);
        let mut done = 0;
        while done < requested && self.index < self.timeline.segments.len() {
            let segment = &self.timeline.segments[self.index];
            let count = (requested - done).min((segment.frames - self.within) as usize);
            let buffer = &mut out[done * channels..(done + count) * channels];
            if let Some(source) = &segment.source {
                if self.reader_source.as_ref() != Some(source) {
                    let mut r = WavReader::open(&source.path)?;
                    if r.info != source.info {
                        return Err(invalid("Source WAV changed; import it again"));
                    }
                    r.seek_frame(segment.source_start + self.within)?;
                    self.reader = Some(r);
                    self.reader_source = Some(source.clone());
                }
                if self.reader.as_ref().expect("opened source").frame
                    != segment.source_start + self.within
                {
                    self.reader
                        .as_mut()
                        .expect("opened source")
                        .seek_frame(segment.source_start + self.within)?;
                }
                if self
                    .reader
                    .as_mut()
                    .expect("opened source")
                    .read_frames(buffer)?
                    != count
                {
                    return Err(invalid("Truncated clip source"));
                }
            } else {
                buffer.fill(0.);
            }
            done += count;
            self.within += count as u64;
            if self.within == segment.frames {
                self.index += 1;
                self.within = 0;
            }
        }
        Ok(done)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mixed_rates_gain_pan_exact_seek_and_unequal_endings() {
        let dir = std::env::temp_dir().join(format!("sonitrelix-mix-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a.wav");
        let b = dir.join("b.wav");
        let mut w = WavWriter::create_float(&a, 24000, 1).unwrap();
        w.write_frames(&[0.2; 4]).unwrap();
        let ai = w.finish().unwrap();
        let mut w = WavWriter::create_float(&b, 48000, 2).unwrap();
        w.write_frames(&[0.1, 0.3].repeat(12)).unwrap();
        let bi = w.finish().unwrap();
        let mix = Timeline::mix(
            48000,
            vec![
                MixTrack {
                    timeline: Timeline::single(a, ai).unwrap(),
                    gain: 2.,
                    pan: -1.,
                },
                MixTrack {
                    timeline: Timeline::single(b, bi).unwrap(),
                    gain: 1.,
                    pan: 0.,
                },
            ],
        )
        .unwrap();
        assert_eq!(mix.info().frames, 12);
        let mut reader = TimelineReader::new(mix);
        let mut output = [0.; 24];
        assert_eq!(reader.read_frames(&mut output).unwrap(), 12);
        for f in output[..16].as_chunks::<2>().0 {
            assert!((f[0] - 0.5).abs() < 0.0001);
            assert!((f[1] - 0.3).abs() < 0.0001);
        }
        for f in output[16..].as_chunks::<2>().0 {
            assert!((f[0] - 0.1).abs() < 0.0001);
        }
        reader.seek_frame(9).unwrap();
        let mut tail = [0.; 6];
        assert_eq!(reader.read_frames(&mut tail).unwrap(), 3);
        assert_eq!(&output[18..], tail);
        drop(reader);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn clipped_sources_and_silence_are_rendered_in_order() {
        let path =
            std::env::temp_dir().join(format!("sonitrelix-timeline-{}.wav", std::process::id()));
        let pcm: Vec<u8> = [1000i16, 2000, 3000, 4000, 5000]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let bytes = crate::tests::wav(16, 1, 1, &pcm);
        std::fs::write(&path, &bytes).unwrap();
        let info = WavReader::open(&path).unwrap().info;
        let source = Source {
            path: path.clone(),
            info,
        };
        let timeline = Timeline::new(
            48000,
            1,
            vec![
                Segment {
                    source: Some(source.clone()),
                    source_start: 1,
                    frames: 3,
                },
                Segment {
                    source: None,
                    source_start: 0,
                    frames: 2,
                },
                Segment {
                    source: Some(source),
                    source_start: 0,
                    frames: 2,
                },
            ],
        )
        .unwrap();
        let controls = LiveMix::new(vec![true]);
        let mix = Timeline::mix(
            48000,
            vec![MixTrack {
                timeline: timeline.clone(),
                gain: 1.,
                pan: -1.,
            }],
        )
        .unwrap()
        .with_live_mix(controls.clone())
        .unwrap();
        let mut live = TimelineReader::new(mix);
        let mut frame = [0.; 2];
        assert_eq!(live.read_frames(&mut frame).unwrap(), 1);
        assert_eq!(frame, [2000. / 32768., 0.]);
        controls.update([false].into_iter());
        assert_eq!(live.read_frames(&mut frame).unwrap(), 1);
        assert_eq!(frame, [0., 0.]);
        controls.update([true].into_iter());
        assert_eq!(live.read_frames(&mut frame).unwrap(), 1);
        assert_eq!(frame, [4000. / 32768., 0.]);
        drop(live);
        let mut r = TimelineReader::new(timeline);
        let mut out = [0f32; 7];
        assert_eq!(r.read_frames(&mut out).unwrap(), 7);
        assert_eq!(
            out,
            [
                2000. / 32768.,
                3000. / 32768.,
                4000. / 32768.,
                0.,
                0.,
                1000. / 32768.,
                2000. / 32768.
            ]
        );
        r.seek_frame(4).unwrap();
        assert_eq!(r.read_frames(&mut out[..3]).unwrap(), 3);
        assert_eq!(out[..3], [0., 1000. / 32768., 2000. / 32768.]);
        drop(r);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        std::fs::remove_file(path).unwrap();
    }
}
