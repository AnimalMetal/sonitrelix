//! Small native labelled forms with sliders, choices, and text inputs.
use super::*;
const RESET_BASE: i32 = 460;
pub(super) enum Field {
    Slider {
        label: String,
        value: f64,
        default: f64,
        range: slider::Range,
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
                let control = GetDlgItem(h, id);
                let value = match field {
                    Field::Slider { range, .. } => Some(range.value(control).to_string()),
                    Field::Choice { options, .. } => {
                        let selected = SendMessageW(control, 0x147, 0, 0) as usize;
                        (selected < options.len()).then(|| selected.to_string())
                    }
                    Field::Text { .. } => {
                        let mut buffer = vec![0u16; 8192];
                        let length =
                            GetWindowTextW(control, buffer.as_mut_ptr(), buffer.len() as i32);
                        let text = String::from_utf16_lossy(&buffer[..length.max(0) as usize]);
                        (text.len() <= 2048).then(|| text.trim().to_owned())
                    }
                };
                let Some(value) = value else {
                    SetWindowTextW(GetDlgItem(h, 920), wide(&self.t("form_invalid")).as_ptr());
                    SetFocus(GetDlgItem(h, 920));
                    return;
                };
                values.push(value);
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
                if let Field::Slider {
                    range,
                    default,
                    label,
                    ..
                } = field
                {
                    let slider = GetDlgItem(h, 101 + i as i32 * 2);
                    if message == 0x114 && l == slider as isize {
                        range.update_accessible_value(slider, access);
                        return true;
                    }
                    if message == 0x111 && w == (RESET_BASE + i as i32) as usize {
                        range.set_value(slider, *default, access);
                        let value = default.to_string();
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
            let id = 100 + i as i32 * 2;
            let y = 12 + i as i32 * 64;
            let label = match field {
                Field::Slider { label, .. }
                | Field::Text { label, .. }
                | Field::Choice { label, .. } => label,
            };
            create(id, "STATIC", label, 0, y, 20, 12, 580);
            if let Field::Slider { range, value, .. } = field {
                let control = create(
                    id + 1,
                    slider::CLASS,
                    "",
                    slider::STYLE,
                    y + 22,
                    26,
                    12,
                    420,
                );
                let reset = create(
                    RESET_BASE + i as i32,
                    "BUTTON",
                    &form.t("reset_value_button"),
                    0x10000,
                    y + 22,
                    26,
                    447,
                    145,
                );
                if control.is_null() || reset.is_null() {
                    DestroyWindow(h);
                    return Err(std::io::Error::last_os_error().to_string());
                }
                let access = form.slider_access.as_ref().expect("numeric form");
                let label = label.replace('&', "");
                access.name(
                    reset,
                    &form.t("reset_value_name").replace("{parameter}", &label),
                );
                let name = format!("{} {}", label, form.t("numeric_slider_suffix"));
                range.configure(control, *value, access, &name);
            } else if let Field::Text { value, .. } = field {
                create(
                    id + 1,
                    "EDIT",
                    value,
                    0x10000 | 0x800000 | 0x80,
                    y + 22,
                    26,
                    12,
                    580,
                );
            } else if let Field::Choice {
                options, selected, ..
            } = field
            {
                let c = create(
                    id + 1,
                    "COMBOBOX",
                    "",
                    0x10000 | 3 | 0x200000,
                    y + 22,
                    240,
                    12,
                    580,
                );
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
            .any(|field| matches!(field, Field::Slider { .. }));
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
fn track_volume_db(gain: f32) -> f32 {
    ((20. * gain.max(0.000001).log10() * 10.).round() / 10.).clamp(-60., 24.)
}

impl App {
    unsafe fn apply_track_levels(&mut self, index: usize, gain: f32, pan: f32) {
        unsafe {
            let track = &self.project.tracks[index];
            if track.gain == gain && track.pan == pan {
                return;
            }
            let was_dirty = self.dirty;
            self.checkpoint();
            self.project.tracks[index].gain = gain;
            self.project.tracks[index].pan = pan;
            self.update_playback_levels(index);
            self.pending_track_row.set(Some(index));
            self.flush_track_row(true);
            if !was_dirty {
                self.refresh_title();
            }
        }
    }

    pub(super) unsafe fn nudge_track_levels(&mut self, command: usize) {
        unsafe {
            let Some(index) = self.selected_track() else {
                return;
            };
            let track = &self.project.tracks[index];
            let (gain, pan, message) = if matches!(command, VOLUME_UP | VOLUME_DOWN) {
                let step = if command == VOLUME_UP { 1. } else { -1. };
                let db = (track_volume_db(track.gain) + step).clamp(-60., 24.);
                let message = self
                    .t("level_shortcut_volume")
                    .replace("{value}", &db.to_string());
                (10f32.powf(db / 20.), track.pan, message)
            } else {
                let step = if command == PAN_LEFT { -5. } else { 5. };
                let percent = ((track.pan * 1000.).round() / 10. + step).clamp(-100., 100.);
                let key = if percent < 0. {
                    "level_shortcut_left"
                } else if percent > 0. {
                    "level_shortcut_right"
                } else {
                    "level_shortcut_center"
                };
                let message = self.t(key).replace("{value}", &percent.abs().to_string());
                (track.gain, percent / 100., message)
            };
            self.apply_track_levels(index, gain, pan);
            self.status(&message);
            announcement::announce(GetDlgItem(self.hwnd, 21), &message);
        }
    }

    pub(super) unsafe fn track_levels(&mut self) {
        unsafe {
            let Some(i) = self.selected_track() else {
                self.status(&self.t("edit_no_track"));
                return;
            };
            let t = &self.project.tracks[i];
            let gain_db = track_volume_db(t.gain);
            let pan_percent = (t.pan * 1000.).round() / 10.;
            let fields = vec![
                Field::Slider {
                    label: self.t("level_gain"),
                    value: f64::from(gain_db),
                    range: slider::Range::new(-60., 24., 0.1),
                    default: 0.,
                },
                Field::Slider {
                    label: self.t("level_pan"),
                    value: f64::from(pan_percent),
                    range: slider::Range::new(-100., 100., 0.1),
                    default: 0.,
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
                    let db: f32 = v[0].parse().unwrap();
                    let percent: f32 = v[1].parse().unwrap();
                    let t = &self.project.tracks[i];
                    let gain = if db == gain_db {
                        t.gain
                    } else {
                        10f32.powf(db / 20.)
                    };
                    let pan = if percent == pan_percent {
                        t.pan
                    } else {
                        percent / 100.
                    };
                    self.apply_track_levels(i, gain, pan);
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
    fn volume_and_pan_sliders_expose_values_and_reset_independently() {
        unsafe {
            slider::initialize().unwrap();
            let mut form = Box::new(Form {
                fields: vec![
                    Field::Slider {
                        label: "Volume (dB)".into(),
                        value: 0.2,
                        range: slider::Range::new(-60., 24., 0.1),
                        default: 0.,
                    },
                    Field::Slider {
                        label: "Pan (%)".into(),
                        value: 0.,
                        range: slider::Range::new(-100., 100., 0.1),
                        default: 0.,
                    },
                ],
                strings: load_strings(false),
                slider_access: Some(slider::AccessibleValues::new()),
                result: None,
            });
            let h = create_dialog(null_mut(), &mut form, "Volume and pan", "").unwrap();
            let volume = GetDlgItem(h, 101);
            let pan = GetDlgItem(h, 103);
            let reset_volume = GetDlgItem(h, RESET_BASE);
            let reset_pan = GetDlgItem(h, RESET_BASE + 1);
            assert_eq!(
                accessibility_tests::name_role_value(volume),
                ("Volume (dB) slider".into(), 0x33, "0.2".into())
            );
            assert_eq!(
                accessibility_tests::name_role_value(pan),
                ("Pan (%) slider".into(), 0x33, "0".into())
            );
            ShowWindow(h, 5);
            SetFocus(volume);
            let mut tab = MSG {
                hwnd: volume,
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
            assert_eq!(accessibility_tests::name_role_value(pan).2, "-0.1");
            SendMessageW(reset_volume, 0x00f5, 0, 0); // BM_CLICK.
            assert_ne!(IsWindow(h), 0);
            assert!(form.result.is_none());
            assert_eq!(accessibility_tests::name_role_value(pan).2, "-0.1");
            SendMessageW(h, 0x111, 1, 0);
            assert_eq!(form.result.take().unwrap(), ["0", "-0.1"]);
        }
    }
}
