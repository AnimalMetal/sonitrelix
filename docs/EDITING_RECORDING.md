# Editing and recording — 0.4.0

This build adds source-preserving WAV clip editing and standalone WASAPI microphone capture. It previews one selected track at a time. It does not yet record vocals over backing music, mix tracks, process effects, or export edited audio.

## Keyboard editing test

1. Import a disposable mono/stereo WAV with Ctrl+I. Tab to Tracks, choose it, and press Space to listen. Ctrl+Space pauses/resumes. Stop before editing.
2. Ctrl+A selects its audio. F7 focuses the selection readout. Return to Tracks; Ctrl+C, Ctrl+Shift+N, Ctrl+V copies into a new track. An empty track adopts the copied rate/channels.
3. Enter selection start/end (for example 00:00:01.000 and 00:00:02.500), activate Apply times, then return to Tracks. Ctrl+X cuts the interval and closes the gap. Ctrl+V pastes. Ctrl+Z/Y restores/reapplies each edit. Ctrl+A, Ctrl+X cuts the entire track; Undo restores it.
4. Home, Right, Ctrl+T splits at the cursor. In Clips use Up/Down and Enter to select exact clip boundaries. Ctrl+Shift+T trims to that selection. Ctrl+L silences it without changing duration. Ctrl+Shift+I inserts silence (selected duration, otherwise one second). Preview and compare timing.
5. F2 renames; Enter commits; Escape cancels. Ctrl+Shift+D duplicates; Alt+Shift+Up/Down reorders. M mutes, A arms. Ctrl+Shift+Delete removes the selected track. Verify undo/redo restores the appropriate track and selection.
6. Save/reopen and compare clip boundaries and source bytes. Sources must remain unchanged. Save As must reject an imported-source destination. Open a version-2 project and save a copy; it upgrades to version 3.
7. In a track-name or time text field type R, A, M, S and use Ctrl+A/C/X/V. These must edit text, without starting capture or editing audio. F1 opens searchable guide; verify sections, Ctrl+F/F3, Escape and restored main-window focus.

## Physical microphone and screen-reader test — pending

Perform with NVDA and then JAWS, independently by keyboard. API tests do not replace these checks.

1. Tools > Preferences (Ctrl+,), Audio devices: choose the microphone and headphone output independently. In Recording/playback choose mono/stereo, buffer and offset 0. Apply/close.
2. Ctrl+Shift+N creates a blank track; F2 name it Vocals; A arms it. Check the list reports name, format, mute and arm. R starts standalone capture. Playback must stop first; there is no input monitoring or backing playback.
3. Speak, F6 reads elapsed recorded duration; repeated F6 refreshes without continuous announcements. Use the Pause button or return to Tracks and Ctrl+Space. Wait and resume; paused time should not appear in the clip.
4. R or Stop finalizes PCM16 WAV and inserts it at the starting cursor. Read its position/duration/source in Clips. Space previews. Listen for correct speed, channel order, missing samples and distortion. Save/reopen, edit and undo; the recorded WAV must remain.
5. Repeat with a nonzero cursor and signed offset. Insertion is cursor plus offset, clamped at zero; a gap is represented by silence. Existing content ripples later. This is manual placement, not measured latency compensation.
6. Disconnect the USB input while recording. A clear error must be readable and all completed recoverable frames retained, without fallback to another microphone. Check pause/stop/close while capture starts and during device loss. Closing an active recording should finalize before the unsaved-project prompt.
7. Repeat invalid/missing input, unsupported channel/rate, full or unwritable disk, and lengthy captures using disposable data. An error must leave a playable completed file when possible. Close/restart with a saved project and verify source references.
8. Test English/Spanish, high contrast, text scaling and focus order. No control may require a mouse. Missing labels, incorrect state announcements or lost focus block release.

Recordings are separate assets in data/recordings beside the executable. Keep them when moving projects. Undo/deleting tracks never deletes audio assets. Recording headers checkpoint about once per second; full crash recovery, project autosave and guaranteed recovery of an incomplete final tail remain unimplemented. No actual microphone/listening/NVDA/JAWS testing has been performed by the automated checks.

## Español

La versión 0.4 edita fragmentos WAV sin alterar originales y graba micrófono de forma independiente. Aún no reproduce acompañamiento durante la grabación, mezcla ni exporta. Consulte SHORTCUTS.md y F1 para los atajos traducidos.

Prueba: importe WAV, seleccione intervalo numérico, copie/corte/pegue, divida, recorte y deshaga. Guarde/reabra y compruebe que los originales permanecen iguales. En Preferencias elija micrófono/canales; cree pista, F2 renombre, A arme, R grabe, Ctrl+Espacio pause y R detenga. F6 lee duración; escuche la captura y compruebe su posición. Repita con NVDA y JAWS, desconexión USB y cierre mientras graba. Estas pruebas físicas y de lector siguen pendientes.