use super::*;
use sonitrelix_effects::fastplugs::{Effect, Plugin};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

pub(super) const OPEN: usize = 851;
const BUNDLED: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/FastPlugs.clap"));

pub(super) struct Scan {
    cancel: Arc<AtomicBool>,
    result: mpsc::Receiver<std::io::Result<(PathBuf, Vec<Plugin>)>>,
}
impl Drop for Scan {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

fn installed(data: &std::path::Path) -> Option<PathBuf> {
    [
        Some(data.join("plugins/FastPlugs.clap")),
        std::env::var_os("CommonProgramFiles")
            .map(|root| PathBuf::from(root).join("CLAP/FastPlugs.clap")),
        std::env::var_os("LOCALAPPDATA")
            .map(|root| PathBuf::from(root).join("Programs/Common/CLAP/FastPlugs.clap")),
    ]
    .into_iter()
    .flatten()
    .find(|path| path.is_file())
}

fn bundled_plugin(directory: &std::path::Path) -> std::io::Result<PathBuf> {
    let path = directory.join(concat!("FastPlugs-", env!("CARGO_PKG_VERSION"), ".clap"));
    if !std::fs::read(&path).is_ok_and(|bytes| bytes == BUNDLED) {
        std::fs::create_dir_all(directory)?;
        sonitrelix_session::atomic_write(&path, BUNDLED)?;
    }
    Ok(path)
}

impl App {
    pub(super) unsafe fn open_fastplugs(&mut self) {
        unsafe {
            if self.fastplugs_scan.is_some() {
                self.timeline_feedback(&self.t("fx_fastplugs_scanning"));
                return;
            }
            let bundle = if BUNDLED.is_empty() {
                self.fastplugs_bundle
                    .clone()
                    .filter(|p| p.is_file())
                    .or_else(|| installed(&self.data))
                    .or_else(|| {
                        self.choose_file_with_title(false, false, Some("fx_fastplugs_open"))
                    })
            } else {
                None
            };
            if BUNDLED.is_empty() && bundle.is_none() {
                return;
            }
            let directory = self.data.join("effects");
            let (send, result) = mpsc::channel();
            let cancel = Arc::new(AtomicBool::new(false));
            let worker_cancel = cancel.clone();
            match std::thread::Builder::new()
                .name("fastplugs-scan".into())
                .spawn(move || {
                    let result = bundle
                        .map_or_else(|| bundled_plugin(&directory), Ok)
                        .and_then(|bundle| {
                            crate::fastplugs_helper::scan(
                                &directory,
                                bundle.clone(),
                                &worker_cancel,
                            )
                            .map(|effects| (bundle, effects))
                        });
                    let _ = send.send(result);
                }) {
                Ok(_) => {
                    self.fastplugs_scan = Some(Scan { cancel, result });
                    self.timeline_feedback(&self.t("fx_fastplugs_scanning"));
                }
                Err(error) => self.error_in("FastPlugs scan", error),
            }
        }
    }

    pub(super) unsafe fn poll_fastplugs(&mut self) {
        unsafe {
            if self.preferences.is_some() || self.timeline_job.is_some() {
                return;
            }
            let Some(scan) = &self.fastplugs_scan else {
                return;
            };
            let result = match scan.result.try_recv() {
                Ok(result) => result,
                Err(mpsc::TryRecvError::Empty) => return,
                Err(mpsc::TryRecvError::Disconnected) => {
                    Err(std::io::Error::other("FastPlugs scan worker stopped"))
                }
            };
            self.fastplugs_scan = None;
            let (bundle, mut plugins) = match result {
                Ok(scan) => scan,
                Err(error) => {
                    self.fastplugs_bundle = None;
                    self.error_in(
                        "FastPlugs scan",
                        format!("{} {error}", self.t("fx_fastplugs_scan_failed")),
                    );
                    return;
                }
            };
            self.fastplugs_bundle = Some(bundle.clone());
            plugins.sort_by_key(|plugin| plugin.name.to_lowercase());
            self.modal.set(true);
            let choice = forms::ask(
                self.hwnd,
                self.strings.clone(),
                "FastPlugs",
                vec![forms::Field::Choice {
                    label: self.t("fx_choose"),
                    options: plugins.iter().map(|plugin| plugin.name.clone()).collect(),
                    selected: 0,
                }],
                &self.t("fx_fastplugs_choose_hint"),
            );
            self.modal.set(false);
            match choice {
                Ok(Some(values)) => {
                    let plugin = plugins.remove(values[0].parse().unwrap());
                    let effect = Effect {
                        bundle,
                        id: plugin.id.clone(),
                        changes: vec![],
                    };
                    self.effects_dialog(Some((plugin, effect)));
                }
                Ok(None) => (),
                Err(error) => self.error(error),
            }
        }
    }
}

pub(super) unsafe fn parameters(
    owner: HWND,
    strings: serde_json::Value,
    plugin: &Plugin,
    effect: &mut Effect,
) -> Result<(), String> {
    unsafe {
        let t = |key: &str| strings[key].as_str().unwrap_or(key).to_owned();
        let mut selected = 0;
        loop {
            let value = |id| {
                effect
                    .changes
                    .iter()
                    .rev()
                    .find(|(key, _)| *key == id)
                    .map(|(_, value)| value.as_str())
            };
            let mut options = vec![t("fx_fastplugs_done"), t("fx_fastplugs_reset")];
            options.extend(
                plugin
                    .parameters
                    .iter()
                    .map(|p| format!("{}: {}", p.name, value(p.id).unwrap_or(&p.current))),
            );
            let Some(fields) = forms::ask(
                owner,
                strings.clone(),
                &plugin.name,
                vec![forms::Field::Choice {
                    label: t("fx_fastplugs_parameter"),
                    options,
                    selected,
                }],
                &t("fx_fastplugs_parameter_hint"),
            )?
            else {
                return Ok(());
            };
            selected = fields[0].parse().unwrap();
            if selected == 0 {
                return Ok(());
            }
            if selected == 1 {
                effect.changes.clear();
                selected = 0;
                continue;
            }
            let parameter = &plugin.parameters[selected - 2];
            let previous = value(parameter.id).unwrap_or(&parameter.current);
            let hint = t("fx_fastplugs_value_hint")
                .replace("{min}", &parameter.min)
                .replace("{max}", &parameter.max);
            if let Some(mut fields) = forms::ask(
                owner,
                strings.clone(),
                &plugin.name,
                vec![forms::Field::Text {
                    label: parameter.name.clone(),
                    value: previous.to_owned(),
                }],
                &hint,
            )? {
                let new_value = fields.remove(0);
                if new_value != previous && !new_value.is_empty() {
                    effect.changes.retain(|(id, _)| *id != parameter.id);
                    effect.changes.push((parameter.id, new_value));
                }
            }
        }
    }
}
