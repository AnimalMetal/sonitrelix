//! Pure, source-preserving edits in integer track frames. Callers commit one undo transaction.
use sonitrelix_session::{Clip, Track};
#[derive(Clone, Debug, PartialEq)]
pub struct Clipboard {
    pub sample_rate: u32,
    pub channels: u16,
    pub clips: Vec<Clip>,
}
impl Clipboard {
    pub fn frames(&self) -> u64 {
        self.clips.iter().map(|c| c.frames).sum()
    }
}
pub fn range(track: &Track, start: u64, end: u64) -> Result<(u64, u64), &'static str> {
    let a = start.min(track.frames());
    let b = end.min(track.frames());
    if b <= a {
        Err("edit_empty_selection")
    } else {
        Ok((a, b))
    }
}
pub fn pieces(track: &Track, start: u64, end: u64) -> Vec<Clip> {
    let mut result = vec![];
    let mut position = 0u64;
    for c in &track.clips {
        let end_clip = position + c.frames;
        let a = start.max(position);
        let b = end.min(end_clip);
        if b > a {
            let mut part = c.clone();
            part.frames = b - a;
            if part.source.is_some() {
                part.source_start += a - position;
            }
            result.push(part);
        }
        position = end_clip;
    }
    result
}
pub fn copy(track: &Track, start: u64, end: u64) -> Result<Clipboard, &'static str> {
    let (a, b) = range(track, start, end)?;
    Ok(Clipboard {
        sample_rate: track.sample_rate,
        channels: track.channels,
        clips: pieces(track, a, b),
    })
}
pub fn delete(track: &mut Track, start: u64, end: u64) -> Result<(), &'static str> {
    let (a, b) = range(track, start, end)?;
    let mut clips = pieces(track, 0, a);
    clips.extend(pieces(track, b, track.frames()));
    track.clips = clips;
    Ok(())
}
pub fn paste(
    track: &mut Track,
    cursor: u64,
    selection: (u64, u64),
    clipboard: &Clipboard,
) -> Result<(u64, u64), &'static str> {
    if clipboard.clips.is_empty() {
        return Err("edit_empty_clipboard");
    }
    if !track.clips.is_empty()
        && (track.sample_rate != clipboard.sample_rate || track.channels != clipboard.channels)
    {
        return Err("edit_format_mismatch");
    }
    let (a, b) = if selection.1 > selection.0 {
        (
            selection.0.min(track.frames()),
            selection.1.min(track.frames()),
        )
    } else {
        (cursor, cursor)
    };
    let inserted = clipboard.frames();
    let end = a.checked_add(inserted).ok_or("edit_too_long")?;
    end.checked_add(track.frames().saturating_sub(b))
        .ok_or("edit_too_long")?;
    let mut clips = pieces(track, 0, a);
    if a > track.frames() {
        clips.push(Clip {
            source: None,
            source_start: 0,
            frames: a - track.frames(),
        });
    }
    clips.extend(clipboard.clips.clone());
    clips.extend(pieces(track, b, track.frames()));
    if clips.len() > 100000 {
        return Err("edit_too_long");
    }
    track.sample_rate = clipboard.sample_rate;
    track.channels = clipboard.channels;
    track.clips = clips;
    Ok((a, end))
}
pub fn split(track: &mut Track, cursor: u64) -> Result<(), &'static str> {
    let mut position = 0;
    for index in 0..track.clips.len() {
        let end = position + track.clips[index].frames;
        if position < cursor && cursor < end {
            let mut right = track.clips[index].clone();
            let left = cursor - position;
            right.frames -= left;
            if right.source.is_some() {
                right.source_start += left;
            }
            track.clips[index].frames = left;
            track.clips.insert(index + 1, right);
            return Ok(());
        }
        position = end;
    }
    Err("edit_split_boundary")
}
pub fn trim(track: &mut Track, start: u64, end: u64) -> Result<(), &'static str> {
    let (a, b) = range(track, start, end)?;
    track.clips = pieces(track, a, b);
    Ok(())
}
pub fn silence(track: &mut Track, start: u64, end: u64) -> Result<(), &'static str> {
    let (a, b) = range(track, start, end)?;
    let mut clips = pieces(track, 0, a);
    clips.push(Clip {
        source: None,
        source_start: 0,
        frames: b - a,
    });
    clips.extend(pieces(track, b, track.frames()));
    track.clips = clips;
    Ok(())
}
pub fn insert_silence(track: &mut Track, cursor: u64, frames: u64) -> Result<(), &'static str> {
    if frames == 0 {
        return Err("edit_empty_selection");
    }
    let clipboard = Clipboard {
        sample_rate: track.sample_rate,
        channels: track.channels,
        clips: vec![Clip {
            source: None,
            source_start: 0,
            frames,
        }],
    };
    paste(track, cursor, (0, 0), &clipboard).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sonitrelix_session::{Media, Project};
    fn track() -> Track {
        let mut p = Project::default();
        p.add_track("Voice".into());
        let mut t = p.tracks.remove(0);
        t.set_media(Media {
            path: std::env::temp_dir().join("source.wav"),
            sample_rate: 48000,
            channels: 1,
            bits: 16,
            frames: 480000,
            float: false,
        });
        t
    }
    #[test]
    fn cut_paste_offsets_and_original_metadata() {
        let mut t = track();
        let original = t.clips[0].source.clone();
        let c = copy(&t, 48000, 144000).unwrap();
        assert_eq!(c.clips[0].source_start, 48000);
        delete(&mut t, 48000, 144000).unwrap();
        assert_eq!(t.frames(), 384000);
        assert_eq!(t.clips[1].source_start, 144000);
        paste(&mut t, 48000, (0, 0), &c).unwrap();
        assert_eq!(t.frames(), 480000);
        assert!(t.sources().all(|m| Some(m.clone()) == original));
    }
    #[test]
    fn split_trim_silence_and_gapped_paste() {
        let mut t = track();
        split(&mut t, 96000).unwrap();
        assert_eq!(t.clips.len(), 2);
        assert_eq!(t.frames(), 480000);
        trim(&mut t, 48000, 144000).unwrap();
        assert_eq!(t.frames(), 96000);
        assert_eq!(t.clips[0].source_start, 48000);
        silence(&mut t, 24000, 72000).unwrap();
        assert_eq!(t.frames(), 96000);
        assert!(t.clips.iter().any(|c| c.source.is_none()));
        let c = copy(&t, 0, 96000).unwrap();
        paste(&mut t, 144000, (0, 0), &c).unwrap();
        assert_eq!(t.frames(), 240000);
    }
    #[test]
    fn mismatch_and_empty_edits_do_not_mutate() {
        let mut t = track();
        let original = t.clone();
        let mut c = copy(&t, 0, 1).unwrap();
        c.channels = 2;
        assert_eq!(paste(&mut t, 0, (0, 0), &c), Err("edit_format_mismatch"));
        assert!(delete(&mut t, 20, 10).is_err());
        assert_eq!(t, original);
    }
}
