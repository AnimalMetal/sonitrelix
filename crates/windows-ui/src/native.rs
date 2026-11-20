#[cfg(test)]
#[path = "accessibility_tests.rs"]
mod accessibility_tests;
#[path = "editing_ui.rs"]
mod editing_ui;
#[path = "help.rs"]
mod help;
#[path = "playback.rs"]
mod playback;
#[path = "preferences.rs"]
mod preferences;
#[path = "recording_ui.rs"]
mod recording_ui;
#[path = "shortcuts.rs"]
mod shortcuts;
#[path = "win32.rs"]
mod w;
use editing_ui::*;
use playback::{END, IMPORT, PAUSE, PLAY, REPORT, START, STOP};
use sonitrelix_session::{Project, parse_time};
use sonitrelix_settings::Store;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    ptr::{null, null_mut},
};
use w::*;

const NEW: usize = 101;
const OPEN: usize = 102;
const SAVE: usize = 103;
const SAVE_AS: usize = 104;
const EXIT: usize = 105;
const ADD: usize = 201;
const DELETE: usize = 202;
const RENAME: usize = 203;
const MUTE: usize = 204;
const SOLO: usize = 205;
const ARM: usize = 206;
const UNDO: usize = 301;
const REDO: usize = 302;
const PREFERENCES: usize = 404;
const HELP: usize = 501;
const ABOUT: usize = 502;
const CHANGELOG: usize = 503;
const APPLY: usize = 601;
struct App {
    hwnd: HWND,
    project: Project,
    path: Option<PathBuf>,
    dirty: bool,
    undo: Vec<History>,
    redo: Vec<History>,
    editor: Editor,
    strings: serde_json::Value,
    es: bool,
    verbose: bool,
    data: PathBuf,
    controls: Vec<HWND>,
    position: u64,
    selection: (u64, u64),
    selection_anchor: Option<u64>,
    store: Store,
    preferences: Option<preferences::Dialog>,
    playback: playback::Playback,
    modal: std::cell::Cell<bool>,
    recording: Option<recording_ui::Recording>,
    recording_close_pending: bool,
    help: Option<help::Viewer>,
}
impl App {
    fn t(&self, key: &str) -> String {
        self.strings[key].as_str().unwrap_or(key).to_owned()
    }
    fn checkpoint(&mut self) {
        self.undo.push(self.history());
        self.redo.clear();
        self.dirty = true;
    }
    fn log(&self, event: &str) {
        if let Ok(mut f) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.data.join("session.jsonl"))
        {
            let _ = writeln!(
                f,
                "{}",
                serde_json::json!({"event":event,"unix_ms":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d|d.as_millis()).unwrap_or(0)})
            );
        }
    }
    fn error(&self, e: impl std::fmt::Display) {
        self.log(&format!("error: {e}"));
        unsafe {
            self.modal.set(true);
            MessageBoxW(
                self.preferences
                    .as_ref()
                    .map(|p| p.hwnd)
                    .unwrap_or(self.hwnd),
                wide(&format!("{}\n{e}", self.t("error"))).as_ptr(),
                wide("Sonitrelix").as_ptr(),
                0x10,
            );
            self.modal.set(false);
        }
    }
    unsafe fn child(&mut self, class: &str, text: &str, id: usize, style: u32) -> HWND {
        let h = unsafe {
            CreateWindowExW(
                0,
                wide(class).as_ptr(),
                wide(text).as_ptr(),
                0x50000000 | style,
                0,
                0,
                1,
                1,
                self.hwnd,
                id as HMENU,
                GetModuleHandleW(null()),
                null_mut(),
            )
        };
        if !h.is_null() {
            unsafe {
                SendMessageW(h, 0x30, GetStockObject(17) as usize, 1);
            }
        }
        self.controls.push(h);
        h
    }
    unsafe fn build(&mut self) {
        unsafe {
            for c in self.controls.drain(..) {
                DestroyWindow(c);
            }
            self.child("STATIC", &self.t("tracks"), 10, 0);
            self.child(
                "LISTBOX",
                "",
                11,
                0x00800000 | 0x00200000 | 0x00010000 | 0x1 | 0x100,
            );
            self.child("STATIC", &self.t("clips_label"), 24, 0);
            self.child(
                "LISTBOX",
                "",
                25,
                0x00800000 | 0x00200000 | 0x00010000 | 0x1 | 0x100,
            );
            self.child("STATIC", &self.t("name"), 12, 0);
            self.child("EDIT", "", 13, 0x00800000 | 0x10000 | 0x80);
            self.child("BUTTON", &self.t("add"), ADD, 0x10000);
            for (key, id) in [
                ("play_button", PLAY),
                ("pause_button", PAUSE),
                ("stop_button", STOP),
                ("record_button", recording_ui::RECORD),
            ] {
                self.child("BUTTON", &self.t(key), id, 0x10000);
            }
            self.child("STATIC", &self.t("audio_position"), 22, 0);
            self.child(
                "EDIT",
                &self.t("audio_stopped"),
                23,
                0x00800000 | 0x10000 | 0x800 | 0x80,
            );
            for (label, id) in [("position", 14), ("start", 16), ("end", 18)] {
                self.child("STATIC", &self.t(label), id, 0);
                self.child("EDIT", "00:00:00.000", id + 1, 0x00800000 | 0x10000 | 0x80);
            }
            self.child("BUTTON", &self.t("apply"), APPLY, 0x10000);
            self.child("STATIC", &self.t("status"), 20, 0);
            self.child(
                "EDIT",
                &self.t("ready"),
                21,
                0x00800000 | 0x10000 | 0x800 | 0x80,
            );
            let menu = CreateMenu();
            for (key, items) in [
                (
                    "file",
                    vec![
                        ("new", NEW),
                        ("open", OPEN),
                        ("import_wav", IMPORT),
                        ("save", SAVE),
                        ("save_as", SAVE_AS),
                        ("exit", EXIT),
                    ],
                ),
                (
                    "edit",
                    vec![
                        ("undo", UNDO),
                        ("redo", REDO),
                        ("cut", CUT),
                        ("copy", COPY),
                        ("paste", PASTE),
                        ("delete_audio", DELETE_AUDIO),
                        ("select_all", SELECT_ALL),
                        ("split", SPLIT),
                        ("trim", TRIM),
                        ("silence", SILENCE),
                        ("insert_silence", INSERT_SILENCE),
                        ("read_selection", READ_SELECTION),
                    ],
                ),
                (
                    "track",
                    vec![
                        ("new_track", ADD_EMPTY),
                        ("rename", RENAME_FOCUS),
                        ("duplicate_track", DUPLICATE),
                        ("move_track_up", MOVE_UP),
                        ("move_track_down", MOVE_DOWN),
                        ("delete", DELETE),
                        ("mute", MUTE),
                        ("solo", SOLO),
                        ("arm", ARM),
                    ],
                ),
                (
                    "transport",
                    vec![
                        ("play", PLAY),
                        ("pause", PAUSE),
                        ("stop", STOP),
                        ("go_start", START),
                        ("go_end", END),
                        ("report_position", REPORT),
                        ("record", recording_ui::RECORD),
                    ],
                ),
                ("tools", vec![("preferences", PREFERENCES)]),
                (
                    "help_menu",
                    vec![("help", HELP), ("changelog", CHANGELOG), ("about", ABOUT)],
                ),
            ] {
                let sub = CreatePopupMenu();
                for (label, id) in items {
                    AppendMenuW(sub, 0, id, wide(&self.t(label)).as_ptr());
                }
                AppendMenuW(menu, 0x10, sub as usize, wide(&self.t(key)).as_ptr());
            }
            let old = GetMenu(self.hwnd);
            SetMenu(self.hwnd, menu);
            if !old.is_null() {
                DestroyMenu(old);
            }
            DrawMenuBar(self.hwnd);
            self.refresh();
            layout(self.hwnd);
            SetFocus(GetDlgItem(self.hwnd, 11));
        }
    }
    unsafe fn refresh(&self) {
        unsafe {
            let list = GetDlgItem(self.hwnd, 11);
            let selected = SendMessageW(list, 0x188, 0, 0);
            SendMessageW(list, 0x184, 0, 0);
            for (i, t) in self.project.tracks.iter().enumerate() {
                let mut row = format!(
                    "{} {}, {}, {}, {}, {}",
                    self.t("track_word"),
                    i + 1,
                    t.name,
                    self.t(if t.mute { "muted" } else { "unmuted" }),
                    self.t(if t.solo { "solo_on" } else { "solo_off" }),
                    self.t(if t.armed { "armed" } else { "unarmed" })
                );
                if !t.clips.is_empty() {
                    row.push_str(&format!(
                        ", {}, {}",
                        self.t(if t.channels == 1 {
                            "audio_mono"
                        } else {
                            "audio_stereo"
                        }),
                        format_time(playback::track_duration(t))
                    ));
                }
                SendMessageW(list, 0x180, 0, wide(&row).as_ptr() as isize);
            }
            if !self.project.tracks.is_empty() {
                SendMessageW(
                    list,
                    0x186,
                    selected.max(0).min(self.project.tracks.len() as isize - 1) as usize,
                    0,
                );
            }
            let title = format!(
                "{}{} — Sonitrelix",
                self.path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or(self.t("untitled")),
                if self.dirty { " *" } else { "" }
            );
            SetWindowTextW(self.hwnd, wide(&title).as_ptr());
            self.refresh_clips();
        }
    }
    unsafe fn text(&self, id: i32) -> String {
        unsafe {
            let h = GetDlgItem(self.hwnd, id);
            let mut b = vec![0; GetWindowTextLengthW(h) as usize + 1];
            let n = GetWindowTextW(h, b.as_mut_ptr(), b.len() as i32);
            String::from_utf16_lossy(&b[..n as usize])
        }
    }
    unsafe fn status(&self, s: &str) {
        unsafe {
            SetWindowTextW(GetDlgItem(self.hwnd, 21), wide(s).as_ptr());
        }
    }
    unsafe fn file(&self, save: bool) -> Option<PathBuf> {
        unsafe { self.choose_file(save, false) }
    }
    unsafe fn choose_file(&self, save: bool, audio: bool) -> Option<PathBuf> {
        use std::os::windows::ffi::OsStringExt;
        unsafe {
            let mut b = vec![0u16; 32768];
            if !audio && let Some(p) = &self.path {
                let s: Vec<_> = p.as_os_str().to_string_lossy().encode_utf16().collect();
                let n = s.len().min(b.len() - 1);
                b[..n].copy_from_slice(&s[..n]);
            }
            let filter = wide(if audio {
                "WAV (*.wav)\0*.wav\0\0"
            } else {
                "Sonitrelix (*.sonitrelix)\0*.sonitrelix\0\0"
            });
            let ext = wide(if audio { "wav" } else { "sonitrelix" });
            let title = wide(
                &self
                    .t(if audio {
                        "import_wav"
                    } else if save {
                        "save_as"
                    } else {
                        "open"
                    })
                    .split('\t')
                    .next()
                    .unwrap_or("Sonitrelix")
                    .replace('&', ""),
            );
            let mut o: OPENFILENAMEW = std::mem::zeroed();
            o.size = std::mem::size_of::<OPENFILENAMEW>() as u32;
            o.owner = self.hwnd;
            o.filter = filter.as_ptr();
            o.file = b.as_mut_ptr();
            o.max_file = b.len() as u32;
            o.default_extension = ext.as_ptr();
            o.title = title.as_ptr();
            o.flags = 0x80000 | 0x8 | if save { 2 } else { 0x1000 };
            self.modal.set(true);
            let ok = if save {
                GetSaveFileNameW(&mut o)
            } else {
                GetOpenFileNameW(&mut o)
            };
            self.modal.set(false);
            if ok == 0 {
                let code = CommDlgExtendedError();
                if code != 0 {
                    self.error(format!("Windows dialog: {code}"));
                }
                return None;
            }
            let n = b.iter().position(|c| *c == 0).unwrap_or(b.len());
            Some(PathBuf::from(std::ffi::OsString::from_wide(&b[..n])))
        }
    }
    unsafe fn save(&mut self, save_as: bool) -> bool {
        let path = if save_as || self.path.is_none() {
            unsafe { self.file(true) }
        } else {
            self.path.clone()
        };
        if let Some(p) = path {
            match self.project.save(&p) {
                Ok(()) => {
                    self.path = Some(p);
                    self.dirty = false;
                    self.log("project_saved");
                    unsafe {
                        self.refresh();
                        self.status(&self.t("saved"));
                    }
                    return true;
                }
                Err(e) => self.error(e),
            }
        }
        false
    }
    unsafe fn can_close(&mut self) -> bool {
        if !self.dirty {
            return true;
        }
        self.modal.set(true);
        let choice = unsafe {
            MessageBoxW(
                self.hwnd,
                wide(&self.t("unsaved")).as_ptr(),
                wide("Sonitrelix").as_ptr(),
                3 | 0x30,
            )
        };
        self.modal.set(false);
        match choice {
            6 => unsafe { self.save(false) },
            7 => true,
            _ => false,
        }
    }
    unsafe fn command(&mut self, id: usize) {
        unsafe {
            if self.playback.closing {
                return;
            }
            if self.recording.is_some()
                && ![
                    recording_ui::RECORD,
                    STOP,
                    PAUSE,
                    REPORT,
                    PREFERENCES,
                    HELP,
                    ABOUT,
                    CHANGELOG,
                    EXIT,
                    CLEAR_SELECTION,
                ]
                .contains(&id)
            {
                self.status(&self.t("record_edit_blocked"));
                return;
            }
            match id {
                recording_ui::RECORD => self.record_command(),
                COPY | CUT | PASTE | DELETE_AUDIO | SELECT_ALL | SPLIT | TRIM | SILENCE
                | INSERT_SILENCE | ADD_EMPTY | DUPLICATE | MOVE_UP | MOVE_DOWN | RENAME_FOCUS
                | CLEAR_SELECTION | READ_SELECTION => self.edit_command(id),
                IMPORT | PLAY | PAUSE | STOP | START | END | REPORT => self.audio_command(id),
                NEW => {
                    if self.can_close() {
                        self.reset_playback();
                        self.cancel_audio_import();
                        self.project = Project {
                            sample_rate: self.store.settings.general.new_project_sample_rate,
                            ..Project::default()
                        };
                        self.path = None;
                        self.dirty = false;
                        self.undo.clear();
                        self.redo.clear();
                        self.position = 0;
                        self.selection = (0, 0);
                        self.selection_anchor = None;
                        self.editor.exact_selection = None;
                        self.editor.exact_cursor = None;
                        self.editor.renaming = false;
                        self.build();
                    }
                }
                OPEN => {
                    if self.can_close()
                        && let Some(p) = self.file(false)
                    {
                        match Project::load(&p) {
                            Ok(project) => {
                                self.reset_playback();
                                self.cancel_audio_import();
                                self.position = 0;
                                self.selection = (0, 0);
                                self.selection_anchor = None;
                                self.editor.exact_selection = None;
                                self.editor.exact_cursor = None;
                                self.editor.renaming = false;
                                self.project = project;
                                self.path = Some(p);
                                self.dirty = false;
                                self.undo.clear();
                                self.redo.clear();
                                self.build();
                                self.status(&self.t("opened"));
                            }
                            Err(e) => self.error(e),
                        }
                    }
                }
                SAVE => {
                    self.save(false);
                }
                SAVE_AS => {
                    self.save(true);
                }
                EXIT => {
                    self.close_audio();
                }
                ADD => {
                    self.editor.renaming = false;
                    let name = self.text(13);
                    if name.trim().is_empty() {
                        self.status(&self.t("enter_name"));
                        SetFocus(GetDlgItem(self.hwnd, 13));
                        return;
                    }
                    self.checkpoint();
                    self.project.add_track(name.trim().into());
                    self.refresh();
                    SendMessageW(
                        GetDlgItem(self.hwnd, 11),
                        0x186,
                        self.project.tracks.len() - 1,
                        0,
                    );
                    self.refresh_clips();
                    SetFocus(GetDlgItem(self.hwnd, 11));
                }
                DELETE | RENAME | MUTE | SOLO | ARM => {
                    self.editor.renaming = false;
                    let index = SendMessageW(GetDlgItem(self.hwnd, 11), 0x188, 0, 0);
                    if index < 0 {
                        return;
                    }
                    let i = index as usize;
                    let name = self.text(13);
                    if id == RENAME && name.trim().is_empty() {
                        SetFocus(GetDlgItem(self.hwnd, 13));
                        self.status(&self.t("enter_name"));
                        return;
                    }
                    self.checkpoint();
                    self.reset_playback();
                    match id {
                        DELETE => {
                            self.project.tracks.remove(i);
                        }
                        RENAME => self.project.tracks[i].name = name.trim().into(),
                        MUTE => self.project.tracks[i].mute ^= true,
                        SOLO => self.project.tracks[i].solo ^= true,
                        ARM => self.project.tracks[i].armed ^= true,
                        _ => (),
                    }
                    self.refresh();
                    SetFocus(GetDlgItem(self.hwnd, 11));
                }
                UNDO | REDO => {
                    let old = if id == UNDO {
                        self.undo.pop()
                    } else {
                        self.redo.pop()
                    };
                    if let Some(p) = old {
                        self.reset_playback();
                        let current = self.history();
                        if id == UNDO {
                            self.redo.push(current);
                        } else {
                            self.undo.push(current);
                        }
                        self.dirty = true;
                        self.restore_history(p);
                    }
                }
                PREFERENCES => {
                    if let Some(dialog) = &self.preferences {
                        SetFocus(dialog.hwnd);
                    } else {
                        match preferences::Dialog::open(self.hwnd, self.store.clone()) {
                            Ok(dialog) => self.preferences = Some(dialog),
                            Err(e) => self.error(e),
                        }
                    }
                }
                APPLY => {
                    let a = parse_time(&self.text(15));
                    let b = parse_time(&self.text(17));
                    let c = parse_time(&self.text(19));
                    match (a, b, c) {
                        (Some(p), Some(s), Some(e)) if e >= s => {
                            self.position = p;
                            self.selection = (s, e);
                            self.selection_anchor = None;
                            self.editor.exact_selection = None;
                            self.editor.exact_cursor = None;
                            self.status(&if self.verbose {
                                format!(
                                    "{} {}. {} {} — {} ({} ms)",
                                    self.t("position").replace('&', ""),
                                    self.text(15),
                                    self.t("selection"),
                                    self.text(17),
                                    self.text(19),
                                    e - s
                                )
                            } else {
                                self.t("time_set")
                            });
                            if self.store.settings.accessibility.focus_status {
                                SetFocus(GetDlgItem(self.hwnd, 21));
                            }
                            self.seek_playback();
                        }
                        _ => {
                            self.error(self.t("invalid_time"));
                            SetFocus(GetDlgItem(self.hwnd, 15));
                        }
                    }
                }
                HELP | ABOUT | CHANGELOG => {
                    let document = match id {
                        ABOUT => help::Document::About,
                        CHANGELOG => help::Document::Changelog,
                        _ => help::Document::Guide,
                    };
                    if let Some(h) = self.help.as_mut().filter(|h| IsWindow(h.hwnd) != 0) {
                        h.show(document);
                    } else {
                        self.help = None;
                        match help::Viewer::open(self.hwnd, self.strings.clone(), document) {
                            Ok(h) => self.help = Some(h),
                            Err(e) => self.error(e),
                        }
                    }
                }
                _ => (),
            }
        }
    }
    unsafe fn apply_store(&mut self, store: Store) {
        unsafe {
            if self.store.settings.devices.output_id != store.settings.devices.output_id
                || self.store.settings.recording.buffer_frames
                    != store.settings.recording.buffer_frames
            {
                self.stop_playback();
            }
            let values = [self.text(13), self.text(15), self.text(17), self.text(19)];
            let selected = SendMessageW(GetDlgItem(self.hwnd, 11), 0x188, 0, 0);
            let focus = GetDlgCtrlID(GetFocus());
            let language_changed = self.es != (store.settings.general.language == "es");
            self.es = store.settings.general.language == "es";
            self.verbose = store.settings.accessibility.detailed_time;
            self.store = store;
            if language_changed {
                self.strings = load_strings(self.es);
                if let Some(viewer) = self.help.as_mut().filter(|h| IsWindow(h.hwnd) != 0) {
                    viewer.update(self.strings.clone());
                }
                self.build();
                for (id, value) in [13, 15, 17, 19].into_iter().zip(values) {
                    SetWindowTextW(GetDlgItem(self.hwnd, id), wide(&value).as_ptr());
                }
                SendMessageW(GetDlgItem(self.hwnd, 11), 0x186, selected as usize, 0);
                if focus > 0 && self.preferences.is_none() {
                    SetFocus(GetDlgItem(self.hwnd, focus));
                }
            }
            self.status(&self.t("pref_saved"));
            self.log("preferences_applied");
        }
    }
    unsafe fn move_cursor(&mut self, right: bool, large: bool, select: bool) {
        unsafe {
            self.align_cursor_to_playhead();
            if self.recording.is_some() {
                return;
            }
            self.editor.exact_selection = None;
            self.editor.exact_cursor = None;
            let step = u64::from(if large {
                self.store.settings.editing.large_step_ms
            } else {
                self.store.settings.editing.step_ms
            });
            let old = self.position;
            self.position = if right {
                old.saturating_add(step)
            } else {
                old.saturating_sub(step)
            };
            if select {
                let anchor = *self.selection_anchor.get_or_insert(old);
                self.selection = (anchor.min(self.position), anchor.max(self.position));
            } else {
                self.selection_anchor = None;
                self.selection = (self.position, self.position);
            }
            for (id, value) in [
                (15, self.position),
                (17, self.selection.0),
                (19, self.selection.1),
            ] {
                SetWindowTextW(
                    GetDlgItem(self.hwnd, id),
                    wide(&format_time(value)).as_ptr(),
                );
            }
            let position = format!(
                "{} {}",
                self.t("position").replace('&', ""),
                format_time(self.position)
            );
            self.status(&if select && self.verbose {
                format!(
                    "{}. {} {} — {} ({} ms)",
                    position,
                    self.t("selection"),
                    format_time(self.selection.0),
                    format_time(self.selection.1),
                    self.selection.1 - self.selection.0
                )
            } else {
                position
            });
            self.seek_playback();
        }
    }
}
fn format_time(ms: u64) -> String {
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        ms / 3600000,
        (ms / 60000) % 60,
        (ms / 1000) % 60,
        ms % 1000
    )
}
fn load_strings(es: bool) -> serde_json::Value {
    serde_json::from_str(if es {
        include_str!("../../../resources/es.json")
    } else {
        include_str!("../../../resources/en.json")
    })
    .expect("validated embedded translation")
}
unsafe fn layout(hwnd: HWND) {
    unsafe {
        let mut r = RECT::default();
        GetClientRect(hwnd, &mut r);
        let dpi = GetDpiForWindow(hwnd).max(96) as i32;
        let scale = |v: i32| v * dpi / 96;
        let width = (r.right * 96 / dpi - 32).max(600);
        let list_h = (r.bottom * 96 / dpi - 400).max(60);
        let positions = [
            (10, 16, 12, width, 22),
            (11, 16, 36, (width - 16) / 2, list_h),
            (24, 32 + (width - 16) / 2, 12, (width - 16) / 2, 22),
            (25, 32 + (width - 16) / 2, 36, (width - 16) / 2, list_h),
            (12, 16, list_h + 48, 160, 24),
            (13, 180, list_h + 48, width - 310, 26),
            (ADD as i32, width - 110, list_h + 48, 120, 28),
            (PLAY as i32, 16, list_h + 86, (width - 30) / 4, 28),
            (
                PAUSE as i32,
                26 + (width - 30) / 4,
                list_h + 86,
                (width - 30) / 4,
                28,
            ),
            (
                STOP as i32,
                36 + 2 * (width - 30) / 4,
                list_h + 86,
                (width - 30) / 4,
                28,
            ),
            (22, 16, list_h + 120, width, 20),
            (
                recording_ui::RECORD as i32,
                46 + 3 * (width - 30) / 4,
                list_h + 86,
                (width - 30) / 4,
                28,
            ),
            (23, 16, list_h + 142, width, 26),
            (14, 16, list_h + 180, 275, 22),
            (15, 310, list_h + 180, width - 294, 26),
            (16, 16, list_h + 214, 275, 22),
            (17, 310, list_h + 214, width - 294, 26),
            (18, 16, list_h + 248, 275, 22),
            (19, 310, list_h + 248, width - 294, 26),
            (APPLY as i32, 16, list_h + 282, width, 28),
            (20, 16, list_h + 320, width, 22),
            (21, 16, list_h + 344, width, 28),
        ];
        for (id, x, y, ww, hh) in positions {
            MoveWindow(
                GetDlgItem(hwnd, id),
                scale(x),
                scale(y),
                scale(ww),
                scale(hh),
                1,
            );
        }
    }
}
unsafe extern "system" fn proc(h: HWND, m: u32, w: usize, l: isize) -> isize {
    unsafe {
        if m == 5 {
            layout(h);
            return 0;
        }
        if m == 0x24 {
            let info = &mut *(l as *mut MINMAXINFO);
            let dpi = GetDpiForWindow(h).max(96) as i32;
            info.min_track.x = 680 * dpi / 96;
            info.min_track.y = 510 * dpi / 96;
            return 0;
        }
        let ptr = GetWindowLongPtrW(h, -21) as *mut App;
        if !ptr.is_null() {
            match m {
                0x113 if w == playback::TIMER => {
                    if !(*ptr).modal.get() {
                        (*ptr).poll_audio();
                    }
                    return 0;
                }
                preferences::SETTINGS_APPLIED => {
                    if (*ptr).modal.get() {
                        return 0;
                    }
                    let store = (*ptr)
                        .preferences
                        .as_mut()
                        .and_then(|dialog| dialog.take_applied());
                    if let Some(store) = store {
                        (*ptr).apply_store(store);
                    }
                    return 0;
                }
                // Ignore control notifications (selection, text change, focus).
                // Only menu commands and BN_CLICKED invoke application actions.
                0x111 => {
                    if (*ptr).modal.get() {
                        return 0;
                    }
                    if w & 0xffff == 11 && w >> 16 == 1 {
                        (*ptr).track_selected();
                        return 0;
                    }
                    if w & 0xffff == 25
                        && (w >> 16 == 1
                            || (w >> 16 == 4 && (*ptr).selection.0 == (*ptr).selection.1))
                    {
                        (*ptr).clip_selected();
                        return 0;
                    }
                    if w >> 16 == 0 && w & 0xffff >= 100 {
                        (*ptr).command(w & 0xffff);
                    }
                    return 0;
                }
                0x10 => {
                    if (*ptr).modal.get() {
                        return 0;
                    }
                    (*ptr).close_audio();
                    return 0;
                }
                2 => {
                    PostQuitMessage(0);
                    return 0;
                }
                _ => (),
            }
        }
        DefWindowProcW(h, m, w, l)
    }
}
pub fn run() {
    unsafe {
        SetProcessDpiAwarenessContext(-2); // System DPI aware; native controls and system colors.
        let data = std::env::current_exe()
            .unwrap_or_else(|_| PathBuf::from("Sonitrelix.exe"))
            .with_file_name("data");
        if let Err(e) = fs::create_dir_all(&data) {
            MessageBoxW(
                null_mut(),
                wide(&format!("Cannot create portable data directory: {e}")).as_ptr(),
                wide("Sonitrelix").as_ptr(),
                0x10,
            );
            return;
        }
        let store = Store::load(data.join("preferences.json"));
        let es = store.settings.general.language == "es";
        let verbose = store.settings.accessibility.detailed_time;
        let mut app = Box::new(App {
            hwnd: null_mut(),
            project: Project {
                sample_rate: store.settings.general.new_project_sample_rate,
                ..Project::default()
            },
            path: None,
            dirty: false,
            undo: vec![],
            redo: vec![],
            editor: Default::default(),
            strings: load_strings(es),
            es,
            verbose,
            data,
            controls: vec![],
            position: 0,
            selection: (0, 0),
            selection_anchor: None,
            store,
            preferences: None,
            playback: Default::default(),
            modal: std::cell::Cell::new(false),
            recording: None,
            recording_close_pending: false,
            help: None,
        });
        let class = wide("SonitrelixMain");
        let instance = GetModuleHandleW(null());
        let wc = WNDCLASSW {
            style: 0,
            proc: Some(proc),
            cls_extra: 0,
            wnd_extra: 0,
            instance,
            icon: null_mut(),
            cursor: LoadCursorW(null_mut(), 32512usize as *const u16),
            background: 6usize as HWND,
            menu: null(),
            name: class.as_ptr(),
        };
        if RegisterClassW(&wc) == 0 {
            app.error(std::io::Error::last_os_error());
            return;
        }
        app.hwnd = CreateWindowExW(
            0x10000,
            class.as_ptr(),
            wide("Sonitrelix").as_ptr(),
            0x00cf0000,
            i32::MIN,
            i32::MIN,
            900,
            680,
            null_mut(),
            null_mut(),
            instance,
            null_mut(),
        );
        if app.hwnd.is_null() {
            app.error(std::io::Error::last_os_error());
            return;
        }
        SetWindowLongPtrW(app.hwnd, -21, (&mut *app as *mut App) as isize);
        app.build();
        SetTimer(app.hwnd, playback::TIMER, 250, 0);
        app.log("startup");
        ShowWindow(app.hwnd, 5);
        SetFocus(GetDlgItem(app.hwnd, 11));
        if !app.store.warnings.is_empty() {
            let warning = format!(
                "{}\n{}",
                app.t("pref_recovery"),
                app.store.warnings.join("\n")
            );
            app.log("preferences_recovery_warning");
            MessageBoxW(
                app.hwnd,
                wide(&warning).as_ptr(),
                wide("Sonitrelix").as_ptr(),
                0x30,
            );
        }
        let mut msg = MSG::default();
        loop {
            let result = GetMessageW(&mut msg, null_mut(), 0, 0);
            if result <= 0 {
                if result < 0 {
                    app.error(std::io::Error::last_os_error());
                }
                break;
            }
            if let Some(h) = app.help.as_ref().map(|h| h.hwnd)
                && help::translate(h, &mut msg)
            {
                if IsWindow(h) == 0 {
                    let focus = app.help.as_ref().map(|h| h.focus_id()).unwrap_or(11);
                    app.help = None;
                    SetFocus(GetDlgItem(app.hwnd, focus));
                }
                continue;
            }
            if let Some(sheet) = app.preferences.as_ref().map(|d| d.hwnd) {
                if msg.message == 0x100
                    && (msg.wParam == 0x70 || (msg.wParam == 0x48 && GetKeyState(0x11) < 0))
                {
                    let text = app
                        .preferences
                        .as_ref()
                        .map(|d| d.help_text())
                        .unwrap_or_default();
                    MessageBoxW(sheet, wide(&text).as_ptr(), wide("Sonitrelix").as_ptr(), 0);
                } else if SendMessageW(sheet, 0x400 + 117, 0, &mut msg as *mut MSG as isize) == 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                // A queued apply event must be consumed even if OK closed the property sheet.
                let applied = app.preferences.as_mut().and_then(|d| d.take_applied());
                if let Some(store) = applied {
                    app.apply_store(store);
                }
                if app.preferences.as_ref().is_some_and(|d| d.closed()) {
                    let focus_id = app.preferences.as_ref().map(|d| d.focus_id()).unwrap_or(11);
                    app.preferences.take();
                    let focus = GetDlgItem(app.hwnd, focus_id);
                    SetFocus(if focus.is_null() {
                        GetDlgItem(app.hwnd, 11)
                    } else {
                        focus
                    });
                }
                continue;
            }
            if [0x100, 0x104].contains(&msg.message) {
                let ctrl = GetKeyState(0x11) < 0;
                let shift = GetKeyState(0x10) < 0;
                let alt = GetKeyState(0x12) < 0;
                let focus = GetFocus();
                if focus == GetDlgItem(app.hwnd, 25)
                    && msg.wParam == 0x0d
                    && !ctrl
                    && !shift
                    && !alt
                {
                    app.clip_selected();
                    continue;
                }
                if app.editor.renaming
                    && focus == GetDlgItem(app.hwnd, 13)
                    && !ctrl
                    && !alt
                    && [0x0d, 0x1b].contains(&msg.wParam)
                {
                    if msg.wParam == 0x0d {
                        app.command(RENAME);
                    } else {
                        app.editor.renaming = false;
                        SetFocus(GetDlgItem(app.hwnd, 11));
                    }
                    continue;
                }
                if focus == GetDlgItem(app.hwnd, 23) && msg.wParam == 0x20 && !shift && !alt {
                    if ctrl {
                        app.audio_command(PAUSE);
                    } else {
                        app.toggle_playback();
                    }
                    continue;
                }
                let mut class = [0u16; 32];
                let n = GetClassNameW(focus, class.as_mut_ptr(), 32);
                let scope = if [GetDlgItem(app.hwnd, 11), GetDlgItem(app.hwnd, 25)].contains(&focus)
                {
                    shortcuts::Scope::Surface
                } else if String::from_utf16_lossy(&class[..n.max(0) as usize]) == "Edit" {
                    shortcuts::Scope::Text
                } else {
                    shortcuts::Scope::Other
                };
                if let Some(action) = shortcuts::route(msg.wParam, ctrl, shift, alt, scope) {
                    match action {
                        shortcuts::Action::Command(id) => app.command(id),
                        shortcuts::Action::TogglePlay => app.toggle_playback(),
                        shortcuts::Action::Cursor {
                            right,
                            large,
                            select,
                        } => app.move_cursor(right, large, select),
                        shortcuts::Action::Boundary { end, select } => {
                            app.align_cursor_to_playhead();
                            let old = app.position;
                            app.editor.exact_selection = None;
                            app.editor.exact_cursor = None;
                            app.audio_command(if end { END } else { START });
                            if select {
                                let anchor = *app.selection_anchor.get_or_insert(old);
                                app.selection =
                                    (anchor.min(app.position), anchor.max(app.position));
                            } else {
                                app.selection = (app.position, app.position);
                                app.selection_anchor = None;
                            }
                            app.sync_times();
                            app.selection_status();
                        }
                    }
                    continue;
                }
            }
            if IsDialogMessageW(app.hwnd, &mut msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        app.log("shutdown");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn translations_are_complete_and_nonempty() {
        let en = load_strings(false);
        let es = load_strings(true);
        let a = en.as_object().unwrap();
        let b = es.as_object().unwrap();
        assert_eq!(a.keys().collect::<Vec<_>>(), b.keys().collect::<Vec<_>>());
        for dict in [a, b] {
            for text in dict.values() {
                assert!(!text.as_str().unwrap().trim().is_empty());
            }
        }
    }
    #[test]
    fn main_window_mnemonics_do_not_collide() {
        for es in [false, true] {
            let dict = load_strings(es);
            let mut keys = std::collections::HashSet::new();
            for label in [
                "file",
                "edit",
                "track",
                "transport",
                "tools",
                "help_menu",
                "tracks",
                "clips_label",
                "name",
                "add",
                "position",
                "start",
                "end",
                "apply",
                "status",
            ] {
                let text = dict[label].as_str().unwrap();
                let key = text
                    .split_once('&')
                    .unwrap()
                    .1
                    .chars()
                    .next()
                    .unwrap()
                    .to_ascii_lowercase();
                assert!(
                    keys.insert(key),
                    "duplicate mnemonic {key} in {label}, es={es}"
                );
            }
        }
    }
    #[test]
    fn native_abi_sizes_x64() {
        assert_eq!(std::mem::size_of::<OPENFILENAMEW>(), 152);
        assert_eq!(std::mem::size_of::<WNDCLASSW>(), 72);
        assert_eq!(std::mem::size_of::<MSG>(), 48);
    }
    #[test]
    fn native_controls_track_commands_and_preferences() {
        // Real Win32 controls in a hidden test window; this is not a screen-reader test.
        unsafe {
            let data = std::env::temp_dir().join(format!("sonitrelix-ui-{}", std::process::id()));
            fs::create_dir_all(&data).unwrap();
            let mut app = Box::new(App {
                hwnd: null_mut(),
                project: Project::default(),
                path: None,
                dirty: false,
                undo: vec![],
                redo: vec![],
                editor: Default::default(),
                strings: load_strings(false),
                es: false,
                verbose: true,
                data: data.clone(),
                controls: vec![],
                position: 0,
                selection: (0, 0),
                selection_anchor: None,
                store: Store::load(data.join("preferences.json")),
                preferences: None,
                playback: Default::default(),
                modal: std::cell::Cell::new(false),
                recording: None,
                recording_close_pending: false,
                help: None,
            });
            let instance = GetModuleHandleW(null());
            let class = wide("SonitrelixNativeTest");
            let wc = WNDCLASSW {
                style: 0,
                proc: Some(proc),
                cls_extra: 0,
                wnd_extra: 0,
                instance,
                icon: null_mut(),
                cursor: null_mut(),
                background: 6usize as HWND,
                menu: null(),
                name: class.as_ptr(),
            };
            assert_ne!(RegisterClassW(&wc), 0);
            app.hwnd = CreateWindowExW(
                0,
                class.as_ptr(),
                wide("test").as_ptr(),
                0x00cf0000,
                0,
                0,
                900,
                680,
                null_mut(),
                null_mut(),
                instance,
                null_mut(),
            );
            assert!(!app.hwnd.is_null());
            SetWindowLongPtrW(app.hwnd, -21, (&mut *app as *mut App) as isize);
            app.build();
            assert!(app.controls.iter().all(|c| !c.is_null()));
            for id in [
                11,
                13,
                15,
                17,
                19,
                21,
                23,
                25,
                ADD as i32,
                APPLY as i32,
                PLAY as i32,
                PAUSE as i32,
                STOP as i32,
                recording_ui::RECORD as i32,
            ] {
                let (name, _) = accessibility_tests::name_and_role(GetDlgItem(app.hwnd, id));
                assert!(
                    !name.is_empty(),
                    "missing native accessible name for control {id}"
                );
            }
            SetWindowTextW(GetDlgItem(app.hwnd, 13), wide("Vocals ñ").as_ptr());
            app.command(ADD);
            assert_eq!(app.project.tracks[0].name, "Vocals ñ");
            app.command(MUTE);
            assert!(app.project.tracks[0].mute);
            app.command(UNDO);
            assert!(!app.project.tracks[0].mute);
            app.command(REDO);
            assert!(app.project.tracks[0].mute);
            SetWindowTextW(GetDlgItem(app.hwnd, 15), wide("00:01:28.500").as_ptr());
            let mut store = app.store.clone();
            let mut settings = store.settings.clone();
            settings.general.language = "es".into();
            store.save(settings).unwrap();
            app.apply_store(store);
            assert!(app.es);
            assert_eq!(app.text(15), "00:01:28.500");
            let prefs: serde_json::Value =
                serde_json::from_slice(&fs::read(data.join("preferences.json")).unwrap()).unwrap();
            assert_eq!(prefs["general"]["language"], "es");
            app.command(APPLY);
            assert_eq!(app.position, 88500);
            app.move_cursor(true, false, true);
            assert_eq!(app.selection, (88500, 88510));
            app.move_cursor(false, true, true);
            assert_eq!(app.position, 87510);
            assert_eq!(app.selection, (87510, 88500));
            assert_eq!(app.text(15), "00:01:27.510");
            app.move_cursor(false, false, false);
            assert_eq!(app.position, 87500);
            app.path = Some(data.join("session.sonitrelix"));
            assert!(app.save(false));
            assert_eq!(
                Project::load(app.path.as_ref().unwrap()).unwrap(),
                app.project
            );
            app.command(DELETE);
            assert!(app.project.tracks.is_empty());
            // Metadata import is asynchronous and leaves the source bytes intact.
            let source = data.join("piano ñ.wav");
            let mut wav = Vec::new();
            wav.extend_from_slice(b"RIFF");
            wav.extend_from_slice(&2036u32.to_le_bytes());
            wav.extend_from_slice(b"WAVEfmt ");
            wav.extend_from_slice(&16u32.to_le_bytes());
            wav.extend_from_slice(&1u16.to_le_bytes());
            wav.extend_from_slice(&1u16.to_le_bytes());
            wav.extend_from_slice(&48000u32.to_le_bytes());
            wav.extend_from_slice(&96000u32.to_le_bytes());
            wav.extend_from_slice(&2u16.to_le_bytes());
            wav.extend_from_slice(&16u16.to_le_bytes());
            wav.extend_from_slice(b"data");
            wav.extend_from_slice(&2000u32.to_le_bytes());
            wav.resize(2044, 0);
            fs::write(&source, &wav).unwrap();
            app.import_wav(source.clone());
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while app.project.tracks.is_empty() {
                assert!(std::time::Instant::now() < deadline, "Import timed out");
                std::thread::sleep(std::time::Duration::from_millis(2));
                app.poll_audio();
            }
            assert_eq!(app.project.tracks[0].frames(), 1000);
            SetFocus(GetDlgItem(app.hwnd, 11));
            app.command(SELECT_ALL);
            app.command(COPY);
            assert_eq!(app.editor.clipboard.as_ref().unwrap().frames(), 1000);
            app.command(CUT);
            assert_eq!(app.project.tracks[0].frames(), 0);
            app.command(PASTE);
            assert_eq!(app.project.tracks[0].frames(), 1000);
            app.command(UNDO);
            assert_eq!(app.project.tracks[0].frames(), 0);
            app.command(REDO);
            assert_eq!(app.project.tracks[0].frames(), 1000);
            sonitrelix_editing::split(&mut app.project.tracks[0], 1).unwrap();
            app.refresh_clips();
            SendMessageW(GetDlgItem(app.hwnd, 25), 0x186, 1, 0);
            app.clip_selected();
            app.command(DELETE_AUDIO);
            assert_eq!(app.project.tracks[0].frames(), 1);
            app.command(UNDO);
            assert_eq!(app.project.tracks[0].frames(), 1000);
            assert!(app.save(false));
            assert_eq!(
                Project::load(app.path.as_ref().unwrap()).unwrap(),
                app.project
            );
            assert_eq!(fs::read(&source).unwrap(), wav);
            app.command(NEW);
            assert!(app.project.tracks.is_empty());
            assert_eq!(fs::read(&source).unwrap(), wav);
            app.command(ADD_EMPTY);
            app.record_command();
            assert!(
                app.recording.is_none(),
                "An unarmed track must not open the microphone"
            );
            let recorded = sonitrelix_engine::recording::Recorded {
                path: fs::canonicalize(&source).unwrap(),
                info: sonitrelix_audio_io::WavReader::open(&source).unwrap().info,
                warning: None,
            };
            app.insert_recorded(0, 480, recorded);
            assert_eq!(app.project.tracks[0].frames(), 1480);
            assert!(app.project.tracks[0].clips[0].source.is_none());
            app.command(UNDO);
            assert!(app.project.tracks[0].clips.is_empty());
            app.command(REDO);
            assert_eq!(app.project.tracks[0].frames(), 1480);
            assert_eq!(fs::read(&source).unwrap(), wav);
            app.command(ADD_EMPTY);
            let empty = app.project.tracks.len() - 1;
            app.position = 1000;
            app.selection = (1000, 1000);
            app.editor.exact_selection = None;
            app.editor.exact_cursor = None;
            app.editor.clipboard = Some(sonitrelix_editing::Clipboard {
                sample_rate: 44100,
                channels: 1,
                clips: vec![sonitrelix_session::Clip {
                    source: None,
                    source_start: 0,
                    frames: 44100,
                }],
            });
            app.command(PASTE);
            assert_eq!(app.project.tracks[empty].sample_rate, 44100);
            assert_eq!(app.project.tracks[empty].frames(), 88200);
            for (command, title) in [
                (HELP, "help_title"),
                (ABOUT, "about_title"),
                (CHANGELOG, "changelog_title"),
            ] {
                app.command(command);
                let viewer = app.help.as_ref().unwrap();
                assert_eq!(GetFocus(), GetDlgItem(viewer.hwnd, 902));
                let mut text = vec![0u16; 256];
                let n = GetWindowTextW(viewer.hwnd, text.as_mut_ptr(), text.len() as i32);
                assert_eq!(String::from_utf16_lossy(&text[..n as usize]), app.t(title));
            }
            app.help = None;
            DestroyWindow(app.hwnd);
            fs::remove_dir_all(data).unwrap();
        }
    }
}
