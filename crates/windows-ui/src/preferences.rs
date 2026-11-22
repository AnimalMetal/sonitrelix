//! Native tabbed Windows Preferences. Workers own discovery and disk writes.
use super::{load_strings, use_spanish, w::*};
use sonitrelix_devices::{Catalog, Device};
use sonitrelix_settings::{
    BITRATES, BUFFER_SIZES, EXPORT_RATES, SAMPLE_RATES, Settings, Store, UI_LANGUAGES,
};
use std::{
    ptr::{null, null_mut},
    sync::mpsc::{self, Receiver, TryRecvError},
};

pub(super) const SETTINGS_APPLIED: u32 = 0x8001;
const PAGE_KEYS: [&str; 6] = [
    "pref_general",
    "pref_devices",
    "pref_recording",
    "pref_keyboard",
    "pref_accessibility",
    "pref_export",
];
const STATUS: i32 = 9009;
const APPLY_ID: i32 = 0x3021;
const PSM_CHANGED: u32 = 0x400 + 104;
const PSM_UNCHANGED: u32 = 0x400 + 109;
type Choice = (String, String);
struct PageInit {
    context: *mut Context,
    index: usize,
}
struct Context {
    owner: HWND,
    sheet: HWND,
    store: Store,
    draft: Settings,
    strings: serde_json::Value,
    pages: [HWND; 6],
    labels: Vec<(HWND, &'static str)>,
    inputs: Vec<Choice>,
    outputs: Vec<Choice>,
    catalog: Option<Catalog>,
    discovery: Option<Receiver<Result<Catalog, String>>>,
    saving: Option<Receiver<Result<Store, String>>>,
    applied: Option<Store>,
    close_after_save: bool,
    allow_close: bool,
}
pub(super) struct Dialog {
    pub hwnd: HWND,
    context: Box<Context>,
    // These allocations must outlive the modeless property sheet.
    _templates: Vec<Vec<u32>>,
    _titles: Vec<Vec<u16>>,
    _init: Box<[PageInit; 6]>,
    _pages: Vec<PROPSHEETPAGEW>,
    focus_id: i32,
}
impl Context {
    fn t(&self, key: &str) -> String {
        self.strings[key].as_str().unwrap_or(key).to_owned()
    }
    unsafe fn control(
        &mut self,
        page: HWND,
        class: &str,
        text: &str,
        id: i32,
        style: u32,
        rect: (i32, i32, i32, i32),
    ) -> HWND {
        unsafe {
            let (x, y, w, h) = rect;
            let mut r = RECT {
                left: x,
                top: y,
                right: x + w,
                bottom: y + h,
            };
            MapDialogRect(page, &mut r);
            let c = CreateWindowExW(
                0,
                wide(class).as_ptr(),
                wide(text).as_ptr(),
                0x50000000 | style,
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                page,
                id as HMENU,
                GetModuleHandleW(null()),
                null_mut(),
            );
            SendMessageW(c, 0x30, SendMessageW(page, 0x31, 0, 0) as usize, 1);
            c
        }
    }
    unsafe fn label(&mut self, page: HWND, key: &'static str, y: i32) {
        unsafe {
            let h = self.control(page, "STATIC", &self.t(key), 0, 0, (10, y, 315, 13));
            self.labels.push((h, key));
        }
    }
    unsafe fn field(&mut self, page: HWND, key: &'static str, id: i32, y: i32, combo: bool) {
        unsafe {
            self.label(page, key, y);
            self.control(
                page,
                if combo { "COMBOBOX" } else { "EDIT" },
                "",
                id,
                0x10000 | if combo { 0x200000 | 3 } else { 0x800000 | 0x80 },
                (10, y + 14, 315, if combo { 100 } else { 15 }),
            );
        }
    }
    unsafe fn check(&mut self, page: HWND, key: &'static str, id: i32, y: i32) {
        unsafe {
            let c = self.control(
                page,
                "BUTTON",
                &self.t(key),
                id,
                0x10000 | 3,
                (10, y, 315, 18),
            );
            self.labels.push((c, key));
        }
    }
    unsafe fn note(&mut self, page: HWND, key: &'static str, y: i32, height: i32) {
        unsafe {
            self.label(page, "pref_information", y);
            let c = self.control(
                page,
                "EDIT",
                &self.t(key),
                9007,
                0x10000 | 0x800 | 4 | 0x40 | 0x200000,
                (10, y + 13, 315, height),
            );
            self.labels.push((c, key));
        }
    }
    unsafe fn create_page(&mut self, h: HWND, index: usize) {
        unsafe {
            self.pages[index] = h;
            self.sheet = GetParent(h);
            match index {
                0 => {
                    self.field(h, "pref_language", 1001, 10, true);
                    self.field(h, "pref_sample_rate", 1002, 52, true);
                    let c = self.control(
                        h,
                        "BUTTON",
                        &self.t("pref_defaults"),
                        1100,
                        0x10000,
                        (10, 96, 315, 19),
                    );
                    self.labels.push((c, "pref_defaults"));
                    self.check(h, "pref_automatic_updates", 1003, 126);
                    self.note(h, "pref_general_help", 152, 35);
                }
                1 => {
                    self.field(h, "pref_input", 2001, 10, true);
                    self.field(h, "pref_output", 2002, 52, true);
                    let c = self.control(
                        h,
                        "BUTTON",
                        &self.t("pref_refresh"),
                        2100,
                        0x10000,
                        (10, 96, 315, 19),
                    );
                    self.labels.push((c, "pref_refresh"));
                    self.note(h, "pref_devices_help", 126, 61);
                }
                2 => {
                    self.field(h, "pref_channels", 3001, 10, true);
                    self.field(h, "pref_buffer", 3002, 52, true);
                    self.field(h, "pref_offset", 3003, 94, false);
                    self.check(h, "pref_monitoring", 3004, 133);
                    self.note(h, "pref_recording_help", 158, 29);
                }
                3 => {
                    self.field(h, "pref_step", 4001, 10, false);
                    self.field(h, "pref_large_step", 4002, 52, false);
                    let c = self.control(
                        h,
                        "BUTTON",
                        &self.t("keys_button"),
                        4003,
                        0x10000,
                        (10, 94, 315, 24),
                    );
                    self.labels.push((c, "keys_button"));
                    self.note(h, "pref_keyboard_help", 126, 61);
                }
                4 => {
                    self.check(h, "pref_detailed_time", 5001, 10);
                    self.check(h, "pref_focus_status", 5002, 38);
                    self.note(h, "pref_accessibility_help", 76, 111);
                }
                5 => {
                    self.field(h, "pref_export_rate", 6001, 10, true);
                    self.field(h, "pref_export_channels", 6002, 52, true);
                    self.field(h, "pref_export_bitrate", 6003, 94, true);
                    self.note(h, "pref_export_help", 138, 49);
                }
                _ => (),
            }
            self.label(h, "pref_status", 206);
            self.control(
                h,
                "EDIT",
                &self.t("pref_ready"),
                STATUS,
                0x10000 | 0x800 | 0x80,
                (10, 220, 315, 15),
            );
            self.populate(index);
        }
    }
    unsafe fn populate(&mut self, index: usize) {
        unsafe {
            let h = self.pages[index];
            if h.is_null() {
                return;
            }
            // Suppress native change notifications while restoring values programmatically.
            let init = GetWindowLongPtrW(h, 16);
            SetWindowLongPtrW(h, 16, 0);
            match index {
                0 => {
                    SendMessageW(
                        GetDlgItem(h, 1003),
                        0xf1,
                        usize::from(self.draft.general.automatic_updates),
                        0,
                    );
                    fill(
                        h,
                        1001,
                        &[
                            self.t("pref_language_auto"),
                            "English".into(),
                            "Español".into(),
                        ],
                        UI_LANGUAGES
                            .iter()
                            .position(|&lang| lang == self.draft.general.language)
                            .unwrap_or(0),
                    );
                    fill(
                        h,
                        1002,
                        &SAMPLE_RATES
                            .iter()
                            .map(|v| format!("{v} Hz"))
                            .collect::<Vec<_>>(),
                        SAMPLE_RATES
                            .iter()
                            .position(|v| *v == self.draft.general.new_project_sample_rate)
                            .unwrap_or(2),
                    );
                }
                1 => {
                    self.inputs = device_choices(
                        self.catalog
                            .as_ref()
                            .map(|c| c.inputs.as_slice())
                            .unwrap_or(&[]),
                        &self.draft.devices.input_id,
                        &self.t("pref_system_default"),
                        &self.t("pref_unavailable"),
                    );
                    self.outputs = device_choices(
                        self.catalog
                            .as_ref()
                            .map(|c| c.outputs.as_slice())
                            .unwrap_or(&[]),
                        &self.draft.devices.output_id,
                        &self.t("pref_system_default"),
                        &self.t("pref_unavailable"),
                    );
                    for (id, choices, saved) in [
                        (2001, &self.inputs, &self.draft.devices.input_id),
                        (2002, &self.outputs, &self.draft.devices.output_id),
                    ] {
                        fill(
                            h,
                            id,
                            &choices
                                .iter()
                                .map(|(_, name)| name.clone())
                                .collect::<Vec<_>>(),
                            choices
                                .iter()
                                .position(|(key, _)| key == saved)
                                .unwrap_or(0),
                        );
                    }
                    EnableWindow(GetDlgItem(h, 2100), i32::from(self.discovery.is_none()));
                }
                2 => {
                    fill(
                        h,
                        3001,
                        &[self.t("pref_mono"), self.t("pref_stereo")],
                        usize::from(self.draft.recording.channels == 2),
                    );
                    fill(
                        h,
                        3002,
                        &BUFFER_SIZES
                            .iter()
                            .map(|v| v.to_string())
                            .collect::<Vec<_>>(),
                        BUFFER_SIZES
                            .iter()
                            .position(|v| *v == self.draft.recording.buffer_frames)
                            .unwrap_or(1),
                    );
                    set(h, 3003, &self.draft.recording.offset_ms.to_string());
                    SendMessageW(
                        GetDlgItem(h, 3004),
                        0xf1,
                        usize::from(self.draft.recording.monitoring),
                        0,
                    );
                }
                3 => {
                    set(h, 4001, &self.draft.editing.step_ms.to_string());
                    set(h, 4002, &self.draft.editing.large_step_ms.to_string());
                }
                4 => {
                    SendMessageW(
                        GetDlgItem(h, 5001),
                        0xf1,
                        usize::from(self.draft.accessibility.detailed_time),
                        0,
                    );
                    SendMessageW(
                        GetDlgItem(h, 5002),
                        0xf1,
                        usize::from(self.draft.accessibility.focus_status),
                        0,
                    );
                }
                5 => {
                    let rates: Vec<_> = EXPORT_RATES
                        .iter()
                        .map(|r| {
                            if *r == 0 {
                                self.t("pref_export_auto")
                            } else {
                                format!("{r} Hz")
                            }
                        })
                        .collect();
                    fill(
                        h,
                        6001,
                        &rates,
                        EXPORT_RATES
                            .iter()
                            .position(|r| *r == self.draft.export.sample_rate)
                            .unwrap_or(0),
                    );
                    fill(
                        h,
                        6002,
                        &[self.t("audio_mono"), self.t("audio_stereo")],
                        usize::from(self.draft.export.channels == 2),
                    );
                    fill(
                        h,
                        6003,
                        &BITRATES
                            .iter()
                            .map(|b| format!("{b} kbps"))
                            .collect::<Vec<_>>(),
                        BITRATES
                            .iter()
                            .position(|b| *b == self.draft.export.mp3_bitrate)
                            .unwrap_or(4),
                    );
                }
                _ => (),
            }
            SetWindowLongPtrW(h, 16, init);
        }
    }
    unsafe fn set_status(&self, key: &str, detail: &str) {
        unsafe {
            if matches!(
                key,
                "pref_worker_error" | "pref_devices_error" | "pref_save_error" | "error"
            ) && let Some(data) = self.store.path().parent()
            {
                super::diagnostics::write(
                    data,
                    key,
                    if detail.is_empty() {
                        self.t(key)
                    } else {
                        detail.to_owned()
                    },
                );
            }
            let text = if detail.is_empty() {
                self.t(key)
            } else {
                format!("{} {detail}", self.t(key))
            };
            for page in self.pages {
                if !page.is_null() {
                    set(page, STATUS, &text);
                }
            }
        }
    }
    unsafe fn read_device_choices(&mut self) {
        unsafe {
            let page = self.pages[1];
            if page.is_null() {
                return;
            }
            if let Some((id, _)) = self.inputs.get(selected(page, 2001)) {
                self.draft.devices.input_id = id.clone();
            }
            if let Some((id, _)) = self.outputs.get(selected(page, 2002)) {
                self.draft.devices.output_id = id.clone();
            }
        }
    }
    unsafe fn discover(&mut self) {
        unsafe {
            if self.discovery.is_some() {
                return;
            }
            self.read_device_choices();
            let (tx, rx) = mpsc::channel();
            match std::thread::Builder::new()
                .name("audio-device-discovery".into())
                .spawn(move || {
                    let _ = tx.send(sonitrelix_devices::enumerate());
                }) {
                Ok(_) => {
                    self.discovery = Some(rx);
                    EnableWindow(GetDlgItem(self.pages[1], 2100), 0);
                    self.set_status("pref_scanning", "");
                }
                Err(e) => self.set_status("pref_worker_error", &e.to_string()),
            }
        }
    }
    unsafe fn read_all(&mut self) -> Result<Settings, (usize, i32, &'static str)> {
        unsafe {
            let mut next = self.draft.clone();
            next.general.automatic_updates =
                SendMessageW(GetDlgItem(self.pages[0], 1003), 0xf0, 0, 0) == 1;
            next.general.language = UI_LANGUAGES
                .get(selected(self.pages[0], 1001))
                .unwrap_or(&"auto")
                .to_string();
            next.general.new_project_sample_rate = *SAMPLE_RATES
                .get(selected(self.pages[0], 1002))
                .ok_or((0, 1002, "pref_sample_rate_error"))?;
            self.read_device_choices();
            next.devices = self.draft.devices.clone();
            next.recording.channels = if selected(self.pages[2], 3001) == 1 {
                2
            } else {
                1
            };
            next.recording.buffer_frames = *BUFFER_SIZES
                .get(selected(self.pages[2], 3002))
                .ok_or((2, 3002, "pref_buffer_error"))?;
            next.recording.offset_ms = text(self.pages[2], 3003)
                .trim()
                .parse()
                .map_err(|_| (2, 3003, "pref_offset_error"))?;
            next.recording.monitoring =
                SendMessageW(GetDlgItem(self.pages[2], 3004), 0xf0, 0, 0) == 1;
            next.editing.step_ms = text(self.pages[3], 4001)
                .trim()
                .parse()
                .map_err(|_| (3, 4001, "pref_step_error"))?;
            next.editing.large_step_ms = text(self.pages[3], 4002)
                .trim()
                .parse()
                .map_err(|_| (3, 4002, "pref_large_step_error"))?;
            next.accessibility.detailed_time =
                SendMessageW(GetDlgItem(self.pages[4], 5001), 0xf0, 0, 0) == 1;
            next.accessibility.focus_status =
                SendMessageW(GetDlgItem(self.pages[4], 5002), 0xf0, 0, 0) == 1;
            next.export.sample_rate = *EXPORT_RATES.get(selected(self.pages[5], 6001)).ok_or((
                5,
                6001,
                "pref_export_error",
            ))?;
            next.export.channels = if selected(self.pages[5], 6002) == 0 {
                1
            } else {
                2
            };
            next.export.mp3_bitrate = *BITRATES.get(selected(self.pages[5], 6003)).ok_or((
                5,
                6003,
                "pref_export_error",
            ))?;
            match next.validate() {
                Ok(()) => Ok(next),
                Err("export") => Err((5, 6001, "pref_export_error")),
                Err("offset") => Err((2, 3003, "pref_offset_error")),
                Err("step") => Err((3, 4001, "pref_step_error")),
                Err("large_step") => Err((3, 4002, "pref_large_step_error")),
                _ => Err((0, 1002, "pref_validation_error")),
            }
        }
    }
    unsafe fn enable(&self, enabled: bool) {
        unsafe {
            for p in self.pages {
                EnableWindow(p, i32::from(enabled));
            }
            for id in [1, 2, APPLY_ID] {
                EnableWindow(GetDlgItem(self.sheet, id), i32::from(enabled));
            }
        }
    }
    unsafe fn save(&mut self, close: bool) -> Result<(), (usize, i32, &'static str)> {
        unsafe {
            if self.saving.is_some() {
                return Ok(());
            }
            let next = self.read_all()?;
            if next == self.store.settings
                && self.store.has_saved_settings()
                && self.store.warnings.is_empty()
            {
                self.close_after_save = close;
                self.allow_close = close;
                if close {
                    PostMessageW(self.sheet, 0x111, 1, 0);
                } else {
                    for p in self.pages {
                        SendMessageW(self.sheet, PSM_UNCHANGED, p as usize, 0);
                    }
                }
                return Ok(());
            }
            let mut store = self.store.clone();
            let (tx, rx) = mpsc::channel();
            match std::thread::Builder::new()
                .name("preferences-save".into())
                .spawn(move || {
                    let result = store.save(next).map(|()| store).map_err(|e| e.to_string());
                    let _ = tx.send(result);
                }) {
                Ok(_) => {
                    self.saving = Some(rx);
                    self.close_after_save = close;
                    self.enable(false);
                    self.set_status("pref_saving", "");
                    SetFocus(GetDlgItem(self.sheet, 0x3020));
                }
                Err(e) => self.set_status("pref_worker_error", &e.to_string()),
            }
            Ok(())
        }
    }
    unsafe fn poll(&mut self) {
        unsafe {
            let discovery = self.discovery.as_ref().map(|rx| rx.try_recv());
            if let Some(result) = discovery {
                match result {
                    Ok(result) => {
                        self.discovery = None;
                        self.read_device_choices();
                        match result {
                            Ok(c) => {
                                let counts = format!("{} / {}", c.inputs.len(), c.outputs.len());
                                self.catalog = Some(c);
                                self.populate(1);
                                self.set_status("pref_devices_found", &counts);
                            }
                            Err(e) => {
                                self.populate(1);
                                self.set_status("pref_devices_error", &e);
                            }
                        }
                    }
                    Err(TryRecvError::Disconnected) => {
                        self.discovery = None;
                        self.populate(1);
                        self.set_status("pref_worker_error", "");
                    }
                    Err(TryRecvError::Empty) => (),
                }
            }
            let saved = self.saving.as_ref().map(|rx| rx.try_recv());
            if let Some(result) = saved {
                match result {
                    Ok(result) => {
                        self.saving = None;
                        self.enable(true);
                        match result {
                            Ok(store) => {
                                self.store = store;
                                self.draft = self.store.settings.clone();
                                self.applied = Some(self.store.clone());
                                self.localize();
                                for index in 0..6 {
                                    self.populate(index);
                                }
                                self.set_status("pref_saved", "");
                                let page = SendMessageW(self.sheet, 0x400 + 118, 0, 0) as HWND;
                                super::announcement::announce(
                                    GetDlgItem(page, STATUS),
                                    &self.t("pref_saved"),
                                );
                                PostMessageW(self.owner, SETTINGS_APPLIED, 0, 0);
                                for page in self.pages {
                                    SendMessageW(self.sheet, PSM_UNCHANGED, page as usize, 0);
                                }
                                if self.close_after_save {
                                    self.allow_close = true;
                                    PostMessageW(self.sheet, 0x111, 1, 0);
                                } else {
                                    SetFocus(GetDlgItem(self.sheet, 1));
                                }
                            }
                            Err(e) => {
                                self.set_status("pref_save_error", &e);
                                SetFocus(GetDlgItem(self.sheet, APPLY_ID));
                            }
                        }
                    }
                    Err(TryRecvError::Disconnected) => {
                        self.saving = None;
                        self.enable(true);
                        self.set_status("pref_worker_error", "");
                    }
                    Err(TryRecvError::Empty) => (),
                }
            }
        }
    }
    unsafe fn localize(&mut self) {
        unsafe {
            self.strings = load_strings(use_spanish(&self.draft.general.language));
            for (h, key) in &self.labels {
                SetWindowTextW(*h, wide(&self.t(key)).as_ptr());
            }
            SetWindowTextW(self.sheet, wide(&self.t("preferences_title")).as_ptr());
            for (id, key) in [(1, "pref_ok"), (2, "pref_cancel"), (APPLY_ID, "pref_apply")] {
                set(self.sheet, id, &self.t(key));
            }
            let tabs = SendMessageW(self.sheet, 0x400 + 116, 0, 0) as HWND;
            for (i, key) in PAGE_KEYS.iter().enumerate() {
                let mut title = wide(&self.t(key));
                let item = TCITEMW {
                    mask: 1,
                    state: 0,
                    state_mask: 0,
                    text: title.as_mut_ptr(),
                    text_max: 0,
                    image: 0,
                    param: 0,
                };
                SendMessageW(tabs, 0x1300 + 61, i, &item as *const _ as isize);
            }
            SendMessageW(self.sheet, 0x400 + 136, 0, 0); // Recalculate tab rows after translation.
        }
    }
}

unsafe extern "system" fn page_proc(page: HWND, message: u32, w: usize, l: isize) -> isize {
    unsafe {
        if message == 0x110 {
            let psp = &*(l as *const PROPSHEETPAGEW);
            let init = &*(psp.param as *const PageInit);
            (*init.context).create_page(page, init.index);
            SetWindowLongPtrW(page, 16, psp.param);
            if init.index == 0 {
                SetTimer(page, 1, 50, 0);
            }
            return 1;
        }
        let p = GetWindowLongPtrW(page, 16) as *const PageInit;
        if p.is_null() {
            return 0;
        }
        let ctx = (*p).context;
        match message {
            0x113 => {
                (*ctx).poll();
                return 1;
            }
            0x111 => {
                let id = w & 0xffff;
                if id == 4003 && w >> 16 == 0 && (*ctx).saving.is_none() {
                    let mut draft = (*ctx).draft.clone();
                    let strings = (*ctx).strings.clone();
                    let sheet = (*ctx).sheet;
                    match super::shortcut_config::edit(sheet, strings, &mut draft) {
                        Ok(true) => {
                            (*ctx).draft.extra = draft.extra;
                            SendMessageW(sheet, PSM_CHANGED, page as usize, 0);
                        }
                        Ok(false) => (),
                        Err(e) => {
                            (*ctx).set_status("error", &e);
                        }
                    }
                    return 1;
                }
                if id == 2100 && w >> 16 == 0 {
                    (*ctx).discover();
                    return 1;
                }
                if id == 1100 && w >> 16 == 0 {
                    // A native confirmation is safe here after ending all Context borrows.
                    let question = (*ctx).t("pref_defaults_confirm");
                    let title = (*ctx).t("preferences_title");
                    let yes = MessageBoxW(
                        GetParent(page),
                        wide(&question).as_ptr(),
                        wide(&title).as_ptr(),
                        4 | 0x20 | 0x100,
                    ) == 6;
                    if yes {
                        let defaults = (*ctx).draft.defaults_preserving_unknown();
                        (*ctx).draft = defaults;
                        for i in 0..6 {
                            (*ctx).populate(i);
                        }
                        (*ctx).set_status("pref_defaults_pending", "");
                        SendMessageW((*ctx).sheet, PSM_CHANGED, page as usize, 0);
                    }
                    return 1;
                }
                if (1000..6000).contains(&id) && [0, 1, 0x300].contains(&(w >> 16)) {
                    SendMessageW((*ctx).sheet, PSM_CHANGED, page as usize, 0);
                }
            }
            0x4e => {
                let n = &*(l as *const PSHNOTIFY);
                match n.hdr.code as i32 {
                    -202 => {
                        if (*ctx).allow_close {
                            SetWindowLongPtrW(page, 0, 0);
                            return 1;
                        }
                        let result = (*ctx).save(n.param != 0);
                        SetWindowLongPtrW(page, 0, 2); // Prevent closing while the save worker runs.
                        if let Err((index, id, key)) = result {
                            let sheet = (*ctx).sheet;
                            let error = (*ctx).t(key);
                            let title = (*ctx).t("preferences_title");
                            SendMessageW(sheet, 0x400 + 101, index, 0);
                            MessageBoxW(sheet, wide(&error).as_ptr(), wide(&title).as_ptr(), 0x10);
                            SetFocus(GetDlgItem((*ctx).pages[index], id));
                        }
                        return 1;
                    }
                    -209 => {
                        SetWindowLongPtrW(page, 0, isize::from((*ctx).saving.is_some()));
                        return 1;
                    }
                    _ => (),
                }
            }
            2 => {
                KillTimer(page, 1);
            }
            _ => (),
        }
        0
    }
}

impl Dialog {
    pub unsafe fn open(owner: HWND, store: Store) -> Result<Self, String> {
        unsafe { Self::create(owner, store, true) }
    }
    unsafe fn create(owner: HWND, store: Store, discover: bool) -> Result<Self, String> {
        unsafe {
            let mut context = Box::new(Context {
                owner,
                sheet: null_mut(),
                draft: store.settings.clone(),
                strings: load_strings(use_spanish(&store.settings.general.language)),
                store,
                pages: [null_mut(); 6],
                labels: vec![],
                inputs: vec![],
                outputs: vec![],
                catalog: None,
                discovery: None,
                saving: None,
                applied: None,
                close_after_save: false,
                allow_close: false,
            });
            let context_ptr = &mut *context as *mut Context;
            let init = Box::new(std::array::from_fn::<_, 6, _>(|index| PageInit {
                context: context_ptr,
                index,
            }));
            let templates: Vec<_> = (0..6).map(|_| template()).collect();
            let titles: Vec<_> = PAGE_KEYS.iter().map(|key| wide(&context.t(key))).collect();
            let pages: Vec<_> = (0..6)
                .map(|i| PROPSHEETPAGEW {
                    size: std::mem::size_of::<PROPSHEETPAGEW>() as u32,
                    flags: 1 | 8 | 0x400,
                    instance: GetModuleHandleW(null()),
                    template: templates[i].as_ptr(),
                    icon: null_mut(),
                    title: titles[i].as_ptr(),
                    proc: Some(page_proc),
                    param: &init[i] as *const PageInit as isize,
                    callback: 0,
                    ref_parent: null_mut(),
                })
                .collect();
            let caption = wide(&context.t("preferences_title"));
            let header = PROPSHEETHEADERW {
                size: std::mem::size_of::<PROPSHEETHEADERW>() as u32,
                flags: 8 | 0x400 | 0x02000000,
                parent: owner,
                instance: GetModuleHandleW(null()),
                icon: null_mut(),
                caption: caption.as_ptr(),
                count: 6,
                start: 0,
                pages: pages.as_ptr(),
                callback: 0,
            };
            let focus_id = GetDlgCtrlID(GetFocus());
            let hwnd = PropertySheetW(&header) as HWND;
            if hwnd.is_null() || hwnd as isize == -1 {
                return Err(format!(
                    "Cannot create Preferences: {}",
                    std::io::Error::last_os_error()
                ));
            }
            context.sheet = hwnd;
            context.localize();
            if discover {
                context.discover();
            }
            EnableWindow(owner, 0);
            SetFocus(GetDlgItem(context.pages[0], 1001));
            Ok(Self {
                hwnd,
                context,
                _templates: templates,
                _titles: titles,
                _init: init,
                _pages: pages,
                focus_id,
            })
        }
    }
    pub fn take_applied(&mut self) -> Option<Store> {
        self.context.applied.take()
    }
    pub fn help_text(&self) -> String {
        self.context.t("pref_help")
    }
    pub unsafe fn closed(&self) -> bool {
        unsafe { IsWindow(self.hwnd) == 0 || SendMessageW(self.hwnd, 0x400 + 118, 0, 0) == 0 }
    }
    pub fn focus_id(&self) -> i32 {
        self.focus_id
    }
}
impl Drop for Dialog {
    fn drop(&mut self) {
        unsafe {
            if IsWindow(self.hwnd) != 0 {
                DestroyWindow(self.hwnd);
            }
            EnableWindow(self.context.owner, 1);
        }
    }
}

fn template() -> Vec<u32> {
    // DWORD-aligned standard DLGTEMPLATE with DS_SETFONT and zero controls.
    // Child controls are created with dialog-unit positions in WM_INITDIALOG.
    let mut bytes = vec![];
    bytes.extend_from_slice(&(0x40000000u32 | 0x400 | 0x40).to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    for v in [0u16, 0, 0, 335, 246, 0, 0, 0, 9] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    for v in "Segoe UI".encode_utf16().chain(Some(0)) {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    while bytes.len() % 4 != 0 {
        bytes.push(0);
    }
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes(*c))
        .collect()
}
unsafe fn set(page: HWND, id: i32, value: &str) {
    unsafe {
        SetWindowTextW(GetDlgItem(page, id), wide(value).as_ptr());
    }
}
unsafe fn text(page: HWND, id: i32) -> String {
    unsafe {
        let h = GetDlgItem(page, id);
        let mut v = vec![0u16; GetWindowTextLengthW(h).max(0) as usize + 1];
        let n = GetWindowTextW(h, v.as_mut_ptr(), v.len() as i32);
        String::from_utf16_lossy(&v[..n.max(0) as usize])
    }
}
unsafe fn selected(page: HWND, id: i32) -> usize {
    unsafe { SendMessageW(GetDlgItem(page, id), 0x147, 0, 0) as usize }
}
unsafe fn fill(page: HWND, id: i32, items: &[String], selected: usize) {
    unsafe {
        let c = GetDlgItem(page, id);
        SendMessageW(c, 0x14b, 0, 0);
        for item in items {
            SendMessageW(c, 0x143, 0, wide(item).as_ptr() as isize);
        }
        SendMessageW(c, 0x14e, selected, 0);
    }
}
fn device_choices(
    devices: &[Device],
    saved: &str,
    default_label: &str,
    unavailable: &str,
) -> Vec<Choice> {
    let default = devices
        .iter()
        .find(|d| d.is_default)
        .map(|d| format!("{default_label}: {}", d.name))
        .unwrap_or_else(|| default_label.into());
    let mut choices = vec![(String::new(), default)];
    for device in devices {
        choices.push((device.id.clone(), device.name.clone()));
    }
    if !saved.is_empty() && !devices.iter().any(|d| d.id == saved) {
        choices.push((saved.into(), format!("{unavailable}: {saved}")));
    }
    choices
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        time::{Duration, Instant},
    };
    #[test]
    fn device_choices_preserve_unavailable_and_default_identity() {
        let devices = vec![Device {
            id: "mic-a".into(),
            name: "Microphone".into(),
            is_default: true,
        }];
        let choices = device_choices(&devices, "missing-mic", "Default", "Unavailable");
        assert_eq!(choices[0].0, "");
        assert!(choices[0].1.contains("Microphone"));
        assert_eq!(choices[1].0, "mic-a");
        assert_eq!(choices[2].0, "missing-mic");
    }
    #[test]
    fn native_preferences_all_tabs_validate_apply_and_cancel() {
        unsafe {
            assert_eq!(std::mem::size_of::<PROPSHEETPAGEW>(), 72);
            assert_eq!(std::mem::size_of::<PROPSHEETHEADERW>(), 72);
            let dir =
                std::env::temp_dir().join(format!("sonitrelix-dialog-{}", std::process::id()));
            fs::create_dir_all(&dir).unwrap();
            let path = dir.join("preferences.json");
            let owner = CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                wide("Test owner").as_ptr(),
                0x00cf0000,
                0,
                0,
                900,
                700,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null()),
                null_mut(),
            );
            assert!(!owner.is_null());
            let mut dialog = Dialog::create(owner, Store::load(&path), false).unwrap();
            ShowWindow(dialog.hwnd, 0);
            assert!(dialog.context.pages.iter().all(|p| !p.is_null()));
            for (index, ids) in [
                (0, vec![1001, 1002, 1003, 1100]),
                (1, vec![2001, 2002, 2100]),
                (2, vec![3001, 3002, 3003, 3004]),
                (3, vec![4001, 4002, 4003]),
                (4, vec![5001, 5002]),
                (5, vec![6001, 6002, 6003]),
            ] {
                for id in ids {
                    assert!(
                        !GetDlgItem(dialog.context.pages[index], id).is_null(),
                        "missing control {id}"
                    );
                    let (name, role) = super::super::accessibility_tests::name_and_role(
                        GetDlgItem(dialog.context.pages[index], id),
                    );
                    assert!(!name.is_empty(), "missing MSAA name for control {id}");
                    assert!(
                        [42, 43, 44, 46].contains(&role),
                        "unexpected role {role} for {id}"
                    );
                }
            }
            let general = dialog.context.pages[0];
            assert_eq!(
                selected(general, 1001),
                0,
                "Automatic is selected by default"
            );
            assert_eq!(SendMessageW(GetDlgItem(general, 1001), 0x146, 0, 0), 3);
            let recording = dialog.context.pages[2];
            let editing = dialog.context.pages[3];
            for (index, id, key, role) in [
                (0, 1001, "pref_language", 46),
                (0, 1002, "pref_sample_rate", 46),
                (1, 2001, "pref_input", 46),
                (1, 2002, "pref_output", 46),
                (2, 3003, "pref_offset", 42),
                (3, 4001, "pref_step", 42),
                (4, 5001, "pref_detailed_time", 44),
            ] {
                let (name, observed_role) = super::super::accessibility_tests::name_and_role(
                    GetDlgItem(dialog.context.pages[index], id),
                );
                assert_eq!(name, dialog.context.t(key).replace('&', ""));
                assert_eq!(observed_role, role);
            }
            dialog.context.catalog = Some(Catalog {
                inputs: vec![
                    Device {
                        id: "mic-a".into(),
                        name: "Microphone A".into(),
                        is_default: true,
                    },
                    Device {
                        id: "mic-b".into(),
                        name: "Microphone B".into(),
                        is_default: false,
                    },
                ],
                outputs: vec![],
            });
            dialog.context.populate(1);
            SendMessageW(GetDlgItem(dialog.context.pages[1], 2001), 0x14e, 2, 0);
            let (tx, rx) = mpsc::channel();
            dialog.context.discovery = Some(rx);
            tx.send(Ok(Catalog {
                inputs: vec![Device {
                    id: "mic-a".into(),
                    name: "Microphone A".into(),
                    is_default: true,
                }],
                outputs: vec![],
            }))
            .unwrap();
            dialog.context.poll();
            assert_eq!(dialog.context.read_all().unwrap().devices.input_id, "mic-b");
            let choices = &dialog.context.inputs;
            assert!(
                choices
                    .iter()
                    .any(|(id, name)| id == "mic-b" && name.contains("unavailable"))
            );
            set(editing, 4001, "0");
            assert_eq!(
                dialog.context.read_all().unwrap_err(),
                (3, 4001, "pref_step_error")
            );
            set(editing, 4001, "25");
            set(editing, 4002, "10");
            assert_eq!(
                dialog.context.read_all().unwrap_err(),
                (3, 4002, "pref_large_step_error")
            );
            set(editing, 4002, "2500");
            set(recording, 3003, "-25");
            SendMessageW(GetDlgItem(general, 1001), 0x14e, 2, 0);
            SendMessageW(GetDlgItem(general, 1002), 0x14e, 4, 0);
            SendMessageW(dialog.hwnd, 0x400 + 113, 4, 0); // Native Apply button path.
            let deadline = Instant::now() + Duration::from_secs(5);
            while dialog.context.saving.is_some() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(5));
                dialog.context.poll();
            }
            assert!(
                dialog.context.saving.is_none(),
                "save worker did not finish"
            );
            let applied = dialog.take_applied().expect("Apply produced new settings");
            assert_eq!(applied.settings.general.language, "es");
            assert_eq!(applied.settings.general.new_project_sample_rate, 96000);
            assert_eq!(applied.settings.recording.offset_ms, -25);
            assert_eq!(applied.settings.editing.step_ms, 25);
            assert_eq!(applied.settings.editing.large_step_ms, 2500);
            assert_eq!(text(dialog.hwnd, 1), "&Aceptar");
            let (name, _) = super::super::accessibility_tests::name_and_role(GetDlgItem(
                dialog.context.pages[1],
                2001,
            ));
            assert_eq!(name, dialog.context.t("pref_input").replace('&', ""));
            assert_eq!(Store::load(&path).settings, applied.settings);
            set(editing, 4001, "30");
            SendMessageW(dialog.hwnd, 0x400 + 113, 5, 0);
            assert!(dialog.closed());
            drop(dialog);
            assert_eq!(Store::load(&path).settings.editing.step_ms, 25);
            let second = Dialog::create(owner, Store::load(&path), false).unwrap();
            ShowWindow(second.hwnd, 0);
            assert_eq!(selected(second.context.pages[0], 1001), 2);
            assert_eq!(selected(second.context.pages[0], 1002), 4);
            assert_eq!(text(second.context.pages[2], 3003), "-25");
            assert_eq!(text(second.context.pages[3], 4001), "25");
            assert_eq!(text(second.context.pages[3], 4002), "2500");
            set(second.context.pages[3], 4001, "40");
            SendMessageW(second.hwnd, 0x400 + 113, 5, 0);
            assert!(second.closed());
            drop(second);
            assert_eq!(Store::load(&path).settings.editing.step_ms, 25);
            let mut third = Dialog::create(owner, Store::load(&path), false).unwrap();
            ShowWindow(third.hwnd, 0);
            set(third.context.pages[3], 4001, "50");
            SendMessageW(third.hwnd, 0x400 + 113, 3, 0); // Native OK path.
            let deadline = Instant::now() + Duration::from_secs(5);
            while third.context.saving.is_some() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(5));
                third.context.poll();
            }
            assert!(third.context.allow_close);
            SendMessageW(third.hwnd, 0x111, 1, 0);
            assert!(third.closed());
            drop(third);
            assert_eq!(Store::load(&path).settings.editing.step_ms, 50);
            let first_run = dir.join("first-run.json");
            let mut fresh = Dialog::create(owner, Store::load(&first_run), false).unwrap();
            ShowWindow(fresh.hwnd, 0);
            SendMessageW(fresh.hwnd, 0x400 + 113, 3, 0);
            let deadline = Instant::now() + Duration::from_secs(5);
            while fresh.context.saving.is_some() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(5));
                fresh.context.poll();
            }
            assert!(Store::load(&first_run).has_saved_settings());
            drop(fresh);
            DestroyWindow(owner);
            fs::remove_dir_all(dir).unwrap();
        }
    }
}
