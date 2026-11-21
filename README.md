# Sonitrelix

Original Rust, Windows-native audio editor with English/Spanish interfaces. **Current Windows release: 0.6.4.** Testing with NVDA and JAWS and physical overdub alignment checks are pending.

Download `Sonitrelix-windows-x64.zip`, extract the whole folder and open `Sonitrelix.exe`. FFmpeg and the required Visual C++ runtime files are included. Press F1 for the user guide. [Windows downloads](https://github.com/AnimalMetal/sonitrelix/releases)

Implemented: non-destructive clip editing, exact times, undo/redo, synchronized multitrack playback, mute/solo, gain/pan, vocal recording while backing music plays, manual offset, effects including reverb, WAV/FLAC/MP3 mix export, six-tab Preferences, editing shortcut customization, recent projects, recovery checkpoints and signed portable EXE updates.

Import supports WAV, MP3, FLAC and AAC in M4A through Windows Media Foundation. The bundled FFmpeg adds Ogg Vorbis, Opus, AIFF, audio extraction from supported video files, AAC and Opus export, and an MP3 encoder fallback. Export also supports PCM16 WAV and FLAC. MP3 may contain encoder padding. Use WAV or FLAC for exact duration. [FFmpeg formats and effects](docs/FFMPEG.md)

Ctrl+I imports music. Ctrl+Shift+N creates a track, F2 names it, A arms it and R records/stops over backing tracks. Use headphones. Space plays/stops the mix; Ctrl+Space pauses. In Tracks/Clips, Ctrl+A selects track audio, Ctrl+C/X/V copies/cuts/pastes, Ctrl+T splits, Ctrl+E opens effects and Ctrl+G adjusts gain/pan. Ctrl+Shift+E exports. Text fields retain normal editing keys.

Keep source files and data/imports and data/recordings: projects reference assets. Dirty metadata is checkpointed every 30 seconds. Ctrl+I can recover complete PCM frames from interrupted Sonitrelix recordings with a readable header. Transport > Test microphone checks the input before a take, F8 reports peak levels and clipping, and optional headphone monitoring is available during recording. Automatic latency measurement, streaming compressed editing and freely overlapping clips remain future work.

```powershell
cargo test --workspace --locked
.\tools\build-release.ps1
.\target\release\Sonitrelix.exe
.\tools\package.ps1
```

Read the [English HTML user guide](docs/user-guide.en.html) or [Spanish HTML guide](docs/user-guide.es.html), from beginner to advanced with shortcut tables. The F1 text viewer includes an Open HTML guide action.

See [building](BUILDING.md), [shortcuts](docs/SHORTCUTS.md), [multitrack test workflow](docs/MULTITRACK.md), [updates](docs/UPDATES.md), [validation](docs/VALIDATION.md), [project format](docs/PROJECT_FORMAT.md) and [licenses](THIRD_PARTY_NOTICES.md). Repository: https://github.com/AnimalMetal/sonitrelix.

## Español

Sonitrelix 0.6.4 incluye mezcla multipista, grabación de voz sobre música, tiempos exactos, efectos con reverberación, exportación WAV/MP3/FLAC, preferencias con seis pestañas, atajos configurables, recientes, recuperación y actualizaciones firmadas. La interfaz y guía F1 están en español e inglés.

Importe música con Ctrl+I, cree pista con Ctrl+Mayús+N, nombre con F2, arme con A y grabe/detenga con R. Use auriculares. Ctrl+G ajusta volumen/panorama, Ctrl+E abre efectos y Ctrl+Mayús+E exporta. Conserve los originales y carpetas de audio de data. F8 anuncia los niveles y la saturación. Pruebe la entrada en Transporte > Probar micrófono y active la monitorización con auriculares si la necesita. El desfase se ajusta manualmente en Preferencias. Siguen pendientes pruebas físicas y con NVDA/JAWS; la accesibilidad no se considera terminada.

## License

Sonitrelix's original code is MIT licensed. Dependencies retain their own notices.
