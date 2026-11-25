use super::*;

pub(super) const SELECT_CLIP: usize = 829;

pub(super) fn opens(key: usize, ctrl: bool, shift: bool, alt: bool) -> bool {
    !ctrl && !alt && matches!((key, shift), (0x5d, false) | (0x79, true))
}

impl App {
    pub(super) unsafe fn show_context_menu(&mut self, control: HWND) -> bool {
        unsafe {
            let id = GetDlgCtrlID(control);
            if ![11, 25, 15].contains(&id) || control != GetDlgItem(self.hwnd, id) {
                return false;
            }
            let editing =
                self.recording.is_none() && self.timeline_job.is_none() && !self.playback.closing;
            let index = self.selected_track();
            let track = index.and_then(|i| self.project.tracks.get(i));
            let has_track = editing && track.is_some();
            let has_audio = editing && track.is_some_and(|t| !t.clips.is_empty());
            let has_range = has_audio
                && index.is_some_and(|i| {
                    let (start, end) = self.selected_range(i);
                    start < end
                });
            let state = |test: fn(&sonitrelix_session::Track) -> bool| track.is_some_and(test);
            let items: &[(&str, usize, bool, bool)] = match id {
                11 => &[
                    ("rename", RENAME_FOCUS, has_track, false),
                    ("mute", MUTE, has_track, state(|t| t.mute)),
                    ("solo", SOLO, has_track, state(|t| t.solo)),
                    ("arm", ARM, has_track, state(|t| t.armed)),
                    ("track_levels", LEVELS, has_track, false),
                    ("duplicate_track", DUPLICATE, has_track, false),
                    ("new_track", ADD_EMPTY, editing, false),
                ],
                25 => &[
                    (
                        "context_select_clip",
                        SELECT_CLIP,
                        has_audio && SendMessageW(control, 0x188, 0, 0) >= 0,
                        false,
                    ),
                    ("context_cut_selection", CUT, has_range, false),
                    ("context_copy_selection", COPY, has_range, false),
                    ("split", SPLIT, has_audio, false),
                    ("effects_open", effects_ui::EFFECTS, has_audio, false),
                ],
                _ => &[
                    ("context_go_to_time", SET_POSITION, editing, false),
                    ("edit_selection_menu", EDIT_SELECTION, has_track, false),
                    ("context_marker", timeline_tools::ADD_MARKER, editing, false),
                ],
            };
            let mut bounds = RECT::default();
            if GetWindowRect(control, &mut bounds) == 0 {
                return false;
            }
            let mut anchor = POINT {
                x: bounds.left + 8,
                y: bounds.top + 8,
            };
            if id != 15 {
                let selected = SendMessageW(control, 0x188, 0, 0);
                let mut row = RECT::default();
                if selected >= 0
                    && SendMessageW(
                        control,
                        0x198,
                        selected as usize,
                        &mut row as *mut RECT as isize,
                    ) >= 0
                {
                    let mut point = POINT {
                        x: row.left + 8,
                        y: row.top,
                    };
                    if ClientToScreen(control, &mut point) != 0
                        && (bounds.top..bounds.bottom).contains(&point.y)
                    {
                        anchor = point;
                    }
                }
            }
            let menu = CreatePopupMenu();
            if menu.is_null() {
                self.error(std::io::Error::last_os_error());
                return true;
            }
            for &(key, command, enabled, checked) in items {
                let flags = (if enabled { 0 } else { 1 }) | (if checked { 8 } else { 0 });
                let label = shortcut_config::menu_label(self, key, command);
                if AppendMenuW(menu, flags, command, wide(&label).as_ptr()) == 0 {
                    let error = std::io::Error::last_os_error();
                    DestroyMenu(menu);
                    self.error(error);
                    return true;
                }
            }
            self.modal.set(true);
            let command =
                TrackPopupMenu(menu, 0x100, anchor.x, anchor.y, 0, self.hwnd, null()) as usize;
            self.modal.set(false);
            DestroyMenu(menu);
            if GetFocus() != control {
                SetFocus(control);
            }
            if command != 0 {
                self.command(command);
            }
            true
        }
    }
}
