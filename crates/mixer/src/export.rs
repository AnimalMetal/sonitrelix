use super::*;
use sonitrelix_audio_io::ffmpeg;
use std::{
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
};
pub struct Exported {
    pub path: PathBuf,
    pub clipped: u64,
}
#[cfg(windows)]
use sonitrelix_audio_io::codecs::Mp3Writer;
enum Writer {
    Wav(sonitrelix_audio_io::WavWriter),
    Flac(sonitrelix_audio_io::FlacWriter),
    #[cfg(windows)]
    Mp3(Mp3Writer),
}
#[derive(Clone, Copy)]
pub struct Options {
    pub sample_rate: u32,
    pub channels: u16,
    pub mp3_bitrate: u32,
}
/// Exact source-track frames to export independently of the project mute/solo mix.
#[derive(Clone, Copy)]
pub struct ExportRange {
    pub track: usize,
    pub start: u64,
    pub end: u64,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            sample_rate: 0,
            channels: 2,
            mp3_bitrate: 192,
        }
    }
}
impl Writer {
    fn write(&mut self, b: &[f32]) -> io::Result<()> {
        match self {
            Self::Wav(w) => w.write_frames(b),
            Self::Flac(w) => w.write_frames(b),
            #[cfg(windows)]
            Self::Mp3(w) => w.write_frames(b),
        }
    }
    fn finish(self) -> io::Result<()> {
        match self {
            Self::Wav(w) => w.finish().map(|_| ()),
            Self::Flac(w) => w.finish().map(|_| ()),
            #[cfg(windows)]
            Self::Mp3(w) => w.finish(),
        }
    }
}
pub fn export(
    p: &Project,
    path: &Path,
    cancel: &AtomicBool,
    progress: &AtomicU32,
) -> io::Result<Exported> {
    export_with(p, path, Options::default(), cancel, progress)
}
pub fn export_with(
    p: &Project,
    path: &Path,
    options: Options,
    cancel: &AtomicBool,
    progress: &AtomicU32,
) -> io::Result<Exported> {
    export_inner(p, path, options, None, cancel, progress)
}

pub fn export_selection(
    p: &Project,
    path: &Path,
    options: Options,
    range: ExportRange,
    cancel: &AtomicBool,
    progress: &AtomicU32,
) -> io::Result<Exported> {
    export_inner(p, path, options, Some(range), cancel, progress)
}

