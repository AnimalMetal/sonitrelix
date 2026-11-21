//! Offline verification of the exact signed release executable; does not run it.
fn main() -> std::io::Result<()> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 3 {
        eprintln!("Usage: verify update.json Sonitrelix.exe");
        std::process::exit(2);
    }
    let m = sonitrelix_updater::manifest(&std::fs::read(&args[1])?)?;
    sonitrelix_updater::verify_package(std::path::Path::new(&args[2]), &m)?;
    println!(
        "Verified Ed25519 signature, size and SHA-256 for Sonitrelix {}",
        m.version
    );
    Ok(())
}
