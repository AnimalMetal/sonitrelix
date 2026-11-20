use super::*;
use std::{fs::OpenOptions, io::Write};
/// Create-new PCM16 writer. Only this newly owned recording file is patched.
pub struct WavWriter {
    file: File,
    rate: u32,
    channels: u16,
    frames: u64,
    bytes: Vec<u8>,
}
impl WavWriter {
    pub fn create(path: &Path, rate: u32, channels: u16) -> io::Result<Self> {
        if !(8000..=192000).contains(&rate) || ![1, 2].contains(&channels) {
            return Err(invalid("Recording format"));
        }
        let file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(path)?;
        let mut s = Self {
            file,
            rate,
            channels,
            frames: 0,
            bytes: Vec::with_capacity(8192 * usize::from(channels) * 2),
        };
        s.file.write_all(&[0; 44])?;
        s.checkpoint()?;
        Ok(s)
    }
    pub fn frames(&self) -> u64 {
        self.frames
    }
    pub fn write_frames(&mut self, samples: &[f32]) -> io::Result<()> {
        if !samples.len().is_multiple_of(usize::from(self.channels)) {
            return Err(invalid("Incomplete recording frame"));
        }
        let frames = samples.len() / usize::from(self.channels);
        if frames > 8192 {
            for chunk in samples.chunks(8192 * usize::from(self.channels)) {
                self.write_frames(chunk)?;
            }
            return Ok(());
        }
        if (self.frames + frames as u64) * u64::from(self.channels) * 2 > u64::from(u32::MAX) - 36 {
            return Err(invalid("RIFF recording size limit reached"));
        }
        self.bytes.clear();
        for v in samples {
            let v = if v.is_finite() { v.clamp(-1., 1.) } else { 0. };
            let s = if v <= -1. {
                i16::MIN
            } else {
                (v * 32767.).round() as i16
            };
            self.bytes.extend_from_slice(&s.to_le_bytes());
        }
        let result = self.file.write_all(&self.bytes);
        let bytes = self.file.stream_position()?.saturating_sub(44);
        self.frames = bytes / (u64::from(self.channels) * 2);
        if result.is_err() {
            let _ = self
                .file
                .set_len(44 + self.frames * u64::from(self.channels) * 2);
        }
        result
    }
    pub fn checkpoint(&mut self) -> io::Result<()> {
        let size = (self.frames * u64::from(self.channels) * 2) as u32;
        let align = self.channels * 2;
        let mut header = vec![];
        header.extend_from_slice(b"RIFF");
        header.extend_from_slice(&(size + 36).to_le_bytes());
        header.extend_from_slice(b"WAVEfmt ");
        header.extend_from_slice(&16u32.to_le_bytes());
        header.extend_from_slice(&1u16.to_le_bytes());
        header.extend_from_slice(&self.channels.to_le_bytes());
        header.extend_from_slice(&self.rate.to_le_bytes());
        header.extend_from_slice(&(self.rate * u32::from(align)).to_le_bytes());
        header.extend_from_slice(&align.to_le_bytes());
        header.extend_from_slice(&16u16.to_le_bytes());
        header.extend_from_slice(b"data");
        header.extend_from_slice(&size.to_le_bytes());
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&header)?;
        self.file.seek(SeekFrom::End(0))?;
        self.file.sync_data()
    }
    pub fn finish(mut self) -> io::Result<WavInfo> {
        self.checkpoint()?;
        Ok(WavInfo {
            sample_rate: self.rate,
            channels: self.channels,
            bits: 16,
            frames: self.frames,
            encoding: Encoding::Pcm,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn writer_roundtrip_and_no_overwrite() {
        let path =
            std::env::temp_dir().join(format!("sonitrelix-writer-{}.wav", std::process::id()));
        let mut w = WavWriter::create(&path, 48000, 2).unwrap();
        w.write_frames(&[-1., 1., 0.5, f32::NAN]).unwrap();
        w.checkpoint().unwrap();
        let info = w.finish().unwrap();
        assert_eq!(info.frames, 2);
        assert!(WavWriter::create(&path, 48000, 2).is_err());
        let mut r = WavReader::open(&path).unwrap();
        let mut out = [0.; 4];
        r.read_frames(&mut out).unwrap();
        assert_eq!(out[0], -1.);
        assert!(out[1] > 0.99);
        assert!((out[2] - 0.5).abs() < 0.0001);
        assert_eq!(out[3], 0.);
        drop(r);
        std::fs::remove_file(path).unwrap();
    }
}
