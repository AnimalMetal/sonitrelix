# Keyboard shortcuts / Atajos — 0.4.0

Audio and track commands act on the selected track when **Tracks or Clips has focus**. In text fields, ordinary typing and Ctrl+A/C/X/V/Z keep Windows text-editing behavior. Ctrl+A never selects all tracks. The clipboard stores Sonitrelix clips internally; it is not the Windows audio clipboard.

| Keys | English | Español |
| --- | --- | --- |
| Ctrl+N / Ctrl+O | New / open project | Nuevo / abrir proyecto |
| Ctrl+S / Ctrl+Shift+S | Save / Save as | Guardar / Guardar como |
| Ctrl+I | Import WAV | Importar WAV |
| Ctrl+, | Tools > Preferences | Herramientas > Preferencias |
| F1 | User guide | Guía |
| F6 / F7 | Focus transport snapshot / selection | Enfocar transporte / selección |
| Ctrl+Z / Ctrl+Y | Undo / redo outside text fields | Deshacer / rehacer fuera de texto |
| Ctrl+Shift+N | New empty track | Nueva pista vacía |
| Up / Down | Select track or clip in focused list | Elegir pista o fragmento |
| Tab / Shift+Tab | Next / previous control | Control siguiente / anterior |
| Ctrl+A | Select all audio on this track | Seleccionar todo el audio de esta pista |
| Ctrl+C / Ctrl+X / Ctrl+V | Copy / cut / paste audio | Copiar / cortar / pegar audio |
| Ctrl+A then Ctrl+X | Cut all audio on selected track | Cortar todo el audio de la pista elegida |
| Delete | Delete selected audio, closing the gap | Eliminar selección cerrando hueco |
| Ctrl+T / Ctrl+Shift+T | Split at cursor / trim to selection | Dividir / conservar selección |
| Ctrl+L | Silence selection, retaining duration | Silenciar selección conservando duración |
| Ctrl+Shift+I | Insert selection-duration silence, or 1 second | Insertar silencio de duración seleccionada, o 1 segundo |
| Left / Right | Move cursor by small step | Mover cursor por paso pequeño |
| Ctrl+Left / Ctrl+Right | Move cursor by large step | Mover cursor por paso grande |
| Shift+Left / Shift+Right | Extend selection by small step | Ampliar selección por paso pequeño |
| Ctrl+Shift+Left / Ctrl+Shift+Right | Extend selection by large step | Ampliar selección por paso grande |
| Home / End | Track start / end | Inicio / fin de pista |
| Shift+Home / Shift+End | Extend selection to start / end | Ampliar selección hasta inicio / fin |
| Escape | Clear audio selection | Quitar selección de audio |
| F2 | Edit track name; Enter commits, Escape cancels | Renombrar; Intro confirma, Escape cancela |
| Ctrl+Shift+D | Duplicate track | Duplicar pista |
| Alt+Shift+Up / Alt+Shift+Down | Move track up / down | Subir / bajar pista |
| Ctrl+Shift+Delete | Delete track | Eliminar pista |
| M / S / A | Toggle mute / solo metadata / recording arm | Alternar silencio / dato solo / armado |
| R | Start/stop standalone recording on armed track | Iniciar/detener grabación independiente en pista armada |
| Space | Play/stop selected track; stop recording if active | Reproducir/detener; detener grabación activa |
| Ctrl+Space | Pause/resume playback or recording | Pausar/continuar reproducción o grabación |

Space/Ctrl+Space also work in the F6 transport readout. Use named transport buttons from elsewhere. Track letters never trigger while typing a name or a time. Enter in Clips selects the whole clip, retaining exact sample boundaries.

Preferences: Ctrl+Tab / Ctrl+Shift+Tab changes category, Tab traverses controls, Escape cancels. F1 / Ctrl+H opens category help. Guide: Ctrl+F focuses Find; F3 finds next; Enter searches from Find or opens a selected section; Escape closes.

Preferencias: Ctrl+Tab / Ctrl+Mayús+Tab cambia categoría; Tab recorre controles y Escape cancela. F1 / Ctrl+H abre ayuda de categoría. Guía: Ctrl+F busca, F3 siguiente, Intro busca o lee sección y Escape cierra.

Shortcuts are fixed in 0.4.0; customization is pending. Solo, gain and pan are not connected to a mixer. Recording stops playback; synchronized overdubs and monitoring are pending.

Help > User guide (F1), Changelog and About all open a read-only text viewer with focus in the content. Arrow keys/Page Up/Page Down navigate, Ctrl+A selects all text and Ctrl+C copies. Ctrl+F searches; F3 finds next. Tab reaches section/search controls. Escape closes and returns to the original main control. No audio shortcuts run in this viewer.

Ayuda > Guía (F1), Registro de cambios y Acerca de abren texto de solo lectura con el foco en el contenido. Flechas/RePág/AvPág recorren el texto; Ctrl+A selecciona todo y Ctrl+C copia. Ctrl+F busca y F3 siguiente. Tab llega a secciones/búsqueda; Escape cierra y devuelve el foco.