use super::shortcuts::{Action, Scope};
use super::*;
#[derive(Clone, Copy, PartialEq, Eq)]
struct Chord {
    key: usize,
    ctrl: bool,
    shift: bool,
    alt: bool,
}
fn chord(s: &str) -> Option<Chord> {
    let mut c = Chord {
        key: 0,
        ctrl: false,
        shift: false,
        alt: false,
    };
    for p in s.split('+') {
        let p = p.trim().to_ascii_uppercase();
        match p.as_str() {
            "CTRL" if !c.ctrl => c.ctrl = true,
            "SHIFT" if !c.shift => c.shift = true,
            "ALT" if !c.alt => c.alt = true,
            _ if c.key == 0 => {
                c.key = match p.as_str() {
                    "DELETE" => 0x2e,
                    "SPACE" => 0x20,
                    "UP" => 0x26,
                    "DOWN" => 0x28,
                    _ if p.len() == 1 && p.as_bytes()[0].is_ascii_alphanumeric() => {
                        p.as_bytes()[0] as usize
                    }
                    _ if p.starts_with('F') => {
                        let n = p[1..].parse::<usize>().ok()?;
                        if !(2..=24).contains(&n) || [6, 7, 8].contains(&n) {
                            return None;
                        }
                        0x6f + n
                    }
                    _ => return None,
                };
            }
            _ => return None,
        }
    }
    if c.key == 0 || c.alt && !c.ctrl && !c.shift {
        return None;
    }
    if [0x26, 0x28].contains(&c.key) && !(c.alt && c.shift && !c.ctrl) {
        return None;
    }
    Some(c)
}
fn commands() -> Vec<(&'static str, &'static str, &'static str, Action)> {
    use Action::Command;
    vec![
        ("copy", "copy", "Ctrl+C", Command(COPY)),
        ("cut", "cut", "Ctrl+X", Command(CUT)),
        ("paste", "paste", "Ctrl+V", Command(PASTE)),
        ("select_all", "select_all", "Ctrl+A", Command(SELECT_ALL)),
        (
            "delete_audio",
            "delete_audio",
            "Delete",
            Command(DELETE_AUDIO),
        ),
        ("split", "split", "Ctrl+T", Command(SPLIT)),
        ("trim", "trim", "Ctrl+Shift+T", Command(TRIM)),
        ("silence", "silence", "Ctrl+L", Command(SILENCE)),
        (
            "insert_silence",
            "insert_silence",
            "Ctrl+Shift+I",
            Command(INSERT_SILENCE),
        ),
        ("rename", "rename", "F2", Command(RENAME_FOCUS)),
        (
            "duplicate",
            "duplicate_track",
            "Ctrl+Shift+D",
            Command(DUPLICATE),
        ),
        ("move_up", "move_track_up", "Alt+Shift+Up", Command(MOVE_UP)),
        (
            "move_down",
            "move_track_down",
            "Alt+Shift+Down",
            Command(MOVE_DOWN),
        ),
        (
            "delete_track",
            "delete",
            "Ctrl+Shift+Delete",
            Command(DELETE),
        ),
        ("mute", "mute", "M", Command(MUTE)),
        ("solo", "solo", "S", Command(SOLO)),
        ("arm", "arm", "A", Command(ARM)),
        ("record", "record", "R", Command(recording_ui::RECORD)),
        (
            "effects",
            "effects_open",
            "Ctrl+E",
            Command(effects_ui::EFFECTS),
        ),
        ("levels", "track_levels", "Ctrl+G", Command(LEVELS)),
        ("play_stop", "play", "Space", Action::TogglePlay),
        ("pause", "pause", "Ctrl+Space", Command(PAUSE)),
    ]
}
fn configured<'a>(
    settings: &'a sonitrelix_settings::Settings,
    id: &str,
    default: &'a str,
) -> &'a str {
    settings
        .extra
        .get("shortcuts")
        .and_then(|m| m.get(id))
        .and_then(|v| v.as_str())
        .filter(|s| chord(s).is_some())
        .unwrap_or(default)
}

