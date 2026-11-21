//! MSAA proxy checks only. These do not establish NVDA or JAWS compatibility.
use super::w::HWND;
use std::{ffi::c_void, ptr::null_mut};
#[repr(C)]
struct Guid {
    a: u32,
    b: u16,
    c: u16,
    d: [u8; 8],
}
const ACCESSIBLE: Guid = Guid {
    a: 0x618736e0,
    b: 0x3c3d,
    c: 0x11cf,
    d: [0x81, 0x0c, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71],
};
#[repr(C)]
struct Variant {
    vt: u16,
    reserved: [u16; 3],
    data: [usize; 2],
}
#[repr(C)]
struct Vtable {
    query: usize,
    add: usize,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    dispatch: [usize; 4],
    parent: usize,
    count: usize,
    child: usize,
    name: unsafe extern "system" fn(*mut c_void, Variant, *mut *mut u16) -> i32,
    value: unsafe extern "system" fn(*mut c_void, Variant, *mut *mut u16) -> i32,
    description: usize,
    role: unsafe extern "system" fn(*mut c_void, Variant, *mut Variant) -> i32,
}
struct Object(*mut c_void);
impl Drop for Object {
    fn drop(&mut self) {
        unsafe {
            let v = &**(self.0 as *const *const Vtable);
            (v.release)(self.0);
        }
    }
}
struct Apartment(bool);
impl Drop for Apartment {
    fn drop(&mut self) {
        if self.0 {
            unsafe {
                CoUninitialize();
            }
        }
    }
}
#[link(name = "oleacc")]
unsafe extern "system" {
    fn AccessibleObjectFromWindow(h: HWND, id: u32, iid: *const Guid, out: *mut *mut c_void)
    -> i32;
}
#[link(name = "ole32")]
unsafe extern "system" {
    fn CoInitializeEx(reserved: *mut c_void, mode: u32) -> i32;
    fn CoUninitialize();
}
#[link(name = "oleaut32")]
unsafe extern "system" {
    fn SysStringLen(text: *const u16) -> u32;
    fn SysFreeString(text: *mut u16);
}
pub(super) unsafe fn name_and_role(hwnd: HWND) -> (String, u32) {
    let (name, role, _) = unsafe { inspect(hwnd, false) };
    (name, role)
}
pub(super) unsafe fn name_role_value(hwnd: HWND) -> (String, u32, String) {
    unsafe { inspect(hwnd, true) }
}
unsafe fn inspect(hwnd: HWND, read_value: bool) -> (String, u32, String) {
    unsafe {
        let initialized = CoInitializeEx(null_mut(), 2);
        assert!(initialized >= 0 || initialized as u32 == 0x80010106);
        let _apartment = Apartment(initialized >= 0);
        let mut pointer = null_mut();
        assert!(AccessibleObjectFromWindow(hwnd, 0xfffffffc, &ACCESSIBLE, &mut pointer) >= 0);
        assert!(!pointer.is_null());
        let object = Object(pointer);
        let v = &**(object.0 as *const *const Vtable);
        let child = || Variant {
            vt: 3,
            reserved: [0; 3],
            data: [0; 2],
        };
        let mut name = null_mut();
        assert!((v.name)(object.0, child(), &mut name) >= 0);
        let label = if name.is_null() {
            String::new()
        } else {
            String::from_utf16_lossy(std::slice::from_raw_parts(
                name,
                SysStringLen(name) as usize,
            ))
        };
        SysFreeString(name);
        let mut role = Variant {
            vt: 0,
            reserved: [0; 3],
            data: [0; 2],
        };
        assert!((v.role)(object.0, child(), &mut role) >= 0);
        assert_eq!(role.vt, 3);
        let mut value = String::new();
        if read_value {
            let mut text = null_mut();
            assert!((v.value)(object.0, child(), &mut text) >= 0);
            if !text.is_null() {
                value = String::from_utf16_lossy(std::slice::from_raw_parts(
                    text,
                    SysStringLen(text) as usize,
                ));
            }
            SysFreeString(text);
        }
        (label, role.data[0] as u32, value)
    }
}
