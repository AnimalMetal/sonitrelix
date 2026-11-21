use std::{
    fs::OpenOptions,
    io::Write,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) fn initialize(data: &Path) {
    let _ = OpenOptions::new()
        .create(true)
        .append(true)
        .open(data.join("error.log"));
    let path = data.to_path_buf();
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        write(&path, "panic", panic);
        previous(panic);
    }));
}

#[track_caller]
pub(super) fn write(data: &Path, operation: &str, detail: impl std::fmt::Display) {
    let location = std::panic::Location::caller();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let detail: String = detail
        .to_string()
        .chars()
        .take(8192)
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let thread = std::thread::current();
    let line = format!(
        "{now} UTC-ms | {} {} | {} | {}:{} | {detail}\n",
        thread.name().unwrap_or("thread"),
        std::process::id(),
        operation,
        location.file(),
        location.line()
    );
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(data.join("error.log"))
    {
        let _ = file.write_all(line.as_bytes());
    }
}
