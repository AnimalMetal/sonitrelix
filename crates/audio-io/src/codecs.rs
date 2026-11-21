//! Windows Media Foundation decoders, run on an import worker. No external codec executable.
use super::*;
use std::{
    os::windows::ffi::OsStrExt,
    sync::atomic::{AtomicBool, Ordering},
};
use windows::{
    Win32::{Media::MediaFoundation::*, System::Com::*},
    core::PCWSTR,
};
struct Platform;
impl Drop for Platform {
    fn drop(&mut self) {
        unsafe {
            let _ = MFShutdown();
            CoUninitialize();
        }
    }
}
fn error(e: windows::core::Error) -> io::Error {
    io::Error::other(e.to_string())
}
pub struct Mp3Writer {
    writer: IMFSinkWriter,
    index: u32,
    rate: u32,
    channels: u16,
    frames: u64,
    _platform: Platform,
}
impl Mp3Writer {
    pub fn create(path: &Path, rate: u32, channels: u16, bitrate: u32) -> io::Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .map_err(error)?;
            if let Err(e) = MFStartup(MF_VERSION, MFSTARTUP_FULL) {
                CoUninitialize();
                return Err(error(e));
            }
            let platform = Platform;
            let types = MFTranscodeGetAudioOutputAvailableTypes(&MFAudioFormat_MP3, 0x3f, None)
                .map_err(error)?;
            let mut output = None;
            use windows::core::Interface;
            for i in 0..types.GetElementCount().map_err(error)? {
                let t: IMFMediaType = types.GetElement(i).map_err(error)?.cast().map_err(error)?;
                if t.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND)
                    .map_err(error)?
                    == rate
                    && t.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS).map_err(error)? == u32::from(channels)
                    && t.GetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND)
                        .map_err(error)?
                        == bitrate * 1000 / 8
                {
                    output = Some(t);
                    break;
                }
            }
            let output = output.ok_or_else(|| {
                invalid("Selected MP3 rate/channels/bitrate is not supported by Windows")
            })?;
            // Claim ownership before Media Foundation opens the output, so source files are never overwritten.
            drop(
                std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(path)?,
            );
            let result = (|| {
                let name: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
                let mut attrs = None;
                MFCreateAttributes(&mut attrs, 1).map_err(error)?;
                let attrs = attrs.unwrap();
                attrs
                    .SetUINT32(&MF_SINK_WRITER_DISABLE_THROTTLING, 1)
                    .map_err(error)?;
                let writer = MFCreateSinkWriterFromURL(PCWSTR(name.as_ptr()), None, &attrs)
                    .map_err(error)?;
                let index = writer.AddStream(&output).map_err(error)?;
                let input = MFCreateMediaType().map_err(error)?;
                input
                    .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)
                    .map_err(error)?;
                input
                    .SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM)
                    .map_err(error)?;
                for (key, v) in [
                    (&MF_MT_AUDIO_SAMPLES_PER_SECOND, rate),
                    (&MF_MT_AUDIO_NUM_CHANNELS, u32::from(channels)),
                    (&MF_MT_AUDIO_BITS_PER_SAMPLE, 16),
                    (&MF_MT_AUDIO_BLOCK_ALIGNMENT, u32::from(channels) * 2),
                    (
                        &MF_MT_AUDIO_AVG_BYTES_PER_SECOND,
                        rate * u32::from(channels) * 2,
                    ),
                ] {
                    input.SetUINT32(key, v).map_err(error)?;
                }
                writer
                    .SetInputMediaType(index, &input, None)
                    .map_err(error)?;
                writer.BeginWriting().map_err(error)?;
                Ok(Self {
                    writer,
                    index,
                    rate,
                    channels,
                    frames: 0,
                    _platform: platform,
                })
            })();
            if result.is_err() {
                let _ = std::fs::remove_file(path);
            }
            result
        }
    }
    pub fn write_frames(&mut self, samples: &[f32]) -> io::Result<()> {
        unsafe {
            if !samples.len().is_multiple_of(self.channels as usize) {
                return Err(invalid("Incomplete MP3 input frame"));
            }
            for block in samples.chunks(8192 * self.channels as usize) {
                let frames = block.len() / self.channels as usize;
                let buffer = MFCreateMemoryBuffer((block.len() * 2) as u32).map_err(error)?;
                let mut data = std::ptr::null_mut();
                buffer.Lock(&mut data, None, None).map_err(error)?;
                let bytes = std::slice::from_raw_parts_mut(data, block.len() * 2);
                for (v, b) in block.iter().zip(bytes.as_chunks_mut::<2>().0.iter_mut()) {
                    let v = if v.is_finite() { v.clamp(-1., 1.) } else { 0. };
                    let s = if v <= -1. {
                        i16::MIN
                    } else {
                        (v * 32767.).round() as i16
                    };
                    b.copy_from_slice(&s.to_le_bytes());
                }
                buffer.Unlock().map_err(error)?;
                buffer
                    .SetCurrentLength((block.len() * 2) as u32)
                    .map_err(error)?;
                let sample = MFCreateSample().map_err(error)?;
                sample.AddBuffer(&buffer).map_err(error)?;
                let start = (u128::from(self.frames) * 10000000 / u128::from(self.rate)) as i64;
                let end = (u128::from(self.frames + frames as u64) * 10000000
                    / u128::from(self.rate)) as i64;
                sample.SetSampleTime(start).map_err(error)?;
                sample.SetSampleDuration(end - start).map_err(error)?;
                self.writer
                    .WriteSample(self.index, &sample)
                    .map_err(error)?;
                self.frames += frames as u64;
            }
            Ok(())
        }
    }
    pub fn finish(mut self) -> io::Result<()> {
        // Windows' MP3 transform retains encoder look-ahead. Supply silent drain frames
        // so very short exports contain enough MPEG frames to decode and retain the tail.
        let silence = vec![0.; 1152 * self.channels as usize];
        for _ in 0..3 {
            self.write_frames(&silence)?;
        }
        unsafe {
            self.writer.NotifyEndOfSegment(self.index).map_err(error)?;
            self.writer.Finalize().map_err(error)
        }
    }
}
pub fn decode(source: &Path, destination: &Path, cancel: &AtomicBool) -> io::Result<WavInfo> {
    unsafe {
        if !source.is_absolute() || !source.is_file() {
            return Err(invalid("Import needs a local audio file"));
        }
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .map_err(error)?;
        if let Err(e) = MFStartup(MF_VERSION, MFSTARTUP_FULL) {
            CoUninitialize();
            return Err(error(e));
        }
        let _platform = Platform;
        let path: Vec<_> = source.as_os_str().encode_wide().chain(Some(0)).collect();
        let reader = MFCreateSourceReaderFromURL(PCWSTR(path.as_ptr()), None).map_err(error)?;
        reader
            .SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)
            .map_err(error)?;
        let stream = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;
        reader.SetStreamSelection(stream, true).map_err(error)?;
        let format = MFCreateMediaType().map_err(error)?;
        format
            .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)
            .map_err(error)?;
        format
            .SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_Float)
            .map_err(error)?;
        reader
            .SetCurrentMediaType(stream, None, &format)
            .map_err(error)?;
        let current = reader.GetCurrentMediaType(stream).map_err(error)?;
        let rate = current
            .GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND)
            .map_err(error)?;
        let channels = current
            .GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS)
            .map_err(error)?;
        if !(8000..=192000).contains(&rate) || ![1, 2].contains(&channels) {
            return Err(invalid(
                "Only mono/stereo audio at 8000–192000 Hz is supported",
            ));
        }
        let mut writer = WavWriter::create_float(destination, rate, channels as u16)?;
        let result = (|| {
            loop {
                if cancel.load(Ordering::Relaxed) {
                    return Err(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "Import cancelled",
                    ));
                }
                let mut flags = 0;
                let mut sample = None;
                reader
                    .ReadSample(stream, 0, None, Some(&mut flags), None, Some(&mut sample))
                    .map_err(error)?;
                if flags & (MF_SOURCE_READERF_ERROR.0 as u32) != 0 {
                    return Err(invalid("Windows audio decoder reported an error"));
                }
                if flags & (MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0 as u32) != 0 {
                    let current = reader.GetCurrentMediaType(stream).map_err(error)?;
                    if current
                        .GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND)
                        .map_err(error)?
                        != rate
                        || current
                            .GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS)
                            .map_err(error)?
                            != channels
                    {
                        return Err(invalid("Audio format changed during import"));
                    }
                }
                if let Some(sample) = sample {
                    let buffer = sample.ConvertToContiguousBuffer().map_err(error)?;
                    let length = buffer.GetCurrentLength().map_err(error)?;
                    if length > 16 * 1024 * 1024 || !length.is_multiple_of(channels * 4) {
                        return Err(invalid("Invalid decoded audio block"));
                    }
                    let mut data = std::ptr::null_mut();
                    let mut length = 0;
                    buffer
                        .Lock(&mut data, None, Some(&mut length))
                        .map_err(error)?;
                    // Copy while locked, then always unlock before disk access or error propagation.
                    let samples = if data.is_null() {
                        vec![]
                    } else {
                        std::slice::from_raw_parts(data, length as usize)
                            .as_chunks::<4>()
                            .0
                            .iter()
                            .map(|v| f32::from_le_bytes(*v))
                            .collect::<Vec<_>>()
                    };
                    let unlock = buffer.Unlock().map_err(error);
                    unlock?;
                    writer.write_frames(&samples)?;
                }
                if flags & (MF_SOURCE_READERF_ENDOFSTREAM.0 as u32) != 0 {
                    break;
                }
            }
            let info = writer.finish()?;
            if info.frames == 0 {
                return Err(invalid("No audio frames decoded"));
            }
            Ok(info)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(destination);
        }
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_mp3_encoder_roundtrips_stereo_audio() {
        let temp = std::env::temp_dir();
        let dir = temp.join(format!("sonitrelix-mp3-encoder-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mp3 = dir.join("encoded.mp3");
        let wav = dir.join("decoded.wav");
        let mut writer = Mp3Writer::create(&mp3, 44100, 2, 192).unwrap();
        let mut b = vec![0.; 4410 * 2];
        for block in 0..10 {
            for (i, f) in b.as_chunks_mut::<2>().0.iter_mut().enumerate() {
                f[0] =
                    ((i + block * 4410) as f32 * std::f32::consts::TAU * 440. / 44100.).sin() * 0.3;
                f[1] = -f[0];
            }
            writer.write_frames(&b).unwrap();
        }
        writer.finish().unwrap();
        let info = decode(&mp3, &wav, &AtomicBool::new(false)).unwrap();
        assert_eq!(info.sample_rate, 44100);
        assert_eq!(info.channels, 2);
        assert!(
            (44100..=44100 + 6912).contains(&info.frames),
            "MP3 includes encoder drain and frame padding: {}",
            info.frames
        );
        let mut r = WavReader::open(&wav).unwrap();
        r.read_frames(&mut b).unwrap();
        assert!(b.iter().any(|v| v.abs() > 0.1));
        drop(r);
        assert!(dir.starts_with(temp));
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn m4a_aac_roundtrip_using_windows_encoder_and_decoder() {
        unsafe {
            let temp = std::env::temp_dir();
            let dir = temp.join(format!("sonitrelix-aac-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let source = dir.join("native.m4a");
            let output = dir.join("decoded.wav");
            CoInitializeEx(None, COINIT_MULTITHREADED).ok().unwrap();
            MFStartup(MF_VERSION, MFSTARTUP_FULL).unwrap();
            let platform = Platform;
            let path: Vec<_> = source.as_os_str().encode_wide().chain(Some(0)).collect();
            let writer = MFCreateSinkWriterFromURL(PCWSTR(path.as_ptr()), None, None).unwrap();
            let format = MFCreateMediaType().unwrap();
            format
                .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)
                .unwrap();
            format.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_AAC).unwrap();
            for (key, value) in [
                (&MF_MT_AUDIO_SAMPLES_PER_SECOND, 44100),
                (&MF_MT_AUDIO_NUM_CHANNELS, 2),
                (&MF_MT_AUDIO_BITS_PER_SAMPLE, 16),
                (&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, 16000),
                (&MF_MT_AAC_PAYLOAD_TYPE, 0),
                (&MF_MT_AAC_AUDIO_PROFILE_LEVEL_INDICATION, 0x29),
            ] {
                format.SetUINT32(key, value).unwrap();
            }
            let index = writer.AddStream(&format).unwrap();
            let input = MFCreateMediaType().unwrap();
            input
                .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)
                .unwrap();
            input.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM).unwrap();
            for (key, value) in [
                (&MF_MT_AUDIO_SAMPLES_PER_SECOND, 44100),
                (&MF_MT_AUDIO_NUM_CHANNELS, 2),
                (&MF_MT_AUDIO_BITS_PER_SAMPLE, 16),
                (&MF_MT_AUDIO_BLOCK_ALIGNMENT, 4),
                (&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, 176400),
            ] {
                input.SetUINT32(key, value).unwrap();
            }
            writer.SetInputMediaType(index, &input, None).unwrap();
            writer.BeginWriting().unwrap();
            for block in 0..10 {
                let n = 4410;
                let bytes = n * 4;
                let buffer = MFCreateMemoryBuffer(bytes).unwrap();
                let mut data = std::ptr::null_mut();
                buffer.Lock(&mut data, None, None).unwrap();
                let samples = std::slice::from_raw_parts_mut(data.cast::<i16>(), n as usize * 2);
                for (i, f) in samples.as_chunks_mut::<2>().0.iter_mut().enumerate() {
                    let v =
                        (((i + block as usize * n as usize) as f32 * std::f32::consts::TAU * 440.
                            / 44100.)
                            .sin()
                            * 10000.) as i16;
                    f[0] = v;
                    f[1] = -v;
                }
                buffer.Unlock().unwrap();
                buffer.SetCurrentLength(bytes).unwrap();
                let sample = MFCreateSample().unwrap();
                sample.AddBuffer(&buffer).unwrap();
                sample.SetSampleTime(i64::from(block) * 1000000).unwrap();
                sample.SetSampleDuration(1000000).unwrap();
                writer.WriteSample(index, &sample).unwrap();
            }
            writer.Finalize().unwrap();
            drop(writer);
            drop(input);
            drop(format);
            drop(platform);
            let info = decode(&source, &output, &AtomicBool::new(false)).unwrap();
            assert_eq!(info.sample_rate, 44100);
            assert_eq!(info.channels, 2);
            assert!((42000..47000).contains(&info.frames));
            let mut r = WavReader::open(&output).unwrap();
            let mut b = [0.; 8192];
            let n = r.read_frames(&mut b).unwrap();
            assert!(n > 0);
            assert!(b.iter().any(|v| v.abs() > 0.1));
            drop(r);
            assert!(dir.starts_with(temp));
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
    #[test]
    fn native_mp3_decoder_reads_silent_mpeg_one_frames() {
        let dir = std::env::temp_dir().join(format!("sonitrelix-mp3-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("silence.mp3");
        let output = dir.join("decoded.wav");
        let mut frame = vec![0u8; 417];
        frame[..4].copy_from_slice(&[0xff, 0xfb, 0x90, 0x00]);
        let bytes = frame.repeat(20);
        std::fs::write(&source, &bytes).unwrap();
        let info = decode(&source, &output, &AtomicBool::new(false)).unwrap();
        assert_eq!(info.sample_rate, 44100);
        assert_eq!(info.channels, 2);
        assert!((10000..=24000).contains(&info.frames));
        let mut r = WavReader::open(&output).unwrap();
        let mut block = [0.; 4096];
        let n = r.read_frames(&mut block).unwrap();
        assert!(n > 0);
        assert!(block.iter().all(|v| v.abs() < 0.0001));
        drop(r);
        assert_eq!(std::fs::read(source).unwrap(), bytes);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn windows_decoder_preserves_wav_samples_without_hardware() {
        let dir = std::env::temp_dir().join(format!("sonitrelix-codec-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("source.wav");
        let output = dir.join("decoded.wav");
        let mut writer = WavWriter::create(&source, 48000, 2).unwrap();
        writer.write_frames(&[0.25, -0.25, 0.5, -0.5]).unwrap();
        writer.finish().unwrap();
        let bytes = std::fs::read(&source).unwrap();
        let info = decode(&source, &output, &AtomicBool::new(false)).unwrap();
        assert_eq!(info.frames, 2);
        let mut r = WavReader::open(&output).unwrap();
        let mut b = [0.; 4];
        r.read_frames(&mut b).unwrap();
        assert!((b[0] - 0.25).abs() < 0.001);
        assert!((b[3] + 0.5).abs() < 0.001);
        drop(r);
        assert_eq!(std::fs::read(source).unwrap(), bytes);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
