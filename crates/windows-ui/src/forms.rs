//! Small native labelled forms with paired sliders and exact edit fields.
use super::*;
const SLIDER_BASE: i32 = 400;
const RESET_BASE: i32 = 460;
pub(super) enum Field {
    Number {
        label: String,
        value: String,
        default: f64,
        range: slider::Range,
        integer: bool,
    },
    Choice {
        label: String,
        options: Vec<String>,
        selected: usize,
    },
    Text {
        label: String,
        value: String,
    },
}
struct Form {
    fields: Vec<Field>,
    strings: serde_json::Value,
    slider_access: Option<slider::AccessibleValues>,
    result: Option<Vec<String>>,
}
impl Form {
    fn t(&self, k: &str) -> String {
        self.strings[k].as_str().unwrap_or(k).into()
    }
    unsafe fn accept(&mut self, h: HWND) {
        unsafe {
            let mut values = vec![];
            for (i, field) in self.fields.iter().enumerate() {
                let id = 101 + i as i32 * 2;
                let mut b = vec![0u16; 8192];
                let n = GetWindowTextW(GetDlgItem(h, id), b.as_mut_ptr(), b.len() as i32);
                let text = String::from_utf16_lossy(&b[..n.max(0) as usize]);
                let valid = match field {
                    Field::Number { range, integer, .. } => {
                        text.trim().replace(',', ".").parse::<f64>().is_ok_and(|v| {
                            (range.min..=range.max).contains(&v) && (!integer || v.fract() == 0.)
                        })
                    }
                    Field::Choice { options, .. } => {
                        let selected = SendMessageW(GetDlgItem(h, id), 0x147, 0, 0);
                        if selected < 0 || selected as usize >= options.len() {
                            false
                        } else {
                            values.push(selected.to_string());
                            continue;
                        }
                    }
                    Field::Text { .. } => text.len() <= 2048,
                };
                if !valid {
                    SetWindowTextW(GetDlgItem(h, 920), wide(&self.t("form_invalid")).as_ptr());
                    SetFocus(GetDlgItem(h, 920));
                    return;
                }
                values.push(if matches!(field, Field::Number { .. }) {
                    text.trim().replace(',', ".")
                } else {
                    text.trim().to_owned()
                });
            }
            self.result = Some(values);
            DestroyWindow(h);
        }
    }
    unsafe fn slider_message(&self, h: HWND, message: u32, w: usize, l: isize) -> bool {
        unsafe {
            let Some(access) = &self.slider_access else {
                return false;
            };
            for (i, field) in self.fields.iter().enumerate() {
                if let Field::Number {
                    range,
                    default,
                    label,
                    ..
                } = field
                {
                    let edit_id = 101 + i as i32 * 2;
                    let edit = GetDlgItem(h, edit_id);
                    let slider = GetDlgItem(h, SLIDER_BASE + i as i32 * 2 + 1);
                    if message == 0x114 && l == slider as isize {
                        range.write_to_edit(slider, edit);
                        return true;
                    }
                    if message == 0x111 && w >> 16 == 0x300 && w & 0xffff == edit_id as usize {
                        range.read_from_edit(slider, edit, access);
                        return true;
                    }
                    if message == 0x111 && w == (RESET_BASE + i as i32) as usize {
                        let value = default.to_string();
                        SetWindowTextW(edit, wide(&value).as_ptr());
                        let message = self
                            .t("reset_value_feedback")
                            .replace("{parameter}", &label.replace('&', ""))
                            .replace("{value}", &value);
                        let status = GetDlgItem(h, 920);
                        SetWindowTextW(status, wide(&message).as_ptr());
                        announcement::announce(status, &message);
                        return true;
                    }
                }
            }
            false
        }
    }
}
unsafe extern "system" fn proc(h: HWND, m: u32, w: usize, l: isize) -> isize {
    unsafe {
        let p = GetWindowLongPtrW(h, -21) as *mut Form;
        if !p.is_null() {
            if [0x114, 0x111].contains(&m) && (*p).slider_message(h, m, w, l) {
                return 0;
            }
            match m {
                0x111 if w >> 16 == 0 && w & 0xffff == 1 => {
                    (*p).accept(h);
                    return 0;
                }
                0x111 if w >> 16 == 0 && w & 0xffff == 2 => {
                    DestroyWindow(h);
                    return 0;
                }
                0x10 => {
                    DestroyWindow(h);
                    return 0;
                }
                _ => (),
            }
        }
        DefWindowProcW(h, m, w, l)
    }
}
unsafe fn create_dialog(
    owner: HWND,
    form: &mut Form,
    title: &str,
    hint: &str,
) -> Result<HWND, String> {
    unsafe {
        let class = wide("SonitrelixForm");
        let instance = GetModuleHandleW(null());
        RegisterClassW(&WNDCLASSW {
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
        });
        let dpi = GetDpiForWindow(owner).max(96) as i32;
        let s = |v| v * dpi / 96;
        let count = form.fields.len() as i32;
        let h = CreateWindowExW(
            0x10000,
            class.as_ptr(),
            wide(title).as_ptr(),
            0x00c80000,
            i32::MIN,
            i32::MIN,
            s(620),
            s(220 + count * 64),
            owner,
            null_mut(),
            instance,
            null_mut(),
        );
        if h.is_null() {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let create = |id: i32,
                      class: &str,
                      text: &str,
                      style: u32,
                      y: i32,
                      height: i32,
                      x: i32,
                      width: i32|
         -> HWND {
            let c = CreateWindowExW(
                0,
                wide(class).as_ptr(),
                wide(text).as_ptr(),
                0x50000000 | style,
                s(x),
                s(y),
                s(width),
                s(height),
                h,
                id as HMENU,
                instance,
                null_mut(),
            );
            SendMessageW(c, 0x30, GetStockObject(17) as usize, 1);
            c
        };
        for (i, field) in form.fields.iter().enumerate() {
            let (label, value) = match field {
                Field::Number { label, value, .. } | Field::Text { label, value } => {
                    (label, value.as_str())
                }
                Field::Choice { label, .. } => (label, ""),
            };
            let numeric = matches!(field, Field::Number { .. });
            create(
                100 + i as i32 * 2,
                "STATIC",
                label,
                0,
                12 + i as i32 * 64,
                20,
                12,
                if numeric { 290 } else { 580 },
            );
            let combo = matches!(field, Field::Choice { .. });
            let c = create(
                101 + i as i32 * 2,
                if combo { "COMBOBOX" } else { "EDIT" },
                value,
                0x10000 | if combo { 3 | 0x200000 } else { 0x800000 | 0x80 },
                34 + i as i32 * 64,
                if combo { 240 } else { 26 },
                12,
                if numeric { 120 } else { 580 },
            );
            if let Field::Number { range, .. } = field {
                let id = SLIDER_BASE + i as i32 * 2;
                let access = form.slider_access.as_ref().expect("numeric form");
                let reset = create(
                    RESET_BASE + i as i32,
                    "BUTTON",
                    &form.t("reset_value_button"),
                    0x10000,
                    34 + i as i32 * 64,
                    26,
                    145,
                    145,
                );
                access.name(
                    reset,
                    &form
                        .t("reset_value_name")
                        .replace("{parameter}", &label.replace('&', "")),
                );
                let slider_label = format!(
                    "{} {}",
                    label.replace('&', ""),
                    form.t("numeric_slider_suffix")
                );
                create(
                    id,
                    "STATIC",
                    &slider_label,
                    0,
                    12 + i as i32 * 64,
                    20,
                    310,
                    282,
                );
                let control = create(
                    id + 1,
                    slider::CLASS,
                    "",
                    slider::STYLE,
                    34 + i as i32 * 64,
                    26,
                    310,
                    282,
                );
                if c.is_null() || control.is_null() || reset.is_null() {
                    DestroyWindow(h);
                    return Err(std::io::Error::last_os_error().to_string());
                }
                range.configure(
                    control,
                    value.parse().unwrap_or(range.min),
                    access,
                    &slider_label,
                );
            }
            if let Field::Choice {
                options, selected, ..
            } = field
            {
                for option in options {
                    SendMessageW(c, 0x143, 0, wide(option).as_ptr() as isize);
                }
                SendMessageW(c, 0x14e, *selected, 0);
            }
        }
        create(
            919,
            "STATIC",
            &form.t("form_status"),
            0,
            14 + count * 64,
            20,
            12,
            580,
        );
        create(
            920,
            "EDIT",
            hint,
            0x10000 | 0x800000 | 0x800 | 4 | 0x40 | 0x200000,
            36 + count * 64,
            80,
            12,
            580,
        );
        create(
            1,
            "BUTTON",
            &form.t("form_ok"),
            0x10000 | 1,
            124 + count * 64,
            30,
            290,
            145,
        );
        create(
            2,
            "BUTTON",
            &form.t("form_cancel"),
            0x10000,
            124 + count * 64,
            30,
            447,
            145,
        );
        SetWindowLongPtrW(h, -21, form as *mut Form as isize);
        Ok(h)
    }
}

pub(super) unsafe fn ask(
    owner: HWND,
    strings: serde_json::Value,
    title: &str,
    fields: Vec<Field>,
    hint: &str,
) -> Result<Option<Vec<String>>, String> {
    unsafe {
        let has_numbers = fields
            .iter()
            .any(|field| matches!(field, Field::Number { .. }));
        if has_numbers {
            slider::initialize().map_err(|e| e.to_string())?;
        }
        let mut form = Box::new(Form {
            fields,
            strings,
            slider_access: has_numbers.then(slider::AccessibleValues::new),
            result: None,
        });
        let h = create_dialog(owner, &mut form, title, hint)?;
        let focus = GetFocus();
        EnableWindow(owner, 0);
        ShowWindow(h, 5);
        SetFocus(GetDlgItem(h, 101));
        let mut msg = MSG::default();
        while IsWindow(h) != 0 {
            let result = GetMessageW(&mut msg, null_mut(), 0, 0);
            if result <= 0 {
                if result == 0 {
                    PostQuitMessage(msg.wParam as i32);
                }
                break;
            }
            if msg.message == 0x100 && msg.wParam == 0x1b {
                let combo = GetFocus();
                if SendMessageW(combo, 0x157, 0, 0) != 0 {
                    SendMessageW(combo, 0x14f, 0, 0);
                } else {
                    DestroyWindow(h);
                }
                continue;
            }
            if !tab_from_readonly(h, GetDlgItem(h, 920), &msg) && IsDialogMessageW(h, &mut msg) == 0
            {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        if IsWindow(h) != 0 {
            DestroyWindow(h);
        }
        EnableWindow(owner, 1);
        SetFocus(focus);
        Ok(form.result.take())
    }
}
impl App {
    pub(super) unsafe fn track_levels(&mut self) {
        unsafe {
            let Some(i) = self.selected_track() else {
                self.status(&self.t("edit_no_track"));
                return;
            };
            let t = &self.project.tracks[i];
            let fields = vec![
                Field::Number {
                    label: self.t("level_gain"),
                    value: format!(
                        "{:.2}",
                        (20. * t.gain.max(0.000001).log10()).clamp(-60., 24.)
                    ),
                    range: slider::Range::new(-60., 24., 1.),
                    default: 0.,
                    integer: false,
                },
                Field::Number {
                    label: self.t("level_pan"),
                    value: format!("{:.1}", t.pan * 100.),
                    range: slider::Range::new(-100., 100., 1.),
                    default: 0.,
                    integer: false,
                },
            ];
            self.modal.set(true);
            let result = ask(
                self.hwnd,
                self.strings.clone(),
                &self.t("level_title"),
                fields,
                &self.t("level_hint"),
            );
            self.modal.set(false);
            match result {
                Ok(Some(v)) => {
                    self.stop_playback();
                    self.checkpoint();
                    self.project.tracks[i].gain = 10f32.powf(v[0].parse::<f32>().unwrap() / 20.);
                    self.project.tracks[i].pan = v[1].parse::<f32>().unwrap() / 100.;
                    self.refresh();
                    self.status(&self.t("level_saved"));
                }
                Ok(None) => (),
                Err(e) => self.error(e),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_and_pan_sliders_keep_exact_fields_and_accessible_values_in_sync() {
        unsafe {
            slider::initialize().unwrap();
            let mut form = Box::new(Form {
                fields: vec![
                    Field::Number {
                        label: "Volume (dB)".into(),
                        value: "0.25".into(),
                        range: slider::Range::new(-60., 24., 1.),
                        default: 0.,
                        integer: false,
                    },
                    Field::Number {
                        label: "Pan (%)".into(),
                        value: "0".into(),
                        range: slider::Range::new(-100., 100., 1.),
                        default: 0.,
                        integer: false,
                    },
                ],
                strings: load_strings(false),
                slider_access: Some(slider::AccessibleValues::new()),
                result: None,
            });
            let h = create_dialog(null_mut(), &mut form, "Volume and pan", "").unwrap();
            let volume = GetDlgItem(h, SLIDER_BASE + 1);
            let pan = GetDlgItem(h, SLIDER_BASE + 3);
            let reset_volume = GetDlgItem(h, RESET_BASE);
            let reset_pan = GetDlgItem(h, RESET_BASE + 1);
            assert_eq!(
                accessibility_tests::name_role_value(volume),
                ("Volume (dB) slider".into(), 0x33, "0".into())
            );
            assert_eq!(
                accessibility_tests::name_role_value(pan),
                ("Pan (%) slider".into(), 0x33, "0".into())
            );
            ShowWindow(h, 5);
            let volume_edit = GetDlgItem(h, 101);
            SetFocus(volume_edit);
            let mut tab = MSG {
                hwnd: volume_edit,
                message: 0x100,
                wParam: 9,
                ..MSG::default()
            };
            assert_ne!(IsDialogMessageW(h, &mut tab), 0);
            assert_eq!(GetFocus(), reset_volume);
            assert_eq!(
                accessibility_tests::name_and_role(reset_volume).0,
                "Reset Volume (dB) to default"
            );
            assert_eq!(
                accessibility_tests::name_and_role(reset_pan).0,
                "Reset Pan (%) to default"
            );
            SendMessageW(pan, 0x100, 0x25, 0); // Left arrow.
            assert_eq!(accessibility_tests::name_role_value(pan).2, "-1");
            SendMessageW(reset_volume, 0x00f5, 0, 0); // BM_CLICK.
            assert_ne!(IsWindow(h), 0);
            assert!(form.result.is_none());
            assert_eq!(accessibility_tests::name_role_value(pan).2, "-1");
            SendMessageW(h, 0x111, 1, 0);
            assert_eq!(form.result.take().unwrap(), ["0", "-1"]);
        }
    }
}
