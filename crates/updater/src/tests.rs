use super::*;
pub(crate) fn key() -> [u8; 32] {
    unhex("d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a").unwrap()
}
pub(crate) fn signed(m: &Manifest) -> String {
    unsafe {
        // Public RFC 8032 test seed; unrelated to the application's real signing key.
        let seed = unhex::<32>("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60")
            .unwrap();
        let pkey = openssl_sys::EVP_PKEY_new_raw_private_key(
            openssl_sys::EVP_PKEY_ED25519,
            std::ptr::null_mut(),
            seed.as_ptr(),
            32,
        );
        assert!(!pkey.is_null());
        let ctx = openssl_sys::EVP_MD_CTX_new();
        assert!(!ctx.is_null());
        let payload = serde_json::to_string(m).unwrap();
        let mut signature = [0u8; 64];
        let mut length = 64;
        assert_eq!(
            openssl_sys::EVP_DigestSignInit(
                ctx,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null_mut(),
                pkey
            ),
            1
        );
        assert_eq!(
            openssl_sys::EVP_DigestSign(
                ctx,
                signature.as_mut_ptr(),
                &mut length,
                payload.as_ptr(),
                payload.len()
            ),
            1
        );
        openssl_sys::EVP_MD_CTX_free(ctx);
        openssl_sys::EVP_PKEY_free(pkey);
        serde_json::json!({"payload":payload,"signature":signature.iter().map(|b|format!("{b:02x}")).collect::<String>()}).to_string()
    }
}
pub(crate) fn example(path: &Path) -> Manifest {
    Manifest {
        protocol: 1,
        app: "Sonitrelix".into(),
        version: "9.9.9".into(),
        file: "Sonitrelix.exe".into(),
        size: std::fs::metadata(path).unwrap().len(),
        sha256: digest_file(path)
            .unwrap()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        url: "https://github.com/AnimalMetal/sonitrelix/releases/download/v9.9.9/Sonitrelix.exe"
            .into(),
        notes_en: "Test".into(),
        notes_es: "Prueba".into(),
    }
}
#[test]
fn ed25519_rfc_vector_and_tampering() {
    let sig=unhex::<64>("e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b").unwrap();
    assert!(verify(&key(), b"", &sig));
    assert!(!verify(&key(), b"altered", &sig));
    let mut wrong = sig;
    wrong[0] ^= 1;
    assert!(!verify(&key(), b"", &wrong));
    assert!(public_key().is_ok());
}
#[test]
fn signed_fields_and_rollback_are_enforced() {
    let p = std::env::current_exe().unwrap();
    let m = example(&p);
    let text = signed(&m);
    assert_eq!(
        manifest_key(text.as_bytes(), &key()).unwrap().version,
        "9.9.9"
    );
    assert!(
        manifest(text.as_bytes()).is_err(),
        "test signing key is not the embedded release key"
    );
    let mut bad_json: serde_json::Value = serde_json::from_str(&text).unwrap();
    bad_json["payload"] = bad_json["payload"]
        .as_str()
        .unwrap()
        .replace("9.9.9", "8.8.8")
        .into();
    assert!(manifest_key(&serde_json::to_vec(&bad_json).unwrap(), &key()).is_err());
    let mut wrong = m.clone();
    wrong.url = "http://untrusted.invalid/package.exe".into();
    assert!(manifest_key(signed(&wrong).as_bytes(), &key()).is_err());
    assert!(eligible(&m, "0.6.0", "10.0.0").is_err());
    assert!(!eligible(&m, "9.9.9", "9.9.9").unwrap());
    assert!(eligible(&m, "0.6.0", "0.6.0").unwrap());
    verify_package(&p, &m).unwrap();
    let mut wrong = m.clone();
    wrong.size += 1;
    assert!(verify_package(&p, &wrong).is_err());
    wrong = m;
    wrong.sha256 = "00".repeat(32);
    assert!(verify_package(&p, &wrong).is_err());
}
