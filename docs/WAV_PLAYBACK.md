> Historical 0.3 playback milestone. Current 0.4 editing/recording behavior and tests are in [EDITING_RECORDING.md](EDITING_RECORDING.md) and [SHORTCUTS.md](SHORTCUTS.md).

# WAV playback — 0.3.0

This milestone previews **one selected WAV track at a time**, starting at timeline zero for that file. It is not synchronized multitrack mixing. Source files are read-only references; no file is rewritten by import or playback.

## Keyboard workflow

1. Tools > Preferences > Audio devices: choose the output device, or Follow Windows multimedia default. Apply and close. Input selection is retained for future recording and does not capture anything.
2. Ctrl+I or File > Import WAV audio opens the native file dialog. Choose a WAV. Header inspection runs in a worker; its new track row includes mono/stereo and duration. No audio starts automatically.
3. Select the track with Up/Down and press Space. Space stops while the list or F6 readout has focus; Ctrl+Space pauses/resumes from those controls. Transport menu and native buttons provide the same actions. Pressing Play on a paused selected track resumes it; pressing Play on another row switches the preview to that row.
4. F6 focuses the read-only playback position/duration. It contains transport state, active track name, and optionally buffer statistics. Automatic position updates pause while that control is focused so the reader can finish reading; press F6 again for a fresh snapshot. Transport state changes still update it. Space and Ctrl+Space remain available there.
5. In the track list, Home goes to start, End to the file end. Left/Right move by the configured small step; Ctrl uses the large step; Shift extends the selection. While playing, movement begins at the current reported playhead. Selection remains a time range only; it does not yet edit or loop audio.
6. For exact seeking, enter `00:00:05.250` in Position and activate Apply times. Active playback restarts at that position, preserving paused state. Seeking past the source clamps to the end. The editable Position is the editing cursor; the separate playback readout follows sound. Ordinary text fields keep their normal keys and Space.
7. Ctrl+S saves the project and its source references. Ctrl+O reopens projects. Keep WAV files in their original locations: assets are not bundled or copied yet. Undo/redo can remove/restore an import. Changing track metadata or undoing stops playback to keep the preview consistent.

Stop retains the last reported playhead in the cursor. Play at the file end restarts at zero. Editing track metadata stops the preview; muted tracks cannot be played. Solo, arm, volume and pan are not yet connected to a mixer. Browsing other track rows while sound plays does not switch it automatically; pressing Play explicitly selects the new source.

Output choice and buffer preference changes in Preferences stop an active preview; the next Play opens the new selection. The empty endpoint ID resolves the Windows multimedia default at each Play. A running stream stays on its opened endpoint. A missing named endpoint is an error with no fallback to speakers or another output.

## Implemented formats and limits

- Little-endian RIFF/WAVE, one data chunk, mono or stereo, 8000–192000 Hz.
- Integer PCM 8/16/24/32-bit and IEEE float32. Supported WAVE_FORMAT_EXTENSIBLE variants require matching valid/container bits, standard mono/stereo masks or unspecified mask, and PCM/float subformat GUIDs.
- Compressed WAV, multichannel, RF64, big-endian RIFX and float64 are rejected. MP3/FLAC/Vorbis/AAC and all audio export remain unimplemented.
- Unknown metadata chunks and odd-byte padding are skipped by seeking. Header chunk count is bounded. Invalid lengths/alignment and truncated data produce errors. Float NaN/infinity become silence; over-range float samples clamp to [-1,1].

The decoder processes at most 8192 frames per block using bounded raw/float scratch buffers. The ring holds 32768 frames (about 256 KiB of stereo float PCM); WAV duration does not change that allocation. A virtual 2 GB WAV test seeks near its end and reads a small block without allocating the file. Actual network/removable disk stress and hours-long physical playback have not been measured.

Windows shared-mode WASAPI converts file-rate/channel float audio to the endpoint mix format. The configured buffer size is a request; Windows may enlarge it. F6 reports the actual client buffer size. Event-driven rendering reads a preallocated SPSC ring and calls the WASAPI buffer APIs; disk I/O and decoding run on another worker. The render loop does not lock, allocate PCM storage, decode, log or speak. It attempts MMCSS Audio scheduling and continues if unavailable. On starvation it leaves the stream empty, allowing the Windows engine to supply silence, and counts underrun episodes. Playback position then advances only for submitted source frames, not the intervening silence.

Position is source frames submitted minus current WASAPI padding, converted with integer arithmetic. It is an approximate output position updated every 250 ms in the UI; it is not acoustic latency measurement. Overdub timing/latency compensation is still unimplemented. Seeks currently reopen the streaming session and can have a brief gap.

## Automated and real-output tests

```powershell
cargo test --workspace --locked
cargo test -p sonitrelix-engine --locked -- --ignored --nocapture
```

The explicit integration test opens the default multimedia endpoint by its **specific endpoint ID**, streams silent 44.1 kHz stereo and 48 kHz mono fixtures, checks progress, pause stability, seeking, exact final frame, invalid-device rejection and shutdown. It does not record a microphone or demonstrate audible fidelity, correct physical speaker routing or screen-reader announcements.

## Manual NVDA/JAWS and listening checklist

1. Use known mono and stereo WAVs with audible speech/music. Test NVDA and JAWS separately, recording versions and results. Import solely by keyboard and verify names, format, duration and focus. Choose headphones and verify sound comes from the chosen physical output.
2. Space play/stop; Ctrl+Space pause/resume; Transport menu and buttons. Verify paused sound does not restart at zero and keyboard navigation remains responsive during playback.
3. F6: verify role, full name/value, position/duration and state. Leave it focused: no continuous speech flooding. Refresh with F6. Stop/resume from it and check the state changes. Return with Tab/Shift+Tab.
4. Seek numerically to a recognizable point, then Home/End and configured steps. Confirm audible timing; test while paused and playing. Type spaces/arrows in track-name/time fields to verify transport does not steal text keys.
5. Import a second WAV. Selecting its row must not interrupt the first until Play is requested; only one track should sound. A muted selected track must not play. Test Unicode names and paths, undo/redo of import, save/reopen/save-as, and verify originals retain their bytes.
6. Switch output in Preferences and play again. Disconnect a USB output while playing; expect a readable error, preserved project/source and recovery after reconnect/selecting output and Play. Physical disconnection has not yet been tested here.
7. Try unsupported/compressed, malformed/truncated and missing WAV files. The interface must remain usable and existing tracks/source files must remain intact. A changed format/duration since import requires re-import instead of trusting stale metadata.
8. Test long local WAVs while browsing controls and changing language. Monitor underrun count and responsiveness. Close during playback and confirm the process exits after stopping its workers.

## Español

Ctrl+I importa un WAV y Ctrl+O abre un proyecto. Seleccione la pista y pulse Espacio para reproducir/detener; Ctrl+Espacio pausa/continúa. F6 enfoca la posición y duración, y vuelve a consultarlas si lo pulsa otra vez. En la lista, Inicio/Fin y las flechas desplazan el cursor y la reproducción; Ctrl usa el paso grande y Mayús extiende la selección. También puede escribir una posición exacta y aplicar tiempos.

Se escucha una sola pista seleccionada, sin mezcla simultánea. La salida de Preferencias se usa al reproducir; si falta, se muestra un error sin cambiar automáticamente de dispositivo. Los WAV originales no se modifican. Todavía no hay edición de audio, grabación, efectos ni exportación. Falta verificar sonido real, rutas físicas, desconexiones y NVDA/JAWS mediante el recorrido anterior.
