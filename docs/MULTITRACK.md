# Multitrack workflow — 0.6.0

Use headphones and a disposable project. Select input/output independently in Tools > Preferences. Recording channels must match an existing target track. Live microphone monitoring is unavailable.

1. Ctrl+I imports instrumental music. Ctrl+Shift+N adds a track; F2 names it Vocals; A arms it. Home moves to its start.
2. R records while the other audible tracks play. Ctrl+Space pauses/resumes both streams. R or Space stops. Completed capture replaces its interval without moving later audio.
3. Listen to alignment. Adjust the manual recording offset in Recording/playback preferences; negative values place capture earlier. Repeat a short click/clap test. Coordinated stream starts do not measure actual hardware latency or drift.
4. Ctrl+G adjusts volume/pan. M/S toggles mute/solo. Space plays the audible project mix, including tracks with different sample rates.
5. Enter exact selection times. Ctrl+A selects the whole selected track; Ctrl+C/X/V, Delete, split and trim edit it. Ctrl+E previews/applies effects, including reverb. Undo restores previous audio references.
6. Preferences > Export selects rate, channels and MP3 bitrate. Ctrl+Shift+E exports WAV, FLAC or MP3. Reopen and compare the result. MP3 includes possible encoder priming/padding; use WAV/FLAC for exact duration.
7. Save/reopen and verify names, state, boundaries and mix. Keep originals and data/imports and data/recordings: projects reference assets rather than embedding them.
8. Leave unsaved edits for 30 seconds, terminate only a disposable instance, then reopen and recover its metadata checkpoint. Incomplete recordings and power loss are not guaranteed recoverable.

Repeat without a mouse using NVDA and then JAWS. Verify names, values, focus, errors, transport, numeric selection, effects, export and recovery. Test real microphone removal, pause/resume alignment, long sessions, high contrast, enlarged text and Windows 10 separately. Serious reader failures block release. Those manual results remain pending.

Clips are consecutive regions with explicit silence gaps, not freely overlapping objects. Compressed imports convert to WAV in a background worker; this is not streaming compressed-file editing.

## Español

Importe música con Ctrl+I, cree pista con Ctrl+Mayús+N, nombre con F2, arme con A y grabe/detenga con R. Use auriculares. Ctrl+Espacio pausa ambos flujos. Ajuste el desfase manual y compruebe la alineación escuchando. Ctrl+G ajusta volumen/panorama, Ctrl+E abre efectos y Ctrl+Mayús+E exporta. Conserve los originales y las carpetas de audio de data. Pruebe todo sin ratón con NVDA y JAWS. La recuperación guarda metadatos cada 30 segundos; no garantiza grabaciones incompletas. Faltan pruebas reales de hardware y lectores.
