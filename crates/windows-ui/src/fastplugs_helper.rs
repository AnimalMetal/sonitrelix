use serde::{Deserialize, Serialize};
use sonitrelix_audio_io::{Timeline, TimelineReader, WavInfo, WavWriter};
use sonitrelix_effects::fastplugs::{self, Effect, Plugin};
use std::{
    fs, io,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Serialize, Deserialize)]
enum Request {
    Scan(PathBuf),
    Render(Effect),
}
type Response = Result<Vec<Plugin>, String>;

pub fn child(directory: &Path) -> io::Result<()> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetErrorMode(mode: u32) -> u32;
    }
    // The editor reports worker failures. Avoid a system error dialog in the helper.
    unsafe {
        SetErrorMode(0x0001 | 0x0002 | 0x8000);
    }
    let request = serde_json::from_slice(&fs::read(directory.join("request.json"))?)?;
    let response: Response = match request {
        Request::Scan(bundle) => fastplugs::scan(&bundle),
        Request::Render(effect) => fastplugs::render(
            &effect,
            &directory.join("input.wav"),
            &directory.join("output.wav"),
        )
        .map(|_| vec![]),
    }
    .map_err(|error| error.to_string());
    fs::write(
        directory.join("result.json"),
        serde_json::to_vec(&response)?,
    )
}

struct JobDirectory(PathBuf);
impl JobDirectory {
    fn create(parent: &Path) -> io::Result<Self> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let directory = parent.join(format!("fastplugs-{}-{stamp}", std::process::id()));
        fs::create_dir_all(parent)?;
        fs::create_dir(&directory)?;
        Ok(Self(directory))
    }
    fn run(
        &self,
        request: &Request,
        cancel: &AtomicBool,
        timeout: Option<Duration>,
    ) -> io::Result<Vec<Plugin>> {
        fs::write(self.0.join("request.json"), serde_json::to_vec(request)?)?;
        let executable = std::env::current_exe()?;
        let mut process = Command::new(executable)
            .arg("--fastplugs-helper")
            .arg(&self.0)
            .creation_flags(0x08000000)
            .spawn()?;
        let started = Instant::now();
        loop {
            let cancelled = cancel.load(Ordering::Relaxed);
            if cancelled || timeout.is_some_and(|limit| started.elapsed() >= limit) {
                let _ = process.kill();
                let _ = process.wait();
                let kind = if cancelled {
                    io::ErrorKind::Interrupted
                } else {
                    io::ErrorKind::TimedOut
                };
                return Err(io::Error::new(
                    kind,
                    if cancelled {
                        "FastPlugs cancelled"
                    } else {
                        "FastPlugs scan timed out"
                    },
                ));
            }
            match process.try_wait() {
                Ok(Some(status)) if status.success() => break,
                Ok(Some(status)) => {
                    return Err(io::Error::other(format!(
                        "FastPlugs helper ended with {status}"
                    )));
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(20)),
                Err(error) => {
                    let _ = process.kill();
                    let _ = process.wait();
                    return Err(error);
                }
            }
        }
        let response: Response = serde_json::from_slice(&fs::read(self.0.join("result.json"))?)?;
        response.map_err(io::Error::other)
    }
}
impl Drop for JobDirectory {
    fn drop(&mut self) {
        for file in ["request.json", "result.json", "input.wav", "output.wav"] {
            let _ = fs::remove_file(self.0.join(file));
        }
        let _ = fs::remove_dir(&self.0);
    }
}

pub fn scan(parent: &Path, bundle: PathBuf, cancel: &AtomicBool) -> io::Result<Vec<Plugin>> {
    JobDirectory::create(parent)?.run(
        &Request::Scan(bundle),
        cancel,
        Some(Duration::from_secs(12)),
    )
}

pub fn render(
    effect: Effect,
    timeline: Timeline,
    output: &Path,
    cancel: &AtomicBool,
    progress: &AtomicU32,
) -> io::Result<WavInfo> {
    let parent = output
        .parent()
        .ok_or_else(|| io::Error::other("Missing effect output directory"))?;
    let job = JobDirectory::create(parent)?;
    let format = timeline.info();
    let mut reader = TimelineReader::new(timeline);
    let mut writer = WavWriter::create_float(
        &job.0.join("input.wav"),
        format.sample_rate,
        format.channels,
    )?;
    let channels = usize::from(format.channels);
    let mut samples = vec![0.; 8192 * channels];
    while !cancel.load(Ordering::Relaxed) {
        let count = reader.read_frames(&mut samples)?;
        if count == 0 {
            break;
        }
        writer.write_frames(&samples[..count * channels])?;
        progress.store(
            (writer.frames() * 20 / format.frames.max(1)) as u32,
            Ordering::Relaxed,
        );
    }
    if cancel.load(Ordering::Relaxed) {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "FastPlugs cancelled",
        ));
    }
    writer.finish()?;
    job.run(&Request::Render(effect), cancel, None)?;
    let rendered = job.0.join("output.wav");
    let info = sonitrelix_audio_io::WavReader::open(&rendered)?.info;
    if (info.sample_rate, info.channels, info.frames)
        != (format.sample_rate, format.channels, format.frames)
    {
        return Err(io::Error::other(
            "FastPlugs returned a different audio format or duration",
        ));
    }
    fs::rename(rendered, output)?;
    progress.store(100, Ordering::Relaxed);
    Ok(info)
}
