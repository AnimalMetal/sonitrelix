//! Bounded, non-destructive RIFF/WAVE PCM reader. No whole-file sample allocation.
mod flac;
mod timeline;
mod writer;
pub use flac::FlacWriter;
#[cfg(windows)]
pub mod codecs;
pub mod ffmpeg;
use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    path::Path,
};
pub use timeline::{LiveMix, MixTrack, Segment, Source, Timeline, TimelineReader};
pub use writer::{WavWriter, recover_pcm16_recording};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Pcm,
    Float,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WavInfo {
    pub sample_rate: u32,
    pub channels: u16,
    pub bits: u16,
    pub frames: u64,
    pub encoding: Encoding,
}
impl WavInfo {
    pub fn duration_ms(&self) -> u64 {
        frames_to_ms(self.frames, self.sample_rate)
    }
}
pub fn frames_to_ms(frames: u64, rate: u32) -> u64 {
    ((frames as u128 * 1000) / u128::from(rate.max(1))).min(u128::from(u64::MAX)) as u64
}
pub fn ms_to_frames(ms: u64, rate: u32) -> u64 {
    ((ms as u128 * u128::from(rate)) / 1000).min(u128::from(u64::MAX)) as u64
}
pub struct WavReader<R = File> {
    source: R,
    pub info: WavInfo,
    data_start: u64,
    frame: u64,
    raw: Vec<u8>,
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn u16le(b: &[u8]) -> u16 {
    u16::from_le_bytes([b[0], b[1]])
}
fn u32le(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}
impl WavReader<File> {
    pub fn open(path: &Path) -> io::Result<Self> {
        Self::new(File::open(path)?)
    }
}
impl<R: Read + Seek> WavReader<R> {
    pub fn new(mut source: R) -> io::Result<Self> {
        let length = source.seek(SeekFrom::End(0))?;
        source.seek(SeekFrom::Start(0))?;
        let mut header = [0u8; 12];
        source.read_exact(&mut header)?;
        if &header[0..4] != b"RIFF" || &header[8..12] != b"WAVE" {
            return Err(invalid(
                "Supported input is little-endian RIFF/WAVE PCM or IEEE float, mono or stereo.",
            ));
        }
        let limit = u64::from(u32le(&header[4..8])) + 8;
        if limit > length || limit < 12 {
            return Err(invalid("Truncated or invalid RIFF length"));
        }
        let mut format = None;
        let mut data = None;
        let mut position = 12u64;
        let mut headers = 0u32;
        // Scan headers, seeking across unknown chunks without allocating their contents.
        while position.checked_add(8).is_some_and(|end| end <= limit) {
            headers += 1;
            if headers > 65536 {
                return Err(invalid("Too many WAV chunks"));
            }
            source.seek(SeekFrom::Start(position))?;
            let mut chunk = [0u8; 8];
            source.read_exact(&mut chunk)?;
            let size = u64::from(u32le(&chunk[4..8]));
            let start = position + 8;
            let end = start
                .checked_add(size)
                .ok_or_else(|| invalid("WAV chunk overflow"))?;
            if end > limit {
                return Err(invalid("WAV chunk exceeds RIFF length"));
            }
            match &chunk[0..4] {
                b"fmt " => {
                    if format.is_some() || size < 16 {
                        return Err(invalid("Missing or duplicate WAV format"));
                    }
                    let mut f = [0u8; 40];
                    let bytes = (size.min(40)) as usize;
                    source.read_exact(&mut f[..bytes])?;
                    let mut tag = u16le(&f[..2]);
                    let channels = u16le(&f[2..4]);
                    let rate = u32le(&f[4..8]);
                    let bits = u16le(&f[14..16]);
                    if tag == 0xfffe {
                        if size < 40 || u16le(&f[16..18]) < 22 || u16le(&f[18..20]) != bits {
                            return Err(invalid("Unsupported WAVE extensible valid-bit format"));
                        }
                        let mask = u32le(&f[20..24]);
                        if ![0, if channels == 1 { 4 } else { 3 }].contains(&mask) {
                            return Err(invalid("Unsupported WAV speaker layout"));
                        }
                        let suffix = [0u8, 0, 0x10, 0, 0x80, 0, 0, 0xaa, 0, 0x38, 0x9b, 0x71];
                        if f[28..40] != suffix || u16le(&f[26..28]) != 0 {
                            return Err(invalid("Unsupported WAV extensible subformat"));
                        }
                        tag = u16le(&f[24..26]);
                    }
                    let encoding = match (tag, bits) {
                        (1, 8 | 16 | 24 | 32) => Encoding::Pcm,
                        (3, 32) => Encoding::Float,
                        _ => {
                            return Err(invalid(
                                "Supported WAV encodings: PCM 8/16/24/32-bit and IEEE float32.",
                            ));
                        }
                    };
                    if ![1, 2].contains(&channels) || !(8000..=192000).contains(&rate) {
                        return Err(invalid("WAV must be mono/stereo at 8000–192000 Hz"));
                    }
                    let align = channels * (bits / 8);
                    if u16le(&f[12..14]) != align || u32le(&f[8..12]) != rate * u32::from(align) {
                        return Err(invalid("Inconsistent WAV block alignment or byte rate"));
                    }
                    format = Some(WavInfo {
                        sample_rate: rate,
                        channels,
                        bits,
                        frames: 0,
                        encoding,
                    });
                }
                b"data" => {
                    if data.is_some() {
                        return Err(invalid("Multiple WAV data chunks are not supported"));
                    }
                    data = Some((start, size));
                }
                _ => (),
            }
            position = end
                .checked_add(size % 2)
                .ok_or_else(|| invalid("WAV padding overflow"))?;
        }
        let mut info = format.ok_or_else(|| invalid("WAV has no fmt chunk"))?;
        let (data_start, size) = data.ok_or_else(|| invalid("WAV has no data chunk"))?;
        let align = u64::from(info.channels) * u64::from(info.bits / 8);
        if size % align != 0 {
            return Err(invalid("Incomplete WAV audio frame"));
        }
        info.frames = size / align;
        source.seek(SeekFrom::Start(data_start))?;
        Ok(Self {
            source,
            info,
            data_start,
            frame: 0,
            raw: vec![0; 8192 * usize::from(info.channels) * usize::from(info.bits / 8)],
        })
    }
    pub fn seek_frame(&mut self, frame: u64) -> io::Result<u64> {
        let frame = frame.min(self.info.frames);
        let align = u64::from(self.info.channels) * u64::from(self.info.bits / 8);
        self.source
            .seek(SeekFrom::Start(self.data_start + frame * align))?;
        self.frame = frame;
        Ok(frame)
    }
    /// Decode at most 8192 frames per call into caller-owned interleaved storage.
    pub fn read_frames(&mut self, output: &mut [f32]) -> io::Result<usize> {
        let channels = usize::from(self.info.channels);
        let bytes = usize::from(self.info.bits / 8);
        let frames = (output.len() / channels)
            .min(8192)
            .min((self.info.frames - self.frame) as usize);
        self.source
            .read_exact(&mut self.raw[..frames * channels * bytes])?;
        for (out, sample) in output[..frames * channels]
            .iter_mut()
            .zip(self.raw[..frames * channels * bytes].chunks_exact(bytes))
        {
            let value = match (self.info.encoding, bytes) {
                (Encoding::Float, 4) => {
                    f32::from_le_bytes(sample.try_into().map_err(|_| invalid("Float sample"))?)
                }
                (_, 1) => (f32::from(sample[0]) - 128.0) / 128.0,
                (_, 2) => f32::from(i16::from_le_bytes([sample[0], sample[1]])) / 32768.0,
                (_, 3) => {
                    let v = i32::from_le_bytes([0, sample[0], sample[1], sample[2]]) >> 8;
                    v as f32 / 8388608.0
                }
                (_, 4) => {
                    i32::from_le_bytes(sample.try_into().map_err(|_| invalid("PCM sample"))?) as f32
                        / 2147483648.0
                }
                _ => return Err(invalid("Unsupported PCM sample size")),
            };
            *out = if value.is_finite() {
                value.clamp(-1.0, 1.0)
            } else {
                0.0
            };
        }
        self.frame += frames as u64;
        Ok(frames)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::io::Cursor;
    pub fn wav(bits: u16, tag: u16, channels: u16, samples: &[u8]) -> Vec<u8> {
        let align = channels * bits / 8;
        let mut b = vec![];
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + samples.len() as u32).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&tag.to_le_bytes());
        b.extend_from_slice(&channels.to_le_bytes());
        b.extend_from_slice(&48000u32.to_le_bytes());
        b.extend_from_slice(&(48000 * u32::from(align)).to_le_bytes());
        b.extend_from_slice(&align.to_le_bytes());
        b.extend_from_slice(&bits.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&(samples.len() as u32).to_le_bytes());
        b.extend_from_slice(samples);
        b
    }
    #[test]
    fn pcm_formats_seek_and_channel_order() {
        for (bits, bytes) in [
            (8, vec![0, 128, 255, 128]),
            (16, vec![0, 128, 0, 0, 255, 127, 0, 0]),
            (24, vec![0, 0, 128, 0, 0, 0, 255, 255, 127, 0, 0, 0]),
            (
                32,
                vec![0, 0, 0, 128, 0, 0, 0, 0, 255, 255, 255, 127, 0, 0, 0, 0],
            ),
        ] {
            let mut r = WavReader::new(Cursor::new(wav(bits, 1, 2, &bytes))).unwrap();
            let mut output = [0.; 4];
            assert_eq!(r.read_frames(&mut output).unwrap(), 2);
            assert_eq!(output[0], -1.);
            assert_eq!(output[1], 0.);
            assert!(output[2] > 0.99);
            assert_eq!(output[3], 0.);
            r.seek_frame(1).unwrap();
            assert_eq!(r.read_frames(&mut output[..2]).unwrap(), 1);
            assert!(output[0] > 0.99);
            assert_eq!(r.seek_frame(u64::MAX).unwrap(), 2);
            assert_eq!(r.read_frames(&mut output).unwrap(), 0);
        }
    }
    #[test]
    fn float_sanitized_and_bad_files_rejected() {
        let bytes: Vec<_> = [f32::NAN, f32::INFINITY, -2.0, 0.5]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let mut r = WavReader::new(Cursor::new(wav(32, 3, 1, &bytes))).unwrap();
        let mut out = [0.; 4];
        r.read_frames(&mut out).unwrap();
        assert_eq!(out, [0., 0., -1., 0.5]);
        let mut bad = wav(16, 1, 1, &[0, 0]);
        bad.pop();
        assert!(WavReader::new(Cursor::new(bad)).is_err());
        assert!(WavReader::new(Cursor::new(wav(16, 6, 1, &[0, 0]))).is_err());
        assert!(WavReader::new(Cursor::new(wav(16, 1, 3, &[0; 6]))).is_err());
    }
    #[test]
    fn odd_chunks_and_integer_timing() {
        let mut b = wav(16, 1, 1, &[0, 0]);
        b.splice(12..12, b"JUNK\x01\0\0\0x\0".iter().copied());
        let n = b.len() as u32 - 8;
        b[4..8].copy_from_slice(&n.to_le_bytes());
        assert_eq!(WavReader::new(Cursor::new(b)).unwrap().info.frames, 1);
        assert_eq!(ms_to_frames(88500, 44100), 3902850);
        assert_eq!(frames_to_ms(3902850, 44100), 88500);
    }
    #[test]
    fn extensible_pcm_and_float() {
        for (bits, tag, samples) in [(16, 1, vec![0u8; 4]), (32, 3, vec![0u8; 8])] {
            let mut b = wav(bits, tag, 1, &samples);
            let mut extension = vec![];
            extension.extend_from_slice(&22u16.to_le_bytes());
            extension.extend_from_slice(&bits.to_le_bytes());
            extension.extend_from_slice(&4u32.to_le_bytes());
            extension.extend_from_slice(&u32::from(tag).to_le_bytes());
            extension.extend_from_slice(&[0, 0, 0x10, 0, 0x80, 0, 0, 0xaa, 0, 0x38, 0x9b, 0x71]);
            b.splice(36..36, extension);
            b[16..20].copy_from_slice(&40u32.to_le_bytes());
            b[20..22].copy_from_slice(&0xfffeu16.to_le_bytes());
            let size = b.len() as u32 - 8;
            b[4..8].copy_from_slice(&size.to_le_bytes());
            let mut r = WavReader::new(Cursor::new(b)).unwrap();
            assert_eq!(r.info.frames, 2);
            let mut out = [1.; 2];
            assert_eq!(r.read_frames(&mut out).unwrap(), 2);
            assert_eq!(out, [0.; 2]);
        }
    }
    #[test]
    fn large_virtual_file_stays_bounded() {
        struct Sparse {
            header: Vec<u8>,
            position: u64,
            length: u64,
        }
        impl Read for Sparse {
            fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
                let n = b
                    .len()
                    .min(self.length.saturating_sub(self.position) as usize);
                b[..n].fill(0);
                for (i, v) in b[..n].iter_mut().enumerate() {
                    if let Some(source) = self.header.get(self.position as usize + i) {
                        *v = *source;
                    }
                }
                self.position += n as u64;
                Ok(n)
            }
        }
        impl Seek for Sparse {
            fn seek(&mut self, s: SeekFrom) -> io::Result<u64> {
                let p = match s {
                    SeekFrom::Start(n) => i128::from(n),
                    SeekFrom::End(n) => i128::from(self.length) + i128::from(n),
                    SeekFrom::Current(n) => i128::from(self.position) + i128::from(n),
                };
                self.position = u64::try_from(p).map_err(|_| invalid("Seek"))?;
                Ok(self.position)
            }
        }
        let size = 2_000_000_000u32;
        let mut header = wav(16, 1, 1, &[]);
        header[4..8].copy_from_slice(&(size + 36).to_le_bytes());
        header[40..44].copy_from_slice(&size.to_le_bytes());
        let mut r = WavReader::new(Sparse {
            header,
            position: 0,
            length: u64::from(size) + 44,
        })
        .unwrap();
        assert_eq!(r.info.frames, 1_000_000_000);
        assert!(r.raw.len() <= 65536);
        r.seek_frame(999_999_990).unwrap();
        let mut out = [1.; 32];
        assert_eq!(r.read_frames(&mut out).unwrap(), 10);
        assert_eq!(out[..10], [0.; 10]);
    }
}
