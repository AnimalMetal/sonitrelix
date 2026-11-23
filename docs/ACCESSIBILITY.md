# Keyboard and screen-reader acceptance

Run the current build without a mouse with NVDA and JAWS separately. Record the reader version, Windows version, app language, exact keys, expected and heard output, focus and result. The automated tests cover native names, roles, state and command behavior. A complete reader walkthrough still needs to be recorded.

1. Launch with no project. Confirm focus in Tracks and that the startup status describes the available actions. Tab and Shift+Tab through the track and clip lists, position and selection fields, transport and status. Verify native menu access with Alt and arrow keys.
2. Create two tracks and give them recognizable names. Verify name, format and mute/solo/arm state. Press M, S and A and listen for the changed state. During playback, try Alt+Arrows in Tracks and listen for the new value and uninterrupted sound.
3. Import an instrumental, set exact times and mark a range with [ and ]. Verify F6 transport, F7 selection and that text fields retain ordinary editing keys. Test cut, copy, paste, split and undo/redo. In Export Options, check format-dependent controls, Metadata and Continue. The [multitrack checklist](MULTITRACK.md) covers the full scenario.
4. Select the microphone and headphones independently. Test **Transport > Test recording input**, F8 and Escape. Arm a voice track and record over the backing. Verify elapsed time, clipping, pause/resume, final take placement and opt-in monitoring. Try a missing input and an unplugged USB device on a disposable project.
5. Use Ctrl+E and Ctrl+G. Verify every slider's name, value, Reset button and Tab order. Check effect Preview, Stop preview, Apply, Cancel and Undo.
6. Save and reopen a project in a path with spaces and Unicode. Test invalid time entry and cancel each file dialog. Confirm that prompts are readable and that focus returns where expected.
7. In Preferences, try Automatic, English and Español. Verify localized labels and unchanged position and selection. Read the [Preferences checklist](PREFERENCES.md) for device and validation checks.
8. Open F1, Changelog, About and Licenses. Verify read-only text, section navigation, Ctrl+F/F3, Copy and focus restoration. From F1 open the HTML guide and navigate its headings and shortcut tables. In the update window check that Install, Cancel and Skip announce their results.
9. Repeat critical actions with large text, Windows high contrast and different DPI settings. Include braille where available and run the same workflow on Windows 10 x64.

## Timeline tools

1. Mark a range with `[` and `]`, press F3 and listen through several repeats. F6 should report a position inside the range. Pause and resume with Ctrl+Space and stop with F3. Clear the range and check the F3 prompt.
2. At two different times, use Ctrl+M to name markers. Verify that F9 and F11 report the correct names and times and that the markers survive saving and reopening the project. Test Transport > Manage markers and undo a deletion.
3. On a copy of a two-track project, arm the track containing an existing take. Mark part of it and use Ctrl+Shift+R. Check the pre-roll and recording status, automatic stop, original audio outside the range, unchanged clip timing, and Ctrl+Z.
4. Select the second of two adjacent audio clips in Clips and press Enter. Use Ctrl+Shift+F and check the status announcement. The later clips on this track move earlier by up to 20 ms. Undo and verify that the old join and timing return.
5. With speakers and microphone in a quiet room or a wired loopback, open Tools > Calibrate recording offset. Confirm once, listen for the result dialog and verify that accepting saves the displayed value. Test Escape and a disconnected audio path. Neither should change the manual setting.

## Español

Realice la prueba sin ratón con NVDA y JAWS por separado. Importe música, grabe voz en otra pista, consulte posición y niveles, aplique un efecto y exporte la mezcla. Anote las versiones, el idioma, las teclas, lo anunciado, el foco y el resultado. Consulte la [guía en español](user-guide.es.html) para los pasos y atajos.
