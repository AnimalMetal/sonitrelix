//! Original ABI declarations for the documented MMDevice APIs (Windows SDK 10).
//! COM interfaces and allocated strings stay on the discovery worker thread.
use super::{Catalog, Device};
use std::{
    ffi::c_void,
    marker::PhantomData,
    ptr::{NonNull, null_mut},
    rc::Rc,
};
#[repr(C)]
struct Guid {
    a: u32,
    b: u16,
    c: u16,
    d: [u8; 8],
}
const CLSID: Guid = Guid {
    a: 0xbcde0395,
    b: 0xe52f,
    c: 0x467c,
    d: [0x8e, 0x3d, 0xc4, 0x57, 0x92, 0x91, 0x69, 0x2e],
};
const IID: Guid = Guid {
    a: 0xa95664d2,
    b: 0x9614,
    c: 0x4f35,
    d: [0xa7, 0x46, 0xde, 0x8d, 0xb6, 0x36, 0x17, 0xe6],
};
#[repr(C)]
struct PropertyKey {
    fmtid: Guid,
    pid: u32,
}
const FRIENDLY_NAME: PropertyKey = PropertyKey {
    fmtid: Guid {
        a: 0xa45c254e,
        b: 0xdf1c,
        c: 0x4efd,
        d: [0x80, 0x20, 0x67, 0xd1, 0x46, 0xa8, 0x50, 0xe0],
    },
    pid: 14,
};
#[repr(C)]
struct Variant {
    vt: u16,
    reserved: [u16; 3],
    data: [usize; 2],
}
impl Drop for Variant {
    fn drop(&mut self) {
        unsafe {
            PropVariantClear(self);
        }
    }
}
#[repr(C)]
struct Unknown {
    query: usize,
    add: usize,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
}
#[repr(C)]
struct EnumeratorVtable {
    unknown: Unknown,
    endpoints: unsafe extern "system" fn(*mut c_void, i32, u32, *mut *mut c_void) -> i32,
    default: unsafe extern "system" fn(*mut c_void, i32, i32, *mut *mut c_void) -> i32,
}
#[repr(C)]
struct CollectionVtable {
    unknown: Unknown,
    count: unsafe extern "system" fn(*mut c_void, *mut u32) -> i32,
    item: unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void) -> i32,
}
#[repr(C)]
struct DeviceVtable {
    unknown: Unknown,
    activate: usize,
    properties: unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void) -> i32,
    id: unsafe extern "system" fn(*mut c_void, *mut *mut u16) -> i32,
}
#[repr(C)]
struct PropertiesVtable {
    unknown: Unknown,
    count: usize,
    at: usize,
    value: unsafe extern "system" fn(*mut c_void, *const PropertyKey, *mut Variant) -> i32,
}
struct Com<T> {
    raw: NonNull<c_void>,
    _type: PhantomData<(T, Rc<()>)>,
}
impl<T> Com<T> {
    fn from_result(hr: i32, raw: *mut c_void, operation: &str) -> Result<Self, String> {
        check(hr, operation)?;
        Ok(Self {
            raw: NonNull::new(raw).ok_or_else(|| format!("{operation}: null COM interface"))?,
            _type: PhantomData,
        })
    }
    fn raw(&self) -> *mut c_void {
        self.raw.as_ptr()
    }
    fn vt(&self) -> &T {
        // SAFETY: caller constructs each Com<T> only from the API for that interface.
        unsafe { &**(self.raw.as_ptr() as *const *const T) }
    }
}
impl<T> Drop for Com<T> {
    fn drop(&mut self) {
        unsafe {
            let vt = &**(self.raw() as *const *const Unknown);
            (vt.release)(self.raw());
        }
    }
}
struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}
struct TaskString(*mut u16);
impl TaskString {
    fn text(&self) -> Result<String, String> {
        unsafe {
            if self.0.is_null() {
                return Err("Missing Windows endpoint string".into());
            }
            // API strings are NUL terminated. Bound scanning to the Windows maximum string size.
            let mut n = 0;
            while n < 32768 && *self.0.add(n) != 0 {
                n += 1;
            }
            if n == 32768 {
                return Err("Windows endpoint string is too long".into());
            }
            Ok(String::from_utf16_lossy(std::slice::from_raw_parts(
                self.0, n,
            )))
        }
    }
}
impl Drop for TaskString {
    fn drop(&mut self) {
        unsafe {
            CoTaskMemFree(self.0.cast());
        }
    }
}
#[link(name = "ole32")]
unsafe extern "system" {
    fn CoInitializeEx(reserved: *mut c_void, mode: u32) -> i32;
    fn CoUninitialize();
    fn CoCreateInstance(
        class: *const Guid,
        outer: *mut c_void,
        context: u32,
        iid: *const Guid,
        out: *mut *mut c_void,
    ) -> i32;
    fn CoTaskMemFree(memory: *mut c_void);
    fn PropVariantClear(value: *mut Variant) -> i32;
}
fn check(hr: i32, operation: &str) -> Result<(), String> {
    if hr < 0 {
        Err(format!("{operation}: Windows error 0x{:08X}", hr as u32))
    } else {
        Ok(())
    }
}
fn device_id(device: &Com<DeviceVtable>) -> Result<String, String> {
    unsafe {
        let mut text = TaskString(null_mut());
        check(
            (device.vt().id)(device.raw(), &mut text.0),
            "Get endpoint ID",
        )?;
        text.text()
    }
}
fn collect(e: &Com<EnumeratorVtable>, flow: i32) -> Result<Vec<Device>, String> {
    unsafe {
        let mut raw = null_mut();
        let hr = (e.vt().default)(e.raw(), flow, 1, &mut raw); // eMultimedia, not communications.
        let default = if hr >= 0 {
            Some(device_id(&Com::<DeviceVtable>::from_result(
                hr,
                raw,
                "Get default endpoint",
            )?)?)
        } else if hr as u32 == 0x80070490 {
            None
        } else {
            return Err(format!(
                "Get default endpoint: Windows error 0x{:08X}",
                hr as u32
            ));
        };
        raw = null_mut();
        let hr = (e.vt().endpoints)(e.raw(), flow, 1, &mut raw); // DEVICE_STATE_ACTIVE
        let collection = Com::<CollectionVtable>::from_result(hr, raw, "Enumerate endpoints")?;
        let mut count = 0;
        check(
            (collection.vt().count)(collection.raw(), &mut count),
            "Count endpoints",
        )?;
        let mut result = Vec::new();
        for i in 0..count {
            raw = null_mut();
            let hr = (collection.vt().item)(collection.raw(), i, &mut raw);
            let device = Com::<DeviceVtable>::from_result(hr, raw, "Read endpoint")?;
            let id = device_id(&device)?;
            raw = null_mut();
            let hr = (device.vt().properties)(device.raw(), 0, &mut raw);
            let props = Com::<PropertiesVtable>::from_result(hr, raw, "Read device properties")?;
            let mut name = Variant {
                vt: 0,
                reserved: [0; 3],
                data: [0; 2],
            };
            check(
                (props.vt().value)(props.raw(), &FRIENDLY_NAME, &mut name),
                "Read device name",
            )?;
            let label = if name.vt == 31 {
                // The PROPVARIANT owns this pointer; do not free it separately.
                let borrowed = std::mem::ManuallyDrop::new(TaskString(name.data[0] as *mut u16));
                borrowed.text()?
            } else {
                id.clone()
            };
            result.push(Device {
                is_default: default.as_ref() == Some(&id),
                id,
                name: label,
            });
        }
        result.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then(a.id.cmp(&b.id))
        });
        Ok(result)
    }
}
pub fn enumerate() -> Result<Catalog, String> {
    unsafe {
        check(CoInitializeEx(null_mut(), 0), "Initialize COM")?;
        let _apartment = Apartment;
        let mut raw = null_mut();
        let hr = CoCreateInstance(&CLSID, null_mut(), 1, &IID, &mut raw);
        let enumerator =
            Com::<EnumeratorVtable>::from_result(hr, raw, "Create audio device enumerator")?;
        Ok(Catalog {
            inputs: collect(&enumerator, 1)?,
            outputs: collect(&enumerator, 0)?,
        })
    }
}
