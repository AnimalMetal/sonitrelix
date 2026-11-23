//! GitHub discovery and user-initiated signed portable updates.
use super::*;
use std::{
    ffi::c_void,
    sync::mpsc::{self, Receiver, TryRecvError},
    time::{Duration, Instant},
};
pub(super) const CHECK: usize = 504;
pub(super) const INSTALL: usize = 505;
pub(super) const CANCEL: usize = 506;
pub(super) const SKIP: usize = 508;
const LIMIT: usize = 256 * 1024;
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Version([u32; 3]);
impl Version {
    fn parse(s: &str) -> Result<Self, String> {
        let s = s.strip_prefix('v').unwrap_or(s);
        let parts: Vec<_> = s.split('.').collect();
        if parts.len() != 3 {
            return Err("Invalid stable release version".into());
        }
        let mut out = [0; 3];
        for (i, p) in parts.iter().enumerate() {
            if p.is_empty()
                || p.len() > 9
                || (p.len() > 1 && p.starts_with('0'))
                || !p.bytes().all(|b| b.is_ascii_digit())
            {
                return Err("Invalid stable release version".into());
            }
            out[i] = p.parse().map_err(|_| "Invalid version number")?;
        }
        Ok(Self(out))
    }
}
#[derive(Clone, Debug)]
pub(super) enum Report {
    NoRelease,
    Current,
    Available {
        tag: String,
        notes: String,
        url: String,
        envelope: Option<String>,
        installable: bool,
        warning: String,
    },
    Error(String),
}
#[derive(Default)]
pub(super) struct Checker {
    receiver: Option<Receiver<Report>>,
    pub report: Option<Report>,
    install_job: Option<Receiver<Result<PathBuf, String>>>,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    progress: std::sync::Arc<std::sync::atomic::AtomicU32>,
    milestone: u32,
    plan_ready: Option<PathBuf>,
    quiet: bool,
    next_check: Option<Instant>,
    ready: Option<(Report, bool)>,
    portable: bool,
    skipped: Option<String>,
    state_job: Option<Receiver<Result<String, String>>>,
}
fn parse(status: u32, bytes: &[u8], current: &str) -> Result<Report, String> {
    if bytes.len() > LIMIT {
        return Err("Release response exceeds size limit".into());
    }
    if status == 404 {
        return Ok(Report::NoRelease);
    }
    if status == 403 || status == 429 {
        return Err("GitHub rate limit reached. Try again later.".into());
    }
    if status != 200 {
        return Err(format!("GitHub HTTP {status}"));
    }
    let v: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| "Invalid GitHub response")?;
    if v["draft"].as_bool() != Some(false) || v["prerelease"].as_bool() != Some(false) {
        return Err("Not a published stable release".into());
    }
    let tag = v["tag_name"].as_str().ok_or("Missing release version")?;
    if Version::parse(tag)? <= Version::parse(current)? {
        return Ok(Report::Current);
    }
    let url = format!("https://github.com/AnimalMetal/sonitrelix/releases/tag/{tag}");
    let expected = format!(
        "https://github.com/AnimalMetal/sonitrelix/releases/download/{tag}/Sonitrelix-windows-x64.zip"
    );
    if !v["assets"].as_array().is_some_and(|a| {
        a.iter().any(|a| {
            a["name"].as_str() == Some("Sonitrelix-windows-x64.zip")
                && a["state"].as_str() == Some("uploaded")
                && a["size"].as_u64().is_some_and(|s| s > 0)
                && a["browser_download_url"].as_str() == Some(&expected)
        })
    }) {
        return Err("Release has no published Sonitrelix Windows package".into());
    }
    let notes = v["body"]
        .as_str()
        .unwrap_or("")
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .take(20000)
        .collect();
    Ok(Report::Available {
        envelope: None,
        installable: false,
        warning: String::new(),
        tag: tag.into(),
        notes,
        url,
    })
}
type Internet = *mut c_void;
#[link(name = "shell32")]
unsafe extern "system" {
    fn ShellExecuteW(
        owner: HWND,
        operation: *const u16,
        file: *const u16,
        parameters: *const u16,
        directory: *const u16,
        show: i32,
    ) -> Internet;
}
pub(super) unsafe fn open_release(owner: HWND, url: &str) -> Result<(), String> {
    unsafe {
        let tag = url
            .strip_prefix("https://github.com/AnimalMetal/sonitrelix/releases/tag/")
            .ok_or("Invalid release page")?;
        Version::parse(tag)?;
        let result = ShellExecuteW(
            owner,
            wide("open").as_ptr(),
            wide(url).as_ptr(),
            null(),
            null(),
            1,
        ) as isize;
        if result <= 32 {
            Err("Could not open the release page in your browser".into())
        } else {
            Ok(())
        }
    }
}
#[link(name = "winhttp")]
unsafe extern "system" {
    fn WinHttpOpen(
        agent: *const u16,
        access: u32,
        proxy: *const u16,
        bypass: *const u16,
        flags: u32,
    ) -> Internet;
    fn WinHttpConnect(session: Internet, host: *const u16, port: u16, reserved: u32) -> Internet;
    fn WinHttpOpenRequest(
        connect: Internet,
        verb: *const u16,
        path: *const u16,
        version: *const u16,
        referer: *const u16,
        accept: *const *const u16,
        flags: u32,
    ) -> Internet;
    fn WinHttpSetTimeouts(
        session: Internet,
        resolve: i32,
        connect: i32,
        send: i32,
        receive: i32,
    ) -> i32;
    fn WinHttpSetOption(handle: Internet, option: u32, buffer: *const c_void, length: u32) -> i32;
    fn WinHttpSendRequest(
        handle: Internet,
        headers: *const u16,
        header_len: u32,
        optional: *const c_void,
        optional_len: u32,
        total: u32,
        context: usize,
    ) -> i32;
    fn WinHttpReceiveResponse(handle: Internet, reserved: *mut c_void) -> i32;
    fn WinHttpQueryHeaders(
        handle: Internet,
        info: u32,
        name: *const u16,
        buffer: *mut c_void,
        length: *mut u32,
        index: *mut u32,
    ) -> i32;
    fn WinHttpReadData(handle: Internet, buffer: *mut c_void, size: u32, read: *mut u32) -> i32;
    fn WinHttpCloseHandle(handle: Internet) -> i32;
}
struct Handle(Internet);
impl Handle {
    fn new(h: Internet) -> Result<Self, String> {
        if h.is_null() {
            Err(std::io::Error::last_os_error().to_string())
        } else {
            Ok(Self(h))
        }
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            WinHttpCloseHandle(self.0);
        }
    }
}
fn check() -> Result<Report, String> {
    unsafe {
        let ok = |b| {
            if b != 0 {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error().to_string())
            }
        };
        let session = Handle::new(WinHttpOpen(
            wide(concat!("Sonitrelix/", env!("CARGO_PKG_VERSION"))).as_ptr(),
            4,
            null(),
            null(),
            0,
        ))?;
        ok(WinHttpSetTimeouts(session.0, 5000, 5000, 5000, 5000))?;
        let connect = Handle::new(WinHttpConnect(
            session.0,
            wide("api.github.com").as_ptr(),
            443,
            0,
        ))?;
        let request = Handle::new(WinHttpOpenRequest(
            connect.0,
            wide("GET").as_ptr(),
            wide("/repos/AnimalMetal/sonitrelix/releases/latest").as_ptr(),
            null(),
            null(),
            null(),
            0x800000,
        ))?;
        let disable = 2u32;
        ok(WinHttpSetOption(
            request.0,
            63,
            &disable as *const _ as *const c_void,
            4,
        ))?;
        let headers =
            wide("Accept: application/vnd.github+json\r\nX-GitHub-Api-Version: 2026-03-10\r\n");
        ok(WinHttpSendRequest(
            request.0,
            headers.as_ptr(),
            (headers.len() - 1) as u32,
            null(),
            0,
            0,
            0,
        ))?;
        ok(WinHttpReceiveResponse(request.0, null_mut()))?;
        let mut status = 0u32;
        let mut size = 4;
        ok(WinHttpQueryHeaders(
            request.0,
            19 | 0x20000000,
            null(),
            &mut status as *mut _ as *mut c_void,
            &mut size,
            null_mut(),
        ))?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        let mut bytes = vec![];
        let mut block = [0u8; 8192];
        loop {
            if std::time::Instant::now() > deadline {
                return Err("Update response timed out".into());
            }
            let mut n = 0;
            ok(WinHttpReadData(
                request.0,
                block.as_mut_ptr() as *mut c_void,
                block.len() as u32,
                &mut n,
            ))?;
            if n == 0 {
                break;
            }
            if bytes.len() + n as usize > LIMIT {
                return Err("Release response exceeds size limit".into());
            }
            bytes.extend_from_slice(&block[..n as usize]);
        }
        parse(status, &bytes, env!("CARGO_PKG_VERSION"))
    }
}
pub(super) fn install_text(strings: &mut serde_json::Value, report: Option<&Report>) {
    let t = |k: &str| strings[k].as_str().unwrap_or(k).to_owned();
    let (body, url) = match report {
        Some(Report::Available {
            tag,
            notes,
            url,
            envelope,
            installable,
            warning,
        }) => (
            format!(
                "{} {tag}\n\n{notes}\n\n{}",
                t("update_available"),
                if envelope.is_some() && *installable {
                    t("update_signed_ready")
                } else {
                    format!("{}\n{}", t("update_manual"), warning)
                }
            ),
            url.clone(),
        ),
        Some(Report::Current) => (t("update_current"), String::new()),
        Some(Report::NoRelease) => (t("update_none"), String::new()),
        Some(Report::Error(e)) => (format!("{}\n{e}", t("update_error")), String::new()),
        None => (t("update_checking"), String::new()),
    };
    strings["update_text"] = serde_json::Value::String(format!(
        "{} {}\n\n{body}",
        t("update_installed"),
        env!("CARGO_PKG_VERSION")
    ));
    strings["update_url"] = serde_json::Value::String(url);
    strings["update_skipped_here"] = false.into();
    strings["update_can_install"] = serde_json::Value::Bool(matches!(
        report,
        Some(Report::Available {
            envelope: Some(_),
            installable: true,
            ..
        })
    ));
}
impl App {
    pub(super) fn sync_update_action_flags(&mut self) {
        self.strings["update_downloading"] = self.updates.install_job.is_some().into();
        self.strings["update_saving_skip"] = self.updates.state_job.is_some().into();
    }

