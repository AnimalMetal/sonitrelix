use std::{env, fs, path::PathBuf};

fn main() {
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    println!("cargo:rerun-if-env-changed=SONITRELIX_FASTPLUGS_CLAP");
    let plugin = match env::var_os("SONITRELIX_FASTPLUGS_CLAP") {
        Some(path) => {
            let path = PathBuf::from(path);
            println!("cargo:rerun-if-changed={}", path.display());
            let bytes = fs::read(path).expect("read FastPlugs.clap");
            assert!(
                bytes.starts_with(b"MZ"),
                "FastPlugs must be a Windows binary"
            );
            bytes
        }
        None => Vec::new(),
    };
    fs::write(output.join("FastPlugs.clap"), plugin).unwrap();

    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let folder = root.join("third-party-licenses");
    println!("cargo:rerun-if-changed={}", folder.display());
    println!(
        "cargo:rerun-if-changed={}",
        root.join("THIRD_PARTY_NOTICES.md").display()
    );
    let mut files: Vec<_> = fs::read_dir(folder)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_file() && p.file_name().unwrap() != "inventory.json")
        .collect();
    files.sort();
    let mut text = fs::read_to_string(root.join("THIRD_PARTY_NOTICES.md")).unwrap();
    for file in files {
        text.push_str(&format!(
            "\n\n## {}\n",
            file.file_name().unwrap().to_string_lossy()
        ));
        text.push_str(&fs::read_to_string(file).unwrap());
    }
    fs::write(output.join("licenses.txt"), text).unwrap();
}
