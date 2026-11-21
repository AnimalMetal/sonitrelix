//! Original FLAC stream writer using constant or verbatim subframes (RFC 9639).
//! Lossless PCM16; deliberately no compression dependency or external process.
use super::*;
use std::io::Write;
fn crc8(bytes: &[u8]) -> u8 {
    let mut crc = 0u8;
    for b in bytes {
        crc ^= b;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ 7
            } else {
                crc << 1
            };
        }
    }
    crc
}
fn crc16(bytes: &[u8]) -> u16 {
    let mut crc = 0u16;
    for b in bytes {
        crc ^= u16::from(*b) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x8005
            } else {
                crc << 1
            };
        }
    }
    crc
}
fn utf8_number(n: u32) -> Vec<u8> {
    if n < 128 {
        return vec![n as u8];
    }
    let count = if n < 1 << 11 {
        2
    } else if n < 1 << 16 {
        3
    } else if n < 1 << 21 {
        4
    } else if n < 1 << 26 {
        5
    } else {
        6
    };
    let mut out = vec![0; count];
    let mut x = n;
    for i in (1..count).rev() {
        out[i] = 0x80 | (x as u8 & 63);
        x >>= 6;
    }
    out[0] = (!0u8 << (8 - count)) | x as u8;
    out
}
pub struct FlacWriter {
    file: File,
    rate: u32,
    channels: u16,
    total: u64,
    written: u64,
    index: u32,
    pending: Vec<i16>,
}
impl FlacWriter {
    pub fn create(path: &Path, rate: u32, channels: u16, total: u64) -> io::Result<Self> {
        if !(8000..=192000).contains(&rate)
            || ![1, 2].contains(&channels)
            || total >= 1 << 36
            || total == 0
        {
            return Err(invalid("FLAC output format or duration"));
        }
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        file.write_all(b"fLaC\x80\x00\x00\x22")?;
        let block = total.clamp(16, 4096) as u16;
        let mut stream = vec![];
        stream.extend_from_slice(&block.to_be_bytes());
        stream.extend_from_slice(&block.to_be_bytes());
        stream.extend_from_slice(&[0; 6]);
        let packed =
            (u64::from(rate) << 44) | (u64::from(channels - 1) << 41) | (15u64 << 36) | total;
        stream.extend_from_slice(&packed.to_be_bytes());
        // All-zero STREAMINFO MD5 denotes that no MD5 was computed, as specified by FLAC.
        stream.extend_from_slice(&[0; 16]);
        file.write_all(&stream)?;
        Ok(Self {
            file,
            rate,
            channels,
            total,
            written: 0,
            index: 0,
            pending: Vec::with_capacity(4096 * channels as usize),
        })
    }
    pub fn write_frames(&mut self, samples: &[f32]) -> io::Result<()> {
        if !samples.len().is_multiple_of(self.channels as usize) {
            return Err(invalid("Incomplete FLAC frame"));
        }
        for frame in samples.chunks_exact(self.channels as usize) {
            for v in frame {
                let v = if v.is_finite() { v.clamp(-1., 1.) } else { 0. };
                self.pending.push(if v <= -1. {
                    i16::MIN
                } else {
                    (v * 32767.).round() as i16
                });
            }
            if self.pending.len() == 4096 * self.channels as usize {
                self.flush_block()?;
            }
        }
        Ok(())
    }
    fn flush_block(&mut self) -> io::Result<()> {
        let frames = self.pending.len() / self.channels as usize;
        if frames == 0 {
            return Ok(());
        }
        if self.written + frames as u64 > self.total {
            return Err(invalid("FLAC length exceeded"));
        }
        let mut data = vec![0xff, 0xf8, 0x70, (((self.channels - 1) as u8) << 4) | 8];
        data.extend(utf8_number(self.index));
        data.extend_from_slice(&((frames - 1) as u16).to_be_bytes());
        data.push(crc8(&data));
        for c in 0..self.channels as usize {
            let first = self.pending[c];
            let constant = self
                .pending
                .iter()
                .skip(c)
                .step_by(self.channels as usize)
                .all(|v| *v == first);
            data.push(if constant { 0 } else { 2 });
            if constant {
                data.extend_from_slice(&first.to_be_bytes());
            } else {
                for v in self.pending.iter().skip(c).step_by(self.channels as usize) {
                    data.extend_from_slice(&v.to_be_bytes());
                }
            }
        }
        data.extend_from_slice(&crc16(&data).to_be_bytes());
        self.file.write_all(&data)?;
        self.written += frames as u64;
        self.index += 1;
        self.pending.clear();
        Ok(())
    }
    pub fn finish(mut self) -> io::Result<WavInfo> {
        self.flush_block()?;
        if self.written != self.total {
            return Err(invalid("Incomplete FLAC output"));
        }
        self.file.sync_all()?;
        Ok(WavInfo {
            sample_rate: self.rate,
            channels: self.channels,
            bits: 16,
            frames: self.total,
            encoding: Encoding::Pcm,
        })
    }
}
#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn flac_roundtrip_through_independent_windows_decoder() {
        let dir = std::env::temp_dir().join(format!("sonitrelix-flac-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for channels in [1u16, 2] {
            let source = dir.join(format!("{channels}.flac"));
            let decoded = dir.join(format!("{channels}.wav"));
            let mut samples = vec![0f32; 9001 * channels as usize];
            for (i, frame) in samples.chunks_exact_mut(channels as usize).enumerate() {
                if i >= 4096 {
                    frame[0] = ((i as f32) * 0.1).sin() * 0.5;
                    if channels == 2 {
                        frame[1] = -frame[0];
                    }
                }
            }
            let mut w = FlacWriter::create(&source, 44100, channels, 9001).unwrap();
            for block in samples.chunks(237 * channels as usize) {
                w.write_frames(block).unwrap();
            }
            w.finish().unwrap();
            let info = crate::codecs::decode(
                &source,
                &decoded,
                &std::sync::atomic::AtomicBool::new(false),
            )
            .unwrap();
            assert_eq!(info.frames, 9001);
            assert_eq!(info.channels, channels);
            assert_eq!(info.sample_rate, 44100);
            let mut reader = WavReader::open(&decoded).unwrap();
            let mut output = vec![];
            let mut block = vec![0.; 8192 * channels as usize];
            loop {
                let n = reader.read_frames(&mut block).unwrap();
                if n == 0 {
                    break;
                }
                output.extend_from_slice(&block[..n * channels as usize]);
            }
            for (a, b) in samples.iter().zip(output) {
                assert!((*a - b).abs() < 0.0001);
            }
            drop(reader);
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
