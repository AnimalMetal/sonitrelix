use super::*;
use sonitrelix_editing as edit;
pub(super) const COPY: usize = 801;
pub(super) const CUT: usize = 802;
pub(super) const PASTE: usize = 803;
pub(super) const DELETE_AUDIO: usize = 804;
pub(super) const SELECT_ALL: usize = 805;
pub(super) const SPLIT: usize = 806;
pub(super) const TRIM: usize = 807;
pub(super) const SILENCE: usize = 808;
pub(super) const INSERT_SILENCE: usize = 809;
pub(super) const ADD_EMPTY: usize = 810;
pub(super) const DUPLICATE: usize = 811;
pub(super) const MOVE_UP: usize = 812;
pub(super) const MOVE_DOWN: usize = 813;
pub(super) const RENAME_FOCUS: usize = 814;
pub(super) const CLEAR_SELECTION: usize = 815;
pub(super) const READ_SELECTION: usize = 817;
pub(super) const MARK_START: usize = 818;
pub(super) const MARK_END: usize = 819;
#[derive(Default)]
pub(super) struct Editor {
    pub clipboard: Option<edit::Clipboard>,
    pub renaming: bool,
    pub exact_selection: Option<(usize, u64, u64)>,
    pub exact_cursor: Option<(usize, u64)>,
    pub bracket_start: Option<(usize, u64)>,
}
#[derive(Clone)]
pub(super) struct History {
    project: Project,
    position: u64,
    selection: (u64, u64),
    track: Option<usize>,
    exact: Option<(usize, u64, u64)>,
    cursor: Option<(usize, u64)>,
}
impl App {
    pub(super) fn history(&self) -> History {
        History {
            project: self.project.clone(),
            position: self.position,
            selection: self.selection,
            track: unsafe { self.selected_track() },
            exact: self.editor.exact_selection,
            cursor: self.editor.exact_cursor,
        }
    }
    pub(super) unsafe fn restore_history(&mut self, h: History) {
        unsafe {
            self.project = h.project;
            self.position = h.position;
            self.selection = h.selection;
            self.editor.exact_selection = h.exact;
            self.editor.exact_cursor = h.cursor;
            self.editor.bracket_start = None;
            self.selection_anchor = None;
            self.refresh();
            if let Some(index) = h.track {
                SendMessageW(GetDlgItem(self.hwnd, 11), 0x186, index, 0);
            }
            self.refresh_clips();
            self.sync_times();
        }
    }
    pub(super) unsafe fn sync_times(&self) {
        unsafe {
            for (id, value) in [
                (15, self.position),
                (17, self.selection.0),
                (19, self.selection.1),
            ] {
                SetWindowTextW(
                    GetDlgItem(self.hwnd, id),
                    wide(&format_time(value)).as_ptr(),
                );
            }
        }
    }
    pub(super) unsafe fn refresh_clips(&self) {
        unsafe {
            let list = GetDlgItem(self.hwnd, 25);
            if list.is_null() {
                return;
            }
            let old = SendMessageW(list, 0x188, 0, 0);
            SendMessageW(list, 0x184, 0, 0);
            if let Some(index) = self.selected_track() {
                let t = &self.project.tracks[index];
                let mut start = 0;
                for (i, c) in t.clips.iter().enumerate() {
                    let mut row = format!(
                        "{} {}, {}, {} {} — {}, {} {}",
                        self.t("clip_word"),
                        i + 1,
                        self.t(if c.source.is_some() {
                            "clip_audio"
                        } else {
                            "clip_silence"
                        }),
                        self.t("clip_position"),
                        format_time(sonitrelix_audio_io::frames_to_ms(start, t.sample_rate)),
                        format_time(ceil_ms(start + c.frames, t.sample_rate)),
                        self.t("clip_source_offset"),
                        format_time(sonitrelix_audio_io::frames_to_ms(
                            c.source_start,
                            t.sample_rate
                        ))
                    );
                    if self.verbose {
                        row.push_str(&format!(
                            ", {} {} — {}",
                            self.t("sample_frames"),
                            start,
                            start + c.frames
                        ));
                    }
                    if let Some(source) = &c.source
                        && let Some(name) = source.path.file_name()
                    {
                        row.push_str(&format!(
                            ", {} {}",
                            self.t("clip_source_file"),
                            name.to_string_lossy()
                        ));
                    }
                    SendMessageW(list, 0x180, 0, wide(&row).as_ptr() as isize);
                    start += c.frames;
                }
                if !t.clips.is_empty() {
                    SendMessageW(
                        list,
                        0x186,
                        old.max(0).min(t.clips.len() as isize - 1) as usize,
                        0,
                    );
                }
            }
        }
    }
    pub(super) unsafe fn track_selected(&mut self) {
        unsafe {
            self.editor.renaming = false;
            self.editor.exact_selection = None;
            self.editor.exact_cursor = None;
            self.editor.bracket_start = None;
            self.selection = (self.position, self.position);
            self.selection_anchor = None;
            self.sync_times();
            self.refresh_clips();
        }
    }
    pub(super) unsafe fn clip_selected(&mut self) {
        unsafe {
            let Some(index) = self.selected_track() else {
                return;
            };
            let selected = SendMessageW(GetDlgItem(self.hwnd, 25), 0x188, 0, 0);
            if selected < 0 {
                return;
            }
            let track = &self.project.tracks[index];
            let i = selected as usize;
            if i >= track.clips.len() {
                return;
            }
            let start: u64 = track.clips[..i].iter().map(|c| c.frames).sum();
            let end = start + track.clips[i].frames;
            self.editor.bracket_start = None;
            self.editor.exact_selection = Some((index, start, end));
            self.editor.exact_cursor = Some((index, start));
            self.position = sonitrelix_audio_io::frames_to_ms(start, track.sample_rate);
            self.selection = (self.position, ceil_ms(end, track.sample_rate));
            self.selection_anchor = None;
            self.sync_times();
            self.selection_status();
        }
    }
    pub(super) unsafe fn selection_status(&self) {
        unsafe {
            let mut text = format!(
                "{} {} — {} ({} ms)",
                self.t("selection"),
                format_time(self.selection.0),
                format_time(self.selection.1),
                self.selection.1.saturating_sub(self.selection.0)
            );
            if let Some((_, a, b)) = self.editor.exact_selection {
                text.push_str(&format!(
                    "; {} {}",
                    b.saturating_sub(a),
                    self.t("sample_frames")
                ));
            }
            self.status(&text);
        }
    }
    pub(super) fn selected_range(&self, index: usize) -> (u64, u64) {
        let track = &self.project.tracks[index];
        let (start, end) = self
            .editor
            .exact_selection
            .filter(|(i, _, _)| *i == index)
            .map(|(_, start, end)| (start, end))
            .unwrap_or_else(|| {
                (
                    sonitrelix_audio_io::ms_to_frames(self.selection.0, track.sample_rate),
                    sonitrelix_audio_io::ms_to_frames(self.selection.1, track.sample_rate),
                )
            });
        (start.min(track.frames()), end.min(track.frames()))
    }

