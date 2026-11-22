# Editing and recording

Sonitrelix edits audio without rewriting source files. It can record a single track or capture vocals while the other audible tracks play. See the [full overdub walkthrough](MULTITRACK.md).

## Keyboard editing

1. Import audio with **Ctrl+I**. Move to Tracks or Clips and choose the track. **Space** plays the project mix.
2. Enter numeric times and **Apply times**, or use **[** and **]** for a range. **F7** reports the selection. **Ctrl+A** selects the whole selected track. **Ctrl+C/X/V** copy, cut or paste audio from Tracks or Clips.
3. To paste at an exact time on an empty track, enter the time in **Position** and apply it. Return to Tracks, clear any selection with **Escape**, then paste. Sonitrelix fills the gap with silence. On a track that has audio, pasting at the cursor shifts later clips on that track. An active range is replaced.
4. **Ctrl+T** splits at the cursor. **Enter** in Clips selects the focused clip. **Ctrl+Shift+T** trims to the selection. **Ctrl+L** silences the range and **Ctrl+Shift+I** inserts silence.
5. **Ctrl+Z/Y** undo and redo. Save and reopen to verify the arrangement. Keep referenced audio with the project. Older supported projects migrate to [schema version 5](PROJECT_FORMAT.md) when saved.

## Microphone and recording

1. In **Tools > Preferences > Audio devices**, choose input and output separately. Set channels, buffering and the manual offset on the **Recording / playback** tab.
2. Choose **Transport > Test microphone**, speak, then press **F8** for the peak and clipping. **Escape** ends the check. Optional headphone monitoring applies during recording and starts off by default.
3. **Ctrl+Shift+N** creates an empty track, **F2** renames it and **A** arms it. **R** starts recording on the selected armed track. With backing audio in the project, the other audible tracks play at the same time. **F6** reports elapsed time and **F8** reports microphone and backing peaks.
4. **Ctrl+Space** pauses or resumes; **R** or **Space** stops and finalizes the take. The take replaces existing audio within its recording interval. Sonitrelix leaves the cursor at the beginning of the take.
5. Recordings are stored in `data/recordings`. After an interrupted take, **Ctrl+I** can import complete PCM16 frames from a recording with a readable header and leave the original intact.

For hands-on testing, try pause/resume, alignment with a short cue, device removal, a long capture and a full or unwritable disk. Verify the selected device, readable errors and preservation of completed audio. Repeat the workflow with NVDA and JAWS.

## Español

Use Ctrl+I para importar, A para armar la pista y R para grabar o detener. Sonarán las otras pistas audibles durante la toma. F6 abre la lectura de tiempo y F8 anuncia niveles y saturación. Puede probar el micrófono desde Transporte y activar la monitorización con auriculares en Preferencias. Consulte la [guía en español](user-guide.es.html) para editar, exportar y recuperar una grabación interrumpida.
