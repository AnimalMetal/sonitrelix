> Current 0.4 additions: native Clips list, keyboard audio edits, recording arm/transport, searchable bilingual F1 help. Follow [EDITING_RECORDING.md](EDITING_RECORDING.md) for current manual tests and [SHORTCUTS.md](SHORTCUTS.md) for context-sensitive bindings. Actual NVDA/JAWS acceptance remains pending.

# Accessibility acceptance / Pruebas de accesibilidad

Status: **pending actual NVDA and JAWS tests**. Native controls are a starting point, not proof. Automated tests create native Win32 controls and verify commands, serialization, language preservation and MSAA names/roles, not reader output. Computer Use access to Sonitrelix was denied in this environment, so no UI Automation inspection result is recorded.

For 0.3.0, use the [WAV playback manual checklist](WAV_PLAYBACK.md), especially selected-output routing, text-safe shortcuts, F6 position snapshots, seek/pause behavior, errors and physical USB disconnection. The added readout/buttons have native MSAA name checks; actual reader speech, focus behavior and visual layout still need independent verification.

For 0.2.0, start with the [Preferences-specific manual checklist](PREFERENCES.md). Tools replaces Options; language and detailed confirmation are now inside Preferences (Ctrl+Comma). New native tests query MSAA names and roles on main-window and preference controls. Those checks are API-level evidence only; reader output and independent keyboard use remain pending. Native property-sheet integration tests briefly create and hide their own dialog; no external application is automated.

Run every step without a mouse, once in English and once in Spanish, separately with current NVDA and JAWS on Windows 11. Repeat critical cases on Windows 10. Record Windows/reader version, expected and actual speech/braille, focus, outcome and evidence. A serious failure blocks release.

1. Launch Sonitrelix. Initial focus should be the empty Tracks list. NVDA+Tab or the JAWS focus command must identify the list and its name.
2. Tab through Tracks, Track name, Add track, Position, Selection start, Selection end, Apply times, Status. Reverse with Shift+Tab. Verify names, roles, values, focus visibility and read-only Status. Alt mnemonics should reach each intended control without collisions.
3. Enter `Vocals ñ` in Track name; activate Add track by keyboard. Focus should return to the selected row. Confirm the full name and mute/solo/arm state are available. Create a second track and use arrows between rows.
4. Through Track menu, toggle mute, solo and arm. These flags are storage only in this build; ensure that limitation is announced in the arm label. Re-read the row to verify current state; check spontaneous duplicate announcements and stale focus. Rename using the name field, then delete a test track; verify focus goes to the remaining row. Never perform this with valuable real projects.
5. From the list, Ctrl+Z/Y undo/redo the last track change. In a text field Ctrl+Z must remain local text undo. Ctrl+X/C/V/A must retain normal text editing and typing `R` or Space must not start transport.
6. Enter `00:01:15.000`, start `00:00:12.000`, end `00:00:18.000`; Apply times. Tab to Status and confirm the exact position, selection and 6000 ms duration. Disable detailed time confirmation through Tools > Preferences > Accessibility; repeat and verify shorter Status. Neither setting replaces native names/values.
7. Enter an invalid minute (`00:60:00.000`) or end before start. Apply. The native error must be readable, Enter/Escape close it, and focus must return to Position for correction. Improve focus to the precise failing field before a public editor release.
8. Ctrl+S opens the Windows Save dialog. Give a path with spaces and Unicode. Save, change a track, Ctrl+Shift+S to another path. Ctrl+O reopens the first project; verify independent contents. Cancel dialogs; focus should return to the initiating control. Try malformed/future JSON and an unwritable location; ensure current project survives and errors are readable.
9. With edits pending, Ctrl+N and Alt+F4 show Yes/No/Cancel using system language. Cancel must retain data and focus; Yes must save successfully before proceeding; canceling that Save dialog must retain the project.
10. Change Tools > Preferences > General to Español with a track and typed times. Confirm translated menus and labels, preservation of values, and useful focus. Restart and confirm persistence. Windows common-dialog buttons follow OS language.
11. F1 opens the native guide; verify all content can be read. Close and verify focus retention. Long text in a message box and lack of search are known limitations; replace with a read-only searchable viewer if readers cannot navigate it.
12. Test 100%, 150%, 200% scaling, system text-size changes, Windows high contrast, minimum size and maximize. No clipped labels, hidden controls or loss of focus. Check screen-reader braille output and native selection indication.

## Later milestones — not executable in 0.1

For playback: open fixture WAV/MP3/FLAC, play/stop/seek while traversing controls; verify audible timing and queryable position without speech flooding. For editing: numeric selection/split/cut/paste/trim, source hashes unchanged, undo/redo and reopen. For recording: select USB microphone and headphones independently; disconnect capture while recording and preserve completed data. For multitrack acceptance: import instrumental, create/arm Vocals, record while backing plays, compensate offset, edit/process vocals independently, mix/export and reopen/check output. For effects: every numerical parameter, preview, apply, cancel and undo must be usable by keyboard. Mark all these pending until implemented and actually tested.

## Español: registro mínimo por caso

Anote lector y versión, Windows y versión, idioma, pasos exactos, resultado esperado, resultado observado, foco final y estado aprobado/fallido/pendiente. Una prueba automática no equivale a «probado con NVDA/JAWS». El flujo de grabación de voz sobre música sigue pendiente.


## Help text acceptance check

Open Help > User guide, Changelog and About, in English and Spanish. Each must focus a named read-only text control, not a button. Arrow through lines, select with Shift+arrows or Ctrl+A, copy with Ctrl+C, and verify typing cannot change the text. Use Ctrl+F/F3 to search, section navigation, and Escape to return to the original main control. Repeat with NVDA and JAWS; verify text/read-only role, content and focus announcements. Automated checks verify the native text role, read-only style, input rejection, translated labels and menu commands; real screen-reader confirmation is still pending.