//! User-initiated track changes, sent to assistive technology without moving focus.
use super::w;
use windows::{
    Win32::{
        Foundation::HWND,
        System::Variant::VARIANT,
        UI::Accessibility::{
            IRawElementProviderSimple, IRawElementProviderSimple_Impl, NotificationKind_Other,
            NotificationProcessing_ImportantMostRecent, ProviderOptions,
            ProviderOptions_ServerSideProvider, UIA_PATTERN_ID, UIA_PROPERTY_ID,
            UiaHostProviderFromHwnd,
        },
    },
    core::{BSTR, HRESULT, IUnknown, Interface, Result, implement},
};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetProcAddress(module: w::HWND, name: *const u8) -> *const std::ffi::c_void;
}

#[implement(IRawElementProviderSimple)]
struct StatusSource(isize);

impl IRawElementProviderSimple_Impl for StatusSource_Impl {
    fn ProviderOptions(&self) -> Result<ProviderOptions> {
        Ok(ProviderOptions_ServerSideProvider)
    }

    fn GetPatternProvider(&self, _: UIA_PATTERN_ID) -> Result<IUnknown> {
        Err(windows::core::HRESULT(0x80004002_u32 as i32).into())
    }

    fn GetPropertyValue(&self, _: UIA_PROPERTY_ID) -> Result<VARIANT> {
        Ok(VARIANT::default())
    }

    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        unsafe { UiaHostProviderFromHwnd(HWND(self.0 as _)) }
    }
}

pub(super) fn announce(status: w::HWND, message: &str) {
    if status.is_null() {
        return;
    }
    let provider: IRawElementProviderSimple = StatusSource(status as isize).into();
    // The native status field retains the message if the client ignores UIA notifications.
    unsafe {
        // The notification export is absent on early Windows 10 builds.
        let module = w::GetModuleHandleW(w::wide("uiautomationcore.dll").as_ptr());
        if module.is_null() {
            return;
        }
        let address = GetProcAddress(module, c"UiaRaiseNotificationEvent".as_ptr().cast());
        if address.is_null() {
            return;
        }
        type RaiseNotification = unsafe extern "system" fn(
            *mut std::ffi::c_void,
            i32,
            i32,
            *const u16,
            *const u16,
        ) -> HRESULT;
        let raise: RaiseNotification = std::mem::transmute(address);
        let text = BSTR::from(message);
        let activity = BSTR::from("Sonitrelix.TrackState");
        let _ = raise(
            provider.as_raw(),
            NotificationKind_Other.0,
            NotificationProcessing_ImportantMostRecent.0,
            text.as_ptr(),
            activity.as_ptr(),
        );
    }
}
