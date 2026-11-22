use super::*;
use sonitrelix_engine::recording::{MicrophoneCheck, RecordRequest, Recorded, Recorder};
use sonitrelix_session::{Clip, Media};
pub(super) const RECORD: usize = 816;
pub(super) const CHECK_MICROPHONE: usize = 821;
pub(super) const PUNCH: usize = 827;
const PRE_ROLL_MS: u64 = 3000;
fn punch_capture_window(
    start: u64,
    end: u64,
    rate: u32,
    offset_ms: i32,
) -> (u64, std::ops::Range<u64>) {
    let lead = sonitrelix_audio_io::ms_to_frames(PRE_ROLL_MS, rate);
    let play_from = start.saturating_sub(lead);
    let offset_frames = i128::from(offset_ms) * i128::from(rate) / 1000;
    let skip = (i128::from(start - play_from) - offset_frames).max(0) as u64;
    (play_from, skip..skip.saturating_add(end - start))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn punch_window_keeps_exact_range_after_preroll_and_offset() {
        assert_eq!(
            punch_capture_window(240_001, 264_002, 48000, -25),
            (96_001, 145_200..169_201)
        );
        assert_eq!(punch_capture_window(2400, 4812, 48000, 150), (0, 0..2412));
    }

    #[test]
    #[ignore = "Uses the default microphone briefly and a silent backing track; deletes all test audio"]
    fn native_punch_captures_only_marked_frames_over_backing_and_undo_restores_take() {
        use sonitrelix_audio_io::{WavReader, WavWriter};
        use sonitrelix_engine::State;
        use std::time::{Duration, Instant, SystemTime};

        struct FixtureDirectory(PathBuf);
        impl Drop for FixtureDirectory {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.0).expect("delete temporary punch-in audio");
            }
        }
        let stamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory =
            FixtureDirectory(std::env::temp_dir().join(format!("sonitrelix-punch-test-{stamp}")));
        fs::create_dir(&directory.0).unwrap();
        unsafe {
            let mut app = super::super::tests::native_test_app(&directory.0, "SonitrelixPunchTest");
            app.store.settings.recording.offset_ms = -25;
            app.store.settings.recording.buffer_frames = 512;
            let original = directory.0.join("original.wav");
            let frames = 192_000;
            let mut writer = WavWriter::create(&original, 48000, 1).unwrap();
            writer.write_frames(&vec![0.; frames as usize]).unwrap();
            let info = writer.finish().unwrap();
            let original_bytes = fs::read(&original).unwrap();
            app.project.add_track("Silent backing".into());
            app.project.tracks[0].clips.push(Clip {
                source: None,
                source_start: 0,
                frames,
            });
            app.project.add_track("Take to replace".into());
            app.project.tracks[1].set_media(Media {
                path: original.clone(),
                sample_rate: info.sample_rate,
                channels: info.channels,
                frames,
                bits: info.bits,
                float: false,
            });
            app.refresh();
            SendMessageW(GetDlgItem(app.hwnd, 11), 0x186, 1, 0);
            SetFocus(GetDlgItem(app.hwnd, 11));
            app.command(ARM);
            let before = app.project.clone();
            let track = 1;
            let (start, end) = (144_123, 158_899);
            app.editor.exact_cursor = Some((track, start));
            app.command(shortcuts::bracket(b'[' as usize).unwrap());
            app.editor.exact_cursor = Some((track, end));
            app.command(shortcuts::bracket(b']' as usize).unwrap());
            assert_eq!(app.selected_range(track), (start, end));

            let Some(shortcuts::Action::Command(punch)) =
                shortcuts::route(0x52, true, true, false, shortcuts::Scope::Tracks)
            else {
                panic!("Missing Ctrl+Shift+R command");
            };
            app.command(punch);
            assert_eq!(app.text(21), app.t("punch_preroll"));
            let deadline = Instant::now() + Duration::from_secs(12);
            let mut heard_preroll = false;
            let mut backing_during_capture = false;
            while let Some(recording) = &app.recording {
                assert!(
                    Instant::now() < deadline,
                    "Punch-in did not stop automatically"
                );
                let backing = recording.backing.as_ref().unwrap().snapshot();
                let captured = recording.recorder.frames();
                assert!(captured <= end - start, "Capture exceeded the selection");
                heard_preroll |=
                    backing.state == State::Playing && backing.frame < start && captured == 0;
                backing_during_capture |= backing.state == State::Playing && captured != 0;
                app.poll_recording();
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(heard_preroll && backing_during_capture);
            assert_eq!(app.project.tracks[0], before.tracks[0]);
            assert_eq!(app.project.tracks[track].frames(), frames);
            assert_eq!(app.selected_range(track), (start, end));
            let clips = &app.project.tracks[track].clips;
            assert_eq!(clips.len(), 3);
            assert_eq!((clips[0].source_start, clips[0].frames), (0, start));
            assert_eq!(
                (clips[2].source_start, clips[2].frames),
                (end, frames - end)
            );
            assert_eq!(clips[0].source, clips[2].source);
            assert_eq!(clips[0].source.as_ref().unwrap().path, original);
            let captured = clips[1].source.as_ref().unwrap();
            let captured_path = captured.path.clone();
            assert_ne!(captured_path, original);
            assert_eq!(clips[1].frames, end - start);
            assert_eq!(
                WavReader::open(&captured_path).unwrap().info.frames,
                end - start
            );
            assert_eq!(fs::read(&original).unwrap(), original_bytes);

            let punched = app.project.clone();
            app.command(UNDO);
            assert_eq!(app.project, before);
            app.command(REDO);
            assert_eq!(app.project, punched);
            app.command(UNDO);
            app.command(punch);
            assert!(app.recording.is_some());
            app.command(STOP);
            let deadline = Instant::now() + Duration::from_secs(5);
            while app.recording.is_some() {
                assert!(Instant::now() < deadline, "Punch-in did not cancel");
                app.poll_recording();
                std::thread::sleep(Duration::from_millis(10));
            }
            assert_eq!(app.text(21), app.t("punch_cancelled"));
            assert_eq!(app.project, before);
            assert_eq!(
                fs::read_dir(directory.0.join("recordings"))
                    .unwrap()
                    .count(),
                1
            );
            println!(
                "WASAPI preroll and backing verified; captured {} frames; exact splice, Undo, Redo and pre-roll cancellation passed.",
                end - start
            );
            DestroyWindow(app.hwnd);
        }
    }
}
pub(super) struct Recording {
    recorder: Recorder,
    index: usize,
    at: u64,
    rate: u32,
    paused: bool,
    stopping: bool,
    clip_announced: bool,
    punch: bool,
    punch_announced: bool,
    backing_end: Option<u64>,
    pending_take: Option<Recorded>,
    backing: Option<sonitrelix_engine::Engine>,
}
impl App {
    pub(super) unsafe fn record_command(&mut self) {
        unsafe { self.start_recording(false) };
    }
    pub(super) unsafe fn start_recording(&mut self, punch: bool) {
        unsafe {
            if self.recording.is_some() {
                self.stop_recording();
                return;
            }
            let Some(index) = self.selected_track() else {
                let message = self.t("edit_no_track");
                self.status(&message);
                announcement::announce(GetDlgItem(self.hwnd, 21), &message);
                return;
            };
            if !self.project.tracks[index].armed {
                let message = self.t("record_arm_first");
                self.status(&message);
                announcement::announce(GetDlgItem(self.hwnd, 21), &message);
                return;
            }
            let t = &self.project.tracks[index];
            let rate = if t.clips.is_empty() {
                self.project.sample_rate
            } else {
                t.sample_rate
            };
            let channels = self.store.settings.recording.channels;
            if !t.clips.is_empty() && channels != t.channels {
                self.status(&self.t("record_channels_mismatch"));
                return;
            }
            let (start, end) = self.selected_range(index);
            let offset = self.store.settings.recording.offset_ms;
            let (at, play_from, capture_frames) = if punch {
                if end <= start || self.editor.bracket_start.is_some() {
                    self.timeline_feedback(&self.t("punch_select_range"));
                    return;
                }
                let (play_from, window) = punch_capture_window(start, end, rate, offset);
                (
                    start,
                    playback::scale_frame(play_from, rate, self.project.sample_rate),
                    Some(window),
                )
            } else {
                self.align_cursor_to_playhead();
                let ms = (i128::from(self.position) + i128::from(offset))
                    .clamp(0, i128::from(u64::MAX)) as u64;
                (
                    sonitrelix_audio_io::ms_to_frames(ms, rate),
                    sonitrelix_audio_io::ms_to_frames(self.position, self.project.sample_rate),
                    None,
                )
            };
            self.microphone_check = None;
            self.stop_playback();
            self.cancel_audio_import();
            let directory = self.data.join("recordings");
            if let Err(e) = fs::create_dir_all(&directory) {
                self.error(e);
                return;
            }
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let path = directory.join(format!("recording-{stamp}-{}.wav", std::process::id()));
            let backing_timeline = if punch {
                let mut backing = self.project.clone();
                let track = &mut backing.tracks[index];
                track.clips = sonitrelix_editing::pieces(track, 0, start);
                track.solo = false;
                sonitrelix_mixer::project(&backing, None)
            } else {
                sonitrelix_mixer::project(&self.project, Some(index))
            };
            let backing_timeline = match backing_timeline {
                Ok(t) => t,
                Err(e) => {
                    self.error(e);
                    return;
                }
            };
            let mut backing = None;
            let gate = if backing_timeline.info().frames > play_from {
                let gate = std::sync::Arc::new(sonitrelix_engine::StartGate::new());
                let mut engine = match sonitrelix_engine::Engine::new() {
                    Ok(e) => e,
                    Err(e) => {
                        self.error(e);
                        return;
                    }
                };
                if let Err(e) = engine.play(sonitrelix_engine::PlayRequest {
                    gate: Some(gate.clone()),
                    timeline: backing_timeline,
                    start_ms: 0,
                    start_frame: Some(play_from),
                    loop_frames: None,
                    output_id: self.store.settings.devices.output_id.clone(),
                    buffer_frames: self.store.settings.recording.buffer_frames,
                    paused: false,
                }) {
                    self.error(e);
                    return;
                }
                backing = Some(engine);
                Some(gate)
            } else {
                None
            };
            let request = RecordRequest {
                gate,
                path,
                input_id: self.store.settings.devices.input_id.clone(),
                sample_rate: rate,
                channels,
                buffer_frames: self.store.settings.recording.buffer_frames,
                capture_frames,
                monitoring_output: self
                    .store
                    .settings
                    .recording
                    .monitoring
                    .then(|| self.store.settings.devices.output_id.clone()),
            };
            match Recorder::start(request) {
                Ok(recorder) => {
                    self.recording = Some(Recording {
                        recorder,
                        index,
                        at,
                        rate,
                        paused: false,
                        stopping: false,
                        clip_announced: false,
                        punch,
                        punch_announced: false,
                        backing_end: (punch && offset > 0)
                            .then(|| playback::scale_frame(end, rate, self.project.sample_rate)),
                        pending_take: None,
                        backing,
                    });
                    self.timeline_feedback(&self.t(if punch {
                        "punch_preroll"
                    } else {
                        "record_starting"
                    }));
                    self.record_readout(true);
                    self.log("recording_requested");
                }
                Err(e) => self.error(e),
            }
        }
    }
    pub(super) unsafe fn stop_recording(&mut self) {
        unsafe {
            if let Some(r) = &mut self.recording {
                r.recorder.stop();
                if let Some(engine) = &r.backing {
                    engine.stop();
                }
                r.stopping = true;
                self.status(&self.t("record_finishing"));
                self.record_readout(true);
            }
        }
    }
    pub(super) unsafe fn pause_recording(&mut self) {
        unsafe {
            if let Some(r) = &mut self.recording {
                if r.stopping {
                    return;
                }
                if r.recorder.done() {
                    return;
                }
                r.paused ^= true;
                r.recorder.pause(r.paused);
                if let Some(engine) = &r.backing {
                    engine.pause(r.paused);
                }
                let key = if r.paused {
                    "record_paused"
                } else {
                    "record_active"
                };
                self.status(&self.t(key));
                self.record_readout(true);
            }
        }
    }
    pub(super) unsafe fn record_readout(&self, force: bool) {
        unsafe {
            if let Some(r) = &self.recording {
                for (id, key) in [
                    (recording_ui::RECORD as i32, "record_stop_button"),
                    (
                        PAUSE as i32,
                        if r.paused {
                            "record_resume_button"
                        } else {
                            "record_pause_button"
                        },
                    ),
                ] {
                    let value = self.t(key);
                    if self.text(id) != value {
                        SetWindowTextW(GetDlgItem(self.hwnd, id), wide(&value).as_ptr());
                    }
                }
                let text = format!(
                    "{}: {}. {}",
                    self.t(if r.stopping {
                        "record_finishing"
                    } else if r.paused {
                        "record_paused"
                    } else if r.recorder.frames() == 0 {
                        if r.punch {
                            "punch_preroll"
                        } else {
                            "record_starting"
                        }
                    } else {
                        "record_active"
                    }),
                    self.project.tracks[r.index].name,
                    format_time(sonitrelix_audio_io::frames_to_ms(
                        r.recorder.frames(),
                        r.rate
                    ))
                );
                if force || GetFocus() != GetDlgItem(self.hwnd, 23) {
                    SetWindowTextW(GetDlgItem(self.hwnd, 23), wide(&text).as_ptr());
                }
            }
        }
    }
    pub(super) unsafe fn report_recording(&self) {
        unsafe {
            if let Some(r) = &self.recording {
                let text = format!(
                    "{}: {}",
                    self.t(if r.paused {
                        "record_paused"
                    } else if r.recorder.frames() == 0 {
                        if r.punch {
                            "punch_preroll"
                        } else {
                            "record_starting"
                        }
                    } else {
                        "record_active"
                    }),
                    format_time(sonitrelix_audio_io::frames_to_ms(
                        r.recorder.frames(),
                        r.rate
                    ))
                );
                SetWindowTextW(GetDlgItem(self.hwnd, 23), wide(&text).as_ptr());
                SetFocus(GetDlgItem(self.hwnd, 23));
            }
        }
    }
    pub(super) fn report_levels(&self) {
        let describe = |key, level: sonitrelix_engine::Level| {
            let value = if level.peak == 0. {
                self.t("level_silent")
            } else {
                format!("{:.0} dBFS", 20. * level.peak.log10())
            };
            let mut message = format!("{}: {value}", self.t(key));
            if level.clipped {
                message.push_str(&format!(". {}", self.t("level_clipped")));
            }
            message
        };
        let mut levels = vec![];
        if let Some(r) = &self.recording {
            levels.push(describe("meter_microphone", r.recorder.input_level()));
            if let Some(backing) = &r.backing
                && matches!(
                    backing.snapshot().state,
                    sonitrelix_engine::State::Playing
                        | sonitrelix_engine::State::Buffering
                        | sonitrelix_engine::State::Paused
                )
            {
                levels.push(describe("meter_backing_output", backing.output_level()));
            }
        } else {
            if let Some(check) = &self.microphone_check {
                levels.push(describe("meter_microphone", check.level()));
            }
            if let Some(level) = self.playback_level() {
                levels.push(describe("meter_output", level));
            }
        }
        let text = if levels.is_empty() {
            self.t("meter_inactive")
        } else {
            levels.join(". ")
        };
        self.announce_recording_status(&text);
    }
    fn announce_recording_status(&self, text: &str) {
        unsafe {
            self.status(text);
            announcement::announce(GetDlgItem(self.hwnd, 21), text);
        }
    }
    pub(super) fn toggle_microphone_check(&mut self) {
        let key = if self.microphone_check.take().is_some() {
            "microphone_check_ended"
        } else {
            match MicrophoneCheck::start(
                self.store.settings.devices.input_id.clone(),
                self.store.settings.general.new_project_sample_rate,
                self.store.settings.recording.channels,
                self.store.settings.recording.buffer_frames,
            ) {
                Ok(check) => {
                    self.microphone_check = Some(check);
                    "microphone_check_started"
                }
                Err(error) => {
                    self.error_in("microphone check", error);
                    return;
                }
            }
        };
        let message = self.t(key);
        self.announce_recording_status(&message);
    }

