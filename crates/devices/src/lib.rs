//! Read-only Windows Core Audio endpoint discovery. No stream or microphone is opened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}
#[derive(Clone, Debug, Default)]
pub struct Catalog {
    pub inputs: Vec<Device>,
    pub outputs: Vec<Device>,
}
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::enumerate;
#[cfg(not(windows))]
pub fn enumerate() -> Result<Catalog, String> {
    Err("Audio endpoint discovery requires Windows.".into())
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    #[ignore = "Read-only integration check against this machine's Windows audio service"]
    fn real_windows_endpoint_discovery() {
        let catalog = super::enumerate().expect("Windows endpoint enumeration");
        for list in [&catalog.inputs, &catalog.outputs] {
            let mut ids = std::collections::HashSet::new();
            for d in list {
                assert!(!d.id.is_empty());
                assert!(!d.name.is_empty());
                assert!(ids.insert(&d.id));
            }
            assert!(list.iter().filter(|d| d.is_default).count() <= 1);
        }
        println!(
            "Active endpoints: {} input, {} output",
            catalog.inputs.len(),
            catalog.outputs.len()
        );
    }
}
