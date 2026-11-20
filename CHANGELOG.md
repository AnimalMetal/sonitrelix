# Changelog / Cambios

## 0.4.0 — Clip editing and standalone recording / Edición y grabación

- Native accessible clip list; exact selection, internal copy/cut/paste, delete, split, trim, silence and insert silence. Source files remain untouched.
- Track keyboard shortcuts for creating, renaming, duplicating, reordering, deleting, muting, solo metadata and recording arm; normal text editing keys preserved.
- Schema 3 stores ordered source slices/silence; versions 1/2 migrate. Undo/redo retains selection and source references.
- Standalone shared-mode WASAPI input capture to background PCM16 WAV, pause/resume, periodic header checkpoints, signed manual placement offset and retention of completed audio on errors where possible.
- Help > User Guide, Changelog and About display selectable read-only text in English/Spanish, with direct text focus, sections, search and copy support.
- Playback still previews one selected track. No backing-track overdubs, monitoring, effects, edited-audio export or full crash recovery yet. Real microphone/listening/NVDA/JAWS validation remains pending.

Edición WAV con teclado y grabación independiente; pistas armadas con A, grabar/detener con R. F1 incluye guía traducida con búsqueda. Aún no hay grabación sobre música ni exportación.

## 0.3.0 — WAV playback / Reproducción WAV

- Import mono/stereo RIFF/WAVE PCM 8/16/24/32-bit and IEEE float32 into a new track with Ctrl+I, including supported extensible headers.
- Stream one selected track through the chosen output with native shared-mode WASAPI and Windows rate/channel conversion.
- Added play, pause/resume, stop, keyboard/numeric seeking and an accessible F6 position/duration readout. No transport shortcuts intercept editable text fields.
- Added bounded SPSC buffering, separate disk and render workers, event-driven output, optional MMCSS audio scheduling, underrun counting and orderly asynchronous shutdown.
- Project schema 2 persists absolute source references and reads schema 1 projects. Import is undoable and preserves original files.
- Added decoder, queue, large virtual file, native UI/import and real WASAPI silence integration checks. Refuse unavailable selected outputs instead of falling back.

Se pueden importar y escuchar pistas WAV individualmente. Espacio reproduce/detiene; Ctrl+Espacio pausa/continúa; F6 consulta la posición. Aún no hay mezcla simultánea, grabación, efectos ni exportación. NVDA/JAWS siguen pendientes de pruebas reales.

## 0.2.0 — Preferences / Preferencias

- Renamed Options to Tools; added Preferences with Ctrl+,.
- Native Windows tabbed Preferences: General, Audio devices, Recording/playback, Editing/keyboard, Accessibility.
- Real background Windows Core Audio endpoint discovery, independent remembered input/output choices, refresh and unavailable-device retention.
- New-project sample rate, configurable cursor movement/selection steps, optional focus on time confirmation and immediate English/Spanish updates.
- Saved future recording channel/buffer/alignment/monitoring choices, explicitly identified as awaiting the audio engine.
- Apply/OK/Cancel, default reset, background saving, legacy migration, backup recovery, validation and unknown-setting retention.
- Added native property-sheet command tests and actual MSAA name/role checks. NVDA/JAWS acceptance remains pending.

Renombrado Opciones a Herramientas y añadido Preferencias con cinco pestañas. Se conservan los ajustes anteriores y los dispositivos guardados que no estén conectados. Todavía no hay reproducción ni grabación.

## 0.1.0 — Foundation / Base inicial

- Original Rust workspace with Windows-native controls and project model.
- English and Spanish menus, labels, guide and messages from the outset.
- Empty track management, state flags and undo/redo.
- Exact millisecond time input and selection validation.
- Versioned JSON project files, atomic replacement and unsaved-change prompts.
- Portable language/detail preferences and structured event logs.
- Automated model, localization, ABI and native-control tests.

Audio features and actual NVDA/JAWS acceptance remain pending. Esta versión todavía no reproduce, graba ni exporta audio.
