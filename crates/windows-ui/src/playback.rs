use super::*;
use sonitrelix_audio_io::{Encoding, WavReader};
use sonitrelix_engine::{Engine, PlayRequest, State};
use sonitrelix_session::Media;
use std::sync::mpsc::{self, Receiver, TryRecvError};
pub(super) const IMPORT: usize = 106;
pub(super) const OPEN_LINK: usize = 110;
pub(super) const PLAY: usize = 701;
pub(super) const PAUSE: usize = 702;
pub(super) const STOP: usize = 703;
pub(super) const START: usize = 704;
pub(super) const END: usize = 705;
pub(super) const REPORT: usize = 706;
pub(super) const REPORT_LEVELS: usize = 707;
pub(super) const LOOP_SELECTION: usize = 708;
pub(super) const TIMER: usize = 77;
type ImportResult = Result<PendingImport, String>;
struct PendingImport(Option<(Media, String, Option<PathBuf>)>);
impl PendingImport {
    fn new(media: Media, name: String, original: Option<PathBuf>) -> Self {
        Self(Some((media, name, original)))
    }
    fn accept(mut self) -> (Media, String, Option<PathBuf>) {
        self.0.take().expect("pending import has not been accepted")
    }
}
impl Drop for PendingImport {
    fn drop(&mut self) {
        if let Some((media, _, original)) = &self.0
            && original.as_ref() != Some(&media.path)
        {
            let _ = fs::remove_file(&media.path);
        }
    }
}

fn imported_media(path: PathBuf, info: sonitrelix_audio_io::WavInfo) -> Media {
    Media {
        path,
        sample_rate: info.sample_rate,
        channels: info.channels,
        bits: info.bits,
        frames: info.frames,
        float: info.encoding == Encoding::Float,
    }
}

fn decode_import(
    source: &std::path::Path,
    destination: &std::path::Path,
    cancel: &std::sync::atomic::AtomicBool,
) -> std::io::Result<sonitrelix_audio_io::WavInfo> {
    sonitrelix_audio_io::codecs::decode(source, destination, cancel).or_else(|native_error| {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(native_error);
        }
        let _ = fs::remove_file(destination);
        let exe = sonitrelix_audio_io::ffmpeg::installed().ok_or_else(|| std::io::Error::other(
            format!("{native_error}. FFmpeg fallback unavailable. Place ffmpeg.exe beside Sonitrelix.exe or install it on PATH.")
        ))?;
        sonitrelix_audio_io::ffmpeg::decode(&exe, source, destination, cancel)
    })
}

