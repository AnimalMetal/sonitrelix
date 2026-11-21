use std::{env, fs, path::PathBuf};
fn main() {
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
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("licenses.txt"),
        text,
    )
    .unwrap();
}
