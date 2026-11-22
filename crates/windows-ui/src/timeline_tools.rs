use super::*;
use sonitrelix_audio_io::{
    Timeline, TimelineReader, WavInfo, WavWriter, frames_to_ms, ms_to_frames,
};
use sonitrelix_session::Marker;
use std::{
    io,
    ops::Range,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

pub(super) const ADD_MARKER: usize = 822;
pub(super) const PREVIOUS_MARKER: usize = 823;
pub(super) const NEXT_MARKER: usize = 824;
pub(super) const MANAGE_MARKERS: usize = 825;
pub(super) const CROSSFADE: usize = 826;
pub(super) const CALIBRATE: usize = 828;
const CROSSFADE_MS: u64 = 20;

enum ResultData {
    Crossfade {
        track: usize,
        range: Range<u64>,
        path: PathBuf,
        info: WavInfo,
    },
    Calibration(Result<i32, String>),
}

pub(super) struct Job {
    result: mpsc::Receiver<Result<ResultData, String>>,
    cancel: Arc<AtomicBool>,
}

fn crossfade_range(
    track: &sonitrelix_session::Track,
    cursor: u64,
) -> Option<(Range<u64>, Range<u64>)> {
    let mut join = 0u64;
    for clips in track.clips.windows(2) {
        join += clips[0].frames;
        let tolerance = u64::from(track.sample_rate).div_ceil(1000);
        if join.abs_diff(cursor) > tolerance || clips.iter().any(|clip| clip.source.is_none()) {
            continue;
        }
        let length = ms_to_frames(CROSSFADE_MS, track.sample_rate)
            .min(clips[0].frames)
            .min(clips[1].frames);
        if length >= 2 {
            return Some((join - length..join, join..join + length));
        }
    }
    None
}

fn render_crossfade(
    left: Timeline,
    right: Timeline,
    path: &std::path::Path,
) -> io::Result<WavInfo> {
    let info = left.info();
    let channels = usize::from(info.channels);
    let frames = info.frames as usize;
    let mut a = vec![0.; frames * channels];
    let mut b = vec![0.; a.len()];
    if TimelineReader::new(left).read_frames(&mut a)? != frames
        || TimelineReader::new(right).read_frames(&mut b)? != frames
    {
        return Err(io::Error::other(
            "Could not read clips at crossfade boundary",
        ));
    }
    for (frame, (a, b)) in a
        .chunks_exact_mut(channels)
        .zip(b.chunks_exact(channels))
        .enumerate()
    {
        let mix = frame as f32 / (frames - 1) as f32;
        for (a, b) in a.iter_mut().zip(b) {
            *a = *a * (1. - mix) + *b * mix;
        }
    }
    let mut writer = WavWriter::create_float(path, info.sample_rate, info.channels)?;
    writer.write_frames(&a)?;
    writer.finish()
}

impl App {
    fn start_timeline_job(
        &mut self,
        action: impl FnOnce(&AtomicBool) -> Result<ResultData, String> + Send + 'static,
    ) -> io::Result<()> {
        let (sender, result) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        std::thread::Builder::new()
            .name("timeline-tools".into())
            .spawn(move || {
                if let Err(mpsc::SendError(Ok(ResultData::Crossfade { path, .. }))) =
                    sender.send(action(&worker_cancel))
                {
                    let _ = fs::remove_file(path);
                }
            })?;
        self.timeline_job = Some(Job { result, cancel });
        Ok(())
    }

    pub(super) fn cancel_timeline_job(&self) -> bool {
        if let Some(job) = &self.timeline_job {
            job.cancel.store(true, Ordering::Release);
            true
        } else {
            false
        }
    }

    pub(super) unsafe fn poll_timeline_job(&mut self) {
        unsafe {
            let Some(job) = &self.timeline_job else {
                return;
            };
            let result = match job.result.try_recv() {
                Ok(result) => result,
                Err(mpsc::TryRecvError::Empty) => return,
                Err(mpsc::TryRecvError::Disconnected) => Err("Audio worker stopped".into()),
            };
            let cancelled = job.cancel.load(Ordering::Acquire);
            self.timeline_job = None;
            if cancelled || self.playback.closing {
                if let Ok(ResultData::Crossfade { path, .. }) = result {
                    let _ = fs::remove_file(path);
                }
                if !self.playback.closing {
                    self.timeline_feedback(&self.t("timeline_cancelled"));
                }
                return;
            }
            match result {
                Ok(ResultData::Crossfade {
                    track,
                    range,
                    path,
                    info,
                }) => {
                    if self.commit_effect(track, range.start, range.end, path.clone(), info) {
                        self.timeline_feedback(&self.t("crossfade_done"));
                    } else {
                        let _ = fs::remove_file(path);
                    }
                }
                Ok(ResultData::Calibration(Ok(offset))) => {
                    let message = self
                        .t("calibrate_result")
                        .replace("{offset}", &offset.to_string());
                    self.modal.set(true);
                    let choice = MessageBoxW(
                        self.hwnd,
                        wide(&message).as_ptr(),
                        wide(&self.t("calibrate_title")).as_ptr(),
                        4 | 0x20 | 0x100,
                    );
                    self.modal.set(false);
                    if choice == 6 {
                        let mut next = self.store.settings.clone();
                        next.recording.offset_ms = offset;
                        match self.store.save(next) {
                            Ok(()) => self.timeline_feedback(&self.t("calibrate_saved")),
                            Err(error) => self.error_in("recording offset", error),
                        }
                    } else {
                        self.timeline_feedback(&self.t("timeline_cancelled"));
                    }
                }
                Ok(ResultData::Calibration(Err(error))) => {
                    self.log_error("recording offset calibration", error);
                    self.error(self.t("calibrate_failed"));
                }
                Err(error) => self.error_in("timeline audio tool", error),
            }
        }
    }

    pub(super) unsafe fn crossfade_command(&mut self) {
        unsafe {
            let Some(index) = self.selected_track() else {
                self.timeline_feedback(&self.t("edit_no_track"));
                return;
            };
            let track = &self.project.tracks[index];
            let Some((left, right)) = crossfade_range(track, self.track_cursor_frame(index)) else {
                self.timeline_feedback(&self.t("crossfade_choose_join"));
                return;
            };
            let range = left.start..right.end;
            let left = effects_ui::effect_timeline(track, left.start, left.end);
            let right = effects_ui::effect_timeline(track, right.start, right.end);
            let (left, right) = match left.and_then(|left| right.map(|right| (left, right))) {
                Ok(pair) => pair,
                Err(error) => {
                    self.error(error);
                    return;
                }
            };
            self.stop_playback();
            self.cancel_audio_import();
            let directory = self.data.join("effects");
            let path = directory.join(format!("crossfade-{}.wav", job_stamp()));
            let result = self.start_timeline_job(move |_| {
                fs::create_dir_all(directory).map_err(|error| error.to_string())?;
                match render_crossfade(left, right, &path) {
                    Ok(info) => Ok(ResultData::Crossfade {
                        track: index,
                        range,
                        path,
                        info,
                    }),
                    Err(error) => {
                        let _ = fs::remove_file(path);
                        Err(error.to_string())
                    }
                }
            });
            match result {
                Ok(()) => self.timeline_feedback(&self.t("crossfade_working")),
                Err(error) => self.error(error),
            }
        }
    }

    pub(super) unsafe fn calibrate_command(&mut self) {
        unsafe {
            self.modal.set(true);
            let confirm = MessageBoxW(
                self.hwnd,
                wide(&self.t("calibrate_instructions")).as_ptr(),
                wide(&self.t("calibrate_title")).as_ptr(),
                1 | 0x20 | 0x100,
            );
            self.modal.set(false);
            if confirm != 1 {
                return;
            }
            self.microphone_check = None;
            self.stop_playback();
            let directory = self.data.join("calibration");
            let temporary = directory.join(job_stamp());
            let input = self.store.settings.devices.input_id.clone();
            let output = self.store.settings.devices.output_id.clone();
            let channels = self.store.settings.recording.channels;
            let buffer = self.store.settings.recording.buffer_frames;
            let result = self.start_timeline_job(move |cancelled| {
                fs::create_dir_all(directory).map_err(|error| error.to_string())?;
                fs::create_dir(&temporary).map_err(|error| error.to_string())?;
                let result = sonitrelix_engine::calibration::calibrate(
                    &temporary, input, output, channels, buffer, cancelled,
                );
                let _ = fs::remove_dir(&temporary);
                Ok(ResultData::Calibration(result))
            });
            match result {
                Ok(()) => self.timeline_feedback(&self.t("calibrate_running")),
                Err(error) => self.error(error),
            }
        }
    }
    pub(super) fn timeline_feedback(&self, message: &str) {
        unsafe {
            self.status(message);
            announcement::announce(GetDlgItem(self.hwnd, 21), message);
        }
    }

    fn marker_label(&self, marker: &Marker) -> String {
        format!(
            "{}. {}",
            marker.name,
            format_time(frames_to_ms(marker.frame, self.project.sample_rate))
        )
    }

    unsafe fn jump_marker(&mut self, index: usize) {
        unsafe {
            let marker = &self.project.markers[index];
            let frame = marker.frame;
            let label = self.marker_label(marker);
            self.position = frames_to_ms(frame, self.project.sample_rate);
            self.selection_anchor = None;
            self.editor.exact_cursor = self.selected_track().map(|i| {
                let rate = self.project.tracks[i].sample_rate;
                (
                    i,
                    playback::scale_frame(frame, self.project.sample_rate, rate),
                )
            });
            self.sync_times();
            self.seek_playback();
            self.timeline_feedback(&label);
        }
    }

    pub(super) unsafe fn marker_command(&mut self, command: usize) {
        unsafe {
            self.align_cursor_to_playhead();
            if command == ADD_MARKER {
                let frame = self.selected_track().map_or_else(
                    || ms_to_frames(self.position, self.project.sample_rate),
                    |track| {
                        playback::scale_frame(
                            self.track_cursor_frame(track),
                            self.project.tracks[track].sample_rate,
                            self.project.sample_rate,
                        )
                    },
                );
                self.modal.set(true);
                let result = forms::ask(
                    self.hwnd,
                    self.strings.clone(),
                    &self.t("marker_add"),
                    vec![forms::Field::Text {
                        label: self.t("marker_name"),
                        value: String::new(),
                    }],
                    &format_time(self.position),
                );
                self.modal.set(false);
                match result {
                    Ok(Some(mut fields)) => {
                        let name = fields.remove(0);
                        if name.is_empty() || name.len() > 256 || name.chars().any(char::is_control)
                        {
                            self.timeline_feedback(&self.t("marker_invalid_name"));
                            return;
                        }
                        if self.project.markers.len() >= 10000 {
                            self.timeline_feedback(&self.t("marker_limit"));
                            return;
                        }
                        let marker = Marker { name, frame };
                        let label = self.marker_label(&marker);
                        let index = self.project.markers.partition_point(|m| m.frame <= frame);
                        self.checkpoint();
                        self.project.markers.insert(index, marker);
                        self.refresh_title();
                        self.timeline_feedback(&format!("{} {label}", self.t("marker_added")));
                    }
                    Ok(None) => {}
                    Err(error) => self.error(error),
                }
                return;
            }
            if self.project.markers.is_empty() {
                self.timeline_feedback(&self.t("marker_none"));
                return;
            }
            if command == MANAGE_MARKERS {
                let options = self
                    .project
                    .markers
                    .iter()
                    .map(|marker| self.marker_label(marker))
                    .collect();
                self.modal.set(true);
                let result = forms::ask(
                    self.hwnd,
                    self.strings.clone(),
                    &self.t("marker_manage"),
                    vec![
                        forms::Field::Choice {
                            label: self.t("marker_choose"),
                            options,
                            selected: 0,
                        },
                        forms::Field::Choice {
                            label: self.t("marker_action"),
                            options: vec![self.t("marker_go"), self.t("marker_delete")],
                            selected: 0,
                        },
                    ],
                    &self.t("marker_manage_hint"),
                );
                self.modal.set(false);
                match result {
                    Ok(Some(values)) => {
                        let index: usize = values[0].parse().unwrap();
                        if values[1] == "0" {
                            self.jump_marker(index);
                        } else {
                            self.checkpoint();
                            self.project.markers.remove(index);
                            self.refresh_title();
                            self.timeline_feedback(&self.t("marker_deleted"));
                        }
                    }
                    Ok(None) => {}
                    Err(error) => self.error(error),
                }
                return;
            }
            // Compare at displayed millisecond precision so repeated navigation skips the current marker.
            let rate = self.project.sample_rate;
            let position = self.position;
            let index = if command == PREVIOUS_MARKER {
                self.project
                    .markers
                    .partition_point(|m| frames_to_ms(m.frame, rate) < position)
                    .checked_sub(1)
            } else {
                let next = self
                    .project
                    .markers
                    .partition_point(|m| frames_to_ms(m.frame, rate) <= position);
                (next < self.project.markers.len()).then_some(next)
            };
            if let Some(index) = index {
                self.jump_marker(index);
            } else {
                self.timeline_feedback(&self.t(if command == PREVIOUS_MARKER {
                    "marker_no_previous"
                } else {
                    "marker_no_next"
                }));
            }
        }
    }
}

fn job_stamp() -> String {
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{time}-{}", std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sonitrelix_session::Media;

    #[test]
    fn crossfade_blends_the_overlap_at_the_clip_join() {
        let directory = std::env::temp_dir().join(format!("sonitrelix-crossfade-{}", job_stamp()));
        fs::create_dir(&directory).unwrap();
        let mut project = Project::default();
        project.add_track("Clips".into());
        let track = &mut project.tracks[0];
        for (name, volume) in [("left", 0.5), ("right", -0.25)] {
            let path = directory.join(format!("{name}.wav"));
            let mut writer = WavWriter::create_float(&path, 48000, 1).unwrap();
            writer.write_frames(&[volume; 2400]).unwrap();
            let info = writer.finish().unwrap();
            track.clips.push(sonitrelix_session::Clip {
                source: Some(Media {
                    path,
                    sample_rate: 48000,
                    channels: 1,
                    bits: 32,
                    frames: info.frames,
                    float: true,
                }),
                source_start: 0,
                frames: info.frames,
            });
        }
        assert!(crossfade_range(track, 500).is_none());
        let (left, right) = crossfade_range(track, 2400).unwrap();
        assert_eq!((left.clone(), right.clone()), (1440..2400, 2400..3360));
        let overlap = directory.join("overlap.wav");
        let info = render_crossfade(
            effects_ui::effect_timeline(track, left.start, left.end).unwrap(),
            effects_ui::effect_timeline(track, right.start, right.end).unwrap(),
            &overlap,
        )
        .unwrap();
        assert_eq!(info.frames, 960);
        let mut samples = [0.; 960];
        sonitrelix_audio_io::WavReader::open(&overlap)
            .unwrap()
            .read_frames(&mut samples)
            .unwrap();
        assert_eq!(samples[0], 0.5);
        assert_eq!(samples[959], -0.25);
        assert!(samples.windows(2).all(|pair| pair[0] > pair[1]));
        for path in [
            directory.join("left.wav"),
            directory.join("right.wav"),
            overlap,
        ] {
            fs::remove_file(path).unwrap();
        }
        fs::remove_dir(directory).unwrap();
    }
}