fn import_remote_media(
    directory: PathBuf,
    url: &str,
    cancel: &std::sync::atomic::AtomicBool,
    unreadable: String,
) -> Result<Media, String> {
    use std::sync::atomic::Ordering::Relaxed;
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let name = format!("linked-{timestamp}-{}", std::process::id());
    let extension = url
        .split(['?', '#'])
        .next()
        .and_then(|s| s.rsplit_once('.'))
        .map(|(_, suffix)| suffix.to_ascii_lowercase())
        .filter(|suffix| {
            [
                "wav", "mp3", "flac", "m4a", "aac", "ogg", "opus", "aiff", "aif",
            ]
            .contains(&suffix.as_str())
        })
        .unwrap_or_else(|| "download".into());
    let temporary = directory.join(format!("{name}-transfer.{extension}"));
    let destination = directory.join(format!("{name}.wav"));
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| error.to_string())?;
    let result = (|| {
        sonitrelix_updater::http::download_media(url, &mut file, cancel)
            .map_err(|error| error.to_string())?;
        drop(file);
        if cancel.load(Relaxed) {
            return Err("Download cancelled".into());
        }
        let info = if let Ok(info) = WavReader::open(&temporary).map(|reader| reader.info) {
            if info.frames == 0 {
                return Err(unreadable);
            }
            fs::rename(&temporary, &destination).map_err(|error| error.to_string())?;
            info
        } else {
            decode_import(&temporary, &destination, cancel).map_err(|_| unreadable)?
        };
        if cancel.load(Relaxed) {
            return Err("Audio import cancelled".into());
        }
        Ok(imported_media(destination.clone(), info))
    })();
    let _ = fs::remove_file(temporary);
    if result.is_err() {
        let _ = fs::remove_file(destination);
    }
    result
}
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
    looping: bool,
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
            looping: false,
            closing: false,
        }
    }
}
impl App {
    pub(super) fn playback_level(&self) -> Option<sonitrelix_engine::Level> {
        self.playback.engine.as_ref().and_then(|engine| {
            let now = engine.snapshot();
            (now.id == self.playback.id
                && matches!(now.state, State::Playing | State::Buffering | State::Paused))
            .then(|| engine.output_level())
        })
    }
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
            let recordings = self.data.join("recordings");

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
                        let is_recording = name
                            .strip_prefix("recording-")
                            .and_then(|s| s.split_once('-'))
                            .is_some_and(|(stamp, pid)| {
                                stamp.parse::<u128>().is_ok() && pid.parse::<u32>().is_ok()
                            })
                            && path
                                .extension()
                                .is_some_and(|s| s.eq_ignore_ascii_case("wav"))
                            && fs::canonicalize(&recordings).ok().as_deref() == path.parent();
                        let stamp = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_nanos();
                        let destination =
                            directory.join(format!("import-{stamp}-{}.wav", std::process::id()));
                        let recovered = if is_recording {
                            fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
                            sonitrelix_audio_io::recover_pcm16_recording(
                                &path,
                                &destination,
                                &cancel,
                            )
                            .map_err(|e| e.to_string())?
                        } else {
                            None
                        };
                        let native_wav = if recovered.is_none()
                            && path
                                .extension()
                                .is_some_and(|s| s.eq_ignore_ascii_case("wav"))
                        {
                            WavReader::open(&path).ok().map(|r| r.info)
                        } else {
                            None
                        };
                        let (path, info) = if let Some(info) = recovered {
                            (destination, info)
                        } else if let Some(info) = native_wav {
                            (path, info)
                        } else {
                            fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
                            let result = decode_import(&path, &destination, &cancel);
                            let info = result.map_err(|error| {
                                let _ = fs::remove_file(&destination);
                                error.to_string()
                            })?;
                            (destination, info)
                        };
                        if info.frames == 0 {
                            return Err("Audio has no frames".into());
                        }
                        Ok(PendingImport::new(
                            imported_media(path, info),
                            name,
                            Some(original),
                        ))
                    })();
                    let _ = tx.send(result);
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

    unsafe fn open_audio_link(&mut self) {
        unsafe {
            if self.import_pending() {
                self.timeline_feedback(&self.t("audio_import_busy"));
                return;
            }
            let mut hint = self.t("audio_link_hint");
            let mut value = String::new();
            loop {
                self.modal.set(true);
                let result = forms::ask(
                    self.hwnd,
                    self.strings.clone(),
                    &self.t("audio_link_title"),
                    vec![forms::Field::Text {
                        label: self.t("audio_link_address"),
                        value,
                    }],
                    &hint,
                );
                self.modal.set(false);
                match result {
                    Ok(Some(mut values)) => {
                        value = values.remove(0);
                        if sonitrelix_updater::http::valid_audio_url(&value) {
                            self.import_audio_link(value);
                            return;
                        }
                        hint = self.t("audio_link_invalid");
                    }
                    Ok(None) => return,
                    Err(error) => {
                        self.error(error);
                        return;
                    }
                }
            }
        }
    }

    pub(super) unsafe fn import_audio_link(&mut self, url: String) {
        unsafe {
            if self.import_pending() {
                self.timeline_feedback(&self.t("audio_import_busy"));
                return;
            }
            let (tx, rx) = mpsc::channel();
            self.playback.import_cancel = Default::default();
            let cancel = self.playback.import_cancel.clone();
            let directory = self.data.join("imports");
            let name = self.t("audio_link_track");
            let unreadable = self.t("audio_link_unreadable");
            let worker = std::thread::Builder::new()
                .name("audio-link-import".into())
                .spawn(move || {
                    let result = import_remote_media(directory, &url, &cancel, unreadable)
                        .map(|media| PendingImport::new(media, name, None));
                    let _ = tx.send(result);
                });
            match worker {
                Ok(_) => {
                    self.playback.import_selection = self.selected_track();
                    self.playback.import_focus = GetFocus();
                    self.playback.import = Some(rx);
                    self.timeline_feedback(&self.t("audio_link_downloading"));
                }
                Err(error) => self.error(error),
            }
        }
    }
    unsafe fn start_playback(
        &mut self,
        index: usize,
        paused: bool,
        repeat: Option<std::ops::Range<u64>>,
    ) {
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
            let looping = repeat.is_some();
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
                    loop_frames: repeat,
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
                    self.playback.looping = looping;
                    self.status(&self.t(if looping {
                        "loop_started"
                    } else {
                        "audio_starting"
                    }));
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
    pub(super) fn update_playback_levels(&self, index: usize) {
        if let Some(controls) = &self.playback.mix_controls {
            let track = &self.project.tracks[index];
            controls.set_levels(index, track.gain, track.pan);
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
            self.playback.looping = false;
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
        self.playback.import = None;
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
            if self.timeline_job.is_some() {
                self.timeline_feedback(&self.t("timeline_busy"));
                return;
            }
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
            ) && self.timeline_job.is_none()
                && let Some(index) = self.playback.active
            {
                self.start_playback(index, self.playback.paused, None);
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
            self.fastplugs_scan = None;
            self.cancel_timeline_job();
            self.microphone_check = None;
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
                && self.timeline_job.is_none()
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
                OPEN_LINK => self.open_audio_link(),
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
                        self.start_playback(index, false, None);
                    } else {
                        self.status(&self.t("audio_no_wav"));
                    }
                }
                LOOP_SELECTION => {
                    if self.playback.looping {
                        self.audio_command(STOP);
                        let message = self.t("loop_stopped");
                        self.status(&message);
                        announcement::announce(GetDlgItem(self.hwnd, 21), &message);
                        return;
                    }
                    let Some(index) = self.selected_track() else {
                        self.status(&self.t("edit_no_track"));
                        return;
                    };
                    let (start, end) = self.selected_range(index);
                    let rate = self.project.tracks[index].sample_rate;
                    let scale = |f| scale_frame(f, rate, self.project.sample_rate);
                    let range = scale(start)..scale(end);
                    if range.is_empty() || self.editor.bracket_start.is_some() {
                        let message = self.t("loop_select_range");
                        self.status(&message);
                        announcement::announce(GetDlgItem(self.hwnd, 21), &message);
                        return;
                    }
                    self.start_playback(index, false, Some(range));
                    if self.playback.looping {
                        let message = self.t("loop_started");
                        announcement::announce(GetDlgItem(self.hwnd, 21), &message);
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
                let key = if self.playback.looping && snapshot.state == State::Playing {
                    "loop_started"
                } else {
                    state_key(snapshot.state)
                };
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
            self.poll_microphone_check();
            self.poll_timeline_job();
            self.poll_updates();
            if self.recording.is_some() {
                self.poll_recording();
                if self.recording.is_some() {
                    return;
                }
            }
            let mut changed = false;
            if self.playback.closing {
                if let Some(result) = self.playback.import.as_ref().map(Receiver::try_recv) {
                    match result {
                        Ok(_) | Err(TryRecvError::Disconnected) => self.playback.import = None,
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
                    && self.timeline_job.is_none()
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
                            Ok(pending) => {
                                let (media, name, original) = pending.accept();
                                self.checkpoint();
                                if let Some(original) = original {
                                    self.project.original_sources.push(original);
                                }
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
                                self.timeline_feedback(&self.t("audio_imported"));
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
                        self.playback.looping = false;
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
            self.poll_fastplugs();
        }
    }
}
pub(super) fn track_duration(t: &sonitrelix_session::Track) -> u64 {
    sonitrelix_audio_io::frames_to_ms(t.frames(), t.sample_rate)
}
pub(super) fn scale_frame(frame: u64, from: u32, to: u32) -> u64 {
    (u128::from(frame) * u128::from(to) / u128::from(from)).min(u128::from(u64::MAX)) as u64
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

#[cfg(test)]
mod link_tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader, Write},
        net::TcpListener,
        thread,
        time::{Duration, Instant, SystemTime},
    };

    #[test]
    fn linked_audio_is_cached_and_available_for_the_overdub_mix_after_reopen() {
        let stamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("sonitrelix-link-test-{stamp}"));
        fs::create_dir(&root).unwrap();
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let source = root.join("source.wav");
        let mut writer = sonitrelix_audio_io::WavWriter::create(&source, 48_000, 1).unwrap();
        writer.write_frames(&[0.25; 4800]).unwrap();
        writer.finish().unwrap();
        let audio = fs::read(&source).unwrap();
        let server = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "WinHTTP did not connect");
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("{error}"),
                }
            };
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut reader = BufReader::new(&mut socket);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            assert!(line.starts_with("GET /track.wav?access=local-test "));
            loop {
                line.clear();
                if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
                    break;
                }
            }
            write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: audio/wav\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", audio.len()).unwrap();
            socket.write_all(&audio).unwrap();
        });
        unsafe {
            let mut app = super::super::tests::native_test_app(&root, "SonitrelixAudioLinkTest");
            app.import_audio_link(format!(
                "http://127.0.0.1:{port}/track.wav?access=local-test"
            ));
            let deadline = Instant::now() + Duration::from_secs(8);
            while app.import_pending() {
                assert!(
                    Instant::now() < deadline,
                    "Audio link import did not finish"
                );
                thread::sleep(Duration::from_millis(10));
                app.poll_audio();
            }
            server.join().unwrap();
            assert_eq!(app.project.tracks[0].name, app.t("audio_link_track"));
            assert!(app.project.original_sources.is_empty());
            let imported = app.project.tracks[0].sources().next().unwrap().path.clone();
            assert_eq!(imported.parent(), Some(root.join("imports").as_path()));
            assert_eq!(fs::read_dir(root.join("imports")).unwrap().count(), 1);
            app.project.add_track("Vocals".into());
            app.project.tracks[1].armed = true;
            let path = root.join("mix.sonitrelix");
            app.project.save(&path).unwrap();
            let settings = fs::read_to_string(&path).unwrap();
            assert!(!settings.contains("access=local-test"));
            let reopened = Project::load(&path).unwrap();
            let backing = sonitrelix_mixer::project(&reopened, Some(1)).unwrap();
            assert_eq!(backing.info().frames, 4800);
            let mut sample = [0.; 2];
            assert_eq!(
                sonitrelix_audio_io::TimelineReader::new(backing)
                    .read_frames(&mut sample)
                    .unwrap(),
                1
            );
            assert!(sample.iter().all(|v| *v > 0.15));
            DestroyWindow(app.hwnd);
        }
        fs::remove_dir_all(root).unwrap();
    }
}
