# Preferences / Preferencias — 0.4.0

Open Tools > Preferences, or Ctrl+Comma. Options has been renamed Tools; language and accessibility settings have moved into this dialog.

Five native Windows property-sheet tabs provide standard Ctrl+Tab / Ctrl+Shift+Tab category navigation. Tab/Shift+Tab traverse fields and buttons; Left/Right select tabs when the tab strip has focus. F1 or Ctrl+H opens Preferences help. Escape cancels; if a combo dropdown is open, it closes the dropdown first.

| Category | Settings | Current effect |
| --- | --- | --- |
| General | English/Spanish, new-project sample rate, Restore defaults | Language updates the main window and dialog after successful Apply. Rate affects new projects, not existing ones. Defaults reset all known choices in the dialog; Apply is still required. |
| Audio devices | Independent input/output, follow Windows multimedia default, Refresh | Lists endpoints in a worker and remembers IDs. Play opens the selected output; the default is resolved at each Play. Unavailable saved outputs fail without fallback. Record opens the selected input; unavailable saved inputs fail without fallback. Preferences itself opens no stream. |
| Recording / playback | Mono/stereo, buffer frames, manual offset, monitoring | Buffer size is requested by playback/capture; Windows may enlarge it. Channels and signed offset apply to the next capture. Monitoring is not implemented. |
| Editing / keyboard | Small/large cursor steps, shortcut reference | Left/Right in the main track list move by the small step; Ctrl uses the large step; Shift extends selection from its anchor. Text fields retain their own keys. Shortcut remapping is still pending. |
| Accessibility | Detailed time confirmation, focus Status after Apply times | Changes main-window confirmation detail and optional focus immediately after successful saving. It never disables native control names/values. |

Apply validates all tabs, saves on a background thread, then updates the application without closing. OK follows the same path and closes only after success. Cancel/window close/Escape discard changes since the last successful Apply; they do not undo an earlier Apply. While saving, editing and closing are temporarily disabled until the result is known. A save error leaves the previous preferences active and the dialog open, with the error in its focusable Status fields. Device discovery and file writes do not run in an audio callback or on the UI thread.

Changing output or buffer size stops an active preview; the next Play uses the new choice. Language and accessibility changes keep audio running. For transport shortcuts and WAV limitations see EDITING_RECORDING.md and SHORTCUTS.md.

Limits: small step 1–60000 ms; large step from small step to 3600000 ms; signed offset -2000–2000 ms. Positive offsets place captured audio later, negative earlier, clamped to timeline zero. Recording preferences are not a promise that a given device supports the chosen format.

Settings remain at the existing portable path, `data/preferences.json` beside the executable. The original flat `language`/`verbose` keys migrate in memory into schema 1 sections. The file is updated only after Apply/OK. Defaults and missing keys live in code. Unknown settings survive load/save and default reset. Previous data is retained in `preferences.json.bak`; malformed JSON is copied to a timestamped `.corrupt-*` sibling before recovery. Read failures and future schema versions prevent overwriting that file. User-visible recovery warnings are shown at startup.

## Manual tests for this build

1. Open Preferences with Ctrl+Comma. Visit every tab with Ctrl+Tab and return with Ctrl+Shift+Tab. Read every field and check native roles, names, values, checkbox state and focus using NVDA, then JAWS separately.
2. Audio devices: wait for discovery, select a microphone and a different output, Apply, close and reopen. Choices should persist. Refresh after plugging in a device. A removed saved device must remain labeled unavailable rather than silently changing to another input. No sound or recording should start.
3. General: select Español and Apply. Both windows, all tabs and buttons should translate while your project, track selection and typed times survive. Switch back to English if preferred.
4. Editing/keyboard: choose small 25 ms, large 2500 ms; Apply and close. In the track list, Right adds 25 ms, Ctrl+Right adds 2500 ms; Shift extends selection. Verify time fields. Repeat in a text field: arrows must edit text instead.
5. Enter an invalid small step (0), a large step below the small one, or offset 2001. Apply should save nothing, display an error, select the relevant tab and focus its field. Correct the value and retry.
6. Change a value and Cancel; reopen and verify it did not persist. Apply a value, make another change, Cancel; the applied value must remain. Repeat OK and Escape. After closing, focus returns to the main control from which you opened Preferences.
7. Change the new-project sample rate to 96000, Apply, create a project and save it. The JSON project sample rate should be 96000. Opening an existing 48000 project must keep 48000.
8. Enable focus Status after Apply times. Apply and close; use Apply times in the main window and verify focus moves to the readable result. Disable and verify focus stays on the button. Detailed confirmation must change available information without affecting labels.
9. Restore defaults, decline confirmation and check values remain. Confirm it, then Cancel and check saved settings remain. Repeat and Apply to retain defaults.
10. Test at 100/150/200% DPI, increased text size, and high contrast. Verify translated tab rows, focused controls and full field labels remain usable. Report clipping or missing announcements as blockers.

## Español

Abra Herramientas > Preferencias con Ctrl+Coma. Ctrl+Tab cambia de categoría, Tab recorre ajustes y botones, Aplicar guarda sin cerrar y Aceptar guarda y cierra. Cancelar descarta únicamente lo que no se haya aplicado. Las pestañas son General, Dispositivos de audio, Grabación/reproducción, Edición/teclado y Accesibilidad. La salida y el búfer se usan para escuchar WAV; cambiarlos detiene la escucha actual. Entrada, canales y desplazamiento se usan para la siguiente captura independiente. Aún no hay monitorización ni grabación sobre música.

Pruebe la lectura de etiquetas, valores y estados con su lector, la conservación de entrada/salida al reabrir, el cambio de idioma, valores inválidos, Cancelar/Aplicar, el retorno del foco y los pasos de cursor. La validación automática de MSAA no sustituye estas pruebas reales.
