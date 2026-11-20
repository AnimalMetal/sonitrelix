use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

pub const SCHEMA_VERSION: u32 = 3;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Clip {
    pub source: Option<Media>,
    pub source_start: u64,
    pub frames: u64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Media {
    pub path: PathBuf,
    pub sample_rate: u32,
    pub channels: u16,
    pub bits: u16,
    pub frames: u64,
    pub float: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Track {
    pub name: String,
    pub mute: bool,
    pub solo: bool,
    pub armed: bool,
    pub gain: f32,
    pub pan: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<Media>,
    #[serde(default = "default_rate")]
    pub sample_rate: u32,
    #[serde(default = "default_channels")]
    pub channels: u16,
    #[serde(default)]
    pub clips: Vec<Clip>,
}
fn default_rate() -> u32 {
    48000
}
fn default_channels() -> u16 {
    1
}
impl Track {
    pub fn frames(&self) -> u64 {
        self.clips
            .iter()
            .fold(0u64, |a, c| a.saturating_add(c.frames))
    }
    pub fn set_media(&mut self, media: Media) {
        self.sample_rate = media.sample_rate;
        self.channels = media.channels;
        self.clips = vec![Clip {
            frames: media.frames,
            source: Some(media),
            source_start: 0,
        }];
        self.media = None;
    }
    pub fn sources(&self) -> impl Iterator<Item = &Media> {
        self.clips.iter().filter_map(|c| c.source.as_ref())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub version: u32,
    pub sample_rate: u32,
    pub tracks: Vec<Track>,
}
impl Default for Project {
    fn default() -> Self {
        Self {
            version: SCHEMA_VERSION,
            sample_rate: 48000,
            tracks: vec![],
        }
    }
}
impl Project {
    pub fn add_track(&mut self, name: String) {
        self.tracks.push(Track {
            name,
            mute: false,
            solo: false,
            armed: false,
            gain: 1.0,
            pan: 0.0,
            media: None,
            sample_rate: self.sample_rate,
            channels: 1,
            clips: vec![],
        });
    }
    pub fn load(path: &Path) -> io::Result<Self> {
        let mut p: Self = serde_json::from_slice(&fs::read(path)?)?;
        if p.version == 1 && p.tracks.iter().any(|t| t.media.is_some()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Version 1 does not contain audio references",
            ));
        }
        if [1, 2].contains(&p.version) {
            for track in &mut p.tracks {
                if !track.clips.is_empty() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Legacy project cannot contain timeline clips",
                    ));
                }
                if let Some(media) = track.media.take() {
                    track.set_media(media);
                } else {
                    track.sample_rate = p.sample_rate;
                }
            }
            p.version = SCHEMA_VERSION;
        }
        p.validate()?;
        Ok(p)
    }
    pub fn validate(&self) -> io::Result<()> {
        if self.version != SCHEMA_VERSION
            || !(8000..=384000).contains(&self.sample_rate)
            || self.tracks.iter().any(|t| {
                t.name.trim().is_empty()
                    || !t.gain.is_finite()
                    || t.gain < 0.0
                    || !t.pan.is_finite()
                    || !(-1.0..=1.0).contains(&t.pan)
                    || t.media.is_some()
                    || !(8000..=192000).contains(&t.sample_rate)
                    || ![1, 2].contains(&t.channels)
                    || t.clips.len() > 100000
                    || t.clips
                        .iter()
                        .try_fold(0u64, |sum, c| sum.checked_add(c.frames))
                        .is_none()
                    || t.clips.iter().any(|c| {
                        c.frames == 0
                            || match &c.source {
                                None => c.source_start != 0,
                                Some(m) => {
                                    !valid_media(m)
                                        || m.sample_rate != t.sample_rate
                                        || m.channels != t.channels
                                        || c.source_start
                                            .checked_add(c.frames)
                                            .is_none_or(|end| end > m.frames)
                                }
                            }
                    })
                    || t.media.as_ref().is_some_and(|m| {
                        !m.path.is_absolute()
                            || !(8000..=192000).contains(&m.sample_rate)
                            || ![1, 2].contains(&m.channels)
                            || (m.float && m.bits != 32)
                            || ![8, 16, 24, 32].contains(&m.bits)
                            || m.frames
                                > u64::from(u32::MAX)
                                    / (u64::from(m.channels) * u64::from(m.bits / 8))
                    })
            })
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid or unsupported project",
            ));
        }
        Ok(())
    }
    pub fn save(&self, path: &Path) -> io::Result<()> {
        self.validate()?;
        if !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("sonitrelix"))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Project filename must end in .sonitrelix",
            ));
        }
        if let Ok(destination) = fs::canonicalize(path) {
            for source in self.tracks.iter().flat_map(|t| t.sources()) {
                if fs::canonicalize(&source.path).is_ok_and(|s| s == destination) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "A project cannot overwrite an imported audio source",
                    ));
                }
            }
        }
        atomic_write(path, &serde_json::to_vec_pretty(self)?)
    }
}
fn valid_media(m: &Media) -> bool {
    m.path.is_absolute()
        && (8000..=192000).contains(&m.sample_rate)
        && [1, 2].contains(&m.channels)
        && (!m.float || m.bits == 32)
        && [8, 16, 24, 32].contains(&m.bits)
        && m.frames <= u64::from(u32::MAX) / (u64::from(m.channels) * u64::from(m.bits / 8))
}

