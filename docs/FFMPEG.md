# FFmpeg import, export and effects

The official Windows ZIP includes ffmpeg.exe beside Sonitrelix.exe. If you build Sonitrelix yourself, you can install a Windows build from the [FFmpeg downloads page](https://ffmpeg.org/download.html#build-windows) beside Sonitrelix.exe or on Windows PATH. A replacement build needs the native AAC encoder, libopus and libmp3lame for all the export options. Keep any DLLs supplied by that build alongside ffmpeg.exe.

- Ctrl+I tries the built-in decoders first. FFmpeg handles additional local audio formats such as Ogg Vorbis, Opus and AIFF, and extracts the first audio stream from supported video files such as MP4. A video with no audio produces an import error.
- Ctrl+Shift+E offers AAC in M4A at 192 kbps and Opus at 128 kbps and 48 kHz with FFmpeg available. MP3 uses FFmpeg if Windows cannot initialize its encoder for the requested settings.
- Escape cancels an active import or export from the main window. Converted audio goes into the project’s normal audio workflow. Export renders the mix into a temporary WAV before encoding, so allow room for that file. Original audio and the existing export destination are preserved if conversion fails.

## Vocal and tempo effects

Open **Effects** with Ctrl+E and use Preview before applying. These effects work on the selection or the whole track when you leave the selection empty. Ctrl+Z undoes Apply.

| Effect | What it does |
| --- | --- |
| Noise Reduction | Estimates steady noise from quieter parts of the selected audio. Include a brief pause with the same background noise and use the strength slider or enter a value in decibels. |
| De-esser | Softens sharp S and SH sounds. Adjust strength from 0 to 100 percent. |
| De-hum | Targets 50 or 60 Hz electrical hum and its next two harmonics. |
| Change Tempo | Changes speed while preserving pitch. Changing the clip length moves later audio on the edited track. |
| Automatic Ducking | Lowers the selected backing track when another track contains voice at the same point on the timeline. |

For ducking, select the **backing track**, open Effects and choose Automatic Ducking. Tab to the controlling voice track and choose the vocal. Preview auditions the processed backing alone. Apply renders the change as an undoable edit to the backing track. Play the project to hear it together with the voice.

Each numerical effect setting has a slider and a number field. Tab to a slider and use the arrow keys for small moves or Page Up and Page Down for larger moves. Use the number field for exact decimals.

## Español

El ZIP oficial para Windows incluye ffmpeg.exe junto a Sonitrelix.exe. Si compila Sonitrelix, puede instalar una versión de la [página de descargas de FFmpeg](https://ffmpeg.org/download.html#build-windows). Coloque ffmpeg.exe junto a Sonitrelix.exe o añada su carpeta a PATH. Conserve junto a FFmpeg las DLL de la compilación elegida.

Ctrl+I puede importar Ogg Vorbis, Opus, AIFF y extraer la primera pista de audio de vídeos admitidos como MP4. Ctrl+Mayús+E permite exportar M4A/AAC y Opus. FFmpeg también se usa si Windows no puede iniciar su codificador MP3 con los ajustes elegidos. Escape cancela la operación activa en la ventana principal.

En Efectos (Ctrl+E) puede reducir ruido de fondo, suavizar sibilancias, eliminar zumbido de 50 o 60 Hz, cambiar tempo sin cambiar el tono y atenuar automáticamente el acompañamiento con la voz. Para atenuar, seleccione el acompañamiento y elija la pista de voz dentro del efecto. La escucha previa reproduce el acompañamiento procesado. Aplicar se puede deshacer con Ctrl+Z. Cambiar tempo puede desplazar el audio posterior en la pista editada.

La reducción de ruido estima el ruido constante a partir de los momentos más silenciosos del intervalo. Incluya una breve pausa con ese mismo ruido de fondo. Pulse Tab hasta un deslizador y use las flechas para ajustar un valor. El campo numérico permite escribir decimales con precisión.
