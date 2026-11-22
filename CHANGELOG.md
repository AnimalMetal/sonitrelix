# Changelog

## 0.6.5

- Added selection loop playback, named project markers, punch-in recording, clip crossfades, and recording offset calibration.
- Effects show the track and time range they will process and prompt you to finish an incomplete bracket selection.
- Added offline support for a separately installed FastPlugs CLAP bundle with accessible parameter controls.
- Added spoken feedback for Copy, Cut, Paste, Select All, Undo and Redo. Preferences confirms completed saves and can recover a missing primary settings file from backup.

## 0.6.4

- Added a microphone test, F8 peak readings and clipping feedback, and optional headphone monitoring. Imports can recover complete frames from interrupted Sonitrelix recordings.
- Added Automatic language selection. New installations use Spanish when Windows displays in Spanish and English otherwise.

## 0.6.3

- Fixed Install update, Cancel transfer, and Skip this version in the update window. The window now reports download progress, skip confirmation, and errors.

## 0.6.2

- Added Alt+Arrow shortcuts in Tracks to adjust the selected track's volume and pan during playback.
- Simplified Volume and pan and Effects to use sliders with individual Reset buttons.

## 0.6.1

- Updated the documentation, license notices, and English and Spanish About text.

## 0.6.0 — first Windows release

- The portable Windows ZIP includes FFmpeg for additional import and export formats and audio effects.

- Refreshed both user guides to reflect the current recording, selection, effects and export workflow.

- Added individual Reset buttons for numeric effect, volume and pan settings.

- Noise Reduction now estimates the noise floor. Effects and Volume and Pan include keyboard sliders and exact numeric fields.

- Mark selection boundaries with [ and ], export the range, hear clearer Solo feedback and inspect failures in data/error.log.

- Added Noise Reduction, De-esser, De-hum, Change Tempo and Automatic Ducking with FFmpeg, Preview and Undo.

- After recording, playback starts at the new take. Tab leaves the Volume and Pan help text and reaches OK.
- Optional FFmpeg support expands Ctrl+I and adds M4A/AAC and Opus export, plus an MP3 encoder fallback.

- Added an English/Spanish HTML user guide with beginner-to-advanced lessons, linked contents and shortcut tables; open it from the F1 guide.

Multitrack mix with gain/pan and resampling; vocal overdubs with coordinated pause/resume and manual offset; MP3/FLAC/M4A imports; WAV/FLAC/MP3 mix export; Export preferences; editing shortcut customization; recent projects; recovery checkpoints; schema 4; signed portable EXE updates with startup rollback. English/Spanish help and license notices are included.

Español: mezcla multipista, voz sobre música, exportación WAV/MP3/FLAC, preferencias de exportación, atajos configurables, recientes, recuperación y actualizaciones firmadas.

## Earlier releases / Versiones anteriores

## 0.5.0 — Effects and release checking / Efectos y versiones

- Open one Effects dialog with Ctrl+E from Tracks/Clips: amplify, normalize, fade in/out, reverse, silence, three-band EQ, compressor, noise gate and reverb.
- Selection defaults to the whole selected track when no interval is selected. Numeric settings, short Preview, Stop preview, Apply and Cancel keep the workflow short.
- Effects render in a cancellable worker to new float32 WAV assets; Apply is undoable and sources remain unchanged. Reverb has wet/decay/tail controls; added tail shifts later audio.
- Help > Check for updates checks published GitHub Releases in a worker and offers the official download page in a read-only viewer. Installation is manual; no automatic executable replacement.
- Added a Windows build workflow for versioned artifacts.
- English/Spanish guide, changelog and labels updated. Added tests for native roles, parameter validation, undo, source preservation and DSP.

Efectos sencillos con Escuchar y Aplicar, incluida reverberación; Ctrl+Z deshace. Ayuda busca versiones de GitHub; la instalación sigue siendo manual.

## 0.4.0 — Clip editing and standalone recording / Edición y grabación

- Native accessible clip list; exact selection, internal copy/cut/paste, delete, split, trim, silence and insert silence. Source files remain untouched.
- Track keyboard shortcuts for creating, renaming, duplicating, reordering, deleting, muting, solo metadata and recording arm; normal text editing keys preserved.
- Schema 3 stores ordered source slices/silence; versions 1/2 migrate. Undo/redo retains selection and source references.
- Standalone shared-mode WASAPI input capture to background PCM16 WAV, pause/resume, periodic header checkpoints, signed manual placement offset and retention of completed audio on errors where possible.
- Help > User Guide, Changelog and About display selectable read-only text in English/Spanish, with direct text focus, sections, search and copy support.
- Playback in 0.4.0 previewed one selected track. Mixing and recording over backing music were introduced in 0.6.0.

Edición WAV con teclado y grabación independiente; pistas armadas con A, grabar/detener con R. F1 incluye guía traducida con búsqueda. La versión 0.6.0 incorporó grabación sobre música y exportación de la mezcla.

## 0.3.0 — WAV playback / Reproducción WAV

- Import mono/stereo RIFF/WAVE PCM 8/16/24/32-bit and IEEE float32 into a new track with Ctrl+I, including supported extensible headers.
- Stream one selected track through the chosen output with native shared-mode WASAPI and Windows rate/channel conversion.
- Added play, pause/resume, stop, keyboard/numeric seeking and an accessible F6 position/duration readout. No transport shortcuts intercept editable text fields.
- Added bounded SPSC buffering, separate disk and render workers, event-driven output, optional MMCSS audio scheduling, underrun counting and orderly asynchronous shutdown.
- Project schema 2 persists absolute source references and reads schema 1 projects. Import is undoable and preserves original files.
- Added decoder, queue, large virtual file, native UI/import and real WASAPI silence integration checks. Refuse unavailable selected outputs instead of falling back.

La versión 0.3.0 incorporó la reproducción WAV. Espacio reproducía o detenía, Ctrl+Espacio pausaba o continuaba y F6 consultaba la posición.

## 0.2.0 — Preferences / Preferencias

- Renamed Options to Tools; added Preferences with Ctrl+,.
- Native Windows tabbed Preferences: General, Audio devices, Recording/playback, Editing/keyboard, Accessibility.
- Real background Windows Core Audio endpoint discovery, independent remembered input/output choices, refresh and unavailable-device retention.
- New-project sample rate, configurable cursor movement/selection steps, optional focus on time confirmation and immediate English/Spanish updates.
- Saved future recording channel/buffer/alignment/monitoring choices, explicitly identified as awaiting the audio engine.
- Apply/OK/Cancel, default reset, background saving, legacy migration, backup recovery, validation and unknown-setting retention.
- Added native property-sheet command tests and MSAA name and role checks.

Renombrado Opciones a Herramientas y añadido Preferencias con cinco pestañas. Se conservan los ajustes anteriores y los dispositivos guardados que no estén conectados.

## 0.1.0 — Foundation / Base inicial

- Original Rust workspace with Windows-native controls and project model.
- English and Spanish menus, labels, guide and messages from the outset.
- Empty track management, state flags and undo/redo.
- Exact millisecond time input and selection validation.
- Versioned JSON project files, atomic replacement and unsaved-change prompts.
- Portable language/detail preferences and structured event logs.
- Automated model, localization, ABI and native-control tests.

This release established the application and project model for later audio features.
