# Contributing

Priorities: accessibility, reliable audio, accurate editing, clear keyboard workflows, performance, then advanced features.

Keep UI and unsafe Win32 calls in windows-ui. Keep session operations free of native UI and audio devices. Every implemented command needs keyboard operation and localized English/Spanish labels. Do not add cosmetic buttons for future features.

Run format, Clippy, and the workspace tests listed in BUILDING.md. Add regression tests for timing, editing, persistence, audio or recovery changes. Tests must use temporary directories and must not require audio hardware or Internet. Record manual NVDA and JAWS results separately with versions, steps, expected announcements and actual results; API or mock tests do not establish screen-reader support.

Future audio callbacks must not allocate, lock, decode, perform I/O, log, or announce. New dependencies require a maintenance review, feature selection, license inventory and retained notices before inclusion. Do not copy Elion AGPL source into the MIT codebase.

Report accessibility failures with the affected task, exact keyboard steps, reader/version and observed focus or announcement. Report crashes with a redacted local session log; do not post private paths or recordings.
