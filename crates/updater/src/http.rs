//! Bounded HTTPS transport through Windows proxy/TLS facilities. Used only on workers.
use super::*;
use std::{
    ffi::c_void,
    fs::{self, OpenOptions},
    io::{Seek, SeekFrom, Write},
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
    time::{Duration, Instant},
};
type H = *mut c_void;
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
#[link(name = "winhttp")]
unsafe extern "system" {
    fn WinHttpOpen(a: *const u16, access: u32, p: *const u16, b: *const u16, flags: u32) -> H;
    fn WinHttpConnect(h: H, host: *const u16, port: u16, r: u32) -> H;
    fn WinHttpOpenRequest(
        h: H,
        verb: *const u16,
        path: *const u16,
        version: *const u16,
        referer: *const u16,
        accept: *const *const u16,
        flags: u32,
    ) -> H;
    fn WinHttpSetTimeouts(h: H, a: i32, b: i32, c: i32, d: i32) -> i32;
    fn WinHttpSetOption(h: H, id: u32, p: *const c_void, n: u32) -> i32;
    fn WinHttpQueryOption(h: H, id: u32, p: *mut c_void, n: *mut u32) -> i32;
    fn WinHttpSendRequest(
        h: H,
        headers: *const u16,
        n: u32,
        p: *const c_void,
        l: u32,
        total: u32,
        context: usize,
    ) -> i32;
    fn WinHttpReceiveResponse(h: H, p: *mut c_void) -> i32;
    fn WinHttpQueryHeaders(
        h: H,
        id: u32,
        name: *const u16,
        p: *mut c_void,
        n: *mut u32,
        index: *mut u32,
    ) -> i32;
    fn WinHttpReadData(h: H, p: *mut c_void, n: u32, read: *mut u32) -> i32;
    fn WinHttpCloseHandle(h: H) -> i32;
}
struct Handle(H);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            WinHttpCloseHandle(self.0);
        }
    }
}
fn handle(h: H) -> io::Result<Handle> {
    if h.is_null() {
        Err(io::Error::last_os_error())
    } else {
        Ok(Handle(h))
    }
}
fn check(v: i32) -> io::Result<()> {
    if v == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
struct Response {
    request: Handle,
    _connection: Handle,
    _session: Handle,
    status: u32,
}
fn response(url: &str, offset: u64) -> io::Result<Response> {
    unsafe {
        let rest = url
            .strip_prefix("https://")
            .ok_or_else(|| bad("Updates require HTTPS"))?;
        let (host, path) = rest
            .split_once('/')
            .ok_or_else(|| bad("Missing HTTPS path"))?;
        if !["github.com", "api.github.com"].contains(&host) || path.contains(['\r', '\n']) {
            return Err(bad("Unexpected update origin"));
        }
        let session = handle(WinHttpOpen(
            wide(concat!("Sonitrelix/", env!("CARGO_PKG_VERSION"))).as_ptr(),
            4,
            std::ptr::null(),
            std::ptr::null(),
            0,
        ))?;
        check(WinHttpSetTimeouts(session.0, 5000, 5000, 5000, 5000))?;
        let connection = handle(WinHttpConnect(session.0, wide(host).as_ptr(), 443, 0))?;
        let request = handle(WinHttpOpenRequest(
            connection.0,
            wide("GET").as_ptr(),
            wide(&format!("/{path}")).as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            0x800000,
        ))?;
        let policy = 1u32;
        check(WinHttpSetOption(
            request.0,
            88,
            (&policy as *const u32).cast(),
            4,
        ))?;
        let redirects = 5u32;
        check(WinHttpSetOption(
            request.0,
            89,
            (&redirects as *const u32).cast(),
            4,
        ))?;
        let headers = wide(&format!(
            "Accept: application/octet-stream\r\nAccept-Encoding: identity\r\n{}",
            if offset > 0 {
                format!("Range: bytes={offset}-\r\n")
            } else {
                String::new()
            }
        ));
        check(WinHttpSendRequest(
            request.0,
            headers.as_ptr(),
            (headers.len() - 1) as u32,
            std::ptr::null(),
            0,
            0,
            0,
        ))?;
        check(WinHttpReceiveResponse(request.0, std::ptr::null_mut()))?;
        let mut status = 0u32;
        let mut n = 4;
        check(WinHttpQueryHeaders(
            request.0,
            19 | 0x20000000,
            std::ptr::null(),
            (&mut status as *mut u32).cast(),
            &mut n,
            std::ptr::null_mut(),
        ))?;
        let mut final_url = vec![0u16; 8192];
        let mut n = (final_url.len() * 2) as u32;
        check(WinHttpQueryOption(
            request.0,
            34,
            final_url.as_mut_ptr().cast(),
            &mut n,
        ))?;
        let end = final_url
            .iter()
            .position(|c| *c == 0)
            .unwrap_or(final_url.len());
        if !String::from_utf16_lossy(&final_url[..end]).starts_with("https://") {
            return Err(bad("Update redirected outside HTTPS"));
        }
        Ok(Response {
            request,
            _connection: connection,
            _session: session,
            status,
        })
    }
}
pub fn fetch(url: &str, limit: usize, cancel: &AtomicBool) -> io::Result<Vec<u8>> {
    let r = response(url, 0)?;
    if r.status != 200 {
        return Err(io::Error::other(format!("Update server HTTP {}", r.status)));
    }
    let mut data = vec![];
    read(&r, cancel, Duration::from_secs(25), |b| {
        if data.len() + b.len() > limit {
            return Err(bad("Update metadata exceeds size limit"));
        }
        data.extend_from_slice(b);
        Ok(())
    })?;
    Ok(data)
}
fn read(
    r: &Response,
    cancel: &AtomicBool,
    timeout: Duration,
    mut write: impl FnMut(&[u8]) -> io::Result<()>,
) -> io::Result<()> {
    unsafe {
        let deadline = Instant::now() + timeout;
        let mut block = [0u8; 65536];
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "Update cancelled",
                ));
            }
            if Instant::now() > deadline {
                return Err(io::Error::new(io::ErrorKind::TimedOut, "Update timed out"));
            }
            let mut n = 0;
            check(WinHttpReadData(
                r.request.0,
                block.as_mut_ptr().cast(),
                block.len() as u32,
                &mut n,
            ))?;
            if n == 0 {
                return Ok(());
            }
            write(&block[..n as usize])?;
        }
    }
}
pub fn download(
    m: &Manifest,
    path: &Path,
    cancel: &AtomicBool,
    progress: &AtomicU32,
) -> io::Result<()> {
    let mut file = match OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            let metadata = fs::symlink_metadata(path)?;
            use std::os::windows::fs::MetadataExt;
            if !metadata.is_file() || metadata.file_attributes() & 0x400 != 0 {
                return Err(bad("Unsafe partial download"));
            }
            OpenOptions::new().read(true).write(true).open(path)?
        }
        Err(e) => return Err(e),
    };
    let mut length = file.metadata()?.len();
    if length > m.size {
        return Err(bad("Partial update exceeds signed size"));
    }
    if length < m.size {
        let r = response(&m.url, length)?;
        if r.status == 200 {
            file.set_len(0)?;
            length = 0;
        } else if r.status == 206 && length > 0 {
            unsafe {
                let mut b = [0u16; 256];
                let mut n = (b.len() * 2) as u32;
                check(WinHttpQueryHeaders(
                    r.request.0,
                    53,
                    std::ptr::null(),
                    b.as_mut_ptr().cast(),
                    &mut n,
                    std::ptr::null_mut(),
                ))?;
                let end = b.iter().position(|v| *v == 0).unwrap_or(b.len());
                let range = String::from_utf16_lossy(&b[..end]);
                let expected =
                    format!("bytes {length}-{} /{}", m.size - 1, m.size).replace(" ", "");
                if range.replace(" ", "") != expected {
                    return Err(bad("Invalid resumed Content-Range"));
                }
            }
        } else {
            return Err(io::Error::other(format!(
                "Update download HTTP {}",
                r.status
            )));
        }
        file.seek(SeekFrom::Start(length))?;
        read(&r, cancel, Duration::from_secs(600), |b| {
            if length + b.len() as u64 > m.size {
                return Err(bad("Download exceeds signed size"));
            }
            file.write_all(b)?;
            length += b.len() as u64;
            progress.store((length * 100 / m.size) as u32, Ordering::Relaxed);
            Ok(())
        })?;
        file.sync_all()?;
    }
    drop(file);
    if let Err(e) = verify_package(path, m) {
        let _ = fs::remove_file(path);
        return Err(e);
    }
    Ok(())
}
