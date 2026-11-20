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
}
pub struct TimelineReader {
    timeline: Timeline,
    index: usize,
    within: u64,
    reader: Option<WavReader>,
    reader_source: Option<Source>,
}
impl TimelineReader {
    pub fn new(timeline: Timeline) -> Self {
        Self {
            timeline,
            index: 0,
            within: 0,
            reader: None,
            reader_source: None,
        }
    }
    pub fn info(&self) -> WavInfo {
        self.timeline.info()
    }
    pub fn seek_frame(&mut self, frame: u64) -> io::Result<u64> {
        let target = frame.min(self.timeline.format.frames);
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
