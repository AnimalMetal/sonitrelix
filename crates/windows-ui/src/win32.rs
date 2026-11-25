//! Minimal original Win32 ABI bindings. Keep unsafe code in the UI adapter.
#![allow(
    non_snake_case,
    non_camel_case_types,
    dead_code,
    clippy::upper_case_acronyms
)]
use std::ffi::c_void;
pub type HWND = *mut c_void;
pub type HMENU = *mut c_void;
pub type WNDPROC = unsafe extern "system" fn(HWND, u32, usize, isize) -> isize;
#[repr(C)]
#[derive(Default)]
pub struct POINT {
    pub x: i32,
    pub y: i32,
}
#[repr(C)]
#[derive(Default)]
pub struct RECT {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}
#[repr(C)]
pub struct MINMAXINFO {
    pub reserved: POINT,
    pub max_size: POINT,
    pub max_position: POINT,
    pub min_track: POINT,
    pub max_track: POINT,
}
#[repr(C)]
#[derive(Default)]
pub struct MSG {
    pub hwnd: HWND,
    pub message: u32,
    pub wParam: usize,
    pub lParam: isize,
    pub time: u32,
    pub pt: POINT,
    pub lPrivate: u32,
}
#[repr(C)]
pub struct WNDCLASSW {
    pub style: u32,
    pub proc: Option<WNDPROC>,
    pub cls_extra: i32,
    pub wnd_extra: i32,
    pub instance: HWND,
    pub icon: HWND,
    pub cursor: HWND,
    pub background: HWND,
    pub menu: *const u16,
    pub name: *const u16,
}
#[repr(C)]
pub struct OPENFILENAMEW {
    pub size: u32,
    pub owner: HWND,
    pub instance: HWND,
    pub filter: *const u16,
    pub custom_filter: *mut u16,
    pub max_custom: u32,
    pub filter_index: u32,
    pub file: *mut u16,
    pub max_file: u32,
    pub file_title: *mut u16,
    pub max_title: u32,
    pub initial_dir: *const u16,
    pub title: *const u16,
    pub flags: u32,
    pub file_offset: u16,
    pub extension_offset: u16,
    pub default_extension: *const u16,
    pub custom_data: isize,
    pub hook: Option<unsafe extern "system" fn(HWND, u32, usize, isize) -> usize>,
    pub template: *const u16,
    pub reserved: *mut c_void,
    pub reserved2: u32,
    pub flags_ex: u32,
}
pub type DLGPROC = unsafe extern "system" fn(HWND, u32, usize, isize) -> isize;
#[repr(C)]
pub struct PROPSHEETPAGEW {
    pub size: u32,
    pub flags: u32,
    pub instance: HWND,
    pub template: *const u32,
    pub icon: HWND,
    pub title: *const u16,
    pub proc: Option<DLGPROC>,
    pub param: isize,
    pub callback: usize,
    pub ref_parent: *mut u32,
}
#[repr(C)]
pub struct PROPSHEETHEADERW {
    pub size: u32,
    pub flags: u32,
    pub parent: HWND,
    pub instance: HWND,
    pub icon: HWND,
    pub caption: *const u16,
    pub count: u32,
    pub start: usize,
    pub pages: *const PROPSHEETPAGEW,
    pub callback: usize,
}
#[repr(C)]
pub struct NMHDR {
    pub hwnd: HWND,
    pub id: usize,
    pub code: u32,
}
#[repr(C)]
pub struct PSHNOTIFY {
    pub hdr: NMHDR,
    pub param: isize,
}
#[repr(C)]
pub struct TCITEMW {
    pub mask: u32,
    pub state: u32,
    pub state_mask: u32,
    pub text: *mut u16,
    pub text_max: i32,
    pub image: i32,
    pub param: isize,
}
#[link(name = "user32")]
unsafe extern "system" {
    pub fn RegisterClassW(c: *const WNDCLASSW) -> u16;
    pub fn CreateWindowExW(
        ex: u32,
        class: *const u16,
        text: *const u16,
        style: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        parent: HWND,
        menu: HMENU,
        instance: HWND,
        param: *mut c_void,
    ) -> HWND;
    pub fn DefWindowProcW(h: HWND, m: u32, w: usize, l: isize) -> isize;
    pub fn DestroyWindow(h: HWND) -> i32;
    pub fn ShowWindow(h: HWND, cmd: i32) -> i32;
    pub fn SetFocus(h: HWND) -> HWND;
    pub fn GetFocus() -> HWND;
    pub fn GetClassNameW(h: HWND, text: *mut u16, max: i32) -> i32;
    pub fn GetDpiForWindow(h: HWND) -> u32;
    pub fn GetMessageW(m: *mut MSG, h: HWND, min: u32, max: u32) -> i32;
    pub fn TranslateMessage(m: *const MSG) -> i32;
    pub fn DispatchMessageW(m: *const MSG) -> isize;
    pub fn IsDialogMessageW(h: HWND, m: *mut MSG) -> i32;
    pub fn GetNextDlgTabItem(dialog: HWND, current: HWND, previous: i32) -> HWND;
    pub fn PostQuitMessage(code: i32);
    pub fn SetWindowLongPtrW(h: HWND, index: i32, v: isize) -> isize;
    pub fn GetWindowLongPtrW(h: HWND, index: i32) -> isize;
    pub fn SetWindowTextW(h: HWND, t: *const u16) -> i32;
    pub fn GetWindowTextLengthW(h: HWND) -> i32;
    pub fn GetWindowTextW(h: HWND, t: *mut u16, n: i32) -> i32;
    pub fn SendMessageW(h: HWND, m: u32, w: usize, l: isize) -> isize;
    pub fn MessageBoxW(h: HWND, text: *const u16, title: *const u16, kind: u32) -> i32;
    pub fn CreateMenu() -> HMENU;
    pub fn CreatePopupMenu() -> HMENU;
    pub fn AppendMenuW(h: HMENU, flags: u32, id: usize, t: *const u16) -> i32;
    pub fn TrackPopupMenu(
        h: HMENU,
        flags: u32,
        x: i32,
        y: i32,
        reserved: i32,
        owner: HWND,
        exclude: *const RECT,
    ) -> u32;
    pub fn GetWindowRect(h: HWND, r: *mut RECT) -> i32;
    pub fn ClientToScreen(h: HWND, point: *mut POINT) -> i32;
    pub fn SetMenu(h: HWND, m: HMENU) -> i32;
    pub fn GetMenu(h: HWND) -> HMENU;
    pub fn DestroyMenu(h: HMENU) -> i32;
    pub fn DrawMenuBar(h: HWND) -> i32;
    pub fn CheckMenuItem(h: HMENU, id: u32, flags: u32) -> u32;
    pub fn GetKeyState(key: i32) -> i16;
    pub fn GetClientRect(h: HWND, r: *mut RECT) -> i32;
    pub fn MoveWindow(h: HWND, x: i32, y: i32, w: i32, height: i32, repaint: i32) -> i32;
    pub fn GetDlgItem(h: HWND, id: i32) -> HWND;
    pub fn LoadCursorW(i: HWND, name: *const u16) -> HWND;
    pub fn SystemParametersInfoW(action: u32, param: u32, data: *mut c_void, flags: u32) -> i32;
    pub fn SetProcessDpiAwarenessContext(context: isize) -> i32;
    pub fn GetParent(h: HWND) -> HWND;
    pub fn GetDlgCtrlID(h: HWND) -> i32;
    pub fn IsWindow(h: HWND) -> i32;
    pub fn IsChild(parent: HWND, child: HWND) -> i32;
    pub fn EnableWindow(h: HWND, enabled: i32) -> i32;
    pub fn MapDialogRect(h: HWND, r: *mut RECT) -> i32;
    pub fn PostMessageW(h: HWND, message: u32, w: usize, l: isize) -> i32;
    pub fn SetTimer(h: HWND, id: usize, ms: u32, callback: usize) -> usize;
    pub fn KillTimer(h: HWND, id: usize) -> i32;
}
#[link(name = "comctl32")]
unsafe extern "system" {
    pub fn PropertySheetW(header: *const PROPSHEETHEADERW) -> isize;
}
#[link(name = "gdi32")]
unsafe extern "system" {
    pub fn GetStockObject(id: i32) -> HWND;
}
#[link(name = "kernel32")]
unsafe extern "system" {
    pub fn GetModuleHandleW(n: *const u16) -> HWND;
    pub fn GetUserDefaultUILanguage() -> u16;
}
#[link(name = "comdlg32")]
unsafe extern "system" {
    pub fn GetOpenFileNameW(o: *mut OPENFILENAMEW) -> i32;
    pub fn GetSaveFileNameW(o: *mut OPENFILENAMEW) -> i32;
    pub fn CommDlgExtendedError() -> u32;
}
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Multiline edits can claim Tab as text input. Route it to the next dialog control.
pub unsafe fn tab_from_readonly(dialog: HWND, control: HWND, msg: &MSG) -> bool {
    unsafe {
        if msg.message != 0x100
            || msg.wParam != 9
            || msg.hwnd != control
            || GetKeyState(0x11) < 0
            || GetKeyState(0x12) < 0
        {
            return false;
        }
        let next = GetNextDlgTabItem(dialog, control, i32::from(GetKeyState(0x10) < 0));
        if next.is_null() {
            return false;
        }
        SetFocus(next);
        true
    }
}