fn export_inner(
    p: &Project,
    path: &Path,
    options: Options,
    selection: Option<ExportRange>,
    cancel: &AtomicBool,
    progress: &AtomicU32,
) -> io::Result<Exported> {
    if ![1, 2].contains(&options.channels)
        || ![64, 96, 128, 160, 192, 224, 256, 320].contains(&options.mp3_bitrate)
        || (options.sample_rate != 0 && !(8000..=192000).contains(&options.sample_rate))
    {
        return Err(io::Error::other("Invalid export settings"));
    }
    let mp3 = path
        .extension()
        .is_some_and(|s| s.eq_ignore_ascii_case("mp3"));
    let flac = path
        .extension()
        .is_some_and(|s| s.eq_ignore_ascii_case("flac"));
    let external_format = match path
        .extension()
        .and_then(|s| s.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("m4a") => Some(ffmpeg::Format::Aac),
        Some("opus") => Some(ffmpeg::Format::Opus),
        _ => None,
    };
    if !mp3
        && !flac
        && external_format.is_none()
        && !path
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("wav"))
    {
        return Err(io::Error::other(
            "Choose a WAV, FLAC, MP3, M4A or Opus destination",
        ));
    }
    let destination = if path.exists() {
        std::fs::canonicalize(path)?
    } else {
        std::fs::canonicalize(
            path.parent()
                .ok_or_else(|| io::Error::other("Missing destination folder"))?,
        )?
        .join(path.file_name().unwrap())
    };
    for m in p.tracks.iter().flat_map(|t| t.sources()) {
        if std::fs::canonicalize(&m.path).is_ok_and(|s| s == destination) {
            return Err(io::Error::other(
                "Export cannot overwrite project source audio",
            ));
        }
    }
    for source in &p.original_sources {
        if std::fs::canonicalize(source).is_ok_and(|s| s == destination) {
            return Err(io::Error::other(
                "Export cannot overwrite the original imported audio",
            ));
        }
    }
    let mut snapshot = p.clone();
    snapshot.sample_rate = if matches!(external_format, Some(ffmpeg::Format::Opus)) {
        48000
    } else if options.sample_rate != 0 {
        options.sample_rate
    } else if mp3 && ![32000, 44100, 48000].contains(&p.sample_rate) {
        48000
    } else {
        p.sample_rate
    };
    let timeline = if let Some(range) = selection {
        p.validate()?;
        let track = p
            .tracks
            .get(range.track)
            .ok_or_else(|| io::Error::other("Select a track to export"))?;
        Timeline::mix(
            snapshot.sample_rate,
            vec![MixTrack {
                timeline: super::track(track)?.slice(range.start, range.end)?,
                gain: track.gain,
                pan: track.pan,
            }],
        )?
    } else {
        project(&snapshot, None)?
    };
    let info = timeline.info();
    if info.frames == 0 {
        return Err(io::Error::other("No audible audio to export"));
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp = destination.with_extension(format!(
        "export-{}-{stamp}.tmp.{}",
        std::process::id(),
        if mp3 { "mp3" } else { "wav" }
    ));
    let mut external = external_format;
    let mut helper = if external.is_some() {
        Some(require_ffmpeg()?)
    } else {
        None
    };
    let intermediate = temp.with_extension("input.wav");
    let mut writer = if mp3 {
        #[cfg(windows)]
        {
            match Mp3Writer::create(
                &temp,
                info.sample_rate,
                options.channels,
                options.mp3_bitrate,
            ) {
                Ok(writer) => Writer::Mp3(writer),
                Err(native_error) => {
                    let _ = std::fs::remove_file(&temp);
                    helper =
                        Some(require_ffmpeg().map_err(|error| {
                            io::Error::other(format!("{native_error}. {error}"))
                        })?);
                    external = Some(ffmpeg::Format::Mp3);
                    Writer::Wav(sonitrelix_audio_io::WavWriter::create_float(
                        &intermediate,
                        info.sample_rate,
                        options.channels,
                    )?)
                }
            }
        }
        #[cfg(not(windows))]
        {
            return Err(io::Error::other("Windows MP3 encoder required"));
        }
    } else if external.is_some() {
        Writer::Wav(sonitrelix_audio_io::WavWriter::create_float(
            &intermediate,
            info.sample_rate,
            options.channels,
        )?)
    } else if flac {
        Writer::Flac(sonitrelix_audio_io::FlacWriter::create(
            &temp,
            info.sample_rate,
            options.channels,
            info.frames,
        )?)
    } else {
        Writer::Wav(sonitrelix_audio_io::WavWriter::create(
            &temp,
            info.sample_rate,
            options.channels,
        )?)
    };
    let result = (|| {
        let mut reader = sonitrelix_audio_io::TimelineReader::new(timeline);
        let mut buffer = vec![0.; 8192 * 2];
        let mut done = 0u64;
        let mut clipped = 0;
        let mut mono = vec![0f32; 8192];
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "Export cancelled",
                ));
            }
            let n = reader.read_frames(&mut buffer)?;
            if n == 0 {
                break;
            }
            let block = if options.channels == 1 {
                for (i, f) in buffer[..n * 2].as_chunks::<2>().0.iter().enumerate() {
                    mono[i] = (f[0] + f[1]) * 0.5;
                }
                &mono[..n]
            } else {
                &buffer[..n * 2]
            };
            clipped += block.iter().filter(|v| v.abs() > 1.).count() as u64;
            writer.write(block)?;
            done += n as u64;
            let render_percent = if external.is_some() { 75 } else { 100 };
            progress.store(
                (done * render_percent / info.frames) as u32,
                Ordering::Relaxed,
            );
        }
        writer.finish()?;
        if let (Some(exe), Some(format)) = (helper, external) {
            ffmpeg::encode(
                &exe,
                &intermediate,
                &temp,
                format,
                options.mp3_bitrate,
                cancel,
            )?;
        }
        if cancel.load(Ordering::Relaxed) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Export cancelled",
            ));
        }
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&temp)?
            .sync_all()?;
        sonitrelix_session::replace(&temp, path)?;
        progress.store(100, Ordering::Relaxed);
        Ok(Exported {
            path: path.into(),
            clipped,
        })
    })();
    if external.is_some() {
        let _ = std::fs::remove_file(intermediate);
    }
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    result
}