/// Write and flush a sibling temporary file, then replace the destination.
/// On Windows MoveFileExW performs replacement without deleting the old file first.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temp = path.with_extension(format!("{}.tmp", std::process::id()));
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    let result = (|| {
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        replace(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}
#[cfg(not(windows))]
fn replace(from: &Path, to: &Path) -> io::Result<()> {
    fs::rename(from, to)
}
#[cfg(windows)]
fn replace(from: &Path, to: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> i32;
    }
    let a: Vec<_> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let b: Vec<_> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    if unsafe { MoveFileExW(a.as_ptr(), b.as_ptr(), 1 | 8) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Integer milliseconds avoid floating point drift in text entry.
pub fn parse_time(text: &str) -> Option<u64> {
    let parts: Vec<_> = text.split([':', '.']).collect();
    if parts.len() != 4
        || parts[0].len() < 2
        || parts[1].len() != 2
        || parts[2].len() != 2
        || parts[3].len() != 3
        || !parts.iter().all(|p| p.bytes().all(|c| c.is_ascii_digit()))
    {
        return None;
    }
    let h: u64 = parts[0].parse().ok()?;
    let m: u64 = parts[1].parse().ok()?;
    let s: u64 = parts[2].parse().ok()?;
    let ms: u64 = parts[3].parse().ok()?;
    if m > 59 || s > 59 {
        return None;
    }
    h.checked_mul(3600000)?
        .checked_add(m * 60000 + s * 1000 + ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn precise_time() {
        assert_eq!(parse_time("00:01:28.500"), Some(88500));
        for t in ["00:60:00.000", "-1:00:00.000", "00:00:00.1", "00:00:60.000"] {
            assert_eq!(parse_time(t), None);
        }
    }
    #[test]
    fn save_reopen_replace_and_reject_future() {
        let dir = std::env::temp_dir().join(format!("sonitrelix-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.sonitrelix");
        let mut p = Project::default();
        p.add_track("Voz ñ".into());
        p.save(&path).unwrap();
        assert_eq!(Project::load(&path).unwrap(), p);
        p.tracks[0].mute = true;
        p.save(&path).unwrap();
        assert_eq!(Project::load(&path).unwrap(), p);
        p.version = 99;
        assert!(p.save(&path).is_err());
        fs::write(&path, serde_json::to_vec(&p).unwrap()).unwrap();
        assert!(Project::load(&path).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn invalid_save_and_stale_temp_preserve_previous_project() {
        let dir = std::env::temp_dir().join(format!("sonitrelix-safety-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("safe.sonitrelix");
        let mut p = Project::default();
        p.add_track("Keep me".into());
        p.save(&path).unwrap();
        let original = fs::read(&path).unwrap();
        p.tracks[0].gain = f32::NAN;
        assert!(p.save(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        p.tracks[0].gain = 1.0;
        let stale = path.with_extension(format!("{}.tmp", std::process::id()));
        fs::write(&stale, b"retained temporary content").unwrap();
        assert!(p.save(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(fs::read(&stale).unwrap(), b"retained temporary content");
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn version_one_migrates_and_media_references_round_trip() {
        let dir =
            std::env::temp_dir().join(format!("sonitrelix-media-project-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("legacy.sonitrelix");
        fs::write(&path,br#"{"version":1,"sample_rate":48000,"tracks":[{"name":"Old track","mute":false,"solo":false,"armed":false,"gain":1.0,"pan":0.0}]}"#).unwrap();
        let mut p = Project::load(&path).unwrap();
        assert_eq!(p.version, SCHEMA_VERSION);
        assert!(p.tracks[0].media.is_none());
        p.tracks[0].set_media(Media {
            path: dir.join("source.wav"),
            sample_rate: 44100,
            channels: 2,
            bits: 24,
            frames: 44100,
            float: false,
        });
        p.save(&path).unwrap();
        assert_eq!(Project::load(&path).unwrap(), p);
        p.tracks[0].clips[0].source.as_mut().unwrap().path = PathBuf::from("relative.wav");
        assert!(p.save(&path).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn project_save_never_overwrites_imported_source() {
        let dir =
            std::env::temp_dir().join(format!("sonitrelix-source-safety-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let source = dir.join("audio.sonitrelix");
        fs::write(&source, b"retained source bytes").unwrap();
        let mut project = Project::default();
        project.add_track("Source".into());
        project.tracks[0].set_media(Media {
            path: source.clone(),
            sample_rate: 48000,
            channels: 1,
            bits: 16,
            frames: 10,
            float: false,
        });
        assert!(project.save(&source).is_err());
        assert_eq!(fs::read(&source).unwrap(), b"retained source bytes");
        assert!(project.save(&dir.join("audio.wav")).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn version_two_media_becomes_an_editable_clip() {
        let dir = std::env::temp_dir().join(format!("sonitrelix-v2-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("old.sonitrelix");
        let mut old = Project {
            version: 2,
            ..Default::default()
        };
        old.add_track("Old WAV".into());
        old.tracks[0].media = Some(Media {
            path: dir.join("old.wav"),
            sample_rate: 44100,
            channels: 2,
            bits: 16,
            frames: 1000,
            float: false,
        });
        fs::write(&path, serde_json::to_vec(&old).unwrap()).unwrap();
        let new = Project::load(&path).unwrap();
        assert_eq!(new.version, 3);
        assert!(new.tracks[0].media.is_none());
        assert_eq!(new.tracks[0].sample_rate, 44100);
        assert_eq!(new.tracks[0].clips[0].frames, 1000);
        new.save(&path).unwrap();
        assert_eq!(Project::load(&path).unwrap(), new);
        fs::remove_dir_all(dir).unwrap();
    }
}
