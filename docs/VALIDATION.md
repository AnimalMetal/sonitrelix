# User guide follow-up - 0.6.0

The bilingual HTML user guide now progresses from beginner to advanced, with linked contents and six two-column shortcut tables per language. The native F1 text is generated from the same content; Help provides an Open HTML guide action. HTML is self-contained, embedded and refreshed for both languages under data/docs, and copied into portable packages.

Passed the documentation check (heading levels, IDs, links, table headers/captions, translated section/default-key parity, generated text synchronization), all 17 Windows UI tests, formatting and workspace Clippy with warnings denied. New tests cover embedded HTML extraction/update in a Unicode temporary path and coverage of all 22 configurable defaults in both guides. The release build passed.

Browser preview was unavailable: the browser access host was unavailable and Computer Use did not approve Microsoft Edge. Visual layout, browser navigation/printing, end-to-end default-browser opening and actual NVDA/JAWS reading are unverified. No browser security restriction was bypassed. See USER_GUIDE_MAINTENANCE.md for the remaining manual checks.

## Previous validation

# Current validation - 0.6.0

Passed 53 ordinary workspace tests and 3 separately invoked Windows endpoint/engine checks (56 total). Final native UI rerun: 15 passed. Formatting, Clippy with warnings denied and release compilation passed. The release executable reports 0.6.0.

Tests cover mixed-rate/gain/pan timelines, timing gates, WAV/MP3/FLAC export, MP3/FLAC/M4A import fixtures, schema migration/source overwrite protection, recovery, shortcut routing, native preference controls and update signature/tamper/replacement/rollback. Installer tests use disposable executable stubs and Unicode/apostrophe temporary paths, preserve data and refuse to kill the original process. These are not full real-application update acceptance tests.

The 3 hardware checks found 3 input/4 output endpoints, rendered silence and tested invalid input. No microphone was activated. Actual listening, overdub latency/drift, device removal during capture, NVDA/JAWS, keyboard independence, enlarged text/high contrast, Windows 10 and live GitHub delivery remain pending. OGG and raw ADTS AAC import are unverified. MP3 encoder priming/padding is accepted; WAV/FLAC are tested for exact PCM duration. See MULTITRACK.md for manual steps.

The graph has 31 external packages. OpenSSL is statically linked with retained licenses; Windows media codecs are system components. The updated hosted workflow still needs verification. Source changes have not been pushed from this restricted environment.

