# Effects and updates

## Use an effect

1. Choose a track and optionally select a time range. From Tracks or Clips, press **Ctrl+E** to open Effects.
2. Select an effect and Tab to its sliders. Use arrows for small steps, Page Up or Page Down for larger steps, and each Reset button to restore its default.
3. Preview auditions the first five seconds of the range, plus any reverb tail. Use Stop preview to stop listening; Apply processes the full range, or the whole track when there is no selection. Cancel closes the window and Ctrl+Z undoes a committed effect.

Amplify and Normalize adjust level; Normalize measures the peak first. Fades are linear. Reverse keeps stereo channel order. Silence preserves the range duration. The basic EQ has low, mid and high bands. The compressor and noise gate use linked channels. Reverb uses wet mix, decay and tail sliders. A longer tail extends the selection and moves later audio; a zero tail preserves duration.

Applied effects create new float32 WAV assets in `data/effects`. Preserve them with the project. Undo restores the previous references. Temporary preview files are removed when the Effects window closes. High gains can clip on playback or export. F8 reports playback peaks and clipping. The [FFmpeg guide](FFMPEG.md) covers noise reduction, de-essing, de-hum, tempo and ducking.

For acceptance, verify every parameter's label, value and Reset action, then Preview, Apply, Cancel and Undo with keyboard and screen reader. Compare the affected audio and surrounding clips. Test cancellation on a long selection and check that originals remain intact.

## FastPlugs

Download the Windows x64 CLAP bundle from the [FastPlugs release](https://github.com/masonasons/FastPlugs/releases/tag/latest). Extract `FastPlugs.clap` into `data/plugins` beside your Sonitrelix executable, install it into the standard Windows CLAP folder, or select the file when prompted by **Effects > FastPlugs**.

Select a track and the time range to process. Choose a plug-in and open Parameters to read or enter its own value text, such as `8 dB`. Preview auditions up to five seconds. Apply renders the selected range, or the whole track when no range is active. Output length is unchanged and plug-in latency is compensated. Keep some silence in the target range if you want to hear a reverb or echo decay. Ctrl+Z restores the previous edit. The two live utilities, Buffer and Clipping Monitor, are outside the offline effect list.

## Check for updates

In a portable build, **Help > Check for updates** opens the update result. **Install update** verifies signed metadata and the executable before replacing it and restarting. **Cancel transfer** stops the download. **Skip this version** saves a preference for automatic checks, and a manual check can offer the same version again. **Open download page** opens the complete Windows ZIP when you need a full package.

The signed updater preserves the data folder and rolls back if startup fails. See [signed portable updates](UPDATES.md) for the release process and scope. Live installation to 0.6.4 passed from the published 0.6.3 portable build.

## Español

Ctrl+E abre Efectos. Ajuste los deslizadores con las flechas y use Restablecer para recuperar el valor inicial. Puede escuchar el resultado, aplicar o cancelar. Ctrl+Z deshace el efecto. Conserve los archivos de `data/effects`.

Ayuda > Buscar actualizaciones permite descargar, verificar e instalar una versión portable más reciente. La actualización conserva la carpeta de datos y recupera el ejecutable anterior si falla el arranque.
