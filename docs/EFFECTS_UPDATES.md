# Effects and updates — 0.5.0

## Use an effect

1. Choose a track. Select a time range if wanted; without a range, the effect uses the whole selected track.
2. In Tracks or Clips press Ctrl+E, or open Effects > Effects.
3. Choose an effect with the arrow keys. Tab to its numeric settings, then Preview or Apply. Preview processes the first five seconds plus the reverb tail. Stop preview stops listening. Cancel cancels processing and closes. Ctrl+Z undoes Apply.

Amplify uses gain -60 to +24 dB. Normalize uses peak target -60 to 0 dB and scans the selected audio first. Fades are linear. Reverse preserves channel ordering. Silence preserves duration. Basic EQ uses smooth low/mid/high bands split at about 200/4000 Hz, each -24 to +24 dB. Compressor uses a linked-channel peak envelope, fixed 5 ms attack/100 ms release, threshold -80 to 0 dB, ratio 1–20 and makeup 0–24 dB. Gate uses a linked peak detector, threshold -100 to 0 dB and configurable attack/release. These are basic processors, not mastering tools.

Reverb uses four feedback delay lines per channel. Wet mix 0–100%, decay 0.1–10 seconds and appended tail 0–10 seconds. Added tail extends the selected audio and moves later audio; set tail to zero to preserve duration. This is an offline rendered effect. Input monitoring and real-time recording effects are not implemented.

Rendering streams bounded blocks to a newly created float32 WAV with a fact chunk in data/effects. Keep those files with the project. Original imports/recordings remain unchanged. Undo restores the original source references; it does not delete generated assets. Preview files are also retained in this folder, so repeated previews consume disk space. High gains may exceed full scale and clip on output; normalize or reduce gain. No automatic limiter is applied.

## Test effects with keyboard and screen readers

Use disposable copies, first with NVDA, then JAWS. Import a short mono WAV and a stereo WAV. Ctrl+A, Ctrl+E should open a named effect selector; Tab reads each parameter label/value and Preview, Stop preview, Apply, Cancel, and read-only Status. Verify every effect's valid defaults, invalid/NaN/out-of-range values, decimal entry, keyboard dropdown and Escape behavior. Text fields must keep their editing keys.

Preview each effect, compare listening at safe volume, Apply, undo/redo, save/reopen and preview again. Verify audio outside the selected interval is unchanged. Check reverse channel order, fade endpoints, normalized peak, compressor/gate transitions, reverb tail length and stereo behavior. Set reverb wet/tail to zero for a duration-preserving dry comparison. With a long selection, cancel during rendering and verify the UI remains responsive and the project unchanged. Test unwritable/full disk and source disconnection, focus retention, high contrast, DPI and English/Spanish labels. Actual NVDA/JAWS and listening checks remain pending.

## Check for updates

Help > Check for updates runs a background HTTPS query to the fixed GitHub repository. It compares numeric stable versions, refuses draft/prerelease results and requires a published Windows ZIP on a newer release. Its named read-only result supports reading/copy/search. Open download page opens the fixed official release page in the browser. No executable is downloaded, extracted or installed by the application.

The request uses Windows WinHTTP with certificate validation, system proxy discovery, timeouts, no redirects and a 256 KiB response limit. It sends the application version in User-Agent; it uses no GitHub token. A missing/private repository and no published release can both produce 404. Network errors and rate limits are shown as failures, not as “up to date.” A development version newer than the latest release is never offered a downgrade. Results are not saved as a trusted anti-rollback floor.

Test no releases, same/older version, newer release with the expected ZIP, missing ZIP, malformed versions, oversized response, invalid JSON, rate limits and offline errors. Verify language changes, text focus and download-page keyboard access. Parser/native-control tests pass locally; live HTTPS, browser routing, first tagged release publishing and an end-to-end update remain unverified here because network access is restricted.

## Automatic installation remains pending

This is a release checker and manual-download flow. It is not a self-updater. Automatic replacement needs an embedded trusted public key and signed manifests, package validation, safe extraction, data preservation, backup/health confirmation and tested rollback. No executable replacement is allowed without that trust path. No private signing key has been generated or committed. The updater skill's rule is “Sin firma válida, sin claves configuradas ... se rechaza.” It informed the decision to keep this stage read-only.

References: [GitHub Releases API](https://docs.github.com/en/rest/releases/releases#get-the-latest-release), [WinHTTP request API](https://learn.microsoft.com/en-us/windows/win32/api/winhttp/nf-winhttp-winhttpopenrequest), [WinHTTP timeouts](https://learn.microsoft.com/en-us/windows/win32/api/winhttp/nf-winhttp-winhttpsettimeouts), [GitHub release publishing](https://cli.github.com/manual/gh_release_create).

## Español

Seleccione una pista y, si quiere, un intervalo. Ctrl+E abre Efectos: elija efecto, cambie valores y use Escuchar o Aplicar. Sin intervalo se procesa la pista completa. Ctrl+Z deshace. Reverberación añade una cola configurable que desplaza audio posterior; cero conserva duración. Se guardan WAV nuevos en data/effects; conserve esa carpeta y los originales.

Ayuda > Buscar actualizaciones consulta Releases en segundo plano y muestra texto de solo lectura. Abrir descarga abre la página oficial. La instalación es manual; el programa no descarga ni sustituye ejecutables. Actualización automática con firmas y vuelta atrás sigue pendiente. Pruebas de escucha, NVDA/JAWS, publicación de etiquetas y conexión HTTPS real siguen pendientes.