Reproduce:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test -p sonitrelix-devices -p sonitrelix-engine --locked -- --ignored --nocapture
powershell -NoProfile -ExecutionPolicy Bypass -File tools/build-release.ps1
```

Portable packaging passed ZIP CRC/integrity, marker presence, byte-for-byte EXE matching, SHA-256 and exclusion of data/private keys. The real release manifest was signed offline and verified by the Rust updater against the exact EXE (Ed25519, size and SHA-256). No release was published. A fresh portable EXE reached an input-idle/responding state, logged startup and closed gracefully after WM_CLOSE targeted SonitrelixMain. The generic process helper initially selected an input-method window; no app shutdown defect was observed. This is a process/API smoke check, not screen-reader or independent keyboard validation.

## Historical records

# Validation record — 2026-11-20

## Historical build — 0.5.0

Passed 38 ordinary workspace tests and three explicitly invoked Windows audio integration checks (41 checks total). Format checking, Clippy with warnings denied and release compilation passed.

New tests cover gain, normalization, fades, unity EQ, linked compressor/gate, reverb impulse/tail and chunk continuity, reverse preserving stereo ordering, float32 render roundtrip, cancellation and original-source preservation. Native tests cover all ten effect choices and default parameters, NaN rejection, background Apply, output length/encoding, field names and mnemonic conflicts in English/Spanish. Applying a rendered effect is exercised through project undo/redo. Release fixtures cover numeric versions, draft/prerelease boundaries, rate limits, missing/mismatched package URLs and oversized response rejection. Updates use the tested read-only help viewer.

The three audio integration checks passed on the existing active Windows endpoints with silent playback fixtures and an invalid-input test only; no microphone was activated. Actual effect preview listening, independent keyboard/NVDA/JAWS use, screen layouts at all scales, real HTTPS responses, browser download routing and the new tag-to-Release publisher remain unverified. Automatic installation/signature verification is not implemented. See EFFECTS_UPDATES.md for the current manual checklist and limits.

The external dependency graph remains unchanged; effects are original Rust code and WinHTTP is a Windows system API. No new codec, cryptography or networking package was introduced.

## Historical build — 0.4.0

Passed 33 ordinary workspace tests and three explicitly invoked Windows integration checks (36 total). Formatting, Clippy with warnings denied, and release compilation succeeded on Windows x64 with the existing stable Rust/MSVC toolchain.

Coverage: immutable source offsets through cut/copy/paste/split/trim/silence, empty-track format adoption, exact one-frame selections, save/reopen and versions 1/2 migration, synthetic recorded-clip insertion/undo, bounded timeline streaming across source slices and silence, PCM16 writer roundtrip and no-overwrite, shortcut routing and text-input protection, bilingual strings/mnemonics, real native controls/MSAA names, tabbed preferences, and searchable native help with Unicode offsets.

The Windows checks discovered 3 active inputs and 4 outputs; real output playback used silent fixtures and verified pause, seek including exact frame 52921 at 44.1 kHz, EOF and missing-output rejection. The input test deliberately used a nonexistent endpoint and verified failure without fallback. No microphone was activated or recorded.

Not established: physical capture fidelity, actual input pause/resume or USB disconnection, long recordings/disk-full recovery, headphone routing, audible timing, independent keyboard operation, real NVDA/JAWS announcements, high-DPI/text-scale layout, or crash recovery. Native accessibility API tests are not screen-reader acceptance. Recording is implemented but hardware validation remains pending. Overdub, multitrack mixing, effects and edited-audio export remain unimplemented.

Manual tests: EDITING_RECORDING.md; shortcut reference: SHORTCUTS.md. The external dependency graph remains the same 23 packages and supplied notices as 0.3; the editing crate is original workspace code, with no new external dependency.

Commands:

```powershell
cargo fmt --all -- --check
cargo test --workspace --locked
cargo test -p sonitrelix-devices -p sonitrelix-engine --locked -- --ignored --nocapture
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --release --workspace --locked
```

## Historical build — 0.3.0

Passed 22 ordinary workspace tests and the two explicitly invoked hardware integration tests (24 total), with Clippy warnings denied and format checking clean. Release compilation succeeded with the same stable Rust/MSVC environment.

New coverage includes integer PCM 8/16/24/32-bit and float32 decoding, extensible PCM/float headers, mono/stereo ordering, exact frame seeking, chunk padding, malformed/truncated input, non-finite float sanitization, integer time conversion and bounded reading near the end of a virtual 2 GB WAV. Concurrent queue tests cover ordering, wrap and backpressure. Project tests cover schema-1 migration, schema-2 source references and rejecting project saves over referenced audio or with an audio extension. The native UI test imports asynchronously, checks undo/redo, saves/reopens and verifies source bytes remain unchanged.

The real WASAPI test used a specific active multimedia endpoint ID, silent 44.1 kHz stereo and 48 kHz mono fixtures. Progress, pause stability, seek to frame 52920 (1.2 s at 44.1 kHz), exact EOF frame, invalid-device rejection without fallback and shutdown passed. Endpoint discovery reported 3 active inputs and 4 outputs. No microphone was opened or recorded.

Not established: audible fidelity, physical routing to the intended headset, live USB disconnection, hours-long physical-file playback, independently operated keyboard workflow, NVDA/JAWS announcements or visual layout at all DPI/text sizes. MSAA names/roles are API-level checks only. Audio editing, multitrack mixing, capture and export are not implemented. See WAV_PLAYBACK.md for the current manual checklist.

The actual 23-package external dependency graph was reviewed through Cargo metadata and supplied licenses; windows-rs and its transitive notices were copied into third-party-licenses. CPAL/Symphonia/Hound/Rubato are not linked.

```powershell
cargo fmt --all -- --check
cargo test --workspace --locked
cargo test -p sonitrelix-devices -p sonitrelix-engine --locked -- --ignored --nocapture
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --release --workspace --locked
```

## Historical build — 0.2.0

Passed 13 ordinary workspace tests, plus the separately invoked Windows endpoint integration check (14 passed checks in total). Clippy with warnings denied and the release build passed. The endpoint check reported 3 active inputs and 4 active outputs, without opening any stream or recording audio.

The native property-sheet test creates all five tabs, verifies real controls and MSAA names/roles, checks invalid numerical values, exercises native Apply/OK/Cancel, verifies background save results, language updates, persistence and retaining a selected microphone after synthetic disconnection. Main-window tests cover cursor and anchored selection movement using the configured steps. Settings tests cover legacy migration, unknown-key preservation/default reset, backup recovery and refusing invalid/future-version overwrites.

These tests exercise native APIs and application commands; they are not an independent keyboard walkthrough or a screen-reader listening test. NVDA/JAWS, visual layout at multiple DPI/text sizes, and real device disconnection during streaming remain unverified. Audio streaming is not implemented yet. See PREFERENCES.md for the current manual checklist.

Commands:

```powershell
cargo fmt --all -- --check
cargo test --workspace --locked
cargo test -p sonitrelix-devices --locked -- --ignored --nocapture
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --release --workspace --locked
```

## Historical foundation — 0.1.0

Environment: Windows x64, rustc 1.99.1 (58cf8f9c3), Cargo 1.99.1. No audio dependencies or devices required by this milestone.

Executed successfully:

```powershell
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --release --workspace --locked
```

Seven tests passed: precise time parsing; project round-trip/replacement/future rejection; invalid saves and retained temporary files preserving previous data; embedded English/Spanish key parity; main-window mnemonic uniqueness; Win32 x64 ABI sizes; real hidden native controls handling track actions, undo/redo, language/time preservation, preferences and saved project round-trip.

The development executable was started, its process reported responding, and its startup event was written. It was stopped after the check. This confirms startup, not a visually inspected window or independent keyboard use. The released executable was compiled after the final source changes.

Computer Use returned `Computer Use was not approved to use sonitrelix`. No UIA tree, screenshot, keyboard walkthrough, NVDA, JAWS, Narrator, high-contrast, Windows 10 or physical audio result is claimed. Reader checks remain pending in ACCESSIBILITY.md. No audio codec or transport acceptance tests exist because audio is not implemented yet.

Dependency review: actual Cargo.lock/Cargo metadata and local supplied licenses cover all eleven external packages. Notices are retained. Candidate audio dependencies were researched upstream but could not be fetched because the configured network proxy was unreachable; they are not linked.


## Help viewer follow-up — 0.4.0

After adding Help > Changelog and moving About into the shared read-only viewer, all 11 Windows UI tests passed. The native checks exercise all three documents in both languages, verify direct content focus, text role 42, ES_READONLY, refusal of typed replacement, correct document titles and translation. Clippy with warnings denied and the release rebuild passed. Real NVDA/JAWS reading remains pending.