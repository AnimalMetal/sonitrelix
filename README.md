# Sonitrelix

GitHub: [AnimalMetal/sonitrelix](https://github.com/AnimalMetal/sonitrelix). See [publishing and Windows builds](docs/GITHUB.md). In-app updates are not implemented yet.

An original Rust, Windows-native audio editor designed around keyboard access and screen readers. **0.4.0 implements WAV clip editing and standalone microphone recording.** English and Spanish are included. This is a development build; physical microphone, listening and NVDA/JAWS acceptance tests remain pending.

Implemented: native controls, tabbed Tools > Preferences (Ctrl+,), input/output discovery, streaming selected-track WASAPI preview, precise time fields, source-preserving clip edits, internal audio clipboard, split/trim/silence, track management, undo/redo, schema-3 project saving and legacy migration, background PCM16 capture with pause/stop and manual placement offset, structured logs, and searchable F1 help.

Import: mono/stereo RIFF/WAVE PCM 8/16/24/32-bit or float32, 8000–192000 Hz. Capture creates PCM16 WAV assets in data/recordings. Keep imported and recorded assets in place: project files store absolute references, not embedded audio.

Start: Ctrl+I imports WAV. In Tracks or Clips, Space previews; Ctrl+Space pauses; Ctrl+A selects all audio on this track; Ctrl+C/X/V copies/cuts/pastes. Ctrl+T splits, Ctrl+Shift+T trims. Ctrl+Shift+N adds a track, F2 renames, A arms, R starts/stops standalone recording. Playback stops before recording. F6 reads transport; F7 reads selection. Text fields keep native editing keys. See [all shortcuts](docs/SHORTCUTS.md) and [editing/recording tests](docs/EDITING_RECORDING.md).

Not implemented: synchronized multitrack playback/overdubs, live monitoring, effects, audio mixdown/export, MP3/FLAC/OGG/AAC, gain/pan editing, shortcut remapping, recent projects, or full project crash recovery. Solo remains metadata; mute prevents selected-track preview. Periodic recording-header checkpoints do not guarantee recovery after a crash.

Build/run with stable Rust and MSVC build tools:

```powershell
cargo build --release --workspace --locked
.\target\release\Sonitrelix.exe
```

See [BUILDING.md](BUILDING.md), [Preferences](docs/PREFERENCES.md), [architecture](docs/ARCHITECTURE.md), [validation](docs/VALIDATION.md), [accessibility](docs/ACCESSIBILITY.md), [project schema](docs/PROJECT_FORMAT.md), and [dependency notices](THIRD_PARTY_NOTICES.md).

## Español

Sonitrelix 0.4.0 edita fragmentos WAV sin alterar originales y graba micrófono de forma independiente. Incluye Preferencias con cinco pestañas, inglés/español, tiempos exactos, deshacer/rehacer, proyectos y guía F1 con secciones y búsqueda.

En Pistas o Fragmentos: Ctrl+A selecciona todo el audio de la pista; Ctrl+C/X/V copia/corta/pega; Ctrl+T divide y Ctrl+Mayús+T recorta. Ctrl+Mayús+N crea pista, F2 renombra, A arma y R graba/detiene. F6 lee transporte; F7 selección. Los campos de texto conservan sus teclas. La reproducción se detiene antes de grabar: aún no hay grabación sobre acompañamiento, mezcla, efectos ni exportación. Conserve los archivos de origen y de data/recordings.

Faltan pruebas físicas de micrófono y pruebas reales con NVDA/JAWS; no se considera completa la accesibilidad. Consulte [atajos](docs/SHORTCUTS.md) y [pruebas](docs/EDITING_RECORDING.md).
## Reference and license

The [Elion README](https://github.com/Elion-Daw/Elion), [build documentation](https://github.com/Elion-Daw/Elion/blob/main/BUILDING.md), and [licensing inventory](https://github.com/Elion-Daw/Elion/blob/main/LICENSES.md) informed the keyboard-first design review. Elion uses C++/JUCE/Tracktion and AGPL-3.0-only for its original code. No Elion source, documentation text, patches, or assets were copied. Sonitrelix's original code is MIT licensed. Dependencies retain their own notices.
