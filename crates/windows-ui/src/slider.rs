//! Numeric ranges and accessible values for native trackbars.
use super::w::*;
use windows::{
    Win32::{
        Foundation::HWND as WinHwnd,
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
            CoUninitialize,
        },
        UI::Accessibility::{
            CLSID_AccPropServices, IAccPropServices, PROPID_ACC_NAME, PROPID_ACC_VALUE,
        },
    },
    core::{GUID, PCWSTR},
};

pub const CLASS: &str = "msctls_trackbar32";
pub const STYLE: u32 = 0x10000 | 0x10; // WS_TABSTOP | TBS_NOTICKS
const GET_POS: u32 = 0x400;
const SET_POS: u32 = 0x405;
const SET_MIN: u32 = 0x407;
const SET_MAX: u32 = 0x408;
const SET_PAGE: u32 = 0x415;

#[repr(C)]
struct CommonControls {
    size: u32,
    classes: u32,
}
#[link(name = "comctl32")]
unsafe extern "system" {
    fn InitCommonControlsEx(controls: *const CommonControls) -> i32;
}

pub fn initialize() -> std::io::Result<()> {
    let controls = CommonControls {
        size: std::mem::size_of::<CommonControls>() as u32,
        classes: 4, // ICC_BAR_CLASSES
    };
    if unsafe { InitCommonControlsEx(&controls) } != 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[derive(Clone, Copy)]
pub struct Range {
    min: f64,
    max: f64,
    step: f64,
}
/// Keep the native Slider role while exposing the value in its own units.
pub struct AccessibleValues {
    service: Option<IAccPropServices>,
    initialized: bool,
}
impl AccessibleValues {
    pub fn new() -> Self {
        unsafe {
            let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
            let service = CoCreateInstance(&CLSID_AccPropServices, None, CLSCTX_INPROC_SERVER).ok();
            Self {
                service,
                initialized,
            }
        }
    }
    fn annotate(&self, hwnd: HWND, property: GUID, text: &str) {
        if let Some(service) = &self.service {
            let text = wide(text);
            unsafe {
                let _ = service.SetHwndPropStr(
                    WinHwnd(hwnd),
                    0xfffffffc,
                    0,
                    property,
                    PCWSTR(text.as_ptr()),
                );
            }
        }
    }

    pub fn name(&self, hwnd: HWND, text: &str) {
        self.annotate(hwnd, PROPID_ACC_NAME, text);
    }
}
impl Drop for AccessibleValues {
    fn drop(&mut self) {
        drop(self.service.take());
        if self.initialized {
            unsafe { CoUninitialize() };
        }
    }
}
impl Range {
    pub const fn new(min: f64, max: f64, step: f64) -> Self {
        Self { min, max, step }
    }
    fn ticks(self) -> isize {
        ((self.max - self.min) / self.step).round() as isize
    }
    fn position(self, value: f64) -> isize {
        if value == self.max {
            self.ticks()
        } else {
            ((value - self.min) / self.step).round() as isize
        }
    }
    pub unsafe fn set_value(self, handle: HWND, value: f64, access: &AccessibleValues) {
        unsafe {
            let position = self.position(value.clamp(self.min, self.max));
            if SendMessageW(handle, GET_POS, 0, 0) != position {
                SendMessageW(handle, SET_POS, 1, position);
            }
            self.update_accessible_value(handle, access);
        }
    }
    pub unsafe fn configure(
        self,
        handle: HWND,
        value: f64,
        access: &AccessibleValues,
        label: &str,
    ) {
        unsafe {
            access.name(handle, label);
            SendMessageW(handle, SET_MIN, 0, 0);
            SendMessageW(handle, SET_MAX, 1, self.ticks());
            SendMessageW(handle, SET_PAGE, 0, (self.ticks() / 10).max(1));
            self.set_value(handle, value, access);
        }
    }
    pub unsafe fn update_accessible_value(self, handle: HWND, access: &AccessibleValues) {
        access.annotate(
            handle,
            PROPID_ACC_VALUE,
            &unsafe { self.value(handle) }.to_string(),
        );
    }
    pub unsafe fn value(self, handle: HWND) -> f64 {
        let position = unsafe { SendMessageW(handle, GET_POS, 0, 0) };
        if position == self.ticks() {
            return self.max;
        }
        let value = (self.min + position as f64 * self.step).min(self.max);
        let precision = if self.step < 0.1 { 100. } else { 10. };
        (value * precision).round() / precision
    }
}
