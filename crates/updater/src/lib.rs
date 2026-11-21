//! Signed updates. No field from the signed payload is used before Ed25519 verification.
#[cfg(windows)]
pub mod http;
mod install;
#[cfg(test)]
mod tests;
pub use install::{
    Plan, apply, cache, check_space, confirm_running, confirm_start, confirmed, launch_helper,
    portable, prepare,
};
use serde::{Deserialize, Serialize};
use std::{
    io::{self, Read, Seek, SeekFrom},
    path::Path,
};
pub const MAX_PACKAGE: u64 = 100 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(pub [u32; 3]);
impl Version {
    pub fn parse(s: &str) -> io::Result<Self> {
        let parts: Vec<_> = s.split('.').collect();
        if parts.len() != 3 {
            return Err(bad("Invalid update version"));
        }
        let mut out = [0; 3];
        for (i, p) in parts.iter().enumerate() {
            if p.is_empty()
                || p.len() > 9
                || (p.len() > 1 && p.starts_with('0'))
                || !p.bytes().all(|b| b.is_ascii_digit())
            {
                return Err(bad("Invalid update version"));
            }
            out[i] = p.parse().map_err(|_| bad("Invalid update version"))?;
        }
        Ok(Self(out))
    }
}
fn bad(s: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, s)
}
pub fn unhex<const N: usize>(s: &str) -> io::Result<[u8; N]> {
    if s.len() != N * 2 || !s.is_ascii() {
        return Err(bad("Invalid signature/hash encoding"));
    }
    let mut out = [0; N];
    for (i, v) in out.iter_mut().enumerate() {
        *v = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)
            .map_err(|_| bad("Invalid hexadecimal value"))?;
    }
    Ok(out)
}
pub fn public_key() -> io::Result<[u8; 32]> {
    unhex(include_str!("../../../resources/update-public-key.hex").trim())
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub protocol: u32,
    pub app: String,
    pub version: String,
    pub file: String,
    pub size: u64,
    pub sha256: String,
    pub url: String,
    pub notes_en: String,
    pub notes_es: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    payload: String,
    signature: String,
}
pub fn manifest(bytes: &[u8]) -> io::Result<Manifest> {
    manifest_key(bytes, &public_key()?)
}
fn manifest_key(bytes: &[u8], key: &[u8; 32]) -> io::Result<Manifest> {
    if bytes.len() > 256 * 1024 {
        return Err(bad("Update manifest too large"));
    }
    let env: Envelope = serde_json::from_slice(bytes)?;
    let sig = unhex::<64>(&env.signature)?;
    if !verify(key, env.payload.as_bytes(), &sig) {
        return Err(bad("Update signature is invalid"));
    }
    let m: Manifest = serde_json::from_str(&env.payload)?;
    Version::parse(&m.version)?;
    if m.protocol != 1
        || m.app != "Sonitrelix"
        || m.file != "Sonitrelix.exe"
        || m.size < 1024
        || m.size > MAX_PACKAGE
        || m.notes_en.len() > 32000
        || m.notes_es.len() > 32000
        || m.url
            != format!(
                "https://github.com/AnimalMetal/sonitrelix/releases/download/v{}/Sonitrelix.exe",
                m.version
            )
    {
        return Err(bad("Update manifest fields are invalid"));
    }
    unhex::<32>(&m.sha256)?;
    Ok(m)
}
pub fn eligible(m: &Manifest, current: &str, confirmed: &str) -> io::Result<bool> {
    let v = Version::parse(&m.version)?;
    if v < Version::parse(confirmed)? {
        return Err(bad("Update rollback rejected"));
    }
    Ok(v > Version::parse(current)?)
}
pub fn digest_file(path: &Path) -> io::Result<[u8; 32]> {
    unsafe {
        let ctx = openssl_sys::EVP_MD_CTX_new();
        if ctx.is_null() {
            return Err(bad("Cannot allocate SHA-256 context"));
        }
        let result = (|| {
            if openssl_sys::EVP_DigestInit_ex(ctx, openssl_sys::EVP_sha256(), std::ptr::null_mut())
                != 1
            {
                return Err(bad("Cannot initialize SHA-256"));
            }
            let mut file = std::fs::File::open(path)?;
            let mut buffer = [0u8; 65536];
            loop {
                let n = file.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                if openssl_sys::EVP_DigestUpdate(ctx, buffer.as_ptr().cast(), n) != 1 {
                    return Err(bad("Cannot hash update"));
                }
            }
            let mut out = [0; 32];
            let mut n = 0;
            if openssl_sys::EVP_DigestFinal_ex(ctx, out.as_mut_ptr(), &mut n) != 1 || n != 32 {
                return Err(bad("Cannot finish SHA-256"));
            }
            Ok(out)
        })();
        openssl_sys::EVP_MD_CTX_free(ctx);
        result
    }
}
pub fn verify_package(path: &Path, m: &Manifest) -> io::Result<()> {
    if std::fs::metadata(path)?.len() != m.size {
        return Err(bad("Update size mismatch"));
    }
    let hash = digest_file(path)?;
    let expected = unhex::<32>(&m.sha256)?;
    if hash
        .iter()
        .zip(expected)
        .fold(0u8, |diff, (a, b)| diff | (*a ^ b))
        != 0
    {
        return Err(bad("Update SHA-256 mismatch"));
    }
    let mut f = std::fs::File::open(path)?;
    let mut header = [0; 64];
    f.read_exact(&mut header)?;
    if &header[..2] != b"MZ" {
        return Err(bad("Update is not a Windows executable"));
    }
    let offset = u32::from_le_bytes(header[60..64].try_into().unwrap());
    if u64::from(offset) + 6 > m.size || offset > 4 * 1024 * 1024 {
        return Err(bad("Invalid executable header"));
    }
    f.seek(SeekFrom::Start(u64::from(offset)))?;
    let mut pe = [0u8; 6];
    f.read_exact(&mut pe)?;
    if pe != [b'P', b'E', 0, 0, 0x64, 0x86] {
        return Err(bad("Update is not a Windows x64 executable"));
    }
    Ok(())
}
pub fn verify(key: &[u8; 32], message: &[u8], signature: &[u8; 64]) -> bool {
    unsafe {
        let pkey = openssl_sys::EVP_PKEY_new_raw_public_key(
            openssl_sys::EVP_PKEY_ED25519,
            std::ptr::null_mut(),
            key.as_ptr(),
            key.len(),
        );
        if pkey.is_null() {
            return false;
        }
        let ctx = openssl_sys::EVP_MD_CTX_new();
        let valid = if ctx.is_null() {
            false
        } else {
            openssl_sys::EVP_DigestVerifyInit(
                ctx,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null_mut(),
                pkey,
            ) == 1
                && openssl_sys::EVP_DigestVerify(
                    ctx,
                    signature.as_ptr(),
                    signature.len(),
                    message.as_ptr(),
                    message.len(),
                ) == 1
        };
        if !ctx.is_null() {
            openssl_sys::EVP_MD_CTX_free(ctx);
        }
        openssl_sys::EVP_PKEY_free(pkey);
        valid
    }
}
