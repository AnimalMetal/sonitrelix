use super::*;
use sonitrelix_engine::recording::{RecordRequest, Recorded, Recorder};
use sonitrelix_session::{Clip, Media};
pub(super) const RECORD: usize = 816;
pub(super) struct Recording {
    recorder: Recorder,
    index: usize,
    at: u64,
    rate: u32,
    paused: bool,
    stopping: bool,
    backing: Option<sonitrelix_engine::Engine>,
}
impl App {
    pub(super) unsafe fn record_command(&mut self) {
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
            self.stop_playback();
            self.cancel_audio_import();
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
            let ms = (i128::from(self.position)
                + i128::from(self.store.settings.recording.offset_ms))
            .max(0)
            .min(i128::from(u64::MAX)) as u64;
            let at = sonitrelix_audio_io::ms_to_frames(ms, rate);
            let backing_timeline = match sonitrelix_mixer::project(&self.project, Some(index)) {
                Ok(t) => t,
                Err(e) => {
                    self.error(e);
                    return;
                }
            };
            let mut backing = None;
            let gate = if backing_timeline.info().duration_ms() > self.position {
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
                    start_ms: self.position,
                    start_frame: None,
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
                        backing,
                    });
                    self.status(&self.t("record_starting"));
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
                        "record_starting"
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
                        "record_starting"
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
            let result = self
                .recording
                .as_ref()
                .and_then(|r| r.recorder.try_result());
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
