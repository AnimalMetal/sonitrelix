//! Portable recent projects and background recovery snapshots; never writes source audio.
use super::*;
use std::{
    sync::mpsc::{self, Receiver, TryRecvError},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
pub(super) const RECENT: usize = 108;
pub(super) const FIRST: usize = 1200;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn background_checkpoint_reopens_and_recent_queue_keeps_latest() {
        let temp = std::env::temp_dir();
        let dir = temp.join(format!("sonitrelix-recovery-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let mut m = Maintenance::new(&dir);
        let mut p = Project::default();
        p.add_track("Recovered vocals".into());
        m.last = Instant::now() - Duration::from_secs(31);
        assert!(m.poll(&p, true, false).is_none());
        let deadline = Instant::now() + Duration::from_secs(5);
        while m.pending() {
            assert!(Instant::now() < deadline);
            m.poll(&p, true, false);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(Project::load(&m.checkpoint).unwrap(), p);
        for name in ["a.sonitrelix", "b.sonitrelix", "c.sonitrelix"] {
            m.remember(dir.join(name));
            m.poll(&p, false, false);
        }
        while m.pending() || m.recent_dirty {
            assert!(Instant::now() < deadline);
            m.poll(&p, false, false);
            std::thread::sleep(Duration::from_millis(5));
        }
        let read = Maintenance::new(&dir);
        assert_eq!(read.recent[0], dir.join("c.sonitrelix"));
        m.clear();
        assert!(!m.checkpoint.exists());
        assert!(dir.starts_with(temp));
        fs::remove_dir_all(dir).unwrap();
    }
}
pub(super) struct Maintenance {
    pub recent: Vec<PathBuf>,
    directory: PathBuf,
    checkpoint: PathBuf,
    job: Option<Receiver<Result<(), String>>>,
    last: Instant,
    discard: bool,
    recent_dirty: bool,
    started: Instant,
    health_done: bool,
}
impl Maintenance {
    pub fn new(data: &std::path::Path) -> Self {
        let recent = fs::read(data.join("recent-projects.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Vec<PathBuf>>(&b).ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|p| p.is_absolute())
            .take(10)
            .collect();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let directory = data.join("recovery");
        Self {
            recent,
            checkpoint: directory.join(format!(
                "recovery-{}-{stamp}.sonitrelix",
                std::process::id()
            )),
            directory,
            job: None,
            last: Instant::now(),
            discard: false,
            recent_dirty: false,
            started: Instant::now(),
            health_done: false,
        }
    }
    pub fn pending(&self) -> bool {
        self.job.is_some()
    }
    pub fn clear(&mut self) {
        self.discard = true;
        if self.job.is_none() {
            let _ = fs::remove_file(&self.checkpoint);
            self.discard = false;
        }
    }
    pub fn remember(&mut self, path: PathBuf) {
        if !path.is_absolute() {
            return;
        }
        self.recent.retain(|p| p != &path);
        self.recent.insert(0, path);
        self.recent.truncate(10);
        self.recent_dirty = true;
    }
    fn poll(&mut self, project: &Project, dirty: bool, closing: bool) -> Option<String> {
        let mut error = None;
        if let Some(result) = self.job.as_ref().map(|r| r.try_recv()) {
            match result {
                Ok(r) => {
                    self.job = None;
                    error = r.err();
                }
                Err(TryRecvError::Disconnected) => {
                    self.job = None;
                    error = Some("Recovery worker stopped".into());
                }
                Err(TryRecvError::Empty) => (),
            }
        }
        if self.discard && self.job.is_none() {
            let _ = fs::remove_file(&self.checkpoint);
            self.discard = false;
        }
        if !closing
            && !self.health_done
            && self.started.elapsed() >= Duration::from_secs(5)
            && self.job.is_none()
        {
            self.health_done = true;
            let root = self
                .directory
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .to_path_buf();
            let args: Vec<_> = std::env::args_os().collect();
            let nonce = if args.len() == 3 && args[1] == "--post-update" {
                args[2].to_str().map(str::to_owned)
            } else {
                None
            };
            let (tx, rx) = mpsc::channel();
            self.job = Some(rx);
            if let Err(e) = std::thread::Builder::new()
                .name("startup-confirmation".into())
                .spawn(move || {
                    let r = if let Some(nonce) = nonce {
                        sonitrelix_updater::confirm_start(&root, &nonce)
                    } else {
                        sonitrelix_updater::confirm_running(&root)
                    }
                    .map_err(|e| e.to_string());
                    let _ = tx.send(r);
                })
            {
                self.job = None;
                error = Some(e.to_string());
            }
        }
        if self.job.is_none() && self.recent_dirty {
            self.recent_dirty = false;
            let list = self.recent.clone();
            let path = self
                .directory
                .parent()
                .unwrap()
                .join("recent-projects.json");
            let (tx, rx) = mpsc::channel();
            self.job = Some(rx);
            if let Err(e) = std::thread::Builder::new()
                .name("recent-projects".into())
                .spawn(move || {
                    let r = serde_json::to_vec(&list)
                        .map_err(|e| e.to_string())
                        .and_then(|b| {
                            sonitrelix_session::atomic_write(&path, &b).map_err(|e| e.to_string())
                        });
                    let _ = tx.send(r);
                })
            {
                self.job = None;
                error = Some(e.to_string());
            }
        }
        if !closing && dirty && self.job.is_none() && self.last.elapsed() >= Duration::from_secs(30)
        {
            self.last = Instant::now();
            let project = project.clone();
            let path = self.checkpoint.clone();
            let dir = self.directory.clone();
            let (tx, rx) = mpsc::channel();
            self.job = Some(rx);
            if let Err(e) = std::thread::Builder::new()
                .name("recovery-save".into())
                .spawn(move || {
                    let r = fs::create_dir_all(dir)
                        .and_then(|_| project.save(&path))
                        .map_err(|e| e.to_string());
                    let _ = tx.send(r);
                })
            {
                self.job = None;
                error = Some(e.to_string());
            }
        }
        error
    }
    pub fn candidates(&self) -> Vec<PathBuf> {
        let mut files: Vec<_> = fs::read_dir(&self.directory)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|s| s == "sonitrelix") && p != &self.checkpoint)
            .filter(|p| {
                let name = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                let pid = name
                    .strip_prefix("recovery-")
                    .and_then(|s| s.split('-').next())
                    .and_then(|s| s.parse::<u32>().ok());
                pid.is_some_and(|pid| !process_alive(pid))
            })
            .collect();
        files.sort_by_key(|p| fs::metadata(p).and_then(|m| m.modified()).ok());
        files.reverse();
        files
    }
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> HWND;
    fn WaitForSingleObject(h: HWND, time: u32) -> u32;
    fn CloseHandle(h: HWND) -> i32;
}
fn process_alive(pid: u32) -> bool {
    unsafe {
        let h = OpenProcess(0x100000, 0, pid);
        if h.is_null() {
            return std::io::Error::last_os_error().raw_os_error() == Some(5);
        }
        let alive = WaitForSingleObject(h, 0) != 0;
        CloseHandle(h);
        alive
    }
}
impl App {
    pub(super) unsafe fn poll_maintenance(&mut self) {
        unsafe {
            if let Some(error) =
                self.maintenance
                    .poll(&self.project, self.dirty, self.playback.closing)
            {
                self.status(&format!("{}: {error}", self.t("recovery_failed")));
                self.log("recovery_error");
                self.log_error("checkpoint or maintenance worker", error);
            }
        }
    }
    pub(super) unsafe fn offer_recovery(&mut self) {
        unsafe {
            let Some(path) = self.maintenance.candidates().first().cloned() else {
                return;
            };
            let project = match Project::load(&path) {
                Ok(p) => p,
                Err(e) => {
                    self.status(&format!("{}: {e}", self.t("recovery_failed")));
                    self.log_error("open recovery checkpoint", e);
                    return;
                }
            };
            self.modal.set(true);
            let answer = MessageBoxW(
                self.hwnd,
                wide(&self.t("recovery_offer")).as_ptr(),
                wide("Sonitrelix").as_ptr(),
                4 | 0x20,
            );
            self.modal.set(false);
            if answer == 6 {
                self.project = project;
                self.path = None;
                self.editor.bracket_start = None;
                self.dirty = true;
                self.build();
                self.status(&self.t("recovery_restored"));
            }
            let archive = path.with_extension("recovered-backup");
            if !archive.exists() {
                let _ = fs::rename(path, archive);
            }
        }
    }
    pub(super) unsafe fn open_recent(&mut self, index: usize) {
        unsafe {
            let Some(path) = self.maintenance.recent.get(index).cloned() else {
                return;
            };
            if !self.can_close() {
                return;
            }
            match Project::load(&path) {
                Ok(project) => {
                    self.reset_playback();
                    self.cancel_audio_import();
                    self.maintenance.clear();
                    self.maintenance.remember(path.clone());
                    self.project = project;
                    self.path = Some(path);
                    self.dirty = false;
                    self.undo.clear();
                    self.redo.clear();
                    self.position = 0;
                    self.selection = (0, 0);
                    self.selection_anchor = None;
                    self.editor.exact_cursor = None;
                    self.editor.exact_selection = None;
                    self.editor.bracket_start = None;
                    self.build();
                    self.status(&self.t("opened"));
                }
                Err(e) => self.error(e),
            }
        }
    }
}
