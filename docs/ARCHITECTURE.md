# Additions - 0.6.0

The mixer crate builds immutable mixed timelines. Worker-side windowed-sinc resampling converts track rates. StartGate coordinates capture/output starts and resumes; latency offset remains manual. Media Foundation converts compressed imports to WAV and encodes MP3; WAV/FLAC encoding is original. Schema 4 protects original compressed paths. Maintenance workers save recovery/recents. The updater verifies Ed25519 manifests and downloads, replaces only the EXE and rolls back failed startup. See MULTITRACK.md and UPDATES.md.

## Historical notes

# Architecture decisions — current build 0.5.0

## Reference review

Reviewed 2026-11-20: [Elion README](https://github.com/Elion-Daw/Elion), [BUILDING](https://github.com/Elion-Daw/Elion/blob/main/BUILDING.md), [LICENSES](https://github.com/Elion-Daw/Elion/blob/main/LICENSES.md). Its principles support short, discoverable keyboard workflows and non-destructive editing. Its documented current platform is macOS/VoiceOver, with Windows readers planned; its UI is not evidence that a Windows port would satisfy NVDA/JAWS. Elion uses C++ JUCE/Tracktion, pinned submodules and reviewed patches. Sonitrelix uses none of its implementation or dependencies.

## UI choice

Direct Win32 standard controls: menu, BUTTON, LISTBOX, EDIT, STATIC, and common file dialogs. These have native Windows accessibility proxies; labels immediately precede fields under the same parent. [Microsoft documents this naming requirement](https://learn.microsoft.com/en-us/windows/win32/winauto/ensure-that-ui-elements-are-named-correctly). This is a credible MSAA/UIA bridge plan, not an accessibility certification. No custom synthesizer or screen-reader add-on. Track rows include each stored state in their text.

The UI retains its original small `extern system` adapter; ABI sizes and native MSAA control names/roles have tests. Audio now uses maintained Microsoft `windows` 0.62.2 bindings, which became available in the local cache. Prefer those bindings for future COM/UIA work. Custom waveform rendering must remain an optional visual projection of a keyboard-editable timeline; it must never be the sole editing interface.

Use system colors, native focus and a system-DPI-aware layout. Minimum window size prevents controls collapsing. Per-monitor DPI, fonts/text scaling, high contrast, long translations, every Alt mnemonic, and actual UIA names need manual validation before release. F1 uses a modeless native read-only text viewer with section navigation, UTF-16-safe case-insensitive search and restored focus.

## Workspace and ownership

Implemented crates: session (schema-3 source slices and legacy migration), editing (pure frame-range operations), settings (validated preferences/recovery), devices (endpoint discovery), audio-io (bounded WAV/timeline readers and PCM16 writer), engine (WASAPI playback/capture and SPSC queues), windows-ui (native commands, history, translated controls/help/preferences).

Ordered clips are contiguous source slices or silence; all clips on a track share a format. Editing clones metadata, validates the candidate and commits one undo transaction; audio stays immutable. Clip selection retains exact sample boundaries alongside millisecond view fields. History restores project, selected track, cursor and selection.

Preview streams one selected timeline through a background source reader, bounded ring and event-driven WASAPI output. Capture uses a separate event-driven input producer and disk writer consumer; preallocated ring buffers keep disk, decoding, logging and allocations out of packet copy/render paths. Atomics carry stop/pause/progress. The writer checkpoints RIFF headers periodically and finalizes completed frames on stop or recoverable errors. No live monitoring, simultaneous playback/capture, clock drift correction or full crash recovery yet. COM interfaces stay on their creating threads.

UI receives worker results through channels/polling; file decode, discovery, recording writes and preference saves run off the UI thread. Native modal guards prevent project mutations during dialogs. Starting recording stops preview and blocks audio/project mutations until finalization. Closing finalizes capture before prompting about unsaved changes. Assets are retained on undo.

The effects crate now supplies bounded offline processors, including feedback-delay reverb, and renders immutable float32 assets with undoable project replacement. Windows UI has one modal parameter window with worker rendering and preview. A separate WinHTTP release checker reads public GitHub metadata in a worker, then offers the official download page; it never downloads or installs executable files. See EFFECTS_UPDATES.md. Future features: multitrack block mixer with resampling, editable effect chains, additional codecs, mixdown/export, signed automatic installation and project recovery. A manual signed insertion offset is available; measured overdub latency is future work.

## Audio plan and candidate evaluation

Primary upstream repositories were reviewed for license and suitability. Latest-version/transitive maintenance audits must be repeated at the milestone that actually introduces each candidate. Registry networking was unavailable; Microsoft windows-rs was available locally and its resolved notices were retained. CPAL/Symphonia/Hound/Rubato below remain candidates, not linked dependencies.

| Candidate | Upstream license / evaluation |
| --- | --- |
| [CPAL](https://github.com/RustAudio/cpal) | Apache-2.0; established RustAudio device I/O with Windows WASAPI. First candidate; verify timestamp accuracy and supported formats, callback error/disconnect behavior, and bounded latency before committing. ASIO deferred. |
| [Symphonia](https://github.com/pdeljanov/Symphonia) | MPL-2.0; pure Rust decoders with explicit feature gating. First candidate for WAV/MP3/FLAC/Vorbis and later AAC/MP4, with per-codec fixtures and notices. Keep any modifications under MPL requirements. |
| [Hound](https://github.com/ruuda/hound) | Apache-2.0; limited WAV read/write scope is suitable for first PCM export. Its upstream identifies a Codeberg home; verify current release/provenance there before selection. |
| [Rubato](https://github.com/HEnquist/rubato) | MIT/Apache-2.0; prepared block resampling suitable for differing device/project rates. Preallocate all processing; test group delay and channel alignment. |

These are proposals, not installed dependencies or claims of current codec support. MP3/FLAC encoding needs a separate maintained encoder/license choice. Avoid bundling FFmpeg or LAME without reviewing its precise build and obligations.

Workers will decode, resample, stream exports and write recordings. Use bounded SPSC rings with fixed capacities between callback and workers. Capture overruns must become explicit retained errors and stop safely; do not silently discard samples. Playback underruns output silence and increment atomic counters. UI polls state at low frequency and reports user-requested milestones, avoiding speech during capture.

Current playback: the coordinator opens/validates the WAV and owns worker lifecycle. A disk reader decodes blocks into a 32768-frame SPSC ring. A separate WASAPI worker requests shared-mode event callbacks, automatically converts sample rate/channels through Windows, and reads only prepared PCM into the endpoint buffer. It attempts MMCSS Audio scheduling. Control changes signal a Windows event; stale seek requests are superseded by generation. Pause preserves queued audio; seeks restart a session. The render loop performs no disk I/O, mutex locking, PCM allocation, decode, logging or UI announcements. Underruns leave the stream empty so the Windows engine outputs silence. A 250 ms UI timer reads atomic position/state without interrupting editable time fields. F6 provides a readable snapshot that pauses continuous updates while focused.

This follows the documented [WASAPI render/padding model](https://learn.microsoft.com/en-us/windows/win32/coreaudio/rendering-a-stream) and [shared-mode automatic conversion flags](https://learn.microsoft.com/en-us/windows/win32/coreaudio/audclnt-streamflags-xxx-constants). API-level timing checks pass on an actual output with silence. Audible quality, physical routing/disconnection, screen readers and long-duration stress remain manual release gates.

Overdub phase: start an armed capture at the output transport frame; retain input/output timestamps; preserve completed recording chunks on disconnect; place resulting clip using measured/manual input-output offset. Monitoring defaults off. Manual compensation in milliseconds must be editable, persisted and documented before presenting the workflow as usable. Test drift, pause/stop and differing clocks with loopback fixtures and actual devices.

## Milestone gates

1. Foundation implemented; automated checks available. Native-reader verification and polish remain open.
2. WAV import/play/pause/stop/seek implemented for selected-track preview, with actual WASAPI silence checks. WAV export and MP3/FLAC import remain unfinished parts of this milestone.
3. Clip model and exact range editing, transactions, source preservation and recovery checkpoints.
4. Synchronized mixing, track volume/pan and export comparison fixtures.
5. Standalone/overdub recording, device switching/disconnection and offset compensation; headphone workflow validation.
6. Numerically editable, previewable amplify/normalize/fades/reverse/EQ/compressor/gate.
7. More encoders, shortcut customization, packaging and performance. Spanish remains required at every gate.

Do not advance a release past a serious keyboard or screen-reader failure. Full acceptance remains the music-plus-vocals scenario in the project brief; it is not satisfied by milestone 1.
