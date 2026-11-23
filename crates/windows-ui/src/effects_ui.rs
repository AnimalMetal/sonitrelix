//! One native parameter dialog; disk rendering runs on a cancellable worker.
use super::*;
use sonitrelix_audio_io::{Encoding, Segment, Source, Timeline, WavInfo};
use sonitrelix_effects::{Effect, FilteredEffect, Sidechain};
use sonitrelix_engine::{Engine, PlayRequest};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, Ordering},
    mpsc::{self, Receiver, TryRecvError},
};
pub(super) const EFFECTS: usize = 850;
pub(super) fn effect_timeline(
    track: &sonitrelix_session::Track,
    start: u64,
    end: u64,
) -> std::io::Result<Timeline> {
    Timeline::new(
        track.sample_rate,
        track.channels,
        sonitrelix_editing::pieces(track, start, end)
            .into_iter()
            .map(|clip| Segment {
                source: clip.source.map(|media| Source {
                    path: media.path,
                    info: WavInfo {
                        sample_rate: media.sample_rate,
                        channels: media.channels,
                        bits: media.bits,
                        frames: media.frames,
                        encoding: if media.float {
                            Encoding::Float
                        } else {
                            Encoding::Pcm
                        },
                    },
                }),
                source_start: clip.source_start,
                frames: clip.frames,
            })
            .collect(),
    )
}
const NAMES: [&str; 16] = [
    "fx_amplify",
    "fx_normalize",
    "fx_fade_in",
    "fx_fade_out",
    "fx_reverse",
    "fx_silence",
    "fx_eq",
    "fx_compressor",
    "fx_gate",
    "fx_reverb",
    "fx_noise_reduction",
    "fx_deesser",
    "fx_dehum",
    "fx_tempo",
    "fx_ducking",
    "fx_pitch_shift",
];
const DUCKING: isize = 14;
const PITCH_SHIFT: isize = 15;
const SLIDER_BASE: i32 = 940;
const RESET_BASE: i32 = 950;
#[derive(Clone, Copy)]
struct Parameter {
    label: &'static str,
    value: f64,
    range: slider::Range,
}
const fn parameter(label: &'static str, value: f64, min: f64, max: f64, step: f64) -> Parameter {
    Parameter {
        label,
        value,
        range: slider::Range::new(min, max, step),
    }
}
fn parameters(effect: isize) -> &'static [Parameter] {
    match effect {
        0 => const { &[parameter("fx_gain", 0., -60., 24., 1.)] },
        1 => const { &[parameter("fx_target", -1., -60., 0., 1.)] },
        6 => {
            const {
                &[
                    parameter("fx_low", 0., -24., 24., 1.),
                    parameter("fx_mid", 0., -24., 24., 1.),
                    parameter("fx_high", 0., -24., 24., 1.),
                ]
            }
        }
        7 => {
            const {
                &[
                    parameter("fx_threshold", -18., -80., 0., 1.),
                    parameter("fx_ratio", 4., 1., 20., 0.1),
                    parameter("fx_makeup", 0., 0., 24., 1.),
                ]
            }
        }
        8 => {
            const {
                &[
                    parameter("fx_threshold", -45., -100., 0., 1.),
                    parameter("fx_attack", 5., 0.1, 500., 0.1),
                    parameter("fx_release", 100., 1., 5000., 1.),
                ]
            }
        }
        9 => {
            const {
                &[
                    parameter("fx_wet", 30., 0., 100., 1.),
                    parameter("fx_decay", 1.5, 0.1, 10., 0.1),
                    parameter("fx_tail", 1., 0., 10., 0.1),
                ]
            }
        }
        10 => const { &[parameter("fx_noise_amount", 12., 1., 30., 1.)] },
        11 => const { &[parameter("fx_strength", 35., 0., 100., 1.)] },
        12 => const { &[parameter("fx_hum_hz", 60., 50., 60., 10.)] },
        13 => const { &[parameter("fx_tempo_percent", 100., 50., 200., 1.)] },
        DUCKING => const { &[parameter("fx_strength", 70., 0., 100., 1.)] },
        PITCH_SHIFT => const { &[parameter("fx_pitch_semitones", 0., -24., 24., 0.1)] },
        _ => &[],
    }
}
enum EffectChoice {
    Native(Effect),
    Filtered(FilteredEffect, Option<usize>),
    FastPlugs(sonitrelix_effects::fastplugs::Effect),
    PitchShift(fastplugs_ui::BundleSource, i32),
}
type Rendered = Result<(PathBuf, WavInfo), String>;
struct Context {
    strings: serde_json::Value,
    title: String,
    fastplugs: Option<(
        sonitrelix_effects::fastplugs::Plugin,
        sonitrelix_effects::fastplugs::Effect,
    )>,
    fastplugs_source: Option<fastplugs_ui::BundleSource>,
    timeline: Timeline,
    selection_start: u64,
    voice_tracks: Vec<(usize, sonitrelix_session::Track)>,
    directory: PathBuf,
    data: PathBuf,
    slider_access: slider::AccessibleValues,
    output: String,
    buffer: u32,
    receiver: Option<Receiver<Rendered>>,
    cancel: Arc<AtomicBool>,
    progress: Arc<AtomicU32>,
    applying: bool,
    closing: bool,
    milestone: u32,
    engine: Option<Engine>,
    previews: Vec<PathBuf>,
    result: Option<(PathBuf, WavInfo)>,
}
impl Context {
    fn t(&self, key: &str) -> String {
        self.strings[key].as_str().unwrap_or(key).into()
    }
    unsafe fn status(&self, h: HWND, text: &str) {
        unsafe {
            SetWindowTextW(GetDlgItem(h, 920), wide(text).as_ptr());
        }
    }
    unsafe fn configure(&self, h: HWND) {
        unsafe {
            let i = SendMessageW(GetDlgItem(h, 901), 0x147, 0, 0);
            let fields = if self.fastplugs.is_some() {
                &[]
            } else {
                parameters(i)
            };
            for n in 0..3 {
                let field = fields.get(n);
                let slider_label = GetDlgItem(h, SLIDER_BASE + n as i32 * 2);
                let slider = GetDlgItem(h, SLIDER_BASE + n as i32 * 2 + 1);
                let reset = GetDlgItem(h, RESET_BASE + n as i32);
                for control in [slider_label, slider, reset] {
                    ShowWindow(control, if field.is_some() { 5 } else { 0 });
                }
                if let Some(field) = field {
                    let title = self.t(field.label);
                    self.slider_access.name(
                        reset,
                        &self
                            .t("reset_value_name")
                            .replace("{parameter}", &title.replace('&', "")),
                    );
                    let title = format!(
                        "{} {}",
                        title.replace('&', ""),
                        self.t("numeric_slider_suffix")
                    );
                    SetWindowTextW(slider_label, wide(&title).as_ptr());
                    field
                        .range
                        .configure(slider, field.value, &self.slider_access, &title);
                }
            }
            for id in [916, 917] {
                ShowWindow(GetDlgItem(h, id), if i == DUCKING { 5 } else { 0 });
            }
            ShowWindow(
                GetDlgItem(h, 918),
                if self.fastplugs.is_some() { 5 } else { 0 },
            );
            let hint = match i {
                _ if self.fastplugs.is_some() => "fx_fastplugs_render_hint",
                DUCKING if self.voice_tracks.is_empty() => "fx_duck_no_voice",
                DUCKING => "fx_duck_hint",
                13 => "fx_tempo_hint",
                PITCH_SHIFT => "fx_pitch_hint",
                10 => "fx_noise_hint",
                11 | 12 => "fx_ffmpeg_hint",
                _ => "fx_hint",
            };
            self.status(h, &self.t(hint));
        }
    }
    unsafe fn slider_message(&self, h: HWND, message: u32, w: usize, l: isize) -> bool {
        unsafe {
            if self.fastplugs.is_some() {
                return false;
            }
            let selected = SendMessageW(GetDlgItem(h, 901), 0x147, 0, 0);
            for (n, param) in parameters(selected).iter().enumerate() {
                let slider = GetDlgItem(h, SLIDER_BASE + n as i32 * 2 + 1);
                if message == 0x114 && l == slider as isize {
                    param
                        .range
                        .update_accessible_value(slider, &self.slider_access);
                    return true;
                }
                if message == 0x111 && w == (RESET_BASE + n as i32) as usize {
                    param
                        .range
                        .set_value(slider, param.value, &self.slider_access);
                    let value = param.value.to_string();
                    let message = self
                        .t("reset_value_feedback")
                        .replace("{parameter}", &self.t(param.label).replace('&', ""))
                        .replace("{value}", &value);
                    self.status(h, &message);
                    announcement::announce(GetDlgItem(h, 920), &message);
                    return true;
                }
            }
            false
        }
    }
    unsafe fn effect(&self, h: HWND) -> Result<EffectChoice, String> {
        unsafe {
            if let Some((_, effect)) = &self.fastplugs {
                return Ok(EffectChoice::FastPlugs(effect.clone()));
            }
            let selected = SendMessageW(GetDlgItem(h, 901), 0x147, 0, 0);
            let fields = parameters(selected);
            let read = |index: usize| {
                let control = GetDlgItem(h, SLIDER_BASE + index as i32 * 2 + 1);
                fields[index].range.value(control) as f32
            };
            if selected == PITCH_SHIFT {
                let bundle = self
                    .fastplugs_source
                    .clone()
                    .ok_or_else(|| self.t("fx_pitch_missing"))?;
                return Ok(EffectChoice::PitchShift(
                    bundle,
                    (read(0) * 10.).round() as i32,
                ));
            }
            let filtered = match selected {
                10 => Some(FilteredEffect::NoiseReduction(read(0))),
                11 => Some(FilteredEffect::DeEsser(read(0))),
                12 => Some(FilteredEffect::DeHum(read(0))),
                13 => Some(FilteredEffect::Tempo(read(0))),
                DUCKING => Some(FilteredEffect::Ducking(read(0))),
                _ => None,
            };
            if let Some(filtered) = filtered {
                filtered.validate().map_err(|_| self.t("fx_invalid"))?;
                let voice = if selected == DUCKING {
                    let index = SendMessageW(GetDlgItem(h, 917), 0x147, 0, 0) as usize;
                    if index >= self.voice_tracks.len() {
                        return Err(self.t("fx_duck_no_voice"));
                    }
                    Some(index)
                } else {
                    None
                };
                return Ok(EffectChoice::Filtered(filtered, voice));
            }
            let effect = match selected {
                0 => Effect::Amplify(read(0)),
                1 => Effect::Normalize(read(0)),
                2 => Effect::FadeIn,
                3 => Effect::FadeOut,
                4 => Effect::Reverse,
                5 => Effect::Silence,
                6 => Effect::Eq {
                    low: read(0),
                    mid: read(1),
                    high: read(2),
                },
                7 => Effect::Compressor {
                    threshold: read(0),
                    ratio: read(1),
                    makeup: read(2),
                },
                8 => Effect::Gate {
                    threshold: read(0),
                    attack_ms: read(1),
                    release_ms: read(2),
                },
                9 => Effect::Reverb {
                    wet: read(0),
                    decay: read(1),
                    tail: read(2),
                },
                _ => return Err(self.t("fx_invalid")),
            };
            effect.validate().map_err(|_| self.t("fx_invalid"))?;
            Ok(EffectChoice::Native(effect))
        }
    }
    unsafe fn start(&mut self, h: HWND, apply: bool) {
        unsafe {
            if self.receiver.is_some() {
                return;
            }
            let effect = match self.effect(h) {
                Ok(e) => e,
                Err(e) => {
                    self.status(h, &e);
                    SetFocus(GetDlgItem(h, 920));
                    return;
                }
            };
            if matches!(effect, EffectChoice::Filtered(..))
                && sonitrelix_audio_io::ffmpeg::installed().is_none()
            {
                self.status(h, &self.t("fx_ffmpeg_missing"));
                SetFocus(GetDlgItem(h, 920));
                return;
            }
            let voice = match effect {
                EffectChoice::Filtered(_, Some(index)) => Some(self.voice_tracks[index].1.clone()),
                _ => None,
            };
            let selection_start = self.selection_start;
            if let Some(engine) = &self.engine {
                engine.stop();
            }
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let directory = self.directory.clone();
            let path = self
                .directory
                .join(format!("effect-{stamp}-{}.wav", std::process::id()));
            let mut timeline = self.timeline.clone();
            // Preview the first five seconds of the chosen range; full selection on Apply.
            if !apply {
                timeline = match preview_timeline(&timeline) {
                    Ok(t) => t,
                    Err(e) => {
                        self.status(h, &e.to_string());
                        return;
                    }
                };
            }
            self.cancel.store(false, Ordering::Relaxed);
            self.progress.store(0, Ordering::Relaxed);
            self.milestone = 0;
            let stop = self.cancel.clone();
            let progress = self.progress.clone();
            let (tx, rx) = mpsc::channel();
            match std::thread::Builder::new()
                .name("effect-render".into())
                .spawn(move || {
                    let result = (|| {
                        let info = match effect {
                            EffectChoice::FastPlugs(effect) => crate::fastplugs_helper::render(
                                effect, timeline, &path, &stop, &progress,
                            )?,
                            EffectChoice::PitchShift(bundle, tenths) => {
                                let effect = sonitrelix_effects::fastplugs::Effect {
                                    bundle: bundle.prepare(&directory)?,
                                    id: "io.github.masonasons.fastplugs.pitch".into(),
                                    // FastPlugs takes integer semitones and signed cents.
                                    changes: vec![
                                        (
                                            0,
                                            sonitrelix_effects::fastplugs::ParameterValue::Number(
                                                f64::from(tenths / 10),
                                            ),
                                        ),
                                        (
                                            1,
                                            sonitrelix_effects::fastplugs::ParameterValue::Number(
                                                f64::from(tenths % 10 * 10),
                                            ),
                                        ),
                                    ],
                                };
                                crate::fastplugs_helper::render(
                                    effect, timeline, &path, &stop, &progress,
                                )?
                            }
                            EffectChoice::Native(effect) => sonitrelix_effects::render(
                                timeline, effect, &path, &stop, &progress,
                            )?,
                            EffectChoice::Filtered(effect, _) => {
                                let sidechain = if let Some(track) = voice {
                                    Some(Sidechain {
                                        timeline: Timeline::mix(
                                            timeline.info().sample_rate,
                                            vec![sonitrelix_audio_io::MixTrack {
                                                timeline: sonitrelix_mixer::track(&track)?,
                                                gain: 1.,
                                                pan: 0.,
                                            }],
                                        )?,
                                        start: selection_start,
                                    })
                                } else {
                                    None
                                };
                                sonitrelix_effects::render_filtered(
                                    timeline, effect, sidechain, &path, &stop, &progress,
                                )?
                            }
                        };
                        Ok::<_, std::io::Error>((path, info))
                    })()
                    .map_err(|e| e.to_string());
                    if let Err(mpsc::SendError(Ok((path, _)))) = tx.send(result) {
                        let _ = fs::remove_file(path);
                    }
                }) {
                Ok(_) => {
                    self.receiver = Some(rx);
                    self.applying = apply;
                    for id in [901, 917, 918, 930, 931, 941, 943, 945, 950, 951, 952] {
                        EnableWindow(GetDlgItem(h, id), 0);
                    }
                    self.status(h, &self.t("fx_working"));
                    SetFocus(GetDlgItem(h, 920));
                }
                Err(e) => self.status(h, &e.to_string()),
            }
        }
    }
    unsafe fn poll(&mut self, h: HWND) {
        unsafe {
            if let Some(result) = self.receiver.as_ref().map(|r| r.try_recv()) {
                let result = match result {
                    Ok(r) => Some(r),
                    Err(TryRecvError::Disconnected) => Some(Err(self.t("fx_failed"))),
                    Err(TryRecvError::Empty) => None,
                };
                if let Some(result) = result {
                    self.receiver = None;
                    for id in [901, 917, 918, 930, 931, 941, 943, 945, 950, 951, 952] {
                        EnableWindow(GetDlgItem(h, id), 1);
                    }
                    if self.closing {
                        if let Ok((path, _)) = result {
                            let _ = fs::remove_file(path);
                        }
                        DestroyWindow(h);
                        return;
                    }
                    match result {
                        Ok((path, info)) if self.applying => {
                            self.result = Some((path, info));
                            DestroyWindow(h);
                            return;
                        }
                        Ok((path, info)) => {
                            self.previews.push(path.clone());
                            let start = (|| -> std::io::Result<()> {
                                if self.engine.is_none() {
                                    self.engine = Some(Engine::new()?);
                                }
                                self.engine.as_mut().unwrap().play(PlayRequest {
                                    gate: None,
                                    timeline: Timeline::single(path, info)?,
                                    start_ms: 0,
                                    start_frame: None,
                                    loop_frames: None,
                                    output_id: self.output.clone(),
                                    buffer_frames: self.buffer,
                                    paused: false,
                                })?;
                                Ok(())
                            })();
                            match start {
                                Ok(()) => self.status(h, &self.t("fx_previewing")),
                                Err(e) => {
                                    diagnostics::write(&self.data, "effect preview", &e);
                                    self.status(h, &e.to_string());
                                }
                            }
                        }
                        Err(e) => {
                            if !self.cancel.load(Ordering::Relaxed) {
                                diagnostics::write(&self.data, "effect render", &e);
                            }
                            self.status(h, &format!("{}: {e}", self.t("fx_failed")));
                            SetFocus(GetDlgItem(h, 920));
                        }
                    }
                } else {
                    let p = self.progress.load(Ordering::Relaxed) / 25 * 25;
                    if p > self.milestone {
                        self.milestone = p;
                        self.status(h, &format!("{} {p}%", self.t("fx_working")));
                    }
                }
            }
            if let Some(engine) = &self.engine {
                while let Some(event) = engine.try_event() {
                    let sonitrelix_engine::Event::Error { message, .. } = event;
                    diagnostics::write(&self.data, "effect preview playback", &message);
                    self.status(h, &message);
                }
            }
        }
    }
    unsafe fn close(&mut self, h: HWND) {
        unsafe {
            if let Some(engine) = &self.engine {
                engine.stop();
            }
            if self.receiver.is_some() {
                self.closing = true;
                self.cancel.store(true, Ordering::Relaxed);
                self.status(h, &self.t("fx_cancelling"));
            } else {
                DestroyWindow(h);
            }
        }
    }
}
impl Drop for Context {
    fn drop(&mut self) {
        if let Some(engine) = &self.engine {
            engine.stop();
        }
        for path in &self.previews {
            let _ = fs::remove_file(path);
        }
    }
}
fn preview_timeline(t: &Timeline) -> std::io::Result<Timeline> {
    t.slice(0, t.info().frames.min(u64::from(t.info().sample_rate) * 5))
}
unsafe extern "system" fn proc(h: HWND, m: u32, w: usize, l: isize) -> isize {
    unsafe {
        let p = GetWindowLongPtrW(h, -21) as *mut Context;
        if !p.is_null() {
            if [0x114, 0x111].contains(&m) && (*p).slider_message(h, m, w, l) {
                return 0;
            }
            match m {
                0x111 if w & 0xffff == 901 && w >> 16 == 1 => {
                    (*p).configure(h);
                    return 0;
                }
                0x111 if w >> 16 == 0 => {
                    match w & 0xffff {
                        918 if (*p).receiver.is_none() => {
                            if let Some((plugin, mut effect)) = (*p).fastplugs.clone() {
                                KillTimer(h, 1);
                                let strings = (*p).strings.clone();
                                if let Err(error) = super::fastplugs_ui::parameters(
                                    h,
                                    strings,
                                    &plugin,
                                    &mut effect,
                                ) {
                                    (*p).status(h, &error);
                                } else {
                                    (*p).fastplugs = Some((plugin, effect));
                                }
                                SetTimer(h, 1, 150, 0);
                            }
                        }
                        930 => (*p).start(h, false),
                        931 => (*p).start(h, true),
                        932 => {
                            if let Some(e) = &(*p).engine {
                                e.stop();
                            }
                        }
                        2 => (*p).close(h),
                        _ => (),
                    }
                    return 0;
                }
                0x113 => {
                    (*p).poll(h);
                    return 0;
                }
                0x10 => {
                    (*p).close(h);
                    return 0;
                }
                _ => (),
            }
        }
        DefWindowProcW(h, m, w, l)
    }
}
unsafe fn open(owner: HWND, context: &mut Context) -> Result<HWND, String> {
    unsafe {
        slider::initialize().map_err(|e| e.to_string())?;
        let class = wide("SonitrelixEffects");
        let instance = GetModuleHandleW(null());
        RegisterClassW(&WNDCLASSW {
            style: 0,
            proc: Some(proc),
            cls_extra: 0,
            wnd_extra: 0,
            instance,
            icon: null_mut(),
            cursor: LoadCursorW(null_mut(), 32512usize as *const u16),
            background: 6usize as HWND,
            menu: null(),
            name: class.as_ptr(),
        });
        let dpi = GetDpiForWindow(owner).max(96) as i32;
        let s = |v| v * dpi / 96;
        let h = CreateWindowExW(
            0x10000,
            class.as_ptr(),
            wide(&context.title).as_ptr(),
            0x00c80000,
            i32::MIN,
            i32::MIN,
            s(650),
            s(560),
            owner,
            null_mut(),
            instance,
            null_mut(),
        );
        if h.is_null() {
            return Err(std::io::Error::last_os_error().to_string());
        }
        for (id, class, key, style, x, y, width, height) in [
            (900, "STATIC", "fx_choose", 0, 12, 12, 600, 20),
            (
                901,
                "COMBOBOX",
                "",
                0x10000 | 3 | 0x200000,
                12,
                34,
                600,
                300,
            ),
            (940, "STATIC", "", 0, 12, 75, 600, 22),
            (941, slider::CLASS, "", slider::STYLE, 12, 97, 435, 26),
            (
                950,
                "BUTTON",
                "reset_value_button",
                0x10000,
                471,
                97,
                145,
                26,
            ),
            (942, "STATIC", "", 0, 12, 130, 600, 22),
            (943, slider::CLASS, "", slider::STYLE, 12, 152, 435, 26),
            (
                951,
                "BUTTON",
                "reset_value_button",
                0x10000,
                471,
                152,
                145,
                26,
            ),
            (944, "STATIC", "", 0, 12, 185, 600, 22),
            (945, slider::CLASS, "", slider::STYLE, 12, 207, 435, 26),
            (
                952,
                "BUTTON",
                "reset_value_button",
                0x10000,
                471,
                207,
                145,
                26,
            ),
            (916, "STATIC", "fx_voice_track", 0, 12, 130, 600, 20),
            (
                917,
                "COMBOBOX",
                "",
                0x10000 | 3 | 0x200000,
                12,
                152,
                600,
                240,
            ),
            (
                918,
                "BUTTON",
                "fx_fastplugs_parameters",
                0x10000,
                12,
                80,
                235,
                32,
            ),
            (919, "STATIC", "fx_status", 0, 12, 244, 600, 20),
            (
                920,
                "EDIT",
                "",
                0x10000 | 0x800000 | 0x800 | 4 | 0x40 | 0x200000,
                12,
                267,
                600,
                170,
            ),
            (930, "BUTTON", "fx_preview", 0x10000, 12, 450, 145, 30),
            (932, "BUTTON", "fx_stop", 0x10000, 165, 450, 145, 30),
            (931, "BUTTON", "fx_apply", 0x10000, 318, 450, 145, 30),
            (2, "BUTTON", "fx_cancel", 0x10000, 471, 450, 145, 30),
        ] {
            let c = CreateWindowExW(
                0,
                wide(class).as_ptr(),
                wide(&if key.is_empty() {
                    String::new()
                } else {
                    context.t(key)
                })
                .as_ptr(),
                0x50000000 | style,
                s(x),
                s(y),
                s(width),
                s(height),
                h,
                id as HMENU,
                instance,
                null_mut(),
            );
            if c.is_null() {
                DestroyWindow(h);
                return Err("Cannot create effect control".into());
            }
            SendMessageW(c, 0x30, GetStockObject(17) as usize, 1);
        }
        if let Some((plugin, _)) = &context.fastplugs {
            SendMessageW(
                GetDlgItem(h, 901),
                0x143,
                0,
                wide(fastplugs_ui::effect_name(&context.strings, plugin)).as_ptr() as isize,
            );
        } else {
            for key in NAMES {
                SendMessageW(
                    GetDlgItem(h, 901),
                    0x143,
                    0,
                    wide(&context.t(key)).as_ptr() as isize,
                );
            }
        }
        SendMessageW(GetDlgItem(h, 901), 0x14e, 0, 0);
        let voices = GetDlgItem(h, 917);
        for (index, track) in &context.voice_tracks {
            let label = format!("{} {}: {}", context.t("track_word"), index + 1, track.name);
            SendMessageW(voices, 0x143, 0, wide(&label).as_ptr() as isize);
        }
        if context.voice_tracks.len() == 1 {
            SendMessageW(voices, 0x14e, 0, 0);
        }
        context.configure(h);
        SetWindowLongPtrW(h, -21, context as *mut Context as isize);
        SetTimer(h, 1, 150, 0);
        Ok(h)
    }
}
impl App {
    pub(super) fn effect_range(&self, index: usize) -> Result<(u64, u64, bool), &'static str> {
        if self
            .editor
            .bracket_start
            .is_some_and(|(track, _)| track == index)
        {
            return Err("fx_finish_selection");
        }
        let (start, end) = self.selected_range(index);
        if end > start {
            return Ok((start, end, true));
        }
        // A nonempty selection outside the track must not target the whole track.
        if self.selection.1 > self.selection.0 {
            return Err("fx_selection_no_audio");
        }
        let frames = self.project.tracks[index].frames();
        if frames == 0 {
            return Err("audio_no_wav");
        }
        Ok((0, frames, false))
    }

    pub(super) unsafe fn effects_dialog(
        &mut self,
        fastplugs: Option<(
            sonitrelix_effects::fastplugs::Plugin,
            sonitrelix_effects::fastplugs::Effect,
        )>,
    ) {
        unsafe {
            let Some(index) = self.selected_track() else {
                self.status(&self.t("edit_no_track"));
                return;
            };
            let (a, b, selection) = match self.effect_range(index) {
                Ok(range) => range,
                Err(key) => {
                    let message = self.t(key);
                    self.status(&message);
                    announcement::announce(GetDlgItem(self.hwnd, 21), &message);
                    return;
                }
            };
            let track = &self.project.tracks[index];
            let scope = if selection {
                self.t("fx_scope_selection")
                    .replace(
                        "{start}",
                        &format_time(sonitrelix_audio_io::frames_to_ms(a, track.sample_rate)),
                    )
                    .replace(
                        "{end}",
                        &format_time(editing_ui::ceil_ms(b, track.sample_rate)),
                    )
            } else {
                self.t("fx_scope_track")
            };
            let title = format!("{} — {} — {}", self.t("effects_title"), scope, track.name);
            let timeline = match effect_timeline(track, a, b) {
                Ok(t) => t,
                Err(e) => {
                    self.error(e);
                    return;
                }
            };
            let directory = self.data.join("effects");
            if let Err(e) = fs::create_dir_all(&directory) {
                self.error(e);
                return;
            }
            self.stop_playback();
            self.cancel_audio_import();
            let focus = GetFocus();
            let mut context = Box::new(Context {
                strings: self.strings.clone(),
                title,
                fastplugs,
                fastplugs_source: self.fastplugs_source(),
                timeline,
                selection_start: a,
                voice_tracks: self
                    .project
                    .tracks
                    .iter()
                    .enumerate()
                    .filter(|(i, track)| *i != index && track.frames() > 0)
                    .map(|(i, track)| (i, track.clone()))
                    .collect(),
                directory,
                data: self.data.clone(),
                slider_access: slider::AccessibleValues::new(),
                output: self.store.settings.devices.output_id.clone(),
                buffer: self.store.settings.recording.buffer_frames,
                receiver: None,
                cancel: Arc::new(AtomicBool::new(false)),
                progress: Arc::new(AtomicU32::new(0)),
                applying: false,
                closing: false,
                milestone: 0,
                engine: None,
                previews: vec![],
                result: None,
            });
            let h = match open(self.hwnd, &mut context) {
                Ok(h) => h,
                Err(e) => {
                    self.error(e);
                    return;
                }
            };
            self.modal.set(true);
            EnableWindow(self.hwnd, 0);
            ShowWindow(h, 5);
            SetFocus(GetDlgItem(h, 901));
            let mut msg = MSG::default();
            while IsWindow(h) != 0 {
                let result = GetMessageW(&mut msg, null_mut(), 0, 0);
                if result <= 0 {
                    context.cancel.store(true, Ordering::Relaxed);
                    if result == 0 {
                        PostQuitMessage(msg.wParam as i32);
                    }
                    break;
                }
                if msg.message == 0x100 && msg.wParam == 0x1b {
                    if let Some(open_combo) = [901, 917]
                        .into_iter()
                        .map(|id| GetDlgItem(h, id))
                        .find(|combo| SendMessageW(*combo, 0x157, 0, 0) != 0)
                    {
                        SendMessageW(open_combo, 0x14f, 0, 0);
                        continue;
                    }
                    context.close(h);
                    continue;
                }
                if !tab_from_readonly(h, GetDlgItem(h, 920), &msg)
                    && IsDialogMessageW(h, &mut msg) == 0
                {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
            if IsWindow(h) != 0 {
                DestroyWindow(h);
            }
            EnableWindow(self.hwnd, 1);
            self.modal.set(false);
            SetFocus(focus);
            if let Some((path, info)) = context.result.take() {
                self.commit_effect(index, a, b, path, info);
            }
        }
    }
    pub(super) unsafe fn commit_effect(
        &mut self,
        index: usize,
        a: u64,
        b: u64,
        path: PathBuf,
        info: WavInfo,
    ) -> bool {
        unsafe {
            let media = sonitrelix_session::Media {
                path,
                sample_rate: info.sample_rate,
                channels: info.channels,
                bits: info.bits,
                frames: info.frames,
                float: info.encoding == Encoding::Float,
            };
            let clipboard = sonitrelix_editing::Clipboard {
                sample_rate: info.sample_rate,
                channels: info.channels,
                clips: vec![sonitrelix_session::Clip {
                    source: Some(media),
                    source_start: 0,
                    frames: info.frames,
                }],
            };
            let mut candidate = self.project.clone();
            match sonitrelix_editing::paste(&mut candidate.tracks[index], a, (a, b), &clipboard) {
                Ok((a, b)) => {
                    if let Err(e) = candidate.validate() {
                        self.error(e);
                        return false;
                    }
                    self.checkpoint();
                    self.project = candidate;
                    if self.selected_track() == Some(index) {
                        self.editor.bracket_start = None;
                        self.editor.exact_selection = Some((index, a, b));
                        self.editor.exact_cursor = Some((index, a));
                        self.position = sonitrelix_audio_io::frames_to_ms(a, info.sample_rate);
                        self.selection = (self.position, editing_ui::ceil_ms(b, info.sample_rate));
                    }
                    self.refresh();
                    self.sync_times();
                    self.status(&self.t("fx_applied"));
                    self.log("effect_applied");
                    true
                }
                Err(e) => {
                    self.error(self.t(e));
                    false
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_parameter_sliders_and_background_apply() {
        unsafe {
            let dir = std::env::temp_dir().join(format!("sonitrelix-fx-ui-{}", std::process::id()));
            fs::create_dir_all(&dir).unwrap();
            for es in [false, true] {
                let mut c = Box::new(Context {
                    strings: load_strings(es),
                    title: "Effects".into(),
                    fastplugs: None,
                    fastplugs_source: None,
                    timeline: Timeline::new(
                        48000,
                        1,
                        vec![Segment {
                            source: None,
                            source_start: 0,
                            frames: 1000,
                        }],
                    )
                    .unwrap(),
                    selection_start: 0,
                    voice_tracks: vec![],
                    directory: dir.clone(),
                    data: dir.clone(),
                    slider_access: slider::AccessibleValues::new(),
                    output: String::new(),
                    buffer: 256,
                    receiver: None,
                    cancel: Arc::new(AtomicBool::new(false)),
                    progress: Arc::new(AtomicU32::new(0)),
                    applying: false,
                    closing: false,
                    milestone: 0,
                    engine: None,
                    previews: vec![],
                    result: None,
                });
                let h = open(null_mut(), &mut c).unwrap();
                ShowWindow(h, 5);
                let status = GetDlgItem(h, 920);
                SetFocus(status);
                let tab = MSG {
                    hwnd: status,
                    message: 0x100,
                    wParam: 9,
                    ..MSG::default()
                };
                assert!(tab_from_readonly(h, status, &tab));
                assert_eq!(GetFocus(), GetDlgItem(h, 930));
                ShowWindow(h, 0);
                for i in 0..14 {
                    SendMessageW(GetDlgItem(h, 901), 0x14e, i, 0);
                    c.configure(h);
                    assert!(c.effect(h).is_ok(), "default effect {i}");
                    for (n, field) in parameters(i as isize).iter().enumerate() {
                        let slider = GetDlgItem(h, SLIDER_BASE + n as i32 * 2 + 1);
                        let (name, role, value) =
                            super::super::accessibility_tests::name_role_value(slider);
                        assert!(name.contains(&c.t("numeric_slider_suffix")), "{name}");
                        assert_eq!(role, 0x33);
                        assert_eq!(value, field.value.to_string());
                        let reset = GetDlgItem(h, RESET_BASE + n as i32);
                        let (name, _) = super::super::accessibility_tests::name_and_role(reset);
                        assert_eq!(
                            name,
                            c.t("reset_value_name")
                                .replace("{parameter}", &c.t(field.label).replace('&', ""))
                        );
                    }
                    for id in [901, 920, 930, 931, 932, 2] {
                        let (name, _) =
                            super::super::accessibility_tests::name_and_role(GetDlgItem(h, id));
                        assert!(!name.is_empty(), "{id}");
                    }
                    let mut mnemonics = std::collections::HashSet::new();
                    for id in [900, 940, 942, 944, 919, 930, 931, 932, 2] {
                        let control = GetDlgItem(h, id);
                        // Parent is hidden in tests, so check the control's own visible style.
                        if GetWindowLongPtrW(control, -16) & 0x10000000 == 0 {
                            continue;
                        }
                        let mut b = [0u16; 200];
                        let n = GetWindowTextW(control, b.as_mut_ptr(), 200);
                        let s = String::from_utf16_lossy(&b[..n as usize]);
                        if let Some((_, tail)) = s.split_once('&') {
                            let key = tail.chars().next().unwrap().to_lowercase().to_string();
                            assert!(
                                mnemonics.insert(key),
                                "duplicate mnemonic {s}, effect {i}, es={es}"
                            );
                        }
                    }
                }
                SendMessageW(GetDlgItem(h, 901), 0x14e, DUCKING as usize, 0);
                c.configure(h);
                assert!(c.effect(h).is_err(), "ducking needs a controlling track");
                SendMessageW(GetDlgItem(h, 901), 0x14e, PITCH_SHIFT as usize, 0);
                c.configure(h);
                assert_eq!(c.effect(h).err().unwrap(), c.t("fx_pitch_missing"));
                c.fastplugs_source = Some(fastplugs_ui::BundleSource::Embedded);
                let slider = GetDlgItem(h, 941);
                let (name, role, value) = accessibility_tests::name_role_value(slider);
                assert!(name.contains(&c.t("fx_pitch_semitones")));
                assert_eq!(role, 0x33);
                assert_eq!(value, "0");
                SendMessageW(slider, 0x100, 0x25, 0); // Left arrow.
                assert_eq!(accessibility_tests::name_role_value(slider).2, "-0.1");
                assert!(matches!(c.effect(h), Ok(EffectChoice::PitchShift(_, -1))));
                SendMessageW(GetDlgItem(h, 901), 0x14e, 9, 0);
                c.configure(h);
                let slider = GetDlgItem(h, 941);
                assert!(GetDlgItem(h, 911).is_null());
                SendMessageW(slider, 0x100, 0x27, 0); // Right arrow.
                assert_eq!(accessibility_tests::name_role_value(slider).2, "31");
                let decay = GetDlgItem(h, 943);
                parameters(9)[1]
                    .range
                    .set_value(decay, 2.5, &c.slider_access);
                SendMessageW(
                    h,
                    0x111,
                    RESET_BASE as usize,
                    GetDlgItem(h, RESET_BASE) as isize,
                );
                assert!(c.receiver.is_none());
                assert_ne!(IsWindow(h), 0);
                assert!(matches!(
                    c.effect(h).unwrap(),
                    EffectChoice::Native(Effect::Reverb {
                        wet: 30.,
                        decay: 2.5,
                        ..
                    })
                ));
                SendMessageW(
                    h,
                    0x111,
                    (RESET_BASE + 1) as usize,
                    GetDlgItem(h, RESET_BASE + 1) as isize,
                );
                c.start(h, true);
                assert!(c.receiver.is_some());
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
                while IsWindow(h) != 0 {
                    assert!(std::time::Instant::now() < deadline);
                    c.poll(h);
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                let (path, info) = c.result.take().unwrap();
                assert_eq!(info.frames, 49000);
                assert_eq!(info.encoding, Encoding::Float);
                assert_eq!(
                    sonitrelix_audio_io::WavReader::open(&path).unwrap().info,
                    info
                );
                c.fastplugs = Some((
                    sonitrelix_effects::fastplugs::Plugin {
                        id: "test".into(),
                        name: "FastPlugs test".into(),
                        parameters: vec![],
                    },
                    sonitrelix_effects::fastplugs::Effect {
                        id: "test".into(),
                        bundle: PathBuf::new(),
                        changes: vec![],
                    },
                ));
                let h = open(null_mut(), &mut c).unwrap();
                let button = GetDlgItem(h, 918);
                let (name, role) = accessibility_tests::name_and_role(button);
                assert_eq!(name, c.t("fx_fastplugs_parameters").replace('&', ""));
                assert_eq!(role, 43);
                assert!(matches!(c.effect(h), Ok(EffectChoice::FastPlugs(_))));
                DestroyWindow(h);
            }
            fs::remove_dir_all(dir).unwrap();
        }
    }
}
