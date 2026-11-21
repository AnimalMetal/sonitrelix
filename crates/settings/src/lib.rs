//! Portable preferences with legacy migration, validation and recoverable saves.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sonitrelix_session::atomic_write;
use std::{fs, io, path::PathBuf};

pub const SAMPLE_RATES: &[u32] = &[22050, 44100, 48000, 88200, 96000, 192000];
pub const EXPORT_RATES: &[u32] = &[
    0, 8000, 16000, 22050, 32000, 44100, 48000, 88200, 96000, 192000,
];
pub const BITRATES: &[u32] = &[64, 96, 128, 160, 192, 224, 256, 320];
pub const BUFFER_SIZES: &[u32] = &[128, 256, 512, 1024, 2048];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub schema_version: u32,
    pub general: General,
    pub devices: Devices,
    pub recording: Recording,
    pub editing: Editing,
    pub accessibility: Accessibility,
    pub export: Export,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}
macro_rules! section {
    ($name:ident { $($field:ident: $ty:ty = $default:expr),* $(,)? }) => {
        #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
        pub struct $name {
            $(pub $field: $ty,)*
            #[serde(flatten)] pub extra: serde_json::Map<String, Value>,
        }
        impl Default for $name {
            fn default() -> Self { Self { $($field: $default,)* extra: Default::default() } }
        }
    };
}
section!(General {
    language: String = "en".into(),
    new_project_sample_rate: u32 = 48000,
    automatic_updates: bool = true
});
section!(Devices {
    input_id: String = String::new(),
    output_id: String = String::new()
});
section!(Recording {
    channels: u16 = 1,
    buffer_frames: u32 = 256,
    offset_ms: i32 = 0,
    monitoring: bool = false
});
section!(Editing {
    step_ms: u32 = 10,
    large_step_ms: u32 = 1000
});
section!(Accessibility {
    detailed_time: bool = true,
    focus_status: bool = false
});
section!(Export {
    sample_rate: u32 = 0,
    channels: u16 = 2,
    mp3_bitrate: u32 = 192
});
impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: 1,
            general: Default::default(),
            devices: Default::default(),
            recording: Default::default(),
            editing: Default::default(),
            accessibility: Default::default(),
            export: Default::default(),
            extra: Default::default(),
        }
    }
}
impl Settings {
    pub fn defaults_preserving_unknown(&self) -> Self {
        Self {
            general: General {
                extra: self.general.extra.clone(),
                ..Default::default()
            },
            devices: Devices {
                extra: self.devices.extra.clone(),
                ..Default::default()
            },
            recording: Recording {
                extra: self.recording.extra.clone(),
                ..Default::default()
            },
            editing: Editing {
                extra: self.editing.extra.clone(),
                ..Default::default()
            },
            accessibility: Accessibility {
                extra: self.accessibility.extra.clone(),
                ..Default::default()
            },
            export: Export {
                extra: self.export.extra.clone(),
                ..Default::default()
            },
            extra: self.extra.clone(),
            ..Default::default()
        }
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        if !EXPORT_RATES.contains(&self.export.sample_rate)
            || ![1, 2].contains(&self.export.channels)
            || !BITRATES.contains(&self.export.mp3_bitrate)
        {
            return Err("export");
        }
        if !["en", "es"].contains(&self.general.language.as_str()) {
            return Err("language");
        }
        if !SAMPLE_RATES.contains(&self.general.new_project_sample_rate) {
            return Err("sample_rate");
        }
        if ![1, 2].contains(&self.recording.channels) {
            return Err("channels");
        }
        if !BUFFER_SIZES.contains(&self.recording.buffer_frames) {
            return Err("buffer");
        }
        if !(-2000..=2000).contains(&self.recording.offset_ms) {
            return Err("offset");
        }
        if !(1..=60000).contains(&self.editing.step_ms) {
            return Err("step");
        }
        if !(self.editing.step_ms..=3600000).contains(&self.editing.large_step_ms) {
            return Err("large_step");
        }
        for id in [&self.devices.input_id, &self.devices.output_id] {
            if id.len() > 32768 || id.contains('\0') {
                return Err("device");
            }
        }
        if self.schema_version != 1 {
            return Err("schema_version");
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Store {
    path: PathBuf,
    pub settings: Settings,
    pub warnings: Vec<String>,
    writable: bool,
    last_good: Option<Vec<u8>>,
}
impl Store {
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    pub fn load(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let mut store = Self {
            path,
            settings: Settings::default(),
            warnings: vec![],
            writable: true,
            last_good: None,
        };
        match fs::read(&store.path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Err(e) => {
                store.writable = false;
                store
                    .warnings
                    .push(format!("Preferences could not be read: {e}"));
            }
            Ok(bytes) => match decode(&bytes) {
                Ok((s, w)) => {
                    store.settings = s;
                    store.warnings = w;
                    store.last_good = Some(bytes);
                }
                Err(DecodeError::Future) => {
                    store.writable = false;
                    store.warnings.push(
                        "Preferences belong to a newer version; they will not be overwritten."
                            .into(),
                    );
                }
                Err(DecodeError::Corrupt) => {
                    // Preserve the original before allowing any future writes.
                    let stamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos();
                    let retained = store.path.with_extension(format!("json.corrupt-{stamp}"));
                    if let Err(e) = fs::copy(&store.path, &retained) {
                        store.writable = false;
                        store
                            .warnings
                            .push(format!("Cannot preserve damaged preferences: {e}"));
                    }
                    store.warnings.push(
                        "Damaged preferences were detected; a retained copy was attempted.".into(),
                    );
                    match fs::read(store.backup_path())
                        .ok()
                        .and_then(|b| decode(&b).ok().map(|(s, w)| (b, s, w)))
                    {
                        Some((b, s, w)) => {
                            store.settings = s;
                            store.last_good = Some(b);
                            store.warnings.extend(w);
                            store
                                .warnings
                                .push("Restored the previous settings backup.".into());
                        }
                        None => store
                            .warnings
                            .push("Using default preferences; no usable backup was found.".into()),
                    }
                }
            },
        }
        store
    }
    pub fn backup_path(&self) -> PathBuf {
        self.path.with_extension("json.bak")
    }
    pub fn can_save(&self) -> bool {
        self.writable
    }
    /// Call from a worker. The in-memory value changes only after successful replacement.
    pub fn save(&mut self, next: Settings) -> io::Result<()> {
        next.validate()
            .map_err(|field| io::Error::new(io::ErrorKind::InvalidInput, field))?;
        if !self.writable {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Preferences are protected after a read error or unsupported version",
            ));
        }
        let bytes = serde_json::to_vec_pretty(&next)?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        if let Some(previous) = &self.last_good {
            atomic_write(&self.backup_path(), previous)?;
        }
        atomic_write(&self.path, &bytes)?;
        self.last_good = Some(bytes);
        self.settings = next;
        self.warnings.clear();
        Ok(())
    }
}
enum DecodeError {
    Corrupt,
    Future,
}
fn decode(bytes: &[u8]) -> Result<(Settings, Vec<String>), DecodeError> {
    let mut value: Value = serde_json::from_slice(bytes).map_err(|_| DecodeError::Corrupt)?;
    let obj = value.as_object_mut().ok_or(DecodeError::Corrupt)?;
    if let Some(v) = obj.get("schema_version") {
        if v.as_u64().is_none() {
            return Err(DecodeError::Corrupt);
        }
        if v.as_u64().unwrap_or(0) > 1 {
            return Err(DecodeError::Future);
        }
    }
    // Original 0.1 preferences used two flat keys. Migration is idempotent.
    if !obj.contains_key("schema_version") || obj["schema_version"] == 0 {
        if let Some(language) = obj.remove("language") {
            obj.entry("general")
                .or_insert(json!({}))
                .as_object_mut()
                .ok_or(DecodeError::Corrupt)?
                .entry("language")
                .or_insert(language);
        }
        if let Some(verbose) = obj.remove("verbose") {
            obj.entry("accessibility")
                .or_insert(json!({}))
                .as_object_mut()
                .ok_or(DecodeError::Corrupt)?
                .entry("detailed_time")
                .or_insert(verbose);
        }
        obj.insert("schema_version".into(), json!(1));
    }
    let defaults = serde_json::to_value(Settings::default()).map_err(|_| DecodeError::Corrupt)?;
    let mut warnings = vec![];
    merge_defaults(&mut value, &defaults, "", &mut warnings);
    // Check individual ranges without discarding valid unrelated or unknown values.
    for (section, key, valid) in [
        (
            "general",
            "language",
            ["en", "es"].contains(&value["general"]["language"].as_str().unwrap_or("")),
        ),
        (
            "general",
            "new_project_sample_rate",
            value["general"]["new_project_sample_rate"]
                .as_u64()
                .is_some_and(|v| SAMPLE_RATES.iter().any(|r| u64::from(*r) == v)),
        ),
        (
            "recording",
            "channels",
            value["recording"]["channels"]
                .as_u64()
                .is_some_and(|v| [1, 2].contains(&v)),
        ),
        (
            "recording",
            "buffer_frames",
            value["recording"]["buffer_frames"]
                .as_u64()
                .is_some_and(|v| BUFFER_SIZES.iter().any(|s| u64::from(*s) == v)),
        ),
        (
            "recording",
            "offset_ms",
            value["recording"]["offset_ms"]
                .as_i64()
                .is_some_and(|v| (-2000..=2000).contains(&v)),
        ),
        (
            "editing",
            "step_ms",
            value["editing"]["step_ms"]
                .as_u64()
                .is_some_and(|v| (1..=60000).contains(&v)),
        ),
        (
            "editing",
            "large_step_ms",
            value["editing"]["large_step_ms"]
                .as_u64()
                .is_some_and(|v| (1..=3600000).contains(&v)),
        ),
        (
            "devices",
            "input_id",
            valid_id(&value["devices"]["input_id"]),
        ),
        (
            "devices",
            "output_id",
            valid_id(&value["devices"]["output_id"]),
        ),
    ] {
        if !valid {
            value[section][key] = defaults[section][key].clone();
            warnings.push(format!("Reset invalid {section}.{key}"));
        }
    }
    if value["editing"]["large_step_ms"].as_u64() < value["editing"]["step_ms"].as_u64() {
        value["editing"]["large_step_ms"] = value["editing"]["step_ms"].clone();
        warnings.push("Increased large cursor step to the small step.".into());
    }
    let s: Settings = serde_json::from_value(value).map_err(|_| DecodeError::Corrupt)?;
    s.validate().map_err(|_| DecodeError::Corrupt)?;
    Ok((s, warnings))
}
fn valid_id(v: &Value) -> bool {
    v.as_str()
        .is_some_and(|s| s.len() <= 32768 && !s.contains('\0'))
}
fn merge_defaults(value: &mut Value, default: &Value, path: &str, warnings: &mut Vec<String>) {
    if let (Some(object), Some(defaults)) = (value.as_object_mut(), default.as_object()) {
        for (key, d) in defaults {
            let p = format!("{path}.{key}");
            if let Some(v) = object.get_mut(key) {
                merge_defaults(v, d, &p, warnings);
            } else {
                object.insert(key.clone(), d.clone());
            }
        }
    } else {
        let valid = match default {
            Value::Bool(_) => value.is_boolean(),
            Value::String(_) => value.is_string(),
            Value::Number(_) => value.as_i64().is_some() || value.as_u64().is_some(),
            Value::Object(_) => value.is_object(),
            _ => false,
        };
        if !valid {
            *value = default.clone();
            warnings.push(format!("Reset invalid type {path}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn dir(name: &str) -> PathBuf {
        let p =
            std::env::temp_dir().join(format!("sonitrelix-settings-{}-{name}", std::process::id()));
        fs::create_dir_all(&p).unwrap();
        p
    }
    #[test]
    fn legacy_defaults_and_unknown_values() {
        let (s,_)=decode(br#"{"language":"es","verbose":false,"plugin":{"future":12},"devices":{"custom":true}}"#).ok().unwrap();
        assert_eq!(s.general.language, "es");
        assert!(!s.accessibility.detailed_time);
        assert_eq!(s.extra["plugin"]["future"], 12);
        assert_eq!(s.devices.extra["custom"], true);
        assert_eq!(
            s.defaults_preserving_unknown().devices.extra["custom"],
            true
        );
        assert_eq!(decode(&serde_json::to_vec(&s).unwrap()).ok().unwrap().0, s);
    }
    #[test]
    fn invalid_types_ranges_and_cross_fields_are_repaired() {
        let (s,w)=decode(br#"{"general":{"language":true},"recording":{"offset_ms":9999,"channels":-2},"editing":{"step_ms":3000,"large_step_ms":2},"accessibility":{"detailed_time":1}}"#).ok().unwrap();
        assert_eq!(s.general.language, "en");
        assert_eq!(s.recording.offset_ms, 0);
        assert_eq!(s.recording.channels, 1);
        assert!(s.accessibility.detailed_time);
        assert_eq!(s.editing.large_step_ms, 3000);
        assert!(!w.is_empty());
    }
    #[test]
    fn backup_recovery_and_future_protection() {
        let dir = dir("recovery");
        let p = dir.join("preferences.json");
        let mut store = Store::load(&p);
        let first = Settings::default();
        store.save(first.clone()).unwrap();
        let mut second = first.clone();
        second.general.language = "es".into();
        store.save(second).unwrap();
        fs::write(&p, b"{broken").unwrap();
        let restored = Store::load(&p);
        assert_eq!(restored.settings, first);
        assert!(!restored.warnings.is_empty());
        assert!(fs::read_dir(&dir).unwrap().any(|p| {
            p.unwrap()
                .file_name()
                .to_string_lossy()
                .contains("corrupt-")
        }));
        fs::write(&p, b"{\"schema_version\":99}").unwrap();
        let mut future = Store::load(&p);
        assert!(!future.can_save());
        assert!(future.save(first).is_err());
        assert_eq!(fs::read(&p).unwrap(), b"{\"schema_version\":99}");
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn failed_save_keeps_memory_and_file() {
        let dir = dir("failure");
        let p = dir.join("preferences.json");
        let mut store = Store::load(&p);
        store.save(Settings::default()).unwrap();
        let before = fs::read(&p).unwrap();
        let mut invalid = store.settings.clone();
        invalid.editing.step_ms = 0;
        assert!(store.save(invalid).is_err());
        assert_eq!(fs::read(&p).unwrap(), before);
        assert_eq!(store.settings.editing.step_ms, 10);
        fs::remove_dir_all(dir).unwrap();
    }
}
