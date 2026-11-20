use super::editing_ui::*;
use super::*;
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Scope {
    Surface,
    Text,
    Other,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Action {
    Command(usize),
    Cursor {
        right: bool,
        large: bool,
        select: bool,
    },
    Boundary {
        end: bool,
        select: bool,
    },
    TogglePlay,
}
pub(super) fn route(
    key: usize,
    ctrl: bool,
    shift: bool,
    alt: bool,
    scope: Scope,
) -> Option<Action> {
    use Action::*;
    if alt {
        return if scope == Scope::Surface && shift && !ctrl {
            match key {
                0x26 => Some(Command(MOVE_UP)),
                0x28 => Some(Command(MOVE_DOWN)),
                _ => None,
            }
        } else {
            None
        };
    }
    if ctrl {
        let global = match (key, shift) {
            (0x4e, false) => Some(NEW),
            (0x4e, true) => Some(ADD_EMPTY),
            (0x4f, false) => Some(OPEN),
            (0x53, false) => Some(SAVE),
            (0x53, true) => Some(SAVE_AS),
            (0x49, false) => Some(IMPORT),
            (0xbc, false) => Some(PREFERENCES),
            _ => None,
        };
        if let Some(id) = global {
            return Some(Command(id));
        }
        if scope != Scope::Text {
            let undo = match key {
                0x5a if !shift => Some(UNDO),
                0x59 if !shift => Some(REDO),
                _ => None,
            };
            if let Some(id) = undo {
                return Some(Command(id));
            }
        }
    }
    if !ctrl && !shift {
        match key {
            0x70 => return Some(Command(HELP)),
            0x75 => return Some(Command(REPORT)),
            0x76 => return Some(Command(READ_SELECTION)),
            _ => (),
        }
    }
    if scope != Scope::Surface {
        return None;
    }
    if ctrl {
        let id = match (key, shift) {
            (0x41, false) => SELECT_ALL,
            (0x43, false) => COPY,
            (0x58, false) => CUT,
            (0x56, false) => PASTE,
            (0x54, false) => SPLIT,
            (0x54, true) => TRIM,
            (0x4c, false) => SILENCE,
            (0x49, true) => INSERT_SILENCE,
            (0x44, true) => DUPLICATE,
            (0x2e, true) => DELETE,
            (0x20, false) => PAUSE,
            _ => 0,
        };
        if id != 0 {
            return Some(Command(id));
        }
    }
    if !ctrl && !shift {
        let id = match key {
            0x2e => DELETE_AUDIO,
            0x71 => RENAME_FOCUS,
            0x4d => MUTE,
            0x53 => SOLO,
            0x41 => ARM,
            0x1b => CLEAR_SELECTION,
            0x52 => super::recording_ui::RECORD,
            _ => 0,
        };
        if id != 0 {
            return Some(Command(id));
        }
        if key == 0x20 {
            return Some(TogglePlay);
        }
    }
    if [0x25, 0x27].contains(&key) {
        return Some(Cursor {
            right: key == 0x27,
            large: ctrl,
            select: shift,
        });
    }
    if !ctrl && [0x24, 0x23].contains(&key) {
        return Some(Boundary {
            end: key == 0x23,
            select: shift,
        });
    }
    None
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn documented_shortcuts_match_commands() {
        let docs = include_str!("../../../docs/SHORTCUTS.md");
        for (label, key, ctrl, shift, alt, command) in [
            ("Ctrl+A", 0x41, true, false, false, SELECT_ALL),
            ("Ctrl+C", 0x43, true, false, false, COPY),
            ("Ctrl+X", 0x58, true, false, false, CUT),
            ("Ctrl+V", 0x56, true, false, false, PASTE),
            ("Ctrl+T", 0x54, true, false, false, SPLIT),
            ("Ctrl+Shift+T", 0x54, true, true, false, TRIM),
            ("Ctrl+L", 0x4c, true, false, false, SILENCE),
            ("Ctrl+Shift+I", 0x49, true, true, false, INSERT_SILENCE),
            ("Ctrl+Shift+N", 0x4e, true, true, false, ADD_EMPTY),
            ("Ctrl+Shift+D", 0x44, true, true, false, DUPLICATE),
            ("Alt+Shift+Up", 0x26, false, true, true, MOVE_UP),
            ("Alt+Shift+Down", 0x28, false, true, true, MOVE_DOWN),
            ("Ctrl+Shift+Delete", 0x2e, true, true, false, DELETE),
            ("F2", 0x71, false, false, false, RENAME_FOCUS),
            (
                "R",
                0x52,
                false,
                false,
                false,
                super::super::recording_ui::RECORD,
            ),
        ] {
            assert!(docs.contains(label), "undocumented binding: {label}");
            assert_eq!(
                route(key, ctrl, shift, alt, Scope::Surface),
                Some(Action::Command(command)),
                "{label}"
            );
        }
    }
    #[test]
    fn text_keys_remain_native() {
        for key in [0x41, 0x43, 0x58, 0x56, 0x5a, 0x59, 0x54, 0x4c] {
            assert_eq!(route(key, true, false, false, Scope::Text), None);
        }
        for key in [0x4d, 0x53, 0x41, 0x52, 0x20, 0x2e] {
            assert_eq!(route(key, false, false, false, Scope::Text), None);
        }
    }
    #[test]
    fn audio_and_track_bindings() {
        assert_eq!(
            route(0x52, false, false, false, Scope::Surface),
            Some(Action::Command(super::super::recording_ui::RECORD))
        );
        assert_eq!(
            route(0x58, true, false, false, Scope::Surface),
            Some(Action::Command(CUT))
        );
        assert_eq!(
            route(0x2e, true, true, false, Scope::Surface),
            Some(Action::Command(DELETE))
        );
        assert_eq!(
            route(0x26, false, true, true, Scope::Surface),
            Some(Action::Command(MOVE_UP))
        );
        assert_eq!(
            route(0x25, true, true, false, Scope::Surface),
            Some(Action::Cursor {
                right: false,
                large: true,
                select: true
            })
        );
    }
}