fn require_ffmpeg() -> io::Result<PathBuf> {
    ffmpeg::installed().ok_or_else(|| io::Error::other(
        "FFmpeg is required for this export. Place ffmpeg.exe beside Sonitrelix.exe or install it on PATH.",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Requires FFmpeg on PATH with AAC, libopus, libmp3lame and libvorbis"]
    fn ffmpeg_export_and_audio_extraction_round_trip() {
        let exe = require_ffmpeg().unwrap();
        let dir = std::env::temp_dir().join(format!("sonitrelix-ffmpeg-{}-á", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        let dir = std::fs::canonicalize(dir).unwrap();
        let source = dir.join("input.wav");
        let mut writer = sonitrelix_audio_io::WavWriter::create_float(&source, 44100, 1).unwrap();
        let samples: Vec<_> = (0..4410)
            .map(|i| (i as f32 * 0.06269).sin() * 0.2)
            .collect();
        writer.write_frames(&samples).unwrap();
        let info = writer.finish().unwrap();
        let mut project = Project::default();
        project.add_track("Test".into());
        project.tracks[0].set_media(sonitrelix_session::Media {
            path: source.clone(),
            sample_rate: 44100,
            channels: 1,
            bits: 32,
            frames: info.frames,
            float: true,
        });
        let cancel = AtomicBool::new(false);
        let progress = AtomicU32::new(0);
        for ext in ["m4a", "opus", "mp3"] {
            let encoded = dir.join(format!("mix.{ext}"));
            if ext == "mp3" {
                ffmpeg::encode(&exe, &source, &encoded, ffmpeg::Format::Mp3, 128, &cancel).unwrap();
            } else {
                export_with(
                    &project,
                    &encoded,
                    Options {
                        channels: 1,
                        ..Options::default()
                    },
                    &cancel,
                    &progress,
                )
                .unwrap();
                assert_eq!(progress.load(Ordering::Relaxed), 100);
            }
            let decoded = dir.join(format!("decoded-{ext}.wav"));
            let info = ffmpeg::decode(&exe, &encoded, &decoded, &cancel).unwrap();
            assert_eq!(info.channels, 1);
            assert!(info.frames >= 4000);
            let mut reader = sonitrelix_audio_io::WavReader::open(&decoded).unwrap();
            let mut block = [0.; 8192];
            let count = reader.read_frames(&mut block).unwrap();
            assert!(block[..count].iter().any(|s| s.abs() > 0.1));
        }
        for (ext, args) in [
            ("ogg", vec!["-c:a", "libvorbis"]),
            ("aiff", vec!["-c:a", "pcm_s16be"]),
            (
                "mp4",
                vec![
                    "-f",
                    "lavfi",
                    "-i",
                    "color=s=32x32:d=0.2",
                    "-c:v",
                    "mpeg4",
                    "-c:a",
                    "aac",
                    "-shortest",
                ],
            ),
        ] {
            let encoded = dir.join(format!("input.{ext}"));
            assert!(
                std::process::Command::new(&exe)
                    .args(["-nostdin", "-loglevel", "error", "-n", "-i"])
                    .arg(&source)
                    .args(args)
                    .arg(&encoded)
                    .status()
                    .unwrap()
                    .success()
            );
            let decoded = dir.join(format!("decoded-{ext}.wav"));
            assert!(
                ffmpeg::decode(&exe, &encoded, &decoded, &cancel)
                    .unwrap()
                    .frames
                    >= 4000
            );
        }
        assert!(dir.starts_with(std::fs::canonicalize(std::env::temp_dir()).unwrap()));
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[cfg(windows)]
    #[test]
    fn wav_flac_mp3_export_rate_channels_and_original_protection() {
        let temp = std::env::temp_dir();
        let dir = temp.join(format!("sonitrelix-formats-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("voice.wav");
        let mut w = sonitrelix_audio_io::WavWriter::create_float(&source, 44100, 2).unwrap();
        let samples: Vec<_> = (0..4410)
            .flat_map(|i| {
                let v = (i as f32 * std::f32::consts::TAU * 440. / 44100.).sin() * 0.2;
                [v, v]
            })
            .collect();
        w.write_frames(&samples).unwrap();
        let info = w.finish().unwrap();
        let mut p = Project::default();
        p.add_track("Voice".into());
        p.tracks[0].set_media(sonitrelix_session::Media {
            path: source,
            sample_rate: 44100,
            channels: 2,
            bits: 32,
            frames: info.frames,
            float: true,
        });
        let original = dir.join("original.mp3");
        std::fs::write(&original, b"original compressed source").unwrap();
        p.original_sources.push(original.clone());
        let cancel = AtomicBool::new(false);
        let progress = AtomicU32::new(0);
        assert!(export(&p, &original, &cancel, &progress).is_err());
        assert_eq!(
            std::fs::read(original).unwrap(),
            b"original compressed source"
        );
        for ext in ["wav", "flac", "mp3"] {
            let output = dir.join(format!("mix.{ext}"));
            let options = Options {
                sample_rate: 32000,
                channels: 1,
                mp3_bitrate: 128,
            };
            export_with(&p, &output, options, &cancel, &progress).unwrap();
            let decoded = if ext == "wav" {
                output
            } else {
                let wav = dir.join(format!("{ext}-decoded.wav"));
                sonitrelix_audio_io::codecs::decode(&output, &wav, &cancel).unwrap();
                wav
            };
            let mut r = sonitrelix_audio_io::WavReader::open(&decoded).unwrap();
            assert_eq!(r.info.sample_rate, 32000);
            assert_eq!(r.info.channels, 1);
            if ext == "mp3" {
                assert!(
                    (3200..=3200 + 6912).contains(&r.info.frames),
                    "MP3 codec padding: {}",
                    r.info.frames
                );
            } else {
                assert_eq!(r.info.frames, 3200);
            }
            let mut b = [0.; 4096];
            r.read_frames(&mut b).unwrap();
            assert!(b.iter().any(|v| v.abs() > 0.1));
        }
        assert!(dir.starts_with(temp));
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn export_mixes_without_overwriting_sources_and_cancel_keeps_destination() {
        let temp = std::env::temp_dir();
        let dir = temp.join(format!("sonitrelix-export-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("source.wav");
        let mut w = sonitrelix_audio_io::WavWriter::create_float(&source, 48000, 2).unwrap();
        w.write_frames(&[0.2, 0.4].repeat(50)).unwrap();
        let info = w.finish().unwrap();
        let original = std::fs::read(&source).unwrap();
        let mut p = Project::default();
        for _ in 0..2 {
            p.add_track("copy".into());
            p.tracks
                .last_mut()
                .unwrap()
                .set_media(sonitrelix_session::Media {
                    path: source.clone(),
                    sample_rate: 48000,
                    channels: 2,
                    bits: 32,
                    frames: info.frames,
                    float: true,
                });
        }
        let cancel = AtomicBool::new(false);
        let progress = AtomicU32::new(0);
        assert!(export(&p, &source, &cancel, &progress).is_err());
        assert_eq!(std::fs::read(&source).unwrap(), original);
        let out = dir.join("mix.wav");
        let result = export(&p, &out, &cancel, &progress).unwrap();
        assert_eq!(result.clipped, 0);
        let mut r = sonitrelix_audio_io::WavReader::open(&out).unwrap();
        assert_eq!(r.info.frames, 50);
        let mut b = [0.; 4];
        r.read_frames(&mut b).unwrap();
        assert!((b[0] - 0.4).abs() < 0.0001);
        assert!((b[1] - 0.8).abs() < 0.0001);
        drop(r);
        let prior = std::fs::read(&out).unwrap();
        p.tracks[0].mute = true;
        p.tracks[1].solo = true;
        let selected = dir.join("selected.wav");
        let mut range = ExportRange {
            track: 0,
            start: 7,
            end: 20,
        };
        export_selection(&p, &selected, Options::default(), range, &cancel, &progress).unwrap();
        let mut reader = sonitrelix_audio_io::WavReader::open(&selected).unwrap();
        assert_eq!(reader.info.frames, 13);
        reader.read_frames(&mut b).unwrap();
        assert!((b[0] - 0.2).abs() < 0.0001);
        assert!((b[1] - 0.4).abs() < 0.0001);
        drop(reader);
        range.end = 7;
        assert!(
            export_selection(&p, &selected, Options::default(), range, &cancel, &progress).is_err()
        );
        cancel.store(true, Ordering::Relaxed);
        assert!(export(&p, &out, &cancel, &progress).is_err());
        assert_eq!(std::fs::read(&out).unwrap(), prior);
        assert_eq!(std::fs::read(source).unwrap(), original);
        assert!(dir.starts_with(temp));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
