# GitHub publishing

Repository: https://github.com/AnimalMetal/sonitrelix

Source belongs in this independent repository, not its parent folder's repository. Build output, portable data, recordings under data/, distribution output and private keys are excluded by .gitignore. Review staged changes before committing. Do not add user recordings or credentials.

The Windows build workflow runs formatting, Clippy, hardware-independent tests and a release build on main pushes, pull requests, version tags and manual dispatch. It uploads a portable development ZIP and SHA-256 checksum as an Actions artifact. It includes dependency notices and user documentation. Real-device and NVDA/JAWS tests remain manual. The workflow itself needs its first successful GitHub run; local builds do not prove hosted CI has passed.

This workflow does not publish a GitHub Release or enable in-app updates. Future update distribution should use versioned GitHub Releases, with a signed update manifest, hash verification, safe installation and preservation of data/. Checksums alone do not authenticate an update. No signing key is stored in this repository.

## Finish the initial push from PowerShell

Run from the Sonitrelix folder in a normal PowerShell window if the coding environment cannot write Git metadata or reach GitHub:

```powershell
# Run these commands from the repository root.
git remote add origin https://github.com/AnimalMetal/sonitrelix.git
git add -- .github .gitignore BUILDING.md CHANGELOG.md CONTRIBUTING.md Cargo.lock Cargo.toml LICENSE README.md THIRD_PARTY_NOTICES.md crates docs resources third-party-licenses
git diff --cached --stat
git commit -m "Initial Sonitrelix 0.4.0 development build"
git push -u origin main
```

If origin already exists, check git remote -v rather than adding it again. Git Credential Manager may request GitHub sign-in. If the remote has existing commits, do not force-push: fetch and review its history before reconciling. An authentication prompt or a rejected push does not mean the upload succeeded.

## Español

El flujo de GitHub comprueba el código, ejecuta pruebas sin hardware y genera un ZIP portable con licencias y suma SHA-256. Falta verificar su primera ejecución en GitHub. No publica versiones ni activa actualizaciones automáticas todavía. Conserve fuera del repositorio grabaciones, datos y claves privadas. No fuerce un envío si el repositorio remoto ya contiene trabajo.
