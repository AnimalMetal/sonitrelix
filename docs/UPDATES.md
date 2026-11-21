# Signed portable updates

Help > Check for updates checks published stable GitHub Releases. Portable packages also check quietly daily while idle; disable this in Preferences > General. Installation requires a newer signed update.json and a portable marker. Unmarked/source builds offer manual downloads. Save work and stop recording before installing. Skip Version suppresses automatic offers; manual checks can offer it again.

The embedded Ed25519 public key verifies the exact payload before trusting its version, URL, size or SHA-256. HTTPS requests use Windows TLS/proxy with bounded timeouts and response sizes. Private partial downloads can resume, then must match the signed size/hash. A highest-confirmed-version floor rejects downgrades.

A helper copied from the installed EXE waits for the original process to exit without killing it. It backs up and replaces only Sonitrelix.exe. Other files and data remain in place. The new UI must confirm startup; failed/hung startup restores the verified backup. Automated tests cover replacement, rollback, preserving data and refusing to kill a running original. Live GitHub delivery and the full real-application update remain unverified. Future runtime/asset changes require a full portable package.

## Release owner

The private key is data/signing/update-ed25519.pem. Keep a secure offline backup. Never commit, upload, package or put it in GitHub secrets. Only resources/update-public-key.hex is public. Losing the key requires a separately delivered trust migration.

1. Review and test the matching source, then prepare the portable ZIP as described in GITHUB.md.
2. On the trusted signing machine, review the executable from that exact build.
3. With Python 3.11+ and cryptography, sign without executing the downloaded artifact:

```powershell
python tools/release_manifest.py sign --exe path/to/Sonitrelix.exe --version 0.6.2 --output update.json --notes-en "Release notes" --notes-es "Novedades"
```

4. Upload update.json beside the exact signed Sonitrelix.exe. Verify that the portable ZIP contains that same executable and complete the release checks before publishing.

An independent fork can generate its own key before building with the tool's generate command. Existing keys are never replaced by that command. The tool does not publish releases.

## Español

Ayuda > Buscar actualizaciones verifica versiones estables. Los paquetes portables comprueban diariamente; se desactiva en Preferencias > General. Instalar verifica firma, tamaño y SHA-256, permite cancelar y conserva data. Si falla el arranque, vuelve al ejecutable anterior. Guarde la clave privada solo en el equipo de firma y su copia segura; nunca en GitHub. Falta verificar la entrega completa por GitHub y el arranque real tras actualizar.
