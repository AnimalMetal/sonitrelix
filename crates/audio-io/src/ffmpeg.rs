//! Optional local-file FFmpeg adapter. All processes run on import/export workers.
use crate::{WavInfo, WavReader};
use std::{
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

const INPUT_FORMATS: &str = "wav,mp3,flac,ogg,mov,mp4,m4a,3gp,3g2,mj2,aac,aiff,matroska,webm,avi,asf,wv,ape,tta,au,mpeg,mpegts";
const MAX_WAV_BYTES: u64 = u32::MAX as u64 - 1_048_576;

pub fn installed() -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    let executable = std::env::current_exe().ok()?;
    let adjacent = executable.parent()?.join(name);
    if adjacent.is_file() {
        return Some(adjacent);
    }
    std::env::split_paths(&std::env::var_os("PATH")?)
        .filter(|path| path.is_absolute())
        .map(|path| path.join(name))
        .find(|path| path.is_file())
}

fn run(
    exe: &Path,
    inputs: &[&Path],
    filter: Option<&str>,
    output: &Path,
    options: &[&str],
    cancel: &AtomicBool,
) -> io::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Audio conversion cancelled",
        ));
    }
    let mut command = Command::new(exe);
    command.args([
        "-nostdin",
        "-hide_banner",
        "-loglevel",
        "error",
        "-xerror",
        "-n",
    ]);
    for input in inputs {
        command
            .args([
                "-protocol_whitelist",
                "file",
                "-format_whitelist",
                INPUT_FORMATS,
                "-i",
            ])
            .arg(input);
    }
    match (filter, inputs.len()) {
        (Some(graph), 2) => {
            command.args(["-filter_complex", graph, "-map", "[out]"]);
        }
        (Some(graph), _) => {
            command.args(["-map", "0:a:0", "-af", graph]);
        }
        (None, _) => {
            command.args(["-map", "0:a:0"]);
        }
    }
    command
        .args([
            "-vn",
            "-sn",
            "-dn",
            "-map_metadata",
            "-1",
            "-map_chapters",
            "-1",
        ])
        .args(options)
        .arg(output)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .env_remove("FFREPORT");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn()?;
    let mut stderr = child.stderr.take().expect("piped FFmpeg errors");
    let error_reader = thread::spawn(move || {
        let mut tail = Vec::with_capacity(8192);
        let mut block = [0; 4096];
        while let Ok(n @ 1..) = stderr.read(&mut block) {
            let excess = (tail.len() + n).saturating_sub(8192);
            tail.drain(..excess);
            tail.extend_from_slice(&block[..n]);
        }
        tail
    });
    let deadline = Instant::now() + Duration::from_secs(30 * 60);
    let status = loop {
        let failure = if cancel.load(Ordering::Relaxed) {
            Some(io::Error::new(
                io::ErrorKind::Interrupted,
                "Audio conversion cancelled",
            ))
        } else if Instant::now() >= deadline {
            Some(io::Error::new(
                io::ErrorKind::TimedOut,
                "FFmpeg conversion timed out",
            ))
        } else if output.metadata().is_ok_and(|m| m.len() > MAX_WAV_BYTES) {
            Some(io::Error::other(
                "Converted audio exceeds the supported file size",
            ))
        } else {
            None
        };
        if let Some(error) = failure {
            let _ = child.kill();
            let _ = child.wait();
            break Err(error);
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => thread::sleep(Duration::from_millis(40)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(error);
            }
        }
    };
    let errors = error_reader.join().unwrap_or_default();
    if status?.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "FFmpeg could not convert the audio: {}",
            String::from_utf8_lossy(&errors).trim()
        )))
    }
}

pub fn decode(exe: &Path, input: &Path, output: &Path, cancel: &AtomicBool) -> io::Result<WavInfo> {
    run(
        exe,
        &[input],
        None,
        output,
        &[
            "-af",
            "aformat=sample_rates=8000|11025|12000|16000|22050|24000|32000|44100|48000|64000|88200|96000|176400|192000:channel_layouts=mono|stereo",
            "-c:a",
            "pcm_f32le",
            "-f",
            "wav",
        ],
        cancel,
    )?;
    let info = WavReader::open(output)?.info;
    if info.frames == 0 {
        return Err(io::Error::other("FFmpeg produced no audio"));
    }
    Ok(info)
}

#[derive(Clone, Copy)]
pub enum Format {
    Mp3,
    Aac,
    Opus,
}

pub fn encode(
    exe: &Path,
    input: &Path,
    output: &Path,
    format: Format,
    mp3_bitrate: u32,
    cancel: &AtomicBool,
) -> io::Result<()> {
    let bitrate = format!("{mp3_bitrate}k");
    let options = match format {
        Format::Mp3 => ["-c:a", "libmp3lame", "-b:a", bitrate.as_str(), "-f", "mp3"],
        Format::Aac => ["-c:a", "aac", "-b:a", "192k", "-f", "ipod"],
        Format::Opus => ["-c:a", "libopus", "-b:a", "128k", "-f", "opus"],
    };
    run(exe, &[input], None, output, &options, cancel)
}

/// Apply an application-generated filter graph to local audio and validate the WAV.
pub fn filter_wav(
    exe: &Path,
    inputs: &[&Path],
    output: &Path,
    graph: &str,
    cancel: &AtomicBool,
) -> io::Result<WavInfo> {
    run(
        exe,
        inputs,
        Some(graph),
        output,
        &["-c:a", "pcm_f32le", "-f", "wav"],
        cancel,
    )?;
    let info = WavReader::open(output)?.info;
    if info.frames == 0 || cancel.load(Ordering::Relaxed) {
        return Err(io::Error::other(
            "The effect produced no audio or was cancelled",
        ));
    }
    Ok(info)
}
