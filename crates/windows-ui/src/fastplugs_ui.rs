use super::*;
use sonitrelix_effects::fastplugs::{Effect, Parameter, ParameterValue, Plugin};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

pub(super) const OPEN: usize = 851;
const BUNDLED: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/FastPlugs.clap"));
const SETTINGS_PAGE_SIZE: usize = 4;

fn translate<'a>(strings: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    static SPANISH: std::sync::LazyLock<serde_json::Value> = std::sync::LazyLock::new(|| {
        serde_json::from_str(include_str!("../../../resources/fastplugs.es.json"))
            .expect("Spanish FastPlugs labels")
    });
    if strings["help_document_language"].as_str() == Some("es") {
        SPANISH[key].as_str()
    } else {
        None
    }
}

pub(super) fn effect_name<'a>(strings: &'a serde_json::Value, plugin: &'a Plugin) -> &'a str {
    translate(strings, &plugin.name).unwrap_or(&plugin.name)
}

#[derive(Clone)]
pub(super) enum BundleSource {
    Embedded,
    Installed(PathBuf),
}

impl BundleSource {
    pub(super) fn prepare(self, directory: &std::path::Path) -> std::io::Result<PathBuf> {
        match self {
            Self::Embedded => bundled_plugin(directory),
            Self::Installed(path) => Ok(path),
        }
    }
}

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
    pub(super) fn fastplugs_source(&self) -> Option<BundleSource> {
        if BUNDLED.is_empty() {
            self.fastplugs_bundle
                .clone()
                .filter(|p| p.is_file())
                .or_else(|| installed(&self.data))
                .map(BundleSource::Installed)
        } else {
            Some(BundleSource::Embedded)
        }
    }

    pub(super) unsafe fn open_fastplugs(&mut self) {
        unsafe {
            if self.fastplugs_scan.is_some() {
                self.timeline_feedback(&self.t("fx_fastplugs_scanning"));
                return;
            }
            let Some(bundle) = self.fastplugs_source().or_else(|| {
                self.choose_file_with_title(false, false, Some("fx_fastplugs_open"))
                    .map(BundleSource::Installed)
            }) else {
                return;
            };
            let directory = self.data.join("effects");
            let (send, result) = mpsc::channel();
            let cancel = Arc::new(AtomicBool::new(false));
            let worker_cancel = cancel.clone();
            match std::thread::Builder::new()
                .name("fastplugs-scan".into())
                .spawn(move || {
                    let result = bundle.prepare(&directory).and_then(|bundle| {
                        crate::fastplugs_helper::scan(&directory, bundle.clone(), &worker_cancel)
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
            plugins.sort_by_key(|plugin| effect_name(&self.strings, plugin).to_lowercase());
            self.modal.set(true);
            let choice = forms::ask(
                self.hwnd,
                self.strings.clone(),
                "FastPlugs",
                vec![forms::Field::Choice {
                    label: self.t("fx_choose"),
                    options: plugins
                        .iter()
                        .map(|plugin| effect_name(&self.strings, plugin).to_owned())
                        .collect(),
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

fn short_name(parameter: &Parameter) -> &str {
    parameter
        .name
        .rsplit_once(", ")
        .map_or(&parameter.name, |(_, name)| name)
}

fn parameter_label(strings: &serde_json::Value, parameter: &Parameter) -> String {
    let name = short_name(parameter);
    let mut label = translate(strings, name).unwrap_or(name).to_owned();
    if parameter.name.starts_with("Bands, ") {
        label = format!("{} {name}", translate(strings, "Band").unwrap_or("Band"));
    }
    let unit = if !parameter.choices.is_empty() {
        ""
    } else if [
        "Bass",
        "Treble",
        "Suppression",
        "Threshold",
        "Range",
        "Ceiling",
        "Target",
        "Maximum boost",
        "Makeup gain",
        "Left level",
        "Right level",
        "Reflections level",
        "Tail level",
    ]
    .contains(&name)
        || name.ends_with(" gain")
        || parameter.name.starts_with("Bands, ")
    {
        "dB"
    } else if [
        "Delay",
        "Pre-delay",
        "Attack",
        "Release",
        "Hold",
        "Lookahead",
        "Reflections delay",
        "Tail delay",
    ]
    .contains(&name)
    {
        "ms"
    } else if ["Low cut", "High cut", "Crossover"].contains(&name) || name.ends_with(" frequency") {
        "Hz"
    } else if [
        "Mix",
        "Feedback",
        "Size",
        "Damping",
        "Width",
        "Density",
        "Diffusion",
        "Amount",
        "Blend",
        "Cone noise",
    ]
    .contains(&name)
    {
        "%"
    } else if name.starts_with("Listener ") {
        "m"
    } else if name == "Decay" {
        "s"
    } else if ["Speaker spread", "Rotation"].contains(&name) {
        translate(strings, "degrees").unwrap_or("degrees")
    } else {
        ""
    };
    if !unit.is_empty() {
        label.push_str(&format!(" ({unit})"));
    }
    if name == "Amount" {
        label.push_str(
            translate(strings, "center amount hint")
                .unwrap_or(", negative extracts, positive removes"),
        );
    }
    label
}

fn display_value(parameter: &Parameter, value: f64) -> f64 {
    let name = short_name(parameter);
    if name.ends_with(" frequency") {
        value.exp2().round()
    } else if name.starts_with("Listener ") {
        value / 10.
    } else {
        value
    }
}

fn raw_value(parameter: &Parameter, value: f64) -> f64 {
    let name = short_name(parameter);
    let value = if name.ends_with(" frequency") {
        value.log2()
    } else if name.starts_with("Listener ") {
        value * 10.
    } else {
        value
    };
    value.clamp(parameter.min, parameter.max)
}

fn numeric_step(parameter: &Parameter) -> f64 {
    match short_name(parameter) {
        "Decay" | "High frequency decay" => 0.01,
        name if name.ends_with(" Q") || name.starts_with("Listener ") => 0.1,
        name if name.ends_with(" frequency") || name == "High cut" => 10.,
        _ => 1.,
    }
}

fn field(strings: &serde_json::Value, parameter: &Parameter, value: f64) -> forms::Field {
    let label = parameter_label(strings, parameter);
    if !parameter.choices.is_empty() {
        return forms::Field::Choice {
            label,
            options: parameter
                .choices
                .iter()
                .map(|choice| translate(strings, choice).unwrap_or(choice).to_owned())
                .collect(),
            selected: (value - parameter.min).round() as usize,
        };
    }
    forms::Field::Slider {
        label,
        value: display_value(parameter, value),
        default: display_value(parameter, parameter.default),
        range: slider::Range::new(
            display_value(parameter, parameter.min),
            display_value(parameter, parameter.max),
            numeric_step(parameter),
        ),
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
        let title = effect_name(&strings, plugin);
        let pages: Vec<_> = plugin.parameters.chunks(SETTINGS_PAGE_SIZE).collect();
        let mut changes = effect.changes.clone();
        let mut selected = 1;
        loop {
            let (page, page_title) = if pages.len() <= 1 {
                let Some(&page) = pages.first() else {
                    return Ok(());
                };
                (page, title.to_owned())
            } else {
                let options = std::iter::once(t("fx_fastplugs_done"))
                    .chain(pages.iter().map(|page| {
                        let first = parameter_label(&strings, &page[0]);
                        let last = parameter_label(&strings, &page[page.len() - 1]);
                        if first == last {
                            first
                        } else {
                            t("fx_fastplugs_group_range")
                                .replace("{first}", &first)
                                .replace("{last}", &last)
                        }
                    }))
                    .chain(std::iter::once(t("fx_fastplugs_reset")))
                    .collect::<Vec<_>>();
                let Some(values) = forms::ask(
                    owner,
                    strings.clone(),
                    title,
                    vec![forms::Field::Choice {
                        label: t("fx_fastplugs_group"),
                        options: options.clone(),
                        selected,
                    }],
                    &t("fx_fastplugs_group_hint"),
                )?
                else {
                    return Ok(());
                };
                selected = values[0].parse().unwrap();
                if selected == 0 {
                    effect.changes = changes;
                    return Ok(());
                }
                if selected == pages.len() + 1 {
                    changes.clear();
                    selected = 1;
                    continue;
                }
                (pages[selected - 1], options[selected].clone())
            };
            let current = page
                .iter()
                .map(|parameter| {
                    changes
                        .iter()
                        .find_map(|(id, setting)| match setting {
                            ParameterValue::Number(value) if *id == parameter.id => Some(*value),
                            _ => None,
                        })
                        .unwrap_or(parameter.value)
                })
                .collect::<Vec<_>>();
            let fields = page
                .iter()
                .zip(&current)
                .map(|(parameter, value)| field(&strings, parameter, *value))
                .collect();
            if let Some(values) = forms::ask(
                owner,
                strings.clone(),
                &page_title,
                fields,
                &t("fx_fastplugs_controls_hint"),
            )? {
                for ((parameter, previous), control) in page.iter().zip(current).zip(values) {
                    let number: f64 = control.parse().unwrap();
                    let value = if parameter.choices.is_empty() {
                        raw_value(parameter, number)
                    } else {
                        parameter.min + number
                    };
                    if (previous - value).abs() > 1e-6 {
                        changes.retain(|(id, _)| *id != parameter.id);
                        changes.push((parameter.id, ParameterValue::Number(value)));
                    }
                }
                if pages.len() == 1 {
                    effect.changes = changes;
                    return Ok(());
                }
            }
            if pages.len() == 1 {
                return Ok(());
            }
        }
    }
}