    unsafe fn refresh_update_actions(&mut self) {
        unsafe {
            self.sync_update_action_flags();
            if let Some(viewer) = self.help.as_mut().filter(|v| v.is_updates()) {
                viewer.update(self.strings.clone());
            }
        }
    }

    unsafe fn update_status(&self, text: &str, focus: bool) {
        unsafe {
            self.status(text);
            if let Some(viewer) = self.help.as_ref().filter(|v| v.is_updates()) {
                let status = GetDlgItem(viewer.hwnd, 906);
                SetWindowTextW(status, wide(text).as_ptr());
                if focus {
                    SetFocus(status);
                } else {
                    announcement::announce(status, text);
                }
            }
        }
    }

    pub(super) unsafe fn cancel_update_download(&self) {
        unsafe {
            if self.updates.install_job.is_some() {
                self.updates.cancel();
                self.update_status(&self.t("update_cancelling"), false);
            }
        }
    }

    pub(super) unsafe fn offer_update_result(&mut self) {
        let path = self.data.join("update-result.txt");
        if let Ok(meta) = fs::metadata(&path)
            && meta.len() <= 65536
            && let Ok(message) = fs::read_to_string(&path)
        {
            unsafe {
                MessageBoxW(
                    self.hwnd,
                    wide(&message).as_ptr(),
                    wide(&self.t("update_title")).as_ptr(),
                    0x40,
                );
            }
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            if let Err(error) = fs::rename(
                &path,
                self.data.join(format!("update-result-seen-{stamp}.txt")),
            ) {
                self.log(&format!("update_result_archive_failed: {error}"));
            }
        }
    }
    pub(super) unsafe fn install_update(&mut self) {
        unsafe {
            if self.updates.pending() {
                return;
            }
            if let Some(plan) = self.updates.plan_ready.take() {
                self.finish_update(plan);
                return;
            }
            let Some(Report::Available {
                envelope: Some(envelope),
                installable: true,
                ..
            }) = &self.updates.report
            else {
                self.update_status(&self.t("update_not_signed"), true);
                return;
            };
            let envelope = envelope.clone();
            let root = self.data.parent().unwrap().to_path_buf();
            self.updates.cancel = Default::default();
            self.updates.progress = Default::default();
            self.updates.milestone = 0;
            let cancel = self.updates.cancel.clone();
            let progress = self.updates.progress.clone();
            let (tx, rx) = mpsc::channel();
            match std::thread::Builder::new()
                .name("signed-update-download".into())
                .spawn(move || {
                    let result = (|| -> std::io::Result<PathBuf> {
                        let manifest = sonitrelix_updater::manifest(envelope.as_bytes())?;
                        sonitrelix_updater::check_space(&root, manifest.size)?;
                        let cache = sonitrelix_updater::cache(&root)?;
                        let package = cache.join(format!("{}.part", manifest.sha256));
                        sonitrelix_updater::http::download(
                            &manifest, &package, &cancel, &progress,
                        )?;
                        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::Interrupted,
                                "Update cancelled",
                            ));
                        }
                        sonitrelix_updater::prepare(
                            &root,
                            envelope.as_bytes(),
                            &package,
                            std::process::id(),
                        )
                    })()
                    .map_err(|e| e.to_string());
                    let _ = tx.send(result);
                }) {
                Ok(_) => {
                    self.updates.install_job = Some(rx);
                    self.refresh_update_actions();
                    self.update_status(&self.t("update_downloading_text"), false);
                }
                Err(e) => {
                    self.log_error("start update download", &e);
                    self.update_status(&format!("{} {e}", self.t("update_install_failed")), true);
                }
            }
        }
    }
    unsafe fn finish_update(&mut self, plan: PathBuf) {
        unsafe {
            if self.playback.closing {
                return;
            }
            if self.recording.is_some() || !self.can_close() {
                self.updates.plan_ready = Some(plan);
                self.update_status(&self.t("update_ready"), true);
                return;
            }
            match sonitrelix_updater::launch_helper(&plan) {
                Ok(()) => {
                    self.update_status(&self.t("update_restarting"), false);
                    self.dirty = false;
                    self.close_audio();
                }
                Err(e) => {
                    self.updates.plan_ready = Some(plan);
                    self.log_error("launch updater", &e);
                    self.update_status(&format!("{} {e}", self.t("update_install_failed")), true);
                }
            }
        }
    }
    unsafe fn poll_install(&mut self) {
        unsafe {
            if let Some(result) = self.updates.state_job.as_ref().map(|r| r.try_recv()) {
                let result = match result {
                    Ok(r) => Some(r),
                    Err(TryRecvError::Disconnected) => Some(Err(self.t("update_action_failed"))),
                    Err(TryRecvError::Empty) => None,
                };
                if let Some(result) = result {
                    self.updates.state_job = None;
                    let text = match result {
                        Ok(tag) => {
                            self.updates.skipped = Some(tag);
                            self.strings["update_skipped_here"] = true.into();
                            self.t("update_skipped")
                        }
                        Err(e) => {
                            self.log_error("save update preference", &e);
                            format!("{} {e}", self.t("update_skip_failed"))
                        }
                    };
                    self.refresh_update_actions();
                    self.update_status(&text, true);
                }
            }
            let result = self.updates.install_job.as_ref().map(|r| r.try_recv());
            let result = match result {
                Some(Ok(r)) => Some(r),
                Some(Err(TryRecvError::Disconnected)) => Some(Err(self.t("update_action_failed"))),
                _ => None,
            };
            if let Some(result) = result {
                self.updates.install_job = None;
                self.refresh_update_actions();
                let cancelled = self
                    .updates
                    .cancel
                    .load(std::sync::atomic::Ordering::Relaxed);
                match result {
                    _ if cancelled => {
                        if !self.playback.closing {
                            self.update_status(&self.t("update_cancelled"), true);
                        }
                    }
                    Ok(plan) => self.finish_update(plan),
                    Err(e) => {
                        if !self.playback.closing {
                            self.log_error("download or stage update", &e);
                            let text = format!("{} {e}", self.t("update_install_failed"));
                            self.update_status(&text, true);
                        }
                    }
                }
            } else if self.updates.install_job.is_some()
                && !self
                    .updates
                    .cancel
                    .load(std::sync::atomic::Ordering::Relaxed)
            {
                let p = self
                    .updates
                    .progress
                    .load(std::sync::atomic::Ordering::Relaxed)
                    / 25
                    * 25;
                if p > self.updates.milestone {
                    self.updates.milestone = p;
                    let text = format!("{} {p}%", self.t("update_downloading_text"));
                    self.update_status(&text, false);
                }
            }
        }
    }
    pub(super) unsafe fn check_updates(&mut self) {
        unsafe {
            if let Some((_, quiet)) = &mut self.updates.ready
                && *quiet
            {
                *quiet = false;
                return;
            }
            self.start_check(false);
        }
    }
    unsafe fn start_check(&mut self, quiet: bool) {
        unsafe {
            if self.updates.receiver.is_some() || self.updates.pending() {
                if !quiet {
                    self.updates.quiet = false;
                    self.status(&self.t("update_checking"));
                }
                return;
            }
            let (tx, rx) = mpsc::channel();
            let root = self.data.parent().unwrap().to_path_buf();
            self.updates.quiet = quiet;
            match std::thread::Builder::new()
                .name("github-updates".into())
                .spawn(move || {
                    let report = check_signed(&root).unwrap_or_else(Report::Error);
                    let _ = tx.send(report);
                }) {
                Ok(_) => {
                    self.updates.receiver = Some(rx);
                    if quiet {
                        self.log("automatic_update_check_started");
                    } else {
                        self.status(&self.t("update_checking"));
                    }
                }
                Err(e) if quiet => self.log_error("start automatic update check", e),
                Err(e) => self.error(e),
            }
        }
    }
    pub(super) unsafe fn poll_updates(&mut self) {
        unsafe {
            self.poll_install();
            if self.playback.closing {
                return;
            }
            self.poll_scheduled();
            if self.updates.pending() {
                return;
            }
            let result = self.updates.receiver.as_ref().map(|r| r.try_recv());
            let report = match result {
                Some(Ok(r)) => Some(r),
                Some(Err(TryRecvError::Disconnected)) => {
                    Some(Report::Error(self.t("update_error")))
                }
                _ => None,
            };
            if let Some(report) = report {
                self.updates.receiver = None;
                if let Report::Error(detail) = &report {
                    self.log_error("check for updates", detail);
                }
                self.updates.ready =
                    if !self.updates.quiet || matches!(report, Report::Available { .. }) {
                        Some((report, self.updates.quiet))
                    } else {
                        None
                    };
            }
            let Some((report, quiet)) = &self.updates.ready else {
                return;
            };
            if *quiet
                && (!self.store.settings.general.automatic_updates
                    || matches!(report, Report::Available { tag, .. } if self.updates.skipped.as_ref() == Some(tag)))
            {
                self.updates.ready = None;
                return;
            }
            if self.recording.is_some()
                || (*quiet
                    && (self.preferences.is_some()
                        || !matches!(
                            self.playback.state,
                            sonitrelix_engine::State::Stopped | sonitrelix_engine::State::Ended
                        )))
            {
                return;
            }
            let (report, _) = self.updates.ready.take().unwrap();
            self.updates.report = Some(report);
            install_text(&mut self.strings, self.updates.report.as_ref());
            if let Some(h) = self.help.as_mut().filter(|h| IsWindow(h.hwnd) != 0) {
                h.update(self.strings.clone());
                h.show(help::Document::Updates);
            } else {
                self.help = None;
                match help::Viewer::open(self.hwnd, self.strings.clone(), help::Document::Updates) {
                    Ok(h) => self.help = Some(h),
                    Err(e) => self.error(e),
                }
            }
        }
    }
}
impl App {
    pub(super) unsafe fn start_update_schedule(&mut self) {
        unsafe {
            self.updates.skipped = fs::read(self.data.join("update-skipped.json"))
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok());
            self.updates.portable = self.data.parent().is_some_and(sonitrelix_updater::portable);
            self.poll_scheduled();
        }
    }
    unsafe fn poll_scheduled(&mut self) {
        unsafe {
            if self.store.settings.general.automatic_updates
                && self.updates.portable
                && self.updates.claim_automatic_check(Instant::now())
            {
                self.start_check(true);
            }
        }
    }
}
impl Checker {
    fn claim_automatic_check(&mut self, now: Instant) -> bool {
        if self.next_check.is_some_and(|next| now < next)
            || self.receiver.is_some()
            || self.pending()
        {
            return false;
        }
        self.next_check = Some(now + CHECK_INTERVAL);
        true
    }
    pub fn pending(&self) -> bool {
        self.install_job.is_some() || self.state_job.is_some()
    }
    pub fn cancel(&self) {
        self.cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}
impl App {
    pub(super) unsafe fn skip_update(&mut self) {
        unsafe {
            let Some(Report::Available { tag, .. }) = &self.updates.report else {
                return;
            };
            if self.updates.pending() || self.strings["update_skipped_here"].as_bool() == Some(true)
            {
                return;
            }
            let tag = tag.clone();
            let path = self.data.join("update-skipped.json");
            let (tx, rx) = mpsc::channel();
            self.updates.state_job = Some(rx);
            if let Err(e) = std::thread::Builder::new()
                .name("update-preferences".into())
                .spawn(move || {
                    let r =
                        sonitrelix_session::atomic_write(&path, &serde_json::to_vec(&tag).unwrap())
                            .map(|()| tag)
                            .map_err(|e| e.to_string());
                    let _ = tx.send(r);
                })
            {
                self.updates.state_job = None;
                self.log_error("start saving update preference", &e);
                self.update_status(&format!("{} {e}", self.t("update_skip_failed")), true);
                return;
            }
            self.refresh_update_actions();
            self.update_status(&self.t("update_saving_skip_text"), true);
        }
    }
}
fn check_signed(root: &std::path::Path) -> Result<Report, String> {
    let mut report = check()?;
    if let Report::Available {
        tag,
        envelope,
        installable,
        warning,
        ..
    } = &mut report
    {
        let checked = (|| -> std::io::Result<Vec<u8>> {
            let bytes = sonitrelix_updater::http::fetch(
                &format!(
                    "https://github.com/AnimalMetal/sonitrelix/releases/download/{tag}/update.json"
                ),
                256 * 1024,
                &std::sync::atomic::AtomicBool::new(false),
            )?;
            let m = sonitrelix_updater::manifest(&bytes)?;
            if format!("v{}", m.version) != *tag {
                return Err(std::io::Error::other(
                    "Signed manifest does not match release",
                ));
            }
            if !sonitrelix_updater::eligible(
                &m,
                env!("CARGO_PKG_VERSION"),
                &sonitrelix_updater::confirmed(root, env!("CARGO_PKG_VERSION")),
            )? {
                return Err(std::io::Error::other("Update is not newer"));
            }
            Ok(bytes)
        })();
        match checked {
            Ok(bytes) => {
                *envelope = String::from_utf8(bytes).ok();
                *installable = sonitrelix_updater::portable(root);
            }
            Err(e) => *warning = format!("Automatic installation unavailable: {e}"),
        }
    }
    Ok(report)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn automatic_update_checks_start_now_and_repeat_after_24_hours() {
        let mut checker = Checker::default();
        let startup = Instant::now();
        assert!(checker.claim_automatic_check(startup));
        assert!(!checker.claim_automatic_check(startup));
        assert!(!checker.claim_automatic_check(startup + CHECK_INTERVAL - Duration::from_secs(1)));
        let (_send, receive) = mpsc::channel();
        checker.receiver = Some(receive);
        let daily = startup + CHECK_INTERVAL;
        assert!(!checker.claim_automatic_check(daily));
        checker.receiver = None;
        assert!(checker.claim_automatic_check(daily));
        assert!(!checker.claim_automatic_check(daily));
        assert!(checker.claim_automatic_check(daily + CHECK_INTERVAL));
        // A new session checks even if the previous session checked immediately beforehand.
        assert!(Checker::default().claim_automatic_check(startup + Duration::from_secs(1)));
    }
    #[test]
    fn versions_and_release_boundaries() {
        assert!(Version::parse("v0.10.0").unwrap() > Version::parse("0.9.9").unwrap());
        for s in ["1.2", "1.2.3-beta", "1.02.3", "1.2.3/evil"] {
            assert!(Version::parse(s).is_err());
        }
        assert!(matches!(
            parse(404, b"", "0.4.0").unwrap(),
            Report::NoRelease
        ));
        assert!(parse(429, b"{}", "0.4.0").is_err());
        let mut v = serde_json::json!({"tag_name":"v0.5.0","draft":false,"prerelease":false,"body":"Notes","assets":[{"name":"Sonitrelix-windows-x64.zip","state":"uploaded","size":123,"browser_download_url":"https://github.com/AnimalMetal/sonitrelix/releases/download/v0.5.0/Sonitrelix-windows-x64.zip"}]});
        assert!(matches!(
            parse(200, &serde_json::to_vec(&v).unwrap(), "0.4.0").unwrap(),
            Report::Available { .. }
        ));
        assert!(matches!(
            parse(200, &serde_json::to_vec(&v).unwrap(), "0.5.0").unwrap(),
            Report::Current
        ));
        v["assets"][0]["browser_download_url"] = "https://evil.invalid/x".into();
        assert!(parse(200, &serde_json::to_vec(&v).unwrap(), "0.4.0").is_err());
        assert!(parse(200, &vec![0; LIMIT + 1], "0.4.0").is_err());
    }
}
