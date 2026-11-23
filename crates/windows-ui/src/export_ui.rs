use super::*;
use sonitrelix_session::ExportMetadata;
use sonitrelix_settings::{BITRATES, EXPORT_FORMATS, EXPORT_RATES};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, Ordering},
    mpsc::{self, Receiver, TryRecvError},
};
pub(super) const EXPORT: usize = 107;
pub(super) const EXPORT_SELECTION: usize = 109;

pub(super) struct OptionsDialog {
    pub formats: Vec<&'static str>,
    pub rates: &'static [u32],
    pub metadata: ExportMetadata,
    pub selection_only: bool,
    pub project_rate: u32,
}
pub(super) fn rates_for(format: &str) -> &'static [u32] {
    match format {
        "opus" => &[48_000],
        "mp3" => &[0, 32_000, 44_100, 48_000],
        "m4a" => &[
            0, 8_000, 16_000, 22_050, 32_000, 44_100, 48_000, 88_200, 96_000,
        ],
        _ => EXPORT_RATES,
    }
}
pub(super) fn quality_key(format: &str) -> &'static str {
    match format {
        "wav" | "flac" => "export_quality_lossless",
        "mp3" => "export_quality_mp3",
        "m4a" => "export_quality_m4a",
        "opus" => "export_quality_opus",
        _ => unreachable!(),
    }
}
impl OptionsDialog {
    fn effective_rate(&self, format: &str) -> u32 {
        let allowed = rates_for(format);
        if allowed.contains(&self.project_rate)
            || (format == "wav" || format == "flac") && self.project_rate <= 192_000
        {
            self.project_rate
        } else {
            48_000
        }
    }
    pub(super) fn rate_label(
        &self,
        strings: &serde_json::Value,
        format: &str,
        rate: u32,
    ) -> String {
        if rate != 0 {
            return format!("{rate} Hz");
        }
        strings["export_project_rate"]
            .as_str()
            .unwrap_or("{rate}")
            .replace("{rate}", &self.effective_rate(format).to_string())
    }
}

pub(super) unsafe fn edit_metadata(
    owner: HWND,
    strings: serde_json::Value,
    mut tags: ExportMetadata,
    selection_only: bool,
) -> Result<Option<ExportMetadata>, String> {
    unsafe {
        let t = |key: &str| strings[key].as_str().unwrap_or(key).to_owned();
        let mut hint = t(if selection_only {
            "export_metadata_selection_hint"
        } else {
            "export_metadata_project_hint"
        });
        loop {
            let fields = [
                ("export_tag_title", &tags.title),
                ("export_tag_artist", &tags.artist),
                ("export_tag_album", &tags.album),
                ("export_tag_track", &tags.track),
                ("export_tag_year", &tags.year),
                ("export_tag_genre", &tags.genre),
                ("export_tag_comment", &tags.comment),
            ]
            .into_iter()
            .map(|(key, value)| forms::Field::Text {
                label: t(key),
                value: value.clone(),
            })
            .collect();
            let Some(values) = forms::ask(
                owner,
                strings.clone(),
                &t("export_metadata_title"),
                fields,
                &hint,
            )?
            else {
                return Ok(None);
            };
            let [title, artist, album, track, year, genre, comment]: [String; 7] =
                values.try_into().expect("seven metadata fields");
            tags = ExportMetadata {
                title,
                artist,
                album,
                track,
                year,
                genre,
                comment,
            };
            if tags.is_valid() {
                return Ok(Some(tags));
            }
            hint = t("export_metadata_invalid");
        }
    }
}
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
            let have_ffmpeg = sonitrelix_audio_io::ffmpeg::installed().is_some();
            let settings = &self.store.settings.export;
            let formats: Vec<_> = EXPORT_FORMATS
                .iter()
                .copied()
                .filter(|f| have_ffmpeg || !["m4a", "opus"].contains(f))
                .collect();
            let selected = formats
                .iter()
                .position(|&format| format == settings.format)
                .unwrap_or(0);
            let initial_format = formats[selected];
            let rates = rates_for(initial_format);
            let draft = OptionsDialog {
                formats,
                rates,
                metadata: self.project.export_metadata.clone(),
                selection_only,
                project_rate: self.project.sample_rate,
            };
            let fields = vec![
                forms::Field::Choice {
                    label: self.t("export_format"),
                    options: draft
                        .formats
                        .iter()
                        .map(|f| match *f {
                            "m4a" => "M4A (AAC)".into(),
                            "opus" => "Opus".into(),
                            _ => f.to_ascii_uppercase(),
                        })
                        .collect(),
                    selected,
                },
                forms::Field::Choice {
                    label: self.t("pref_export_rate"),
                    options: rates
                        .iter()
                        .map(|rate| draft.rate_label(&self.strings, initial_format, *rate))
                        .collect(),
                    selected: rates
                        .iter()
                        .position(|rate| *rate == settings.sample_rate)
                        .unwrap_or(0),
                },
                forms::Field::Choice {
                    label: self.t("pref_export_channels"),
                    options: vec![self.t("pref_mono"), self.t("pref_stereo")],
                    selected: usize::from(settings.channels == 2),
                },
                forms::Field::Choice {
                    label: self.t("pref_export_bitrate"),
                    options: BITRATES.iter().map(|n| format!("{n} kbps")).collect(),
                    selected: BITRATES
                        .iter()
                        .position(|n| *n == settings.mp3_bitrate)
                        .unwrap_or(4),
                },
                forms::Field::Check {
                    label: self.t("export_defaults"),
                    checked: false,
                },
            ];
            let title = self.t(if selection_only {
                "export_options_selection"
            } else {
                "export_options"
            });
            self.modal.set(true);
            let result = forms::ask_export(self.hwnd, self.strings.clone(), &title, fields, draft);
            self.modal.set(false);
            let (values, draft) = match result {
                Ok(Some(values)) => values,
                Ok(None) => return,
                Err(error) => {
                    self.error(error);
                    return;
                }
            };
            let choice = |field: usize| {
                values[field]
                    .parse::<usize>()
                    .expect("validated export form")
            };
            let extension = draft.formats[choice(0)];
            let rate = draft.rates[choice(1)];
            let options = sonitrelix_mixer::Options {
                sample_rate: if rate == 0 {
                    draft.effective_rate(extension)
                } else {
                    rate
                },
                channels: choice(2) as u16 + 1,
                mp3_bitrate: BITRATES[choice(3)],
            };
            if !have_ffmpeg && !draft.metadata.is_empty() {
                self.error(self.t("export_metadata_ffmpeg"));
                return;
            }
            let Some(path) = self.choose_export_path(extension, selection_only) else {
                return;
            };
            if values[4] == "true" {
                let mut next = self.store.settings.clone();
                next.export.format = extension.into();
                next.export.sample_rate = rate;
                next.export.channels = options.channels;
                next.export.mp3_bitrate = options.mp3_bitrate;
                if let Err(error) = self.store.save(next) {
                    self.error_in("save export defaults", error);
                    return;
                }
            }
            if !selection_only && self.project.export_metadata != draft.metadata {
                self.checkpoint();
                self.project.export_metadata = draft.metadata.clone();
                self.refresh_title();
            }
            let mut project = self.project.clone();
            project.export_metadata = draft.metadata;
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
