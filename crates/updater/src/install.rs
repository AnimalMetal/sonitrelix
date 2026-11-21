use super::*;
use std::os::windows::{fs::MetadataExt, process::CommandExt};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub root: PathBuf,
    pub nonce: String,
    pub old_pid: u32,
    pub old_version: String,
    pub envelope: String,
}
#[derive(Serialize, Deserialize)]
struct Health {
    nonce: String,
    version: String,
}
#[derive(Serialize, Deserialize)]
struct State {
    version: String,
}
fn safe_directory(p: &Path) -> io::Result<()> {
    if !p.is_absolute() || p.parent().is_none() {
        return Err(bad("Unsafe update directory"));
    }
    let m = fs::symlink_metadata(p)?;
    if !m.is_dir() || m.file_attributes() & 0x400 != 0 {
        return Err(bad("Update directories cannot be links/reparse points"));
    }
    Ok(())
}
pub fn portable(root: &Path) -> bool {
    root.join(".sonitrelix-portable").is_file()
        && root.join("Sonitrelix.exe").is_file()
        && safe_directory(root).is_ok()
}
pub fn cache(root: &Path) -> io::Result<PathBuf> {
    if !portable(root) {
        return Err(bad(
            "Install updates from the portable distribution, not a development folder",
        ));
    }
    let data = root.join("data");
    fs::create_dir_all(&data)?;
    safe_directory(&data)?;
    let dir = data.join("update-cache");
    fs::create_dir_all(&dir)?;
    safe_directory(&dir)?;
    Ok(dir)
}
fn job(plan: &Plan) -> io::Result<PathBuf> {
    if plan.nonce.len() != 32
        || !plan.nonce.bytes().all(|b| b.is_ascii_hexdigit())
        || plan.old_pid == 0
    {
        return Err(bad("Invalid update transaction"));
    }
    if !portable(&plan.root) || fs::canonicalize(&plan.root)? != plan.root {
        return Err(bad(
            "Only a portable installed application can update itself",
        ));
    }
    let data = plan.root.join("data");
    safe_directory(&data)?;
    let updates = data.join("updates");
    safe_directory(&updates)?;
    let path = updates.join(&plan.nonce);
    safe_directory(&path)?;
    Ok(path)
}
fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut f = OpenOptions::new().write(true).create_new(true).open(path)?;
    f.write_all(bytes)?;
    f.sync_all()
}
fn copy_new(source: &Path, target: &Path) -> io::Result<()> {
    let mut from = fs::File::open(source)?;
    let mut to = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)?;
    io::copy(&mut from, &mut to)?;
    to.sync_all()
}
pub fn confirmed(root: &Path, current: &str) -> String {
    let stored = fs::read(root.join("data/update-state.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<State>(&b).ok());
    stored
        .filter(|s| Version::parse(&s.version).ok() > Version::parse(current).ok())
        .map(|s| s.version)
        .unwrap_or_else(|| current.into())
}
pub fn confirm_running(root: &Path) -> io::Result<()> {
    if !portable(root) {
        return Ok(());
    }
    let version = confirmed(root, env!("CARGO_PKG_VERSION"));
    sonitrelix_session::atomic_write(
        &root.join("data/update-state.json"),
        &serde_json::to_vec(&State { version })?,
    )
}
pub fn prepare(root: &Path, envelope: &[u8], package: &Path, old_pid: u32) -> io::Result<PathBuf> {
    let root = fs::canonicalize(root)?;
    if !portable(&root) {
        return Err(bad(
            "Automatic installation is disabled for development/source builds. Use the portable ZIP.",
        ));
    }
    let m = manifest(envelope)?;
    if !eligible(
        &m,
        env!("CARGO_PKG_VERSION"),
        &confirmed(&root, env!("CARGO_PKG_VERSION")),
    )? {
        return Err(bad("No newer eligible update"));
    }
    verify_package(package, &m)?;
    let data = root.join("data");
    fs::create_dir_all(&data)?;
    safe_directory(&data)?;
    let updates = data.join("updates");
    fs::create_dir_all(&updates)?;
    safe_directory(&updates)?;
    let mut random = [0u8; 16];
    if unsafe { openssl_sys::RAND_bytes(random.as_mut_ptr(), 16) } != 1 {
        return Err(bad("Cannot create update transaction ID"));
    }
    let nonce = random
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let directory = updates.join(&nonce);
    fs::create_dir(&directory)?;
    let plan = Plan {
        root,
        nonce,
        old_pid,
        old_version: env!("CARGO_PKG_VERSION").into(),
        envelope: String::from_utf8(envelope.to_vec()).map_err(|_| bad("Manifest is not UTF-8"))?,
    };
    copy_new(package, &directory.join("package.exe"))?;
    verify_package(&directory.join("package.exe"), &m)?;
    copy_new(&std::env::current_exe()?, &directory.join("helper.exe"))?;
    // The helper starts from its own directory on a machine that may only have app-local CRT DLLs.
    for name in ["vcruntime140.dll", "vcruntime140_1.dll"] {
        let source = plan.root.join(name);
        if source.is_file() {
            copy_new(&source, &directory.join(name))?;
        }
    }
    let path = directory.join("plan.json");
    write_new(&path, &serde_json::to_vec(&plan)?)?;
    Ok(path)
}
pub fn launch_helper(plan: &Path) -> io::Result<()> {
    Command::new(
        plan.parent()
            .ok_or_else(|| bad("Missing update directory"))?
            .join("helper.exe"),
    )
    .arg("--apply-update")
    .arg(plan)
    .creation_flags(0x08000000)
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .spawn()?;
    Ok(())
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetDiskFreeSpaceExW(
        path: *const u16,
        available: *mut u64,
        total: *mut u64,
        free: *mut u64,
    ) -> i32;
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut std::ffi::c_void;
    fn WaitForSingleObject(handle: *mut std::ffi::c_void, ms: u32) -> u32;
    fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
    fn QueryFullProcessImageNameW(
        handle: *mut std::ffi::c_void,
        flags: u32,
        name: *mut u16,
        len: *mut u32,
    ) -> i32;
}
pub fn check_space(root: &Path, package_size: u64) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let needed = package_size.saturating_mul(2).saturating_add(
        fs::metadata(root.join("Sonitrelix.exe"))?
            .len()
            .saturating_mul(2),
    );
    let path: Vec<_> = root.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut available = 0;
    if unsafe {
        GetDiskFreeSpaceExW(
            path.as_ptr(),
            &mut available,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    if available < needed {
        return Err(io::Error::other(
            "Not enough disk space for update and verified backup",
        ));
    }
    Ok(())
}
fn wait_old(plan: &Plan, timeout: Duration) -> io::Result<()> {
    unsafe {
        let h = OpenProcess(0x100000 | 0x1000, 0, plan.old_pid);
        if h.is_null() {
            let e = io::Error::last_os_error();
            if e.raw_os_error() == Some(87) {
                return Ok(());
            }
            return Err(e);
        }
        let result = (|| {
            if WaitForSingleObject(h, 0) == 0 {
                return Ok(());
            }
            let mut b = vec![0u16; 32768];
            let mut len = b.len() as u32;
            if QueryFullProcessImageNameW(h, 0, b.as_mut_ptr(), &mut len) == 0 {
                return Err(io::Error::last_os_error());
            }
            let actual =
                fs::canonicalize(PathBuf::from(String::from_utf16_lossy(&b[..len as usize])))?;
            if actual != fs::canonicalize(plan.root.join("Sonitrelix.exe"))? {
                return Err(bad(
                    "Original application process does not match installation",
                ));
            }
            match WaitForSingleObject(h, timeout.as_millis().min(u128::from(u32::MAX)) as u32) {
                0 => Ok(()),
                258 => Err(bad(
                    "Application did not close; update cancelled without changing it",
                )),
                _ => Err(io::Error::last_os_error()),
            }
        })();
        CloseHandle(h);
        result
    }
}
fn launch(exe: &Path, nonce: Option<&str>) -> io::Result<std::process::Child> {
    let mut c = Command::new(exe);
    if let Some(n) = nonce {
        c.arg("--post-update").arg(n);
    }
    c.creation_flags(0x08000000)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
}
pub fn apply(path: &Path) -> io::Result<()> {
    let plan: Plan = serde_json::from_slice(&fs::read(path)?)?;
    let directory = job(&plan)?;
    if fs::canonicalize(path)? != directory.join("plan.json")
        || fs::canonicalize(std::env::current_exe()?)? != directory.join("helper.exe")
    {
        return Err(bad(
            "Update helper must run inside its prepared transaction",
        ));
    }
    if plan.old_version != env!("CARGO_PKG_VERSION") {
        return Err(bad("Wrong update helper version"));
    }
    let result = apply_inner(
        &plan,
        &public_key()?,
        Duration::from_secs(60),
        Duration::from_secs(60),
    );
    if result.is_err() && wait_old(&plan, Duration::ZERO).is_ok() {
        let target = plan.root.join("Sonitrelix.exe");
        let backup = directory.join("previous.exe");
        if directory.join("package.exe").exists()
            || (backup.exists() && digest_file(&target).ok() == digest_file(&backup).ok())
        {
            let _ = launch(&target, None);
        }
    }
    let message = match &result {
        Ok(()) => "Update installed".into(),
        Err(e) => e.to_string(),
    };
    let _ = sonitrelix_session::atomic_write(&directory.join("result.txt"), message.as_bytes());
    result
}
fn apply_inner(
    plan: &Plan,
    key: &[u8; 32],
    close_timeout: Duration,
    health_timeout: Duration,
) -> io::Result<()> {
    let directory = job(plan)?;
    let m = manifest_key(plan.envelope.as_bytes(), key)?;
    if !eligible(
        &m,
        &plan.old_version,
        &confirmed(&plan.root, &plan.old_version),
    )? {
        return Err(bad("Update downgrade/rollback rejected"));
    }
    let package = directory.join("package.exe");
    verify_package(&package, &m)?;
    wait_old(plan, close_timeout)?;
    let target = plan.root.join("Sonitrelix.exe");
    let backup = directory.join("previous.exe");
    copy_new(&target, &backup)?;
    if digest_file(&target)? != digest_file(&backup)? {
        return Err(bad("Backup verification failed; installation unchanged"));
    }
    verify_package(&package, &m)?;
    sonitrelix_session::replace(&package, &target)?;
    let mut launched = None;
    let attempted = (|| {
        launched = Some(launch(&target, Some(&plan.nonce))?);
        let child = launched.as_mut().unwrap();
        let deadline = Instant::now() + health_timeout;
        loop {
            if child.try_wait()?.is_some() {
                return Err(bad("New version exited without confirming startup"));
            }
            if let Ok(bytes) = fs::read(directory.join("health.json"))
                && let Ok(h) = serde_json::from_slice::<Health>(&bytes)
                && h.nonce == plan.nonce
                && h.version == m.version
            {
                sonitrelix_session::atomic_write(
                    &plan.root.join("data/update-state.json"),
                    &serde_json::to_vec(&State {
                        version: m.version.clone(),
                    })?,
                )?;
                return Ok(());
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(bad("New version did not confirm startup"));
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    })();
    if let Err(error) = attempted {
        if let Some(child) = &mut launched
            && child.try_wait().ok().flatten().is_none()
        {
            let _ = child.kill();
            let _ = child.wait();
        }
        let restore = directory.join("restore.exe");
        copy_new(&backup, &restore)?;
        sonitrelix_session::replace(&restore, &target)?;
        sonitrelix_session::atomic_write(&plan.root.join("data/update-result.txt"),format!("Update was rolled back. Your previous version was restored; data was preserved. {error}").as_bytes())?;
        sonitrelix_session::atomic_write(
            &plan.root.join("data/update-skipped.json"),
            &serde_json::to_vec(&format!("v{}", m.version))?,
        )?;
        return Err(error);
    }
    Ok(())
}
pub fn confirm_start(root: &Path, nonce: &str) -> io::Result<()> {
    unhex::<16>(nonce)?;
    let path = root.join("data/updates").join(nonce).join("plan.json");
    let plan: Plan = serde_json::from_slice(&fs::read(path)?)?;
    let directory = job(&plan)?;
    if plan.nonce != nonce || fs::canonicalize(root)? != plan.root {
        return Err(bad("Wrong update startup transaction"));
    }
    let m = manifest(plan.envelope.as_bytes())?;
    if m.version != env!("CARGO_PKG_VERSION") {
        return Err(bad("Wrong version after update"));
    }
    // The UI calls this only after its message loop has remained alive for five seconds.
    sonitrelix_session::atomic_write(
        &directory.join("health.json"),
        &serde_json::to_vec(&Health {
            nonce: nonce.into(),
            version: m.version,
        })?,
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_replacement_rollback_and_uncancelled_original_preserve_data() {
        let temp = std::env::temp_dir();
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = temp.join(format!(
            "Sonitrelix update ñ's {}-{stamp}",
            std::process::id()
        ));
        fs::create_dir_all(&base).unwrap();
        let old_src = base.join("old.rs");
        let new_src = base.join("new.rs");
        fs::write(
            &old_src,
            r#"fn main(){std::thread::sleep(std::time::Duration::from_secs(10));}"#,
        )
        .unwrap();
        fs::write(&new_src,r#"fn main(){let a:Vec<_>=std::env::args().collect();let root=std::env::current_exe().unwrap().parent().unwrap().to_path_buf();let mode=std::fs::read_to_string(root.join("data/mode")).unwrap();if mode=="healthy"{let text=format!("{{\"nonce\":\"{}\",\"version\":\"9.9.9\"}}",a[2]);std::fs::write(root.join("data/updates").join(&a[2]).join("health.json"),text).unwrap();}else if mode=="crash"{std::process::exit(1);}std::thread::sleep(std::time::Duration::from_secs(10));}"#).unwrap();
        for (src, name) in [(&old_src, "old.exe"), (&new_src, "new.exe")] {
            let output = Command::new("rustc")
                .arg(src)
                .args(["--crate-name", "update_fixture", "-o"])
                .arg(base.join(name))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        for mode in ["healthy", "crash", "hang", "keep-open"] {
            let root = base.join(mode);
            fs::create_dir(&root).unwrap();
            fs::create_dir(root.join("data")).unwrap();
            let root = fs::canonicalize(root).unwrap();
            let target = root.join("Sonitrelix.exe");
            fs::copy(base.join("old.exe"), &target).unwrap();
            fs::write(root.join(".sonitrelix-portable"), b"1").unwrap();
            fs::write(root.join("data/recording.wav"), b"private audio unchanged").unwrap();
            fs::write(root.join("data/mode"), mode).unwrap();
            let nonce = "1234567890abcdef1234567890abcdef";
            let directory = root.join("data/updates").join(nonce);
            fs::create_dir_all(&directory).unwrap();
            let package = directory.join("package.exe");
            fs::copy(base.join("new.exe"), &package).unwrap();
            let m = crate::tests::example(&package);
            let text = crate::tests::signed(&m);
            let mut old = Command::new(&target).spawn().unwrap();
            let plan = Plan {
                root: root.clone(),
                nonce: nonce.into(),
                old_pid: old.id(),
                old_version: "0.6.0".into(),
                envelope: text,
            };
            let original = digest_file(&target).unwrap();
            if mode == "keep-open" {
                assert!(
                    apply_inner(
                        &plan,
                        &crate::tests::key(),
                        Duration::from_millis(20),
                        Duration::from_secs(2)
                    )
                    .is_err()
                );
                assert_eq!(digest_file(&target).unwrap(), original);
                assert!(old.try_wait().unwrap().is_none());
                old.kill().unwrap();
                old.wait().unwrap();
            } else {
                // Exit only this test-owned original; the installer itself never terminates it.
                old.kill().unwrap();
                old.wait().unwrap();
                let result = apply_inner(
                    &plan,
                    &crate::tests::key(),
                    Duration::from_secs(2),
                    Duration::from_secs(2),
                );
                if mode == "healthy" {
                    assert!(result.is_ok(), "{result:?}");
                    assert_ne!(digest_file(&target).unwrap(), original);
                } else {
                    assert!(result.is_err());
                    assert_eq!(digest_file(&target).unwrap(), original);
                }
            }
            assert_eq!(
                fs::read(root.join("data/recording.wav")).unwrap(),
                b"private audio unchanged"
            );
        }
        // The healthy test child exits by itself before removing this explicitly named temp tree.
        std::thread::sleep(Duration::from_secs(10));
        assert!(base.starts_with(&temp));
        fs::remove_dir_all(base).unwrap();
    }
}