    unsafe fn mark_boundary(&mut self, index: usize, at_end: bool) {
        unsafe {
            let track = &self.project.tracks[index];
            let rate = track.sample_rate;
            let frame = self.track_cursor_frame(index).min(track.frames());
            self.position = sonitrelix_audio_io::frames_to_ms(frame, rate);
            self.editor.exact_cursor = Some((index, frame));
            let message;
            if at_end {
                let (start, end) = self.selected_range(index);
                let start = self
                    .editor
                    .bracket_start
                    .filter(|(i, _)| *i == index)
                    .map(|(_, start)| start)
                    .or_else(|| (start < end).then_some(start));
                if let Some(start) = start.filter(|start| *start < frame) {
                    self.editor.exact_selection = Some((index, start, frame));
                    self.selection = (
                        sonitrelix_audio_io::frames_to_ms(start, rate),
                        ceil_ms(frame, rate),
                    );
                    self.editor.bracket_start = None;
                    message = format!(
                        "{} {}. {} {} ms",
                        self.t("selection_mark_end"),
                        format_time(ceil_ms(frame, rate)),
                        self.t("selection_mark_duration"),
                        sonitrelix_audio_io::frames_to_ms(frame - start, rate)
                    );
                } else {
                    message = self.t("selection_mark_end_invalid");
                    self.status(&message);
                    announcement::announce(GetDlgItem(self.hwnd, 21), &message);
                    return;
                }
            } else {
                self.editor.bracket_start = Some((index, frame));
                self.editor.exact_selection = Some((index, frame, frame));
                let time = sonitrelix_audio_io::frames_to_ms(frame, rate);
                self.selection = (time, time);
                message = format!("{} {}", self.t("selection_mark_start"), format_time(time));
            }
            self.selection_anchor = None;
            self.sync_times();
            self.status(&message);
            announcement::announce(GetDlgItem(self.hwnd, 21), &message);
        }
    }
    pub(super) unsafe fn begin_rename(&mut self) {
        unsafe {
            if let Some(i) = self.selected_track() {
                SetWindowTextW(
                    GetDlgItem(self.hwnd, 13),
                    wide(&self.project.tracks[i].name).as_ptr(),
                );
                self.editor.renaming = true;
                SetFocus(GetDlgItem(self.hwnd, 13));
                SendMessageW(GetDlgItem(self.hwnd, 13), 0xb1, 0, -1);
                self.status(&self.t("edit_rename_help"));
            }
        }
    }
    pub(super) unsafe fn edit_command(&mut self, id: usize) {
        unsafe {
            let focus = GetFocus();
            let mut class = [0u16; 32];
            let n = GetClassNameW(focus, class.as_mut_ptr(), 32);
            let text = String::from_utf16_lossy(&class[..n.max(0) as usize]) == "Edit";
            if text && [COPY, CUT, PASTE, DELETE_AUDIO, SELECT_ALL].contains(&id) {
                match id {
                    COPY => {
                        SendMessageW(focus, 0x301, 0, 0);
                    }
                    CUT => {
                        SendMessageW(focus, 0x300, 0, 0);
                    }
                    PASTE => {
                        SendMessageW(focus, 0x302, 0, 0);
                    }
                    DELETE_AUDIO => {
                        SendMessageW(focus, 0x303, 0, 0);
                    }
                    SELECT_ALL => {
                        SendMessageW(focus, 0xb1, 0, -1);
                    }
                    _ => (),
                }
                return;
            }
            if id == ADD_EMPTY {
                self.checkpoint();
                self.editor.exact_cursor = None;
                self.editor.exact_selection = None;
                self.editor.bracket_start = None;
                self.project.add_track(format!(
                    "{} {}",
                    self.t("track_word"),
                    self.project.tracks.len() + 1
                ));
                self.refresh();
                SendMessageW(
                    GetDlgItem(self.hwnd, 11),
                    0x186,
                    self.project.tracks.len() - 1,
                    0,
                );
                self.refresh_clips();
                SetFocus(GetDlgItem(self.hwnd, 11));
                return;
            }
            let Some(i) = self.selected_track() else {
                self.status(&self.t("edit_no_track"));
                return;
            };
            if [MARK_START, MARK_END].contains(&id) {
                self.mark_boundary(i, id == MARK_END);
                return;
            }
            if id == RENAME_FOCUS {
                self.begin_rename();
                return;
            }
            if id == READ_SELECTION {
                self.selection_status();
                SetFocus(GetDlgItem(self.hwnd, 21));
                return;
            }
            if id == CLEAR_SELECTION {
                self.editor.exact_selection = None;
                self.editor.bracket_start = None;
                self.selection = (self.position, self.position);
                self.selection_anchor = None;
                self.sync_times();
                self.selection_status();
                return;
            }
            if [DUPLICATE, MOVE_UP, MOVE_DOWN].contains(&id) {
                let target = if id == MOVE_UP {
                    i.saturating_sub(1)
                } else {
                    (i + 1).min(self.project.tracks.len() - 1)
                };
                if id != DUPLICATE && target == i {
                    return;
                }
                self.checkpoint();
                self.reset_playback();
                self.editor.exact_cursor = None;
                self.editor.exact_selection = None;
                self.editor.bracket_start = None;
                let selected = if id == DUPLICATE {
                    let mut t = self.project.tracks[i].clone();
                    t.name = format!("{} {}", t.name, self.t("edit_copy_suffix"));
                    self.project.tracks.insert(i + 1, t);
                    i + 1
                } else {
                    self.project.tracks.swap(i, target);
                    target
                };
                self.refresh();
                SendMessageW(GetDlgItem(self.hwnd, 11), 0x186, selected, 0);
                self.refresh_clips();
                SetFocus(GetDlgItem(self.hwnd, 11));
                return;
            }
            let old_rate = self.project.tracks[i].sample_rate;
            let rate = if id == PASTE && self.project.tracks[i].clips.is_empty() {
                self.editor
                    .clipboard
                    .as_ref()
                    .map(|c| c.sample_rate)
                    .unwrap_or(old_rate)
            } else {
                old_rate
            };
            let (a, b) = self
                .editor
                .exact_selection
                .filter(|(index, _, _)| *index == i && rate == old_rate)
                .map(|(_, a, b)| (a, b))
                .unwrap_or_else(|| {
                    (
                        sonitrelix_audio_io::ms_to_frames(self.selection.0, rate),
                        sonitrelix_audio_io::ms_to_frames(self.selection.1, rate)
                            .min(self.project.tracks[i].frames()),
                    )
                });
            let cursor = self
                .editor
                .exact_cursor
                .filter(|(index, _)| *index == i && rate == old_rate)
                .map(|(_, f)| f)
                .unwrap_or_else(|| sonitrelix_audio_io::ms_to_frames(self.position, rate));
            if id == SELECT_ALL {
                if self.project.tracks[i].frames() == 0 {
                    self.timeline_feedback(&self.t("edit_empty_selection"));
                    return;
                }
                self.editor.bracket_start = None;
                self.editor.exact_selection = Some((i, 0, self.project.tracks[i].frames()));
                self.selection = (0, ceil_ms(self.project.tracks[i].frames(), rate));
                self.selection_anchor = None;
                self.sync_times();
                self.timeline_feedback(
                    &self
                        .t("edit_all_selected")
                        .replace("{track}", &self.project.tracks[i].name),
                );
                return;
            }
            if id == COPY {
                match edit::copy(&self.project.tracks[i], a, b) {
                    Ok(c) => {
                        self.editor.clipboard = Some(c);
                        self.timeline_feedback(&self.t("edit_copied"));
                    }
                    Err(key) => self.timeline_feedback(&self.t(key)),
                }
                return;
            }
            let mut next = self.project.tracks[i].clone();
            let mut clipboard = None;
            let mut position = cursor;
            let mut selection = (a, b);
            let result = match id {
                CUT => edit::copy(&next, a, b)
                    .and_then(|c| {
                        clipboard = Some(c);
                        edit::delete(&mut next, a, b)
                    })
                    .map(|()| {
                        position = a;
                        selection = (a, a);
                    }),
                DELETE_AUDIO => edit::delete(&mut next, a, b).map(|()| {
                    position = a;
                    selection = (a, a);
                }),
                PASTE => self
                    .editor
                    .clipboard
                    .as_ref()
                    .ok_or("edit_empty_clipboard")
                    .and_then(|c| edit::paste(&mut next, cursor, (a, b), c))
                    .map(|(start, end)| {
                        position = end;
                        selection = (start, end);
                    }),
                SPLIT => edit::split(&mut next, cursor),
                TRIM => edit::trim(&mut next, a, b).map(|()| {
                    position = 0;
                    selection = (0, next.frames());
                }),
                SILENCE => edit::silence(&mut next, a, b),
                INSERT_SILENCE => {
                    let frames = if b > a { b - a } else { u64::from(rate) };
                    edit::insert_silence(&mut next, cursor, frames).map(|()| {
                        selection = (cursor, cursor + frames);
                        position = cursor;
                    })
                }
                _ => return,
            };
            match result {
                Err(key) => self.timeline_feedback(&self.t(key)),
                Ok(()) => {
                    let mut candidate = self.project.clone();
                    candidate.tracks[i] = next.clone();
                    if let Err(e) = candidate.validate() {
                        self.error(e);
                        return;
                    }
                    self.checkpoint();
                    self.reset_playback();
                    self.project.tracks[i] = next;
                    self.editor.bracket_start = None;
                    if let Some(c) = clipboard {
                        self.editor.clipboard = Some(c);
                    }
                    let new_rate = self.project.tracks[i].sample_rate;
                    self.position = ceil_ms(position, new_rate);
                    self.editor.exact_selection = Some((i, selection.0, selection.1));
                    self.editor.exact_cursor = Some((i, position));
                    self.selection = (
                        sonitrelix_audio_io::frames_to_ms(selection.0, new_rate),
                        ceil_ms(selection.1, new_rate),
                    );
                    self.selection_anchor = None;
                    self.dirty = true;
                    self.refresh();
                    self.sync_times();
                    match id {
                        CUT => self.timeline_feedback(&self.t("edit_cut_done")),
                        PASTE => self.timeline_feedback(&self.t("edit_paste_done")),
                        _ => self.selection_status(),
                    }
                    self.log("audio_edit_applied");
                }
            }
        }
    }
}
pub(super) fn ceil_ms(frames: u64, rate: u32) -> u64 {
    ((u128::from(frames) * 1000).div_ceil(u128::from(rate))).min(u128::from(u64::MAX)) as u64
}
