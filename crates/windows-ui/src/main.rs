#![cfg_attr(windows, windows_subsystem = "windows")]
#[cfg(windows)]
mod native;
#[cfg(windows)]
fn main() {
    native::run();
}
#[cfg(not(windows))]
fn main() {
    eprintln!("Sonitrelix requires Windows 10/11 x64.");
}
