use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, Ordering},
    mpsc::{self, Receiver, TryRecvError},
};
pub(super) const EXPORT: usize = 107;
pub(super) const EXPORT_SELECTION: usize = 109;
#[derive(Default)]
pub(super) struct Exporter {
    job: Option<Receiver<Result<sonitrelix_mixer::Exported, String>>>,
    cancel: Arc<AtomicBool>,
    progress: Arc<AtomicU32>,
    milestone: u32,
}
impl Exporter {
    pub fn pending(&self) -> bool {
        self.job.is_some()
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
impl App {
    pub(super) unsafe fn export_audio(&mut self, selection_only: bool) {
        unsafe {
            if self.exporter.pending() {
                self.status(&self.t("export_busy"));
                return;
            }
            let selection = if selection_only {
                let range = self.selected_track().and_then(|track| {
                    let (start, end) = self.selected_range(track);
                    (start < end).then_some(sonitrelix_mixer::ExportRange { track, start, end })
                });
                if range.is_none() {
                    let message = self.t("export_selection_empty");
                    self.status(&message);
                    announcement::announce(GetDlgItem(self.hwnd, 21), &message);
                    return;
                }
                range
            } else {
                None
            };
            let Some(path) = self.choose_file_with_title(
                true,
                true,
                selection_only.then_some("export_selection"),
            ) else {
                return;
            };
            let project = self.project.clone();
            let options = sonitrelix_mixer::Options {
                sample_rate: self.store.settings.export.sample_rate,
                channels: self.store.settings.export.channels,
                mp3_bitrate: self.store.settings.export.mp3_bitrate,
            };
            self.exporter.cancel = Arc::new(AtomicBool::new(false));
            self.exporter.progress = Arc::new(AtomicU32::new(0));
            self.exporter.milestone = 0;
            let cancel = self.exporter.cancel.clone();
            let progress = self.exporter.progress.clone();
            let (tx, rx) = mpsc::channel();
            match std::thread::Builder::new()
                .name("audio-export".into())
                .spawn(move || {
                    let r = if let Some(range) = selection {
                        sonitrelix_mixer::export_selection(
                            &project, &path, options, range, &cancel, &progress,
                        )
                    } else {
                        sonitrelix_mixer::export_with(&project, &path, options, &cancel, &progress)
                    }
                    .map_err(|e| e.to_string());
                    let _ = tx.send(r);
                }) {
                Ok(_) => {
                    self.exporter.job = Some(rx);
                    self.status(&self.t("export_working"));
                    SetFocus(GetDlgItem(self.hwnd, 21));
                }
                Err(e) => self.error(e),
            }
        }
    }
    pub(super) unsafe fn poll_export(&mut self) {
        unsafe {
            if let Some(result) = self.exporter.job.as_ref().map(|r| r.try_recv()) {
                match result {
                    Ok(result) => {
                        self.exporter.job = None;
                        match result {
                            Ok(out) => {
                                self.status(&format!(
                                    "{} {}. {} {}",
                                    self.t("export_saved"),
                                    out.path.display(),
                                    self.t("export_clipped"),
                                    out.clipped
                                ));
                                self.log("audio_exported");
                            }
                            Err(e) => {
                                if !self.exporter.cancel.load(Ordering::Relaxed) {
                                    self.log_error("export audio", &e);
                                }
                                self.status(&format!("{}: {e}", self.t("export_failed")));
                            }
                        }
                    }
                    Err(TryRecvError::Disconnected) => {
                        self.exporter.job = None;
                        self.status(&self.t("export_failed"));
                        self.log_error("export audio", "Worker disconnected");
                    }
                    Err(TryRecvError::Empty) => {
                        let p = self.exporter.progress.load(Ordering::Relaxed) / 25 * 25;
                        if p > self.exporter.milestone {
                            self.exporter.milestone = p;
                            self.status(&format!("{} {p}%", self.t("export_working")));
                        }
                    }
                }
            }
        }
    }
}
