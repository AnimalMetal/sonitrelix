#![cfg_attr(windows, windows_subsystem = "windows")]
#[cfg(windows)]
mod fastplugs_helper;
#[cfg(windows)]
mod native;
#[cfg(windows)]
fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() == 2 && args[1] == "--version" {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if args.len() == 3 && args[1] == "--apply-update" {
        if sonitrelix_updater::apply(std::path::Path::new(&args[2])).is_err() {
            std::process::exit(1);
        }
        return;
    }
    if args.len() == 3 && args[1] == "--fastplugs-helper" {
        if fastplugs_helper::child(std::path::Path::new(&args[2])).is_err() {
            std::process::exit(1);
        }
        return;
    }
    native::run();
}
#[cfg(not(windows))]
fn main() {
    eprintln!("Sonitrelix requires Windows 10/11 x64.");
}
