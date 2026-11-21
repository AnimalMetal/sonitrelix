"""Generate a local Ed25519 key or sign an executable update manifest. Never publish the key."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import tomllib
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

ROOT = Path(__file__).resolve().parents[1]
KEY = ROOT / "data" / "signing" / "update-ed25519.pem"
PUBLIC = ROOT / "resources" / "update-public-key.hex"

def generate():
    if KEY.exists() or PUBLIC.exists():
        raise SystemExit("Key already exists; it will not be replaced.")
    KEY.parent.mkdir(parents=True, exist_ok=True)
    key = Ed25519PrivateKey.generate()
    with KEY.open("xb") as f:
        f.write(key.private_bytes(serialization.Encoding.PEM, serialization.PrivateFormat.PKCS8, serialization.NoEncryption()))
    with PUBLIC.open("x", encoding="ascii") as f:
        f.write(key.public_key().public_bytes(serialization.Encoding.Raw, serialization.PublicFormat.Raw).hex() + "\n")
    print("Created local signing key under data/signing; keep an offline backup. Only the public key belongs in Git.")

def sign(args):
    if not re.fullmatch(r"(?:0|[1-9][0-9]{0,8})\.(?:0|[1-9][0-9]{0,8})\.(?:0|[1-9][0-9]{0,8})", args.version):
        raise SystemExit("Use a stable three-component version.")
    executable = Path(args.exe).resolve(strict=True)
    actual = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
    if actual != args.version:
        raise SystemExit("Source workspace version does not match requested release.")
    # Never execute a downloaded artifact on the machine holding the private signing key.
    with executable.open("rb") as f:
        if f.read(2) != b"MZ":
            raise SystemExit("Expected a Windows executable.")
    key = serialization.load_pem_private_key(Path(args.key).read_bytes(), password=None)
    public = key.public_key().public_bytes(serialization.Encoding.Raw, serialization.PublicFormat.Raw).hex()
    if public != PUBLIC.read_text(encoding="ascii").strip():
        raise SystemExit("Signing key does not match the public key embedded in this source.")
    with executable.open("rb") as f:
        digest = hashlib.file_digest(f, "sha256").hexdigest()
    payload = {"protocol": 1, "app": "Sonitrelix", "version": args.version,
               "file": "Sonitrelix.exe", "size": executable.stat().st_size,
               "sha256": digest,
               "url": f"https://github.com/AnimalMetal/sonitrelix/releases/download/v{args.version}/Sonitrelix.exe",
               "notes_en": args.notes_en, "notes_es": args.notes_es}
    text = json.dumps(payload, sort_keys=True, ensure_ascii=False, separators=(",", ":"))
    envelope = {"payload": text, "signature": key.sign(text.encode("utf-8")).hex()}
    with Path(args.output).open("x", encoding="utf-8") as f:
        json.dump(envelope, f, ensure_ascii=False, indent=2)
    print(f"Signed update manifest: {args.output}. Publish this manifest and Sonitrelix.exe; never publish the private key.")

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("generate")
    p = sub.add_parser("sign")
    p.add_argument("--exe", required=True)
    p.add_argument("--version", required=True)
    p.add_argument("--key", default=str(KEY))
    p.add_argument("--output", default="update.json")
    p.add_argument("--notes-en", default="A new Sonitrelix version is available.")
    p.add_argument("--notes-es", default="Hay una nueva versión de Sonitrelix disponible.")
    args = parser.parse_args()
    generate() if args.command == "generate" else sign(args)
