use super::editing_ui::*;
use super::*;
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Scope {
    Tracks,
    Surface,
    Text,
    Other,
}
impl Scope {
    pub(super) fn editing(self) -> bool {
        matches!(self, Self::Tracks | Self::Surface)
    }
}
pub(super) fn bracket(character: usize) -> Option<usize> {
    match character {
        0x5b => Some(MARK_START),
        0x5d => Some(MARK_END),
        _ => None,
    }
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
        return if scope == Scope::Tracks && !shift && !ctrl {
            match key {
                0x26 => Some(Command(VOLUME_UP)),
                0x28 => Some(Command(VOLUME_DOWN)),
                0x25 => Some(Command(PAN_LEFT)),
                0x27 => Some(Command(PAN_RIGHT)),
                _ => None,
            }
        } else if scope.editing() && shift && !ctrl {
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
            (0x45, true) => Some(super::export_ui::EXPORT),
            (0x49, false) => Some(IMPORT),
            (0x55, false) => Some(OPEN_LINK),
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
    if !ctrl && shift && key == 0x75 {
        return Some(Command(REPORT_STATUS));
    }
    if !ctrl && !shift {
        match key {
            0x70 => return Some(Command(HELP)),
            0x75 => return Some(Command(REPORT)),
            0x76 => return Some(Command(READ_SELECTION)),
            0x77 => return Some(Command(REPORT_LEVELS)),
            0x72 => return Some(Command(LOOP_SELECTION)),
            0x78 => return Some(Command(timeline_tools::PREVIOUS_MARKER)),
            0x7a => return Some(Command(timeline_tools::NEXT_MARKER)),
            _ => (),
        }
    }
    if !scope.editing() {
        return None;
    }
    if ctrl {
        let id = match (key, shift) {
            (0x41, false) => SELECT_ALL,
            (0x45, false) => super::effects_ui::EFFECTS,
            (0x47, false) => super::LEVELS,
            (0x43, false) => COPY,
            (0x58, false) => CUT,
            (0x56, false) => PASTE,
            (0x54, false) => SPLIT,
            (0x54, true) => TRIM,
            (0x58, true) => super::export_ui::EXPORT_SELECTION,
            (0x4c, false) => SILENCE,
            (0x4d, false) => timeline_tools::ADD_MARKER,
            (0x52, true) => super::recording_ui::PUNCH,
            (0x46, true) => timeline_tools::CROSSFADE,
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
    fn f8_audio_levels_work_from_every_main_window_control() {
        assert!(include_str!("../../../docs/SHORTCUTS.md").contains("| F8 |"));
        for scope in [Scope::Tracks, Scope::Surface, Scope::Text, Scope::Other] {
            assert_eq!(
                route(0x77, false, false, false, scope),
                Some(Action::Command(REPORT_LEVELS))
            );
        }
    }
    #[test]
    fn alt_arrows_adjust_only_the_focused_track_list() {
        for (label, key, command) in [
            ("Alt+Up", 0x26, VOLUME_UP),
            ("Alt+Down", 0x28, VOLUME_DOWN),
            ("Alt+Left", 0x25, PAN_LEFT),
            ("Alt+Right", 0x27, PAN_RIGHT),
        ] {
            assert!(include_str!("../../../docs/SHORTCUTS.md").contains(label));
            assert_eq!(
                route(key, false, false, true, Scope::Tracks),
                Some(Action::Command(command))
            );
            for scope in [Scope::Surface, Scope::Text, Scope::Other] {
                assert_eq!(route(key, false, false, true, scope), None);
            }
        }
    }
    #[test]
    fn documented_shortcuts_match_commands() {
        let docs = include_str!("../../../docs/SHORTCUTS.md");
        for (label, key, ctrl, shift, alt, command) in [
            (
                "Ctrl+E",
                0x45,
                true,
                false,
                false,
                super::super::effects_ui::EFFECTS,
            ),
            ("Ctrl+A", 0x41, true, false, false, SELECT_ALL),
            ("Ctrl+C", 0x43, true, false, false, COPY),
            ("Ctrl+X", 0x58, true, false, false, CUT),
            ("Ctrl+V", 0x56, true, false, false, PASTE),
            ("Ctrl+T", 0x54, true, false, false, SPLIT),
            ("Ctrl+Shift+T", 0x54, true, true, false, TRIM),
            ("Ctrl+L", 0x4c, true, false, false, SILENCE),
            ("F3", 0x72, false, false, false, LOOP_SELECTION),
            (
                "F9",
                0x78,
                false,
                false,
                false,
                timeline_tools::PREVIOUS_MARKER,
            ),
            (
                "F11",
                0x7a,
                false,
                false,
                false,
                timeline_tools::NEXT_MARKER,
            ),
            (
                "Ctrl+M",
                0x4d,
                true,
                false,
                false,
                timeline_tools::ADD_MARKER,
            ),
            (
                "Ctrl+Shift+F",
                0x46,
                true,
                true,
                false,
                timeline_tools::CROSSFADE,
            ),
            ("Ctrl+Shift+R", 0x52, true, true, false, recording_ui::PUNCH),
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
