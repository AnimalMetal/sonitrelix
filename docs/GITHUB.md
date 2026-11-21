# GitHub publication

The [Sonitrelix repository](https://github.com/AnimalMetal/sonitrelix) runs formatting, Clippy, tests and Windows builds on main and release tags. The CI ZIP is a development artifact. The release owner prepares and publishes the official ZIP with FFmpeg and the signed update manifest.

1. Check the workspace version, review the actual files going into Git, and run the checks in BUILDING.md.
2. Use a build directory with a non-personal path for native OpenSSL. Build the release executable, and audit its embedded strings for local paths before making it public.
3. Build FFmpeg as described in FFMPEG_BUILD.md and run the FFmpeg integration tests from VALIDATION.md. Run `tools/package.ps1 -FFmpegBuild target/ffmpeg-release` to produce the portable ZIP and corresponding source ZIP.
4. Sign the exact executable in the portable ZIP as described in UPDATES.md. Verify the signature, package contents, and SHA-256 checksums before publishing.
5. Push the reviewed commit and tag it `v<version>`. Create a draft GitHub release and attach the portable ZIP, corresponding source ZIP, update executable, update manifest and checksums. Verify the draft assets before publishing.

Never stage or upload `data/`, recordings, settings, logs, or the private signing key. The embedded verification key is public.
