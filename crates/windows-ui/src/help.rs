//! Modeless native text help with section navigation and case-insensitive search.
use super::w::*;
use std::ptr::{null, null_mut};
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Document {
    Guide,
    About,
    Changelog,
    Updates,
    Licenses,
}
impl Document {
    fn keys(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Guide => ("guide", "help_title", "help_guide_label"),
            Self::Updates => ("update_text", "update_title", "update_label"),
            Self::Licenses => ("license_text", "license_title", "license_label"),
            Self::About => ("about_text", "about_title", "about_content_label"),
            Self::Changelog => (
                "changelog_text",
                "changelog_title",
                "changelog_content_label",
            ),
        }
    }
}
struct View {
    owner: HWND,
    document: Document,
    focus: i32,
    text: String,
    sections: Vec<usize>,
    query: String,
    end: usize,
    strings: serde_json::Value,
}
pub(super) struct Viewer {
    pub hwnd: HWND,
    context: Box<View>,
}
impl View {
    fn t(&self, key: &str) -> String {
        self.strings[key].as_str().unwrap_or(key).to_owned()
    }
    unsafe fn install(&mut self, h: HWND, guide: &str) {
        unsafe {
            let guide = if self.document == Document::Licenses {
                include_str!(concat!(env!("OUT_DIR"), "/licenses.txt"))
            } else {
                guide
            };
            let (_, title_key, label_key) = self.document.keys();
            let combo = GetDlgItem(h, 901);
            SendMessageW(combo, 0x14b, 0, 0);
            self.sections.clear();
            self.text.clear();
            for line in guide.lines() {
                if let Some(title) = line.strip_prefix("## ") {
                    self.sections.push(self.text.encode_utf16().count());
                    SendMessageW(combo, 0x143, 0, wide(title).as_ptr() as isize);
                    self.text.push_str(title);
                } else {
                    self.text.push_str(line);
                }
                self.text.push_str("\r\n");
            }
            if self.sections.is_empty() {
                self.sections.push(0);
                SendMessageW(combo, 0x143, 0, wide(&self.t(title_key)).as_ptr() as isize);
            }
            SendMessageW(combo, 0x14e, 0, 0);
            SetWindowTextW(GetDlgItem(h, 902), wide(&self.text).as_ptr());
            for (id, key) in [
                (900, "help_sections"),
                (907, label_key),
                (908, "help_find_label"),
                (903, "help_next"),
                (1, "help_close"),
                (909, "help_result"),
            ] {
                SetWindowTextW(GetDlgItem(h, id), wide(&self.t(key)).as_ptr());
            }
            SetWindowTextW(h, wide(&self.t(title_key)).as_ptr());
            SetWindowTextW(
                GetDlgItem(h, 910),
                wide(&self.t(if self.document == Document::Guide {
                    "help_open_html"
                } else {
                    "update_download"
                }))
                .as_ptr(),
            );
            ShowWindow(
                GetDlgItem(h, 910),
                if self.document == Document::Guide
                    || (self.document == Document::Updates
                        && self.strings["update_url"]
                            .as_str()
                            .is_some_and(|s| !s.is_empty()))
                {
                    5
                } else {
                    0
                },
            );
            SetWindowTextW(GetDlgItem(h, 904), wide("").as_ptr());
            SetWindowTextW(GetDlgItem(h, 906), wide("").as_ptr());
            self.end = 0;
            self.query.clear();
            SetWindowTextW(
                GetDlgItem(h, 911),
                wide(&self.t(
                    if self.strings["update_downloading"].as_bool() == Some(true) {
                        "update_cancel"
                    } else {
                        "update_install"
                    },
                ))
                .as_ptr(),
            );
            ShowWindow(
                GetDlgItem(h, 911),
                if self.document == Document::Updates
                    && self.strings["update_can_install"].as_bool() == Some(true)
                {
                    5
                } else {
                    0
                },
            );
            SetWindowTextW(GetDlgItem(h, 912), wide(&self.t("update_skip")).as_ptr());
            ShowWindow(
                GetDlgItem(h, 912),
                if self.document == Document::Updates
                    && self.strings["update_url"]
                        .as_str()
                        .is_some_and(|s| !s.is_empty())
                {
                    5
                } else {
                    0
                },
            );
            let downloading = self.strings["update_downloading"].as_bool() == Some(true);
            let saving_skip = self.strings["update_saving_skip"].as_bool() == Some(true);
            let skipped = self.strings["update_skipped_here"].as_bool() == Some(true);
            EnableWindow(GetDlgItem(h, 911), i32::from(!saving_skip));
            EnableWindow(
                GetDlgItem(h, 912),
                i32::from(!downloading && !saving_skip && !skipped),
            );
            layout(h);
        }
    }
    unsafe fn section(&self, h: HWND, focus: bool) {
        unsafe {
            let i = SendMessageW(GetDlgItem(h, 901), 0x147, 0, 0);
            if i >= 0
                && let Some(offset) = self.sections.get(i as usize)
            {
                SendMessageW(GetDlgItem(h, 902), 0xb1, *offset, *offset as isize);
                SendMessageW(GetDlgItem(h, 902), 0xb7, 0, 0);
                if focus {
                    SetFocus(GetDlgItem(h, 902));
                }
            }
        }
    }
    unsafe fn search(&mut self, h: HWND) {
        unsafe {
            let edit = GetDlgItem(h, 904);
            let mut b = vec![0u16; GetWindowTextLengthW(edit).max(0) as usize + 1];
            let n = GetWindowTextW(edit, b.as_mut_ptr(), b.len() as i32);
            let query = String::from_utf16_lossy(&b[..n.max(0) as usize]);
            if query != self.query {
                self.end = 0;
                self.query = query.clone();
            }
            if let Some((start, end)) = find(&self.text, &query, self.end) {
                self.end = end;
                SendMessageW(GetDlgItem(h, 902), 0xb1, start, end as isize);
                SendMessageW(GetDlgItem(h, 902), 0xb7, 0, 0);
                SetWindowTextW(GetDlgItem(h, 906), wide(&self.t("help_found")).as_ptr());
                SetFocus(GetDlgItem(h, 902));
            } else {
                SetWindowTextW(GetDlgItem(h, 906), wide(&self.t("help_not_found")).as_ptr());
                SetFocus(GetDlgItem(h, 906));
            }
        }
    }
}
fn find(text: &str, query: &str, after: usize) -> Option<(usize, usize)> {
    if query.is_empty() {
        return None;
    }
    let mut folded = String::new();
    let mut map = vec![];
    let mut position = 0;
    for c in text.chars() {
        let lower = c.to_lowercase().collect::<String>();
        let end = position + c.len_utf16();
        map.extend(std::iter::repeat_n((position, end), lower.len()));
        folded.push_str(&lower);
        position = end;
    }
    let needle = query.to_lowercase();
    let matches: Vec<_> = folded
        .match_indices(&needle)
        .map(|(start, _)| (map[start].0, map[start + needle.len() - 1].1))
        .collect();
    matches
        .iter()
        .find(|(start, _)| *start >= after)
        .copied()
        .or_else(|| matches.first().copied())
}
unsafe fn layout(h: HWND) {
    unsafe {
        let mut r = RECT::default();
        GetClientRect(h, &mut r);
        let dpi = GetDpiForWindow(h).max(96) as i32;
        let s = |x| x * dpi / 96;
        let width = (r.right - s(24)).max(s(240));
        let extra = if GetWindowLongPtrW(GetDlgItem(h, 910), -16) & 0x10000000 != 0 {
            s(34)
        } else {
            0
        };
        let content = (r.bottom - s(190) - extra).max(s(50));
        for (id, x, y, w, height) in [
            (900, s(12), s(8), width, s(20)),
            (901, s(12), s(30), width, s(200)),
            (912, s(12), s(60), s(168), s(26)),
            (910, r.right - s(355), s(60), s(168), s(26)),
            (911, r.right - s(180), s(60), s(168), s(26)),
            (907, s(12), s(60) + extra, width, s(20)),
            (902, s(12), s(82) + extra, width, content),
            (908, s(12), s(88) + content + extra, width / 2, s(20)),
            (904, s(12), s(110) + content + extra, width - s(220), s(24)),
            (
                903,
                r.right - s(215),
                s(110) + content + extra,
                s(100),
                s(26),
            ),
            (1, r.right - s(108), s(110) + content + extra, s(96), s(26)),
            (909, s(12), s(141) + content + extra, s(110), s(20)),
            (906, s(130), s(141) + content + extra, width - s(118), s(24)),
        ] {
            MoveWindow(GetDlgItem(h, id), x, y, w, height, 1);
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
            let min = &mut *(l as *mut MINMAXINFO);
            let dpi = GetDpiForWindow(h).max(96) as i32;
            min.min_track.x = 600 * dpi / 96;
            min.min_track.y = 380 * dpi / 96;
            return 0;
        }
        let ptr = GetWindowLongPtrW(h, -21) as *mut View;
        if m == 0x111 {
            let id = w & 0xffff;
            if id == 1 && w >> 16 == 0 {
                DestroyWindow(h);
                return 0;
            }
            if !ptr.is_null() {
                if matches!(id, 911 | 912) && w >> 16 == 0 && (*ptr).document == Document::Updates {
                    let view = &*ptr;
                    let command = if id == 912 {
                        super::updates::SKIP
                    } else if view.strings["update_downloading"].as_bool() == Some(true) {
                        super::updates::CANCEL
                    } else {
                        super::updates::INSTALL
                    };
                    // This top-level window has an owner, rather than a child-window parent.
                    if view.owner.is_null() || PostMessageW(view.owner, 0x111, command, 0) == 0 {
                        let status = GetDlgItem(h, 906);
                        SetWindowTextW(status, wide(&view.t("update_action_failed")).as_ptr());
                        SetFocus(status);
                    }
                    return 0;
                }
                if id == 910 && w >> 16 == 0 {
                    let result = if (*ptr).document == Document::Guide {
                        open_html(
                            h,
                            (&(*ptr).strings)["help_document_language"].as_str() == Some("es"),
                        )
                    } else {
                        super::updates::open_release(h, &(*ptr).t("update_url"))
                    };
                    if let Err(e) = result {
                        SetWindowTextW(GetDlgItem(h, 906), wide(&e).as_ptr());
                        SetFocus(GetDlgItem(h, 906));
                    }
                    return 0;
                }
                if id == 903 && w >> 16 == 0 {
                    (*ptr).search(h);
                    return 0;
                }
                if id == 901 && w >> 16 == 1 {
                    (*ptr).section(h, false);
                    return 0;
                }
            }
        }
        DefWindowProcW(h, m, w, l)
    }
}
/// Embed both languages so EXE-only updates also update the offline HTML guide.
fn write_html(folder: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(folder)?;
    for (name, content) in [
        (
            "user-guide.en.html",
            include_bytes!("../../../docs/user-guide.en.html").as_slice(),
        ),
        (
            "user-guide.es.html",
            include_bytes!("../../../docs/user-guide.es.html").as_slice(),
        ),
    ] {
        sonitrelix_session::atomic_write(&folder.join(name), content)?;
    }
    Ok(())
}
unsafe fn open_html(owner: HWND, es: bool) -> Result<(), String> {
    let folder = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("Cannot locate the portable folder")?
        .join("data/docs");
    write_html(&folder).map_err(|e| e.to_string())?;
    let path = folder.join(if es {
        "user-guide.es.html"
    } else {
        "user-guide.en.html"
    });
    let path: Vec<u16> = std::os::windows::ffi::OsStrExt::encode_wide(path.as_os_str())
        .chain(Some(0))
        .collect();
    let result = unsafe {
        ShellExecuteW(
            owner,
            wide("open").as_ptr(),
            path.as_ptr(),
            null(),
            null(),
            1,
        )
    } as isize;
    if result <= 32 {
        Err(if es {
            "No se pudo abrir la guía HTML. Puede seguir leyendo la guía de texto."
        } else {
            "Could not open the HTML guide. You can keep reading the text guide."
        }
        .into())
    } else {
        Ok(())
    }
}
#[link(name = "shell32")]
unsafe extern "system" {
    fn ShellExecuteW(
        owner: HWND,
        operation: *const u16,
        file: *const u16,
        parameters: *const u16,
        directory: *const u16,
        show: i32,
    ) -> HWND;
}
impl Viewer {
    pub unsafe fn open(
        owner: HWND,
        strings: serde_json::Value,
        document: Document,
    ) -> Result<Self, String> {
        unsafe {
            let class = wide("SonitrelixHelp");
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
            RegisterClassW(&wc);
            let hwnd = CreateWindowExW(
                0x10000,
                class.as_ptr(),
                wide("Sonitrelix").as_ptr(),
                0x00cf0000,
                i32::MIN,
                i32::MIN,
                800,
                650,
                owner,
                null_mut(),
                instance,
                null_mut(),
            );
            if hwnd.is_null() {
                return Err(std::io::Error::last_os_error().to_string());
            }
            let focus = GetDlgCtrlID(GetFocus());
            for (id, class, style) in [
                (900, "STATIC", 0),
                (901, "COMBOBOX", 0x10000 | 3 | 0x200000),
                (912, "BUTTON", 0x10000),
                (911, "BUTTON", 0x10000),
                (907, "STATIC", 0),
                (
                    902,
                    "EDIT",
                    0x10000 | 0x800000 | 0x200000 | 0x800 | 4 | 0x40 | 0x1000,
                ),
                (910, "BUTTON", 0x10000),
                (908, "STATIC", 0),
                (904, "EDIT", 0x10000 | 0x800000 | 0x80),
                (903, "BUTTON", 0x10000),
                (1, "BUTTON", 0x10000),
                (909, "STATIC", 0),
                (906, "EDIT", 0x10000 | 0x800000 | 0x800 | 0x80),
            ] {
                let c = CreateWindowExW(
                    0,
                    wide(class).as_ptr(),
                    wide("").as_ptr(),
                    0x50000000 | style,
                    0,
                    0,
                    1,
                    1,
                    hwnd,
                    id as HMENU,
                    instance,
                    null_mut(),
                );
                if c.is_null() {
                    DestroyWindow(hwnd);
                    return Err("Cannot create help control".into());
                }
                SendMessageW(c, 0x30, GetStockObject(17) as usize, 1);
                if id == 902 {
                    SendMessageW(c, 0xc5, 2000000, 0);
                }
            }
            let mut context = Box::new(View {
                owner,
                document,
                focus,
                text: String::new(),
                sections: vec![],
                query: String::new(),
                end: 0,
                strings,
            });
            let guide = context.t(document.keys().0);
            context.install(hwnd, &guide);
            SetWindowLongPtrW(hwnd, -21, (&mut *context as *mut View) as isize);
            layout(hwnd);
            ShowWindow(hwnd, 5);
            SetFocus(GetDlgItem(hwnd, 902));
            Ok(Self { hwnd, context })
        }
    }
    pub unsafe fn show(&mut self, document: Document) {
        unsafe {
            if self.context.document != document {
                self.context.document = document;
                let text = self.context.t(document.keys().0);
                self.context.install(self.hwnd, &text);
            }
            ShowWindow(self.hwnd, 5);
            SetFocus(GetDlgItem(self.hwnd, 902));
        }
    }
    pub unsafe fn update(&mut self, strings: serde_json::Value) {
        unsafe {
            self.context.strings = strings;
            let text = self.context.t(self.context.document.keys().0);
            self.context.install(self.hwnd, &text);
        }
    }
    pub fn focus_id(&self) -> i32 {
        self.context.focus
    }
    pub fn is_updates(&self) -> bool {
        self.context.document == Document::Updates
    }
}
impl Drop for Viewer {
    fn drop(&mut self) {
        unsafe {
            if IsWindow(self.hwnd) != 0 {
                DestroyWindow(self.hwnd);
            }
        }
    }
}
pub(super) unsafe fn translate(h: HWND, msg: &mut MSG) -> bool {
    unsafe {
        if msg.hwnd != h && IsChild(h, msg.hwnd) == 0 {
            return false;
        }
        if tab_from_readonly(h, GetDlgItem(h, 902), msg) {
            return true;
        }
        if msg.message == 0x100 {
            let ptr = GetWindowLongPtrW(h, -21) as *mut View;
            if !ptr.is_null() {
                if msg.wParam == 0x1b {
                    if SendMessageW(GetDlgItem(h, 901), 0x157, 0, 0) != 0 {
                        SendMessageW(GetDlgItem(h, 901), 0x14f, 0, 0);
                        return true;
                    }
                    DestroyWindow(h);
                    return true;
                }
                if msg.wParam == 0x46 && GetKeyState(0x11) < 0 {
                    let search = GetDlgItem(h, 904);
                    SetFocus(search);
                    SendMessageW(search, 0xb1, 0, -1);
                    return true;
                }
                if msg.wParam == 0x41 && GetKeyState(0x11) < 0 && GetFocus() == GetDlgItem(h, 902) {
                    SendMessageW(GetDlgItem(h, 902), 0xb1, 0, -1);
                    return true;
                }
                if msg.wParam == 0x72 || (msg.wParam == 0x0d && GetFocus() == GetDlgItem(h, 904)) {
                    (*ptr).search(h);
                    return true;
                }
                if msg.wParam == 0x0d && GetFocus() == GetDlgItem(h, 901) {
                    (*ptr).section(h, true);
                    return true;
                }
            }
        }
        if IsDialogMessageW(h, msg) == 0 {
            TranslateMessage(msg);
            DispatchMessageW(msg);
        }
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn embedded_html_is_current_and_bilingual_without_browser_launch() {
        let folder =
            std::env::temp_dir().join(format!("sonitrelix-guide-{}-á", std::process::id()));
        write_html(&folder).unwrap();
        for lang in ["en", "es"] {
            let path = folder.join(format!("user-guide.{lang}.html"));
            let text = std::fs::read_to_string(&path).unwrap();
            assert!(text.contains(&format!("<html lang=\"{lang}\">")));
            assert!(text.contains("<caption>"));
            std::fs::write(path, "old guide").unwrap();
        }
        write_html(&folder).unwrap();
        assert!(
            std::fs::read_to_string(folder.join("user-guide.en.html"))
                .unwrap()
                .contains("<h1>Sonitrelix user guide</h1>")
        );
        std::fs::remove_dir_all(folder).unwrap();
    }
    #[test]
    fn search_unicode_offsets_and_wrap() {
        assert_eq!(find("voz Ñandú VOZ", "ñANDÚ", 0), Some((4, 9)));
        assert_eq!(find("voz voz", "VOZ", 3), Some((4, 7)));
        assert_eq!(find("voz voz", "voz", 7), Some((0, 3)));
        assert_eq!(find("a😀b", "b", 0), Some((3, 4)));
        assert_eq!(find("voice", "", 0), None);
    }
    #[test]
    fn native_help_names_sections_and_search() {
        unsafe {
            let owner = CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                wide("Help test").as_ptr(),
                0,
                0,
                0,
                800,
                650,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null()),
                null_mut(),
            );
            let mut viewer =
                Viewer::open(owner, super::super::load_strings(false), Document::Guide).unwrap();
            ShowWindow(viewer.hwnd, 0);
            for id in [901, 902, 904, 903, 1, 906, 910] {
                let (name, _) =
                    super::super::accessibility_tests::name_and_role(GetDlgItem(viewer.hwnd, id));
                assert!(!name.is_empty(), "help control {id}");
            }
            assert!(!viewer.context.sections.is_empty());
            let tab = MSG {
                hwnd: GetDlgItem(viewer.hwnd, 902),
                message: 0x100,
                wParam: 9,
                ..MSG::default()
            };
            viewer.show(Document::Guide);
            assert!(tab_from_readonly(viewer.hwnd, tab.hwnd, &tab));
            assert_eq!(GetFocus(), GetDlgItem(viewer.hwnd, 910));
            SetWindowTextW(GetDlgItem(viewer.hwnd, 904), wide("track").as_ptr());
            viewer.context.search(viewer.hwnd);
            assert!(viewer.context.end > 0);
            viewer.update(super::super::load_strings(true));
            for es in [false, true] {
                viewer.update(super::super::load_strings(es));
                for document in [
                    Document::Guide,
                    Document::About,
                    Document::Changelog,
                    Document::Updates,
                ] {
                    viewer.show(document);
                    let content = GetDlgItem(viewer.hwnd, 902);
                    assert_eq!(GetFocus(), content, "initial focus: {document:?}");
                    let (name, role) = super::super::accessibility_tests::name_and_role(content);
                    assert_eq!(role, 42, "content must expose text, not a button");
                    assert!(name.contains(if es { "solo lectura" } else { "read-only" }));
                    assert_ne!(GetWindowLongPtrW(content, -16) & 0x800, 0, "ES_READONLY");
                    let before = viewer.context.text.clone();
                    SendMessageW(content, 0xb1, 0, -1);
                    SendMessageW(content, 0x102, b'X' as usize, 0);
                    let mut text = vec![0u16; before.encode_utf16().count() + 1];
                    let n = GetWindowTextW(content, text.as_mut_ptr(), text.len() as i32);
                    assert_eq!(String::from_utf16_lossy(&text[..n as usize]), before);
                    // Switching documents resets stale search state and keeps the language.
                    assert!(viewer.context.query.is_empty());
                    assert!(!viewer.context.sections.is_empty());
                }
            }
            DestroyWindow(viewer.hwnd);
            drop(viewer);
            DestroyWindow(owner);
        }
    }
}