    pub(super) fn poll_microphone_check(&mut self) {
        if let Some(result) = self
            .microphone_check
            .as_ref()
            .and_then(MicrophoneCheck::try_result)
        {
            self.microphone_check = None;
            match result {
                Ok(()) => {
                    let message = self.t("microphone_check_ended");
                    self.announce_recording_status(&message);
                }
                Err(error) => self.error_in("microphone check", error),
            }
        }
    }
    pub(super) unsafe fn insert_recorded(&mut self, index: usize, at: u64, recorded: Recorded) {
        unsafe {
            let media = Media {
                path: recorded.path,
                sample_rate: recorded.info.sample_rate,
                channels: recorded.info.channels,
                bits: 16,
                frames: recorded.info.frames,
                float: false,
            };
            let clipboard = sonitrelix_editing::Clipboard {
                sample_rate: media.sample_rate,
                channels: media.channels,
                clips: vec![Clip {
                    frames: media.frames,
                    source: Some(media),
                    source_start: 0,
                }],
            };
            let mut next = self.project.tracks[index].clone();
            let end = at.saturating_add(clipboard.frames()).min(next.frames());
            match sonitrelix_editing::paste(
                &mut next,
                at,
                if end > at { (at, end) } else { (0, 0) },
                &clipboard,
            ) {
                Ok((a, b)) => {
                    let mut candidate = self.project.clone();
                    candidate.tracks[index] = next.clone();
                    if let Err(e) = candidate.validate() {
                        self.error(e);
                        return;
                    }
                    self.checkpoint();
                    self.project.tracks[index] = next;
                    self.editor.bracket_start = None;
                    // Leave playback at the take's beginning so Space auditions it.
                    self.position = sonitrelix_audio_io::frames_to_ms(a, recorded.info.sample_rate);
                    self.selection = (
                        self.position,
                        editing_ui::ceil_ms(b, recorded.info.sample_rate),
                    );
                    self.editor.exact_selection = Some((index, a, b));
                    self.editor.exact_cursor = Some((index, a));
                    self.refresh();
                    self.sync_times();
                    let audible =
                        sonitrelix_mixer::audible_tracks(&self.project).nth(index) == Some(true);
                    let message = self.t(if audible {
                        "record_saved"
                    } else {
                        "record_saved_inaudible"
                    });
                    self.status(&message);
                    announcement::announce(GetDlgItem(self.hwnd, 21), &message);
                    self.log("recording_saved");
                    if let Some(warning) = recorded.warning {
                        self.error_in("recording warning", warning);
                    }
                }
                Err(e) => self.error(format!("{}: {e}", self.t("record_insert_failed"))),
            }
        }
    }
    pub(super) unsafe fn poll_recording(&mut self) {
        unsafe {
            if let Some(r) = &mut self.recording {
                let punch_started =
                    r.punch && !r.punch_announced && r.recorder.frames() > 0 && !r.stopping;
                r.punch_announced |= punch_started;
                let clipped = r.recorder.input_clipped() && !r.clip_announced;
                if clipped {
                    r.clip_announced = true;
                }
                let monitor_error = r.recorder.try_monitor_error();
                if punch_started {
                    self.timeline_feedback(&self.t("record_active"));
                }
                if let Some(detail) = monitor_error {
                    self.log_error("microphone monitoring", detail);
                    let message = self.t("record_monitor_failed");
                    self.status(&message);
                    announcement::announce(GetDlgItem(self.hwnd, 21), &message);
                }
                if clipped {
                    let message = self.t("record_input_clipping");
                    self.status(&message);
                    announcement::announce(GetDlgItem(self.hwnd, 21), &message);
                }
            }
            let failure = self
                .recording
                .as_ref()
                .and_then(|r| r.backing.as_ref())
                .and_then(|e| e.try_event());
            if let Some(sonitrelix_engine::Event::Error { message, .. }) = failure {
                self.stop_recording();
                self.error_in("recording backing playback", message);
            }
            if self.recording.as_ref().is_some_and(|r| {
                !r.stopping
                    && r.backing
                        .as_ref()
                        .is_some_and(|e| e.snapshot().underruns > 0)
            }) {
                self.stop_recording();
                self.error_in(
                    "recording backing playback",
                    self.t("record_backing_underrun"),
                );
            }
            let result = self.recording.as_mut().and_then(|r| {
                let result = r
                    .pending_take
                    .take()
                    .map(Ok)
                    .or_else(|| r.recorder.try_result())?;
                // A positive input correction can finish capture before playback reaches the out point.
                if let Ok(recorded) = &result
                    && recorded.warning.is_none()
                    && !r.stopping
                    && !r.paused
                    && let Some(end) = r.backing_end
                    && let Some(engine) = &r.backing
                {
                    let snapshot = engine.snapshot();
                    if snapshot.frame < end
                        && matches!(
                            snapshot.state,
                            sonitrelix_engine::State::Starting
                                | sonitrelix_engine::State::Buffering
                                | sonitrelix_engine::State::Playing
                        )
                    {
                        r.pending_take = result.ok();
                        return None;
                    }
                }
                Some(result)
            });
            if let Some(result) = result {
                let state = self.recording.take().expect("active recording");
                if let Some(backing) = &state.backing {
                    backing.stop();
                }
                for (id, key) in [
                    (recording_ui::RECORD as i32, "record_button"),
                    (PAUSE as i32, "pause_button"),
                ] {
                    SetWindowTextW(GetDlgItem(self.hwnd, id), wide(&self.t(key)).as_ptr());
                }
                match result {
                    Ok(recorded) => self.insert_recorded(state.index, state.at, recorded),
                    Err(_) if state.punch && state.stopping && state.recorder.frames() == 0 => {
                        self.timeline_feedback(&self.t("punch_cancelled"));
                    }
                    Err(e) => {
                        self.status(&self.t("record_failed"));
                        self.error_in("recording", e);
                    }
                }
                if self.recording_close_pending {
                    self.recording_close_pending = false;
                    self.close_audio();
                }
            } else {
                self.record_readout(false);
            }
        }
    }
}
