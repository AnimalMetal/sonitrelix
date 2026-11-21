use super::*;
use sonitrelix_audio_io::{Encoding, WavReader};
use sonitrelix_engine::{Engine, PlayRequest, State};
use sonitrelix_session::Media;
use std::sync::mpsc::{self, Receiver, TryRecvError};
pub(super) const IMPORT: usize = 106;
pub(super) const PLAY: usize = 701;
pub(super) const PAUSE: usize = 702;
pub(super) const STOP: usize = 703;
pub(super) const START: usize = 704;
pub(super) const END: usize = 705;
pub(super) const REPORT: usize = 706;
pub(super) const TIMER: usize = 77;
type ImportResult = Result<(Media, String, PathBuf), String>;
pub(super) struct Playback {
    engine: Option<Engine>,
    import: Option<Receiver<ImportResult>>,
    import_selection: Option<usize>,
    import_cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    import_focus: HWND,
    active: Option<usize>,
    mix_controls: Option<sonitrelix_audio_io::LiveMix>,
    id: u64,
    pub(super) state: State,
    paused: bool,
    pub closing: bool,
}
impl Default for Playback {
    fn default() -> Self {
        Self {
            engine: None,
            import: None,
            import_selection: None,
            import_cancel: Default::default(),
            import_focus: null_mut(),
            active: None,
            mix_controls: None,
            id: 0,
            state: State::Stopped,
            paused: false,
            closing: false,
        }
    }
}
impl App {
    pub(super) unsafe fn selected_track(&self) -> Option<usize> {
        unsafe {
            let selected = SendMessageW(GetDlgItem(self.hwnd, 11), 0x188, 0, 0);
            if selected >= 0 && (selected as usize) < self.project.tracks.len() {
                Some(selected as usize)
            } else {
                None
            }
        }
    }
    pub(super) unsafe fn import_wav(&mut self, path: PathBuf) {
        unsafe {
            if self.playback.import.is_some() {
                self.status(&self.t("audio_import_busy"));
                return;
            }
            let (tx, rx) = mpsc::channel();
            self.playback.import_cancel = Default::default();
            let cancel = self.playback.import_cancel.clone();
            let directory = self.data.join("imports");

            match std::thread::Builder::new()
                .name("wav-import".into())
                .spawn(move || {
                    let result = (|| {
                        let path = fs::canonicalize(path).map_err(|e| e.to_string())?;
                        let original = path.clone();
                        let name = path
                            .file_stem()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "Audio".into());
                        let native_wav = if path
                            .extension()
                            .is_some_and(|s| s.eq_ignore_ascii_case("wav"))
                        {
                            WavReader::open(&path).ok().map(|r| r.info)
                        } else {
                            None
                        };
                        let (path, info) = if let Some(info) = native_wav {
                            (path, info)
                        } else {
                            fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
                            let stamp = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_nanos();
                            let destination = directory
                                .join(format!("import-{stamp}-{}.wav", std::process::id()));
                            let result = sonitrelix_audio_io::codecs::decode(&path, &destination, &cancel)
                                .or_else(|native_error| {
                                    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                                        return Err(native_error);
                                    }
                                    let _ = fs::remove_file(&destination);
                                    match sonitrelix_audio_io::ffmpeg::installed() {
                                        Some(exe) => sonitrelix_audio_io::ffmpeg::decode(&exe, &path, &destination, &cancel),
                                        None => Err(std::io::Error::other(format!(
                                            "{native_error}. FFmpeg fallback unavailable. Place ffmpeg.exe beside Sonitrelix.exe or install it on PATH."
                                        ))),
                                    }
                                });
                            let info = result.map_err(|error| {
                                let _ = fs::remove_file(&destination);
                                error.to_string()
                            })?;
                            (destination, info)
                        };
                        if info.frames == 0 {
                            return Err("Audio has no frames".into());
                        }
                        Ok((
                            Media {
                                path,
                                sample_rate: info.sample_rate,
                                channels: info.channels,
                                bits: info.bits,
                                frames: info.frames,
                                float: info.encoding == Encoding::Float,
                            },
                            name,
                            original,
                        ))
                    })();
                    if let Err(mpsc::SendError(Ok((media, _, original)))) = tx.send(result)
                        && media.path != original
                    {
                        let _ = fs::remove_file(media.path);
                    }
                }) {
                Ok(_) => {
                    self.playback.import_selection = self.selected_track();
                    self.playback.import_focus = GetFocus();
                    self.playback.import = Some(rx);
                    self.status(&self.t("audio_importing"));
                }
                Err(e) => self.error(e),
            }
        }
    }
    unsafe fn start_playback(&mut self, index: usize, paused: bool) {
        unsafe {
            let (timeline, controls) = match sonitrelix_mixer::live_project(&self.project) {
                Ok((t, controls)) if t.info().frames > 0 => (t, controls),
                Ok(_) => {
                    self.status(&self.t("mix_empty"));
                    return;
                }
                Err(e) => {
                    self.error(e);
                    return;
                }
            };
            if self.playback.engine.is_none() {
                match Engine::new() {
                    Ok(engine) => self.playback.engine = Some(engine),
                    Err(e) => {
                        self.error(e);
                        return;
                    }
                }
            }
            let request =
                PlayRequest {
                    gate: None,
                    timeline,
                    start_ms: self.position,
                    start_frame: self.editor.exact_cursor.filter(|(i, _)| *i == index).map(
                        |(_, f)| {
                            (u128::from(f) * u128::from(self.project.sample_rate)
                                / u128::from(self.project.tracks[index].sample_rate))
                                as u64
                        },
                    ),
                    output_id: self.store.settings.devices.output_id.clone(),
                    buffer_frames: self.store.settings.recording.buffer_frames,
                    paused,
                };
            let result = self
                .playback
                .engine
                .as_mut()
                .expect("created engine")
                .play(request);
            match result {
                Ok(id) => {
                    self.playback.mix_controls = Some(controls);
                    self.playback.id = id;
                    self.playback.active = Some(index);
                    self.playback.state = State::Starting;
                    self.playback.paused = paused;
                    self.status(&self.t("audio_starting"));
                }
                Err(e) => self.error(e),
            }
        }
    }
    pub(super) fn update_playback_mix(&self) {
        if let Some(controls) = &self.playback.mix_controls {
            controls.update(sonitrelix_mixer::audible_tracks(&self.project));
        }
    }
    pub(super) unsafe fn stop_playback(&mut self) {
        unsafe {
            if let Some(engine) = &self.playback.engine {
                let snapshot = engine.snapshot();
                if snapshot.id == self.playback.id {
                    self.position = snapshot.position_ms();
                    self.editor.exact_cursor = None;
                    SetWindowTextW(
                        GetDlgItem(self.hwnd, 15),
                        wide(&format_time(self.position)).as_ptr(),
                    );
                }
                engine.stop();
            }
            self.playback.state = State::Stopped;
            self.playback.mix_controls = None;
            self.playback.paused = false;
            self.playback.id = 0;
        }
    }
    pub(super) unsafe fn reset_playback(&mut self) {
        unsafe {
            self.stop_playback();
            self.playback.id = 0;
            self.playback.active = None;
            SetWindowTextW(
                GetDlgItem(self.hwnd, 23),
                wide(&self.t("audio_stopped")).as_ptr(),
            );
        }
    }
    pub(super) fn cancel_audio_import(&mut self) {
        self.playback
            .import_cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(receiver) = self.playback.import.take()
            && let Ok(Ok((media, _, original))) = receiver.try_recv()
            && media.path != original
        {
            let _ = fs::remove_file(media.path);
        }
    }
    pub(super) fn import_pending(&self) -> bool {
        self.playback.import.is_some()
    }
    pub(super) fn align_cursor_to_playhead(&mut self) {
        if matches!(
            self.playback.state,
            State::Playing | State::Buffering | State::Paused
        ) && let Some(s) = self
            .playback
            .engine
            .as_ref()
            .map(|e| e.snapshot())
            .filter(|s| s.id == self.playback.id)
        {
            self.position = s.position_ms();
            self.editor.exact_cursor = None;
        }
    }
    pub(super) fn track_cursor_frame(&self, index: usize) -> u64 {
        let rate = self.project.tracks[index].sample_rate;
        if matches!(
            self.playback.state,
            State::Starting | State::Playing | State::Buffering | State::Paused
        ) && let Some(snapshot) = self.playback.engine.as_ref().map(Engine::snapshot)
            && snapshot.id == self.playback.id
            && snapshot.sample_rate != 0
        {
            return (u128::from(snapshot.frame) * u128::from(rate)
                / u128::from(snapshot.sample_rate)) as u64;
        }
        self.editor
            .exact_cursor
            .filter(|(i, _)| *i == index)
            .map(|(_, frame)| frame)
            .unwrap_or_else(|| sonitrelix_audio_io::ms_to_frames(self.position, rate))
    }
    pub(super) unsafe fn toggle_playback(&mut self) {
        unsafe {
            if self.recording.is_some() {
                self.stop_recording();
                return;
            }
            self.audio_command(
                if matches!(
                    self.playback.state,
                    State::Starting | State::Playing | State::Paused | State::Buffering
                ) {
                    STOP
                } else {
                    PLAY
                },
            );
        }
    }
    pub(super) unsafe fn seek_playback(&mut self) {
        unsafe {
            if matches!(
                self.playback.state,
                State::Starting | State::Playing | State::Buffering | State::Paused
            ) && let Some(index) = self.playback.active
            {
                self.start_playback(index, self.playback.paused);
            }
        }
    }
    pub(super) unsafe fn close_audio(&mut self) {
        unsafe {
            if self.recording.is_some() {
                self.recording_close_pending = true;
                self.stop_recording();
                return;
            }
            if self.playback.closing || !self.can_close() {
                return;
            }
            self.playback.closing = true;
            self.maintenance.clear();
            self.poll_maintenance();
            self.exporter.cancel();
            self.updates.cancel();
            self.playback
                .import_cancel
                .store(true, std::sync::atomic::Ordering::Relaxed);
            if let Some(engine) = &mut self.playback.engine {
                engine.shutdown();
                self.status(&self.t("audio_closing"));
            } else if !self.exporter.pending()
                && !self.maintenance.pending()
                && !self.updates.pending()
                && self.playback.import.is_none()
            {
                DestroyWindow(self.hwnd);
            }
        }
    }
    pub(super) unsafe fn audio_command(&mut self, id: usize) {
        unsafe {
            if self.recording.is_some() {
                match id {
                    STOP => self.stop_recording(),
                    PAUSE => self.pause_recording(),
                    REPORT => self.report_recording(),
                    _ => self.status(&self.t("record_edit_blocked")),
                }
                return;
            }
            match id {
                IMPORT => {
                    if self.playback.import.is_some() {
                        self.status(&self.t("audio_import_busy"));
                    } else if let Some(path) = self.choose_file(false, true) {
                        self.import_wav(path);
                    }
                }
                PLAY => {
                    if self.playback.paused
                        && let Some(engine) = &self.playback.engine
                    {
                        engine.pause(false);
                        self.playback.paused = false;
                        return;
                    }
                    if matches!(
                        self.playback.state,
                        State::Playing | State::Starting | State::Buffering
                    ) && !self.playback.paused
                    {
                        return;
                    }
                    if let Some(index) = self.selected_track() {
                        if self.position >= project_duration(&self.project) {
                            self.position = 0;
                            self.editor.exact_cursor = Some((index, 0));
                            SetWindowTextW(
                                GetDlgItem(self.hwnd, 15),
                                wide(&format_time(0)).as_ptr(),
                            );
                        }
                        self.start_playback(index, false);
                    } else {
                        self.status(&self.t("audio_no_wav"));
                    }
                }
                PAUSE => {
                    if let Some(engine) = &self.playback.engine
                        && matches!(
                            self.playback.state,
                            State::Playing | State::Starting | State::Paused | State::Buffering
                        )
                    {
                        self.playback.paused ^= true;
                        engine.pause(self.playback.paused);
                    }
                }
                STOP => {
                    self.stop_playback();
                    self.status(&self.t("audio_stopped"));
                    self.update_playback_readout(true);
                }
                START | END => {
                    let index = if matches!(
                        self.playback.state,
                        State::Starting | State::Playing | State::Buffering | State::Paused
                    ) {
                        self.playback.active
                    } else {
                        self.selected_track().or(self.playback.active)
                    };
                    self.position = if id == START {
                        0
                    } else {
                        index
                            .and_then(|i| self.project.tracks.get(i))
                            .map(track_duration)
                            .unwrap_or(0)
                    };
                    self.editor.exact_cursor = index.map(|i| {
                        (
                            i,
                            if id == START {
                                0
                            } else {
                                self.project.tracks[i].frames()
                            },
                        )
                    });
                    SetWindowTextW(
                        GetDlgItem(self.hwnd, 15),
                        wide(&format_time(self.position)).as_ptr(),
                    );
                    self.seek_playback();
                }
                REPORT => {
                    self.update_playback_readout(true);
                    let readout = GetDlgItem(self.hwnd, 23);
                    SetFocus(readout);
                    SendMessageW(readout, 0xb1, 0, -1);
                }
                _ => (),
            }
        }
    }
    unsafe fn update_playback_readout(&self, force: bool) {
        unsafe {
            let h = GetDlgItem(self.hwnd, 23);
            if !force && GetFocus() == h {
                return;
            }
            let text = if let Some(snapshot) = self
                .playback
                .engine
                .as_ref()
                .map(|e| e.snapshot())
                .filter(|s| s.id == self.playback.id && self.playback.id != 0)
            {
                let key = state_key(snapshot.state);
                let name = self.t("mix_name");
                let base = format!(
                    "{}: {}. {} / {}",
                    self.t(key),
                    name,
                    format_time(snapshot.position_ms()),
                    format_time(sonitrelix_audio_io::frames_to_ms(
                        snapshot.total,
                        snapshot.sample_rate
                    ))
                );
                if self.verbose {
                    format!(
                        "{}. {}: {}. {}: {}",
                        base,
                        self.t("audio_underruns"),
                        snapshot.underruns,
                        self.t("audio_device_buffer"),
                        snapshot.device_buffer_frames
                    )
                } else {
                    base
                }
            } else {
                let duration = self
                    .selected_track()
                    .or(self.playback.active)
                    .and_then(|i| self.project.tracks.get(i))
                    .map(track_duration)
                    .unwrap_or(0);
                format!(
                    "{}: {} / {}",
                    self.t("audio_stopped"),
                    format_time(self.position),
                    format_time(duration)
                )
            };
            SetWindowTextW(h, wide(&text).as_ptr());
        }
    }
    pub(super) unsafe fn poll_audio(&mut self) {
        unsafe {
            self.poll_export();
            self.poll_maintenance();
            if self.recording.is_some() {
                self.poll_recording();
                if self.recording.is_some() {
                    return;
                }
            }
            let mut changed = false;
            self.poll_updates();
            if self.playback.closing {
                if let Some(result) = self.playback.import.as_ref().map(Receiver::try_recv) {
                    match result {
                        Ok(Ok((media, _, original))) => {
                            self.playback.import = None;
                            if media.path != original {
                                let _ = fs::remove_file(media.path);
                            }
                        }
                        Ok(Err(_)) | Err(TryRecvError::Disconnected) => self.playback.import = None,
                        Err(TryRecvError::Empty) => {}
                    }
                }
                if self
                    .playback
                    .engine
                    .as_ref()
                    .is_none_or(|e| e.shutdown_complete())
                    && !self.exporter.pending()
                    && !self.maintenance.pending()
                    && !self.updates.pending()
                    && self.playback.import.is_none()
                {
                    DestroyWindow(self.hwnd);
                }
                return;
            }
            if let Some(result) = self.playback.import.as_ref().map(|rx| rx.try_recv()) {
                match result {
                    Ok(result) => {
                        self.playback.import = None;
                        match result {
                            Ok((media, name, original)) => {
                                self.checkpoint();
                                self.project.original_sources.push(original);
                                self.project.add_track(name);
                                self.project
                                    .tracks
                                    .last_mut()
                                    .expect("added track")
                                    .set_media(media);
                                let select_imported = self.selected_track()
                                    == self.playback.import_selection
                                    && GetFocus() == self.playback.import_focus;
                                self.refresh();
                                if select_imported {
                                    SendMessageW(
                                        GetDlgItem(self.hwnd, 11),
                                        0x186,
                                        self.project.tracks.len() - 1,
                                        0,
                                    );
                                    self.track_selected();
                                }
                                // The running playback snapshot must include the new track.
                                if matches!(
                                    self.playback.state,
                                    State::Starting
                                        | State::Playing
                                        | State::Buffering
                                        | State::Paused
                                ) {
                                    self.align_cursor_to_playhead();
                                    self.seek_playback();
                                }
                                self.status(&self.t("audio_imported"));
                                self.log("wav_imported");
                            }
                            Err(e) => {
                                self.status(&self.t("audio_import_error"));
                                self.error_in("import audio", e);
                            }
                        }
                    }
                    Err(TryRecvError::Disconnected) => {
                        self.playback.import = None;
                        self.status(&self.t("pref_worker_error"));
                        self.log_error("import audio", "Worker disconnected");
                    }
                    Err(TryRecvError::Empty) => (),
                }
            }
            while let Some(event) = self.playback.engine.as_ref().and_then(|e| e.try_event()) {
                match event {
                    sonitrelix_engine::Event::Error { id, message } if id == self.playback.id => {
                        self.playback.state = State::Error;
                        self.playback.paused = false;
                        self.status(&self.t("audio_error"));
                        self.error_in("playback", message);
                    }
                    _ => (),
                }
            }
            if let Some(snapshot) = self
                .playback
                .engine
                .as_ref()
                .map(|e| e.snapshot())
                .filter(|s| s.id == self.playback.id && self.playback.id != 0)
            {
                if snapshot.state != self.playback.state {
                    changed = true;
                    self.playback.state = snapshot.state;
                    self.status(&self.t(state_key(snapshot.state)));
                }
                if snapshot.state == State::Ended {
                    self.position = snapshot.position_ms();
                    if GetFocus() != GetDlgItem(self.hwnd, 15) {
                        SetWindowTextW(
                            GetDlgItem(self.hwnd, 15),
                            wide(&format_time(self.position)).as_ptr(),
                        );
                    }
                }
            }
            self.update_playback_readout(changed);
        }
    }
}
pub(super) fn track_duration(t: &sonitrelix_session::Track) -> u64 {
    sonitrelix_audio_io::frames_to_ms(t.frames(), t.sample_rate)
}
fn state_key(state: State) -> &'static str {
    match state {
        State::Stopped => "audio_stopped",
        State::Starting => "audio_starting",
        State::Playing => "audio_playing",
        State::Paused => "audio_paused",
        State::Buffering => "audio_buffering",
        State::Ended => "audio_ended",
        State::Error => "audio_error",
    }
}

pub(super) fn project_duration(p: &sonitrelix_session::Project) -> u64 {
    p.tracks.iter().map(track_duration).max().unwrap_or(0)
}