pub(super) fn menu_label(app: &App, key: &str, command: usize) -> String {
    let label = app.t(key);
    let Some((id, _, standard, _)) = commands()
        .into_iter()
        .find(|(_, _, _, action)| *action == Action::Command(command))
    else {
        return label;
    };
    let binding = configured(&app.store.settings, id, standard);
    if binding == standard {
        return label;
    }
    let title = label.split('\t').next().unwrap_or(&label);
    let binding = binding.to_ascii_uppercase();
    let binding = binding
        .split('+')
        .map(|part| match (app.es, part.trim()) {
            (true, "SHIFT") => "Mayús",
            (true, "DELETE") => "Supr",
            (true, "UP") => "Arriba",
            (true, "DOWN") => "Abajo",
            (true, "SPACE") => "Espacio",
            (_, "SHIFT") => "Shift",
            (_, "CTRL") => "Ctrl",
            (_, "ALT") => "Alt",
            (_, "DELETE") => "Delete",
            (_, "UP") => "Up",
            (_, "DOWN") => "Down",
            (_, "SPACE") => "Space",
            (_, name) => name,
        })
        .collect::<Vec<_>>()
        .join("+");
    format!("{title}\t{binding}")
}
fn fixed_timeline_shortcut(action: Option<Action>) -> bool {
    matches!(
        action,
        Some(Action::Command(
            LOOP_SELECTION
                | timeline_tools::ADD_MARKER
                | timeline_tools::PREVIOUS_MARKER
                | timeline_tools::NEXT_MARKER
                | timeline_tools::CROSSFADE
                | recording_ui::PUNCH
        ))
    )
}
pub(super) fn route(
    key: usize,
    ctrl: bool,
    shift: bool,
    alt: bool,
    scope: Scope,
    settings: &sonitrelix_settings::Settings,
) -> Option<Action> {
    let default = shortcuts::route(key, ctrl, shift, alt, scope);
    if !scope.editing()
        || default == Some(Action::Command(OPEN_LINK))
        || fixed_timeline_shortcut(default)
    {
        return default;
    }
    let pressed = Chord {
        key,
        ctrl,
        shift,
        alt,
    };
    for (id, _, standard, action) in commands() {
        if chord(configured(settings, id, standard)) == Some(pressed) {
            return Some(action);
        }
    }
    if commands().iter().any(|(id, _, standard, action)| {
        Some(*action) == default && configured(settings, id, standard) != *standard
    }) {
        None
    } else {
        default
    }
}
fn valid_assignment(settings: &sonitrelix_settings::Settings, id: &str, text: &str) -> bool {
    let Some(c) = chord(text) else {
        return false;
    };
    if context_menu::opens(c.key, c.ctrl, c.shift, c.alt)
        || shortcuts::route(c.key, c.ctrl, c.shift, c.alt, Scope::Other).is_some()
    {
        return false;
    }
    let surface = shortcuts::route(c.key, c.ctrl, c.shift, c.alt, Scope::Surface);
    if surface == Some(Action::Command(export_ui::EXPORT_SELECTION))
        || fixed_timeline_shortcut(surface)
    {
        return false;
    }
    !commands().iter().any(|(other, _, default, _)| {
        *other != id && chord(configured(settings, other, default)) == Some(c)
    })
}
pub(super) unsafe fn edit(
    owner: HWND,
    strings: serde_json::Value,
    settings: &mut sonitrelix_settings::Settings,
) -> Result<bool, String> {
    unsafe {
        let t = |k: &str| strings[k].as_str().unwrap_or(k).to_owned();
        let commands = commands();
        let mut options: Vec<_> = commands
            .iter()
            .map(|(_, label, _, _)| t(label).split('\t').next().unwrap_or("").replace('&', ""))
            .collect();
        options.push(t("keys_reset"));
        let Some(values) = forms::ask(
            owner,
            strings.clone(),
            &t("keys_title"),
            vec![forms::Field::Choice {
                label: t("keys_command"),
                options,
                selected: 0,
            }],
            &t("keys_hint"),
        )?
        else {
            return Ok(false);
        };
        let index = values[0].parse::<usize>().unwrap();
        if index == commands.len() {
            settings.extra.remove("shortcuts");
            return Ok(true);
        }
        let (id, _, default, _) = commands[index];
        let mut value = configured(settings, id, default).to_owned();
        let mut hint = t("keys_hint");
        loop {
            let Some(values) = forms::ask(
                owner,
                strings.clone(),
                &t("keys_title"),
                vec![forms::Field::Text {
                    label: t("keys_chord"),
                    value: value.clone(),
                }],
                &hint,
            )?
            else {
                return Ok(false);
            };
            value = values[0].clone();
            if !valid_assignment(settings, id, &value) {
                hint = t("keys_invalid");
                continue;
            }
            let map = settings
                .extra
                .entry("shortcuts")
                .or_insert_with(|| serde_json::json!({}));
            if !map.is_object() {
                *map = serde_json::json!({});
            }
            map[id] = value.into();
            return Ok(true);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn html_guides_document_all_configurable_defaults() {
        for guide in [
            include_str!("../../../docs/user-guide.en.html"),
            include_str!("../../../docs/user-guide.es.html"),
        ] {
            for (_, _, default, _) in commands() {
                assert!(
                    guide.contains(&format!("data-key=\"{default}\"")),
                    "undocumented default: {default}"
                );
            }
        }
    }
    #[test]
    fn remapping_protects_typing_and_rejects_collisions() {
        let mut s = sonitrelix_settings::Settings::default();
        assert!(!valid_assignment(&s, "record", "Ctrl+C"));
        assert!(!valid_assignment(&s, "copy", "Ctrl+S"));
        assert!(!valid_assignment(&s, "record", "F1"));
        assert!(!valid_assignment(&s, "record", "Shift+F10"));
        assert!(context_menu::opens(0x5d, false, false, false));
        assert!(context_menu::opens(0x79, false, true, false));
        assert!(!context_menu::opens(0x79, false, false, false));
        assert!(valid_assignment(&s, "record", "Ctrl+Shift+K"));
        s.extra.insert(
            "shortcuts".into(),
            serde_json::json!({"record":"Ctrl+Shift+K"}),
        );
        assert_eq!(route(0x52, false, false, false, Scope::Surface, &s), None);
        assert_eq!(
            route(0x4b, true, true, false, Scope::Surface, &s),
            Some(Action::Command(recording_ui::RECORD))
        );
        assert_eq!(route(0x4b, true, true, false, Scope::Text, &s), None);
    }
}
