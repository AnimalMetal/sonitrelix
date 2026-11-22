# Playback and WAV support

## Keyboard workflow

1. Open **Tools > Preferences > Audio devices**. Choose the output or **Follow Windows multimedia default**, then Apply.
2. Press **Ctrl+I** to import audio. Each import opens in its own track. See [FFmpeg formats and effects](FFMPEG.md) for compressed formats.
3. From Tracks or Clips, **Space** plays or stops the audible project mix. **Ctrl+Space** pauses or resumes. Selecting another track leaves the mix playing. **M** and **S** change mute and solo during playback. In Tracks, **Alt+Up/Down** and **Alt+Left/Right** change the selected track's volume and pan.
4. **F6** focuses the transport snapshot. Its time stops updating while it has focus so you can read it. Press F6 again to refresh. **F8** announces peak level and clipping without moving focus.
5. Use the arrows and configured steps for cursor movement. Enter an exact time such as `00:00:05.250` in **Position**, then choose **Apply times**. An active stream seeks to that position. Ctrl+A/C/X/V and Space retain their normal behavior in text fields.
6. Save with **Ctrl+S** and reopen with **Ctrl+O**. Keep the referenced assets in place.

Changing output or buffer settings stops playback. The next Play opens the selected output. A missing named endpoint produces an error. Select another output or reconnect the device and try Play again.

## Native WAV and output limits

The native reader handles mono/stereo little-endian RIFF/WAVE from 8000 to 192000 Hz, integer PCM 8/16/24/32-bit and float32. Supported extensible variants use standard mono/stereo masks or an unspecified mask and matching valid/container bits. The reader skips supported metadata chunks, handles odd-byte padding and rejects truncated or invalid data. It turns non-finite float samples into silence and clamps source float samples to full scale. The [FFmpeg adapter](FFMPEG.md) provides additional format coverage where supported.

WASAPI shared mode renders prepared float PCM through a fixed-size ring. Disk reads, decoding and resampling run on workers. The requested buffer can differ from the actual WASAPI buffer. Enable detailed transport status to see it and the underrun count in F6. The readout estimates position from submitted frames and WASAPI padding. Seeks reopen the stream and may leave a brief gap.

## Validation

Run the [manual accessibility checks](ACCESSIBILITY.md) with mono and stereo fixtures, a second track at another sample rate and your selected headphones. Verify keyboard seeking and pause, live mute/solo and level changes, F6/F8 feedback, and intact sources after editing and saving. Try an unplugged USB output, malformed WAV and a long local file. The automated WASAPI test uses silence to check endpoint routing by ID, position, pause, seeking and shutdown. Listening, physical device removal and long sessions still need hands-on checks.

## Español

Ctrl+I importa audio y Espacio reproduce o detiene la mezcla audible. Ctrl+Espacio pausa o continúa. F6 abre la lectura de posición y F8 anuncia el nivel pico y la saturación. Elija la salida en Preferencias. Si falta la salida seleccionada, reconéctela o elija otra antes de reproducir. Consulte la [guía completa](user-guide.es.html).
