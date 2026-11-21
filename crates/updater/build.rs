use std::{env, fs, path::PathBuf};
fn main() {
    println!("cargo:rerun-if-env-changed=OPENSSL_DIR");
    // Keep MSVC static OpenSSL's database beside linked debug/test artifacts.
    if let Some(root) = env::var_os("DEP_OPENSSL_ROOT").or_else(|| env::var_os("OPENSSL_DIR")) {
        let pdb = PathBuf::from(root).join("lib/ossl_static.pdb");
        if pdb.is_file() {
            let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
            if let Some(profile) = out.ancestors().nth(3) {
                let deps = profile.join("deps");
                let _ = fs::create_dir_all(&deps);
                let _ = fs::copy(pdb, deps.join("ossl_static.pdb"));
            }
        }
    }
}
