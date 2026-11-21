//! Shared playback/export mix construction, immutable project snapshots.
use sonitrelix_audio_io::{Encoding, LiveMix, MixTrack, Segment, Source, Timeline, WavInfo};
use sonitrelix_session::{Project, Track};
mod export;
pub use export::{ExportRange, Exported, Options, export, export_selection, export_with};
pub fn track(t: &Track) -> std::io::Result<Timeline> {
    Timeline::new(
        t.sample_rate,
        t.channels,
        t.clips
            .iter()
            .map(|c| Segment {
                source: c.source.as_ref().map(|m| Source {
                    path: m.path.clone(),
                    info: WavInfo {
                        sample_rate: m.sample_rate,
                        channels: m.channels,
                        bits: m.bits,
                        frames: m.frames,
                        encoding: if m.float {
                            Encoding::Float
                        } else {
                            Encoding::Pcm
                        },
                    },
                }),
                source_start: c.source_start,
                frames: c.frames,
            })
            .collect(),
    )
}
pub fn project(p: &Project, exclude: Option<usize>) -> std::io::Result<Timeline> {
    p.validate()?;
    let tracks = p
        .tracks
        .iter()
        .zip(audible_tracks_except(p, exclude))
        .filter(|(_, audible)| *audible)
        .map(|(t, _)| {
            Ok(MixTrack {
                timeline: track(t)?,
                gain: t.gain,
                pan: t.pan,
            })
        })
        .collect::<std::io::Result<Vec<_>>>()?;
    Timeline::mix(p.sample_rate, tracks)
}
pub fn audible_tracks(p: &Project) -> impl Iterator<Item = bool> + '_ {
    audible_tracks_except(p, None)
}

fn audible_tracks_except(p: &Project, exclude: Option<usize>) -> impl Iterator<Item = bool> + '_ {
    let eligible = move |i: usize, t: &Track| Some(i) != exclude && !t.mute && !t.clips.is_empty();
    let solo = p
        .tracks
        .iter()
        .enumerate()
        .any(|(i, t)| eligible(i, t) && t.solo);
    p.tracks
        .iter()
        .enumerate()
        .map(move |(i, t)| eligible(i, t) && (!solo || t.solo))
}

/// Keep the entire arrangement on the playback clock, even when every track is muted.
pub fn live_project(p: &Project) -> std::io::Result<(Timeline, LiveMix)> {
    p.validate()?;
    let tracks = p
        .tracks
        .iter()
        .map(|t| {
            Ok(MixTrack {
                timeline: track(t)?,
                gain: t.gain,
                pan: t.pan,
            })
        })
        .collect::<std::io::Result<Vec<_>>>()?;
    let controls = LiveMix::new(audible_tracks(p).collect());
    let timeline = Timeline::mix(p.sample_rate, tracks)?.with_live_mix(controls.clone())?;
    Ok((timeline, controls))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn solo_mute_and_exclusion_define_duration() {
        let mut p = Project::default();
        for n in [48000, 96000] {
            p.add_track("test".into());
            let t = p.tracks.last_mut().unwrap();
            t.clips.push(sonitrelix_session::Clip {
                source: None,
                source_start: 0,
                frames: n,
            });
        }
        assert_eq!(project(&p, None).unwrap().info().frames, 96000);
        p.tracks[0].solo = true;
        assert_eq!(project(&p, None).unwrap().info().frames, 48000);
        p.tracks[0].mute = true;
        assert_eq!(project(&p, None).unwrap().info().frames, 96000);
        assert_eq!(project(&p, Some(1)).unwrap().info().frames, 0);
        p.tracks[0].mute = false;
        assert_eq!(
            project(&p, Some(0)).unwrap().info().frames,
            96000,
            "a soloed recording target excluded from backing must not silence the backing"
        );
        p.add_track("Empty solo".into());
        p.tracks[0].solo = false;
        p.tracks[2].solo = true;
        assert_eq!(audible_tracks(&p).collect::<Vec<_>>(), [true, true, false]);
        assert_eq!(project(&p, None).unwrap().info().frames, 96000);
    }

    #[test]
    fn live_solo_states_change_the_pcm_without_resetting_the_reader() {
        use sonitrelix_audio_io::{TimelineReader, WavWriter};
        let temp = std::env::temp_dir();
        let dir = temp.join(format!("sonitrelix-solo-{}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        let mut project = Project::default();
        for (name, amplitude) in [("Backing", 0.2), ("Vocal", 0.4)] {
            let path = dir.join(format!("{name}.wav"));
            let mut writer = WavWriter::create_float(&path, 48000, 2).unwrap();
            writer.write_frames(&[amplitude; 40]).unwrap();
            let info = writer.finish().unwrap();
            project.add_track(name.into());
            project
                .tracks
                .last_mut()
                .unwrap()
                .set_media(sonitrelix_session::Media {
                    path,
                    sample_rate: 48000,
                    channels: 2,
                    bits: 32,
                    frames: info.frames,
                    float: true,
                });
        }
        let (timeline, controls) = live_project(&project).unwrap();
        let mut reader = TimelineReader::new(timeline);
        let mut assert_next = |expected: f32| {
            let mut pcm = [0.; 2];
            assert_eq!(reader.read_frames(&mut pcm).unwrap(), 1);
            assert!(
                pcm.into_iter().all(|v| (v - expected).abs() < 0.001),
                "{pcm:?}"
            );
        };
        assert_next(0.6);
        project.tracks[1].solo = true;
        controls.update(audible_tracks(&project));
        assert_next(0.4);
        project.tracks[0].solo = true;
        controls.update(audible_tracks(&project));
        assert_next(0.6);
        project.tracks[1].mute = true;
        controls.update(audible_tracks(&project));
        assert_next(0.2);
        project.tracks[0].solo = false;
        project.tracks[1].solo = false;
        project.tracks[1].mute = false;
        controls.update(audible_tracks(&project));
        assert_next(0.6);
        drop(reader);
        assert!(dir.starts_with(temp));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
