# Sonitrelix architecture

## Workspace

| Crate | Responsibility |
| --- | --- |
| `session` | Versioned project files, validation, atomic saving and schema migrations. |
| `settings` | Portable preferences, defaults, validation and backup recovery. |
| `editing` | Non-destructive frame-range operations and the internal audio clipboard. |
| `devices` and `engine` | Windows endpoint discovery, shared-mode WASAPI playback/capture and signal metering. |
| `audio-io` | Bounded WAV/timeline readers, WAV/FLAC output, Windows Media Foundation codecs and the FFmpeg adapter. |
| `mixer` | Audible track selection, live level controls and mixdown/export. |
| `effects` | Built-in offline effects and FFmpeg processing. |
| `updater` | Signed portable download, executable replacement, startup confirmation and rollback. |
| `windows-ui` | Win32 controls, project commands, worker results, preferences, recovery, localized Help and undo/redo. |

## Editing and storage

Each track is an ordered sequence of source slices and explicit silence. Edits validate a candidate before committing the change to undo history. Frame positions preserve sample boundaries alongside the millisecond fields. The version 5 [project schema](PROJECT_FORMAT.md) stores named markers and references to protected source assets. Imports and recordings remain separate files. Metadata recovery checkpoints are written while the project is dirty.

Effects render bounded blocks to new assets, with preview and undo. Compressed imports decode to WAV in a worker. Playback and export mix tracks on a common timeline, with worker-side resampling. The Win32 app uses Windows Media Foundation and a separate bundled FFmpeg process. See [third-party notices](../THIRD_PARTY_NOTICES.md) and [FFmpeg build details](FFMPEG_BUILD.md).

## Playback and capture

The playback coordinator owns worker lifecycle. A decoder feeds a 32768-frame ring. The event-driven WASAPI render worker consumes prepared PCM and records peak and underrun counters. File I/O, decoding, allocation, locking and UI announcements stay outside the render loop. Windows handles endpoint conversion. Seek requests supersede stale sessions; pause preserves queued playback. Live mute, solo and track levels update the mix on the decode worker.

WASAPI capture writes to a separate bounded ring that the disk worker drains to PCM16 WAV. Capture and backing playback coordinate start and resume through a shared gate. The manual offset controls placement. The recording worker checkpoints headers and finalizes completed audio after recoverable errors. Import can recover whole frames beyond the last checkpoint from an interrupted recording with a readable header.

The opt-in microphone monitor uses its own bounded queue and shared-mode output. A slow or unavailable monitor can drop monitoring audio or stop without blocking capture. The microphone test uses the capture endpoint for metering and creates no recording. Peaks are captured on audio threads and reported on demand by the UI. Clock drift correction and automatic acoustic latency measurement remain open.

## Windows UI and updates

Win32 menus, lists, edit boxes, sliders, property sheets and common dialogs provide native keyboard and accessibility semantics. Labels precede their controls. The UI reads state at intervals and uses announcements for user actions and important events. F1 opens a modeless, searchable read-only guide with embedded English and Spanish HTML. Use the [accessibility checklist](ACCESSIBILITY.md) for actual reader validation.

WinHTTP checks GitHub for newer stable releases. The [portable updater](UPDATES.md) verifies signed metadata and the downloaded executable. A helper replaces the executable and confirms startup or rolls back. The same portable folder holds application data.
