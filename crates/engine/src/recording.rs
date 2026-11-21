//! Standalone WASAPI capture: bounded capture queue -> separate PCM16 disk writer.
use super::{ring, wasapi::Signal};
use sonitrelix_audio_io::{WavInfo, WavWriter};
use std::{
    io,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::WAIT_FAILED,
        Media::Audio::*,
        System::{
            Com::{
                CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
            },
            Threading::WaitForSingleObject,
        },
    },
    core::PCWSTR,
};
pub struct RecordRequest {
    pub gate: Option<Arc<super::StartGate>>,
    pub path: PathBuf,
    pub input_id: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub buffer_frames: u32,
}

pub struct Recorded {
    pub path: PathBuf,
    pub info: WavInfo,
    pub warning: Option<String>,
}
pub struct Recorder {
    stop: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    frames: Arc<AtomicU64>,
    done: Arc<AtomicBool>,
    wake: Arc<Signal>,
    result: Receiver<Result<Recorded, String>>,
}
impl Recorder {
    pub fn start(request: RecordRequest) -> io::Result<Self> {
        let wake = Arc::new(Signal::new()?);
        let stop = Arc::new(AtomicBool::new(false));
        let paused = Arc::new(AtomicBool::new(false));
        let frames = Arc::new(AtomicU64::new(0));
        let done = Arc::new(AtomicBool::new(false));
        let (tx, result) = mpsc::channel();
        let (s, p, f, d, w) = (
            stop.clone(),
            paused.clone(),
            frames.clone(),
            done.clone(),
            wake.clone(),
        );
        thread::Builder::new()
            .name("recording-coordinator".into())
            .spawn(move || {
                let _finished = Finished(d);
                let result = record(request, s, p, f, w);
                let _ = tx.send(result);
            })?;
        Ok(Self {
            stop,
            paused,
            frames,
            done,
            wake,
            result,
        })
    }
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Release);
        self.wake.set();
    }
    pub fn frames(&self) -> u64 {
        self.frames.load(Ordering::Acquire)
    }
    pub fn pause(&self, pause: bool) {
        self.paused.store(pause, Ordering::Release);
        self.wake.set();
    }
    pub fn done(&self) -> bool {
        self.done.load(Ordering::Acquire)
    }
    pub fn try_result(&self) -> Option<Result<Recorded, String>> {
        match self.result.try_recv() {
            Ok(v) => Some(v),
            Err(mpsc::TryRecvError::Disconnected) => Some(Err(
                "Recording worker stopped unexpectedly; recording file was retained".into(),
            )),
            Err(mpsc::TryRecvError::Empty) => None,
        }
    }
}
impl Drop for Recorder {
    fn drop(&mut self) {
        self.stop();
    }
}
struct Finished(Arc<AtomicBool>);
impl Drop for Finished {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}
fn record(
    r: RecordRequest,
    stop: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    frames: Arc<AtomicU64>,
    wake: Arc<Signal>,
) -> Result<Recorded, String> {
    let mut writer = WavWriter::create(&r.path, r.sample_rate, r.channels)
        .map_err(|e| format!("Create recording: {e}"))?;
    let (mut producer, mut consumer) = ring::ring(32768 * usize::from(r.channels));
    let captured = Arc::new(AtomicBool::new(false));
    let c = captured.clone();
    let s = stop.clone();
    let f = frames.clone();
    let w = wake.clone();
    let input = r.input_id.clone();
    let rate = r.sample_rate;
    let channels = r.channels;
    let buffer = r.buffer_frames;
    let gate = r.gate.clone();
    let capture = thread::Builder::new()
        .name("wasapi-capture".into())
        .spawn(move || {
            let _finished = Finished(c);
            unsafe {
                capture(
                    (&input, rate, channels, buffer),
                    &s,
                    &paused,
                    &f,
                    &w,
                    &mut producer,
                    gate.as_deref(),
                )
            }
        })
        .map_err(|e| e.to_string())?;
    let mut block = vec![0f32; 8192 * usize::from(r.channels)];
    let mut warning = None;
    let mut checkpoint = Instant::now();
    loop {
        let n = consumer.pop(&mut block);
        if n > 0 {
            if let Err(e) = writer.write_frames(&block[..n]) {
                warning = Some(format!("Recording disk error: {e}"));
                stop.store(true, Ordering::Release);
                wake.set();
                break;
            }
        } else if captured.load(Ordering::Acquire) && consumer.available() == 0 {
            break;
        } else {
            thread::sleep(Duration::from_millis(2));
        }
        if checkpoint.elapsed() >= Duration::from_secs(1) {
            if let Err(e) = writer.checkpoint() {
                warning = Some(format!("Recording checkpoint failed: {e}"));
                stop.store(true, Ordering::Release);
                wake.set();
                break;
            }
            checkpoint = Instant::now();
        }
    }
    stop.store(true, Ordering::Release);
    wake.set();
    match capture.join() {
        Ok(Err(e)) => warning = Some(e),
        Err(_) => warning = Some("Capture worker panicked".into()),
        _ => (),
    }
    let count = writer.frames();
    let info = writer.finish().map_err(|e| {
        format!(
            "Cannot finalize recording; retained at {}: {e}",
            r.path.display()
        )
    })?;
    if count == 0 {
        return Err(warning.unwrap_or_else(|| "No audio was captured".into()));
    }
    Ok(Recorded {
        path: r.path,
        info,
        warning,
    })
}
struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}
struct Client(IAudioClient);
impl Drop for Client {
    fn drop(&mut self) {
        unsafe {
            let _ = self.0.Stop();
        }
    }
}
unsafe fn capture(
    spec: (&str, u32, u16, u32),
    stop: &AtomicBool,
    paused: &AtomicBool,
    frames: &AtomicU64,
    wake: &Signal,
    producer: &mut ring::Producer,
    gate: Option<&super::StartGate>,
) -> Result<(), String> {
    unsafe {
        let (input, rate, channels, buffer) = spec;
        if stop.load(Ordering::Acquire) {
            return Ok(());
        }
        fn err(e: windows::core::Error) -> String {
            format!("WASAPI capture: {e}")
        }
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .map_err(err)?;
        let _apartment = Apartment;
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(err)?;
        let device = if input.is_empty() {
            enumerator
                .GetDefaultAudioEndpoint(eCapture, eMultimedia)
                .map_err(err)?
        } else {
            let id: Vec<_> = input.encode_utf16().chain(Some(0)).collect();
            enumerator.GetDevice(PCWSTR(id.as_ptr())).map_err(err)?
        };
        let client = Client(device.Activate(CLSCTX_ALL, None).map_err(err)?);
        let format = WAVEFORMATEX {
            wFormatTag: 3,
            nChannels: channels,
            nSamplesPerSec: rate,
            nAvgBytesPerSec: rate * u32::from(channels) * 4,
            nBlockAlign: channels * 4,
            wBitsPerSample: 32,
            cbSize: 0,
        };
        client
            .0
            .Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_EVENTCALLBACK
                    | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
                    | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY
                    | AUDCLNT_STREAMFLAGS_NOPERSIST,
                i64::from(buffer.clamp(128, 2048)) * 10000000 / i64::from(rate),
                0,
                &format,
                None,
            )
            .map_err(err)?;
        client.0.SetEventHandle(wake.0).map_err(err)?;
        let capacity = client.0.GetBufferSize().map_err(err)?;
        if capacity == 0 || capacity > rate * 2 {
            return Err("Invalid capture buffer".into());
        }
        let capture: IAudioCaptureClient = client.0.GetService().map_err(err)?;
        let zeros = vec![0f32; capacity as usize * usize::from(channels)];
        if let Some(gate) = gate {
            gate.wait(2, 1, || stop.load(Ordering::Acquire))
                .map_err(|e| e.to_string())?;
        }
        client.0.Start().map_err(err)?;
        let mut first = true;
        let mut running = true;
        let mut start_phase = 1u32;
        while !stop.load(Ordering::Acquire) {
            if paused.load(Ordering::Acquire) {
                if running {
                    client.0.Stop().map_err(err)?;
                    running = false;
                }
                WaitForSingleObject(wake.0, 50);
                continue;
            }
            if !running {
                start_phase += 1;
                if let Some(gate) = gate
                    && let Err(e) = gate.wait(2, start_phase, || stop.load(Ordering::Acquire))
                {
                    if stop.load(Ordering::Acquire) {
                        return Ok(());
                    }
                    return Err(e.to_string());
                }
                client.0.Start().map_err(err)?;
                running = true;
                first = true;
            }
            if WaitForSingleObject(wake.0, 100) == WAIT_FAILED {
                return Err("Capture wait failed".into());
            }
            while capture.GetNextPacketSize().map_err(err)? > 0 && !stop.load(Ordering::Acquire) {
                let mut data = std::ptr::null_mut();
                let mut count = 0;
                let mut flags = 0;
                capture
                    .GetBuffer(&mut data, &mut count, &mut flags, None, None)
                    .map_err(err)?;
                if count > capacity || (!first && flags & 1 != 0) {
                    let _ = capture.ReleaseBuffer(count);
                    return Err("Input discontinuity; completed audio was retained".into());
                }
                let samples = count as usize * usize::from(channels);
                let written = if flags & 2 != 0 {
                    producer.push(&zeros[..samples])
                } else if data.is_null() {
                    0
                } else {
                    producer.push(std::slice::from_raw_parts(data.cast::<f32>(), samples))
                };
                capture.ReleaseBuffer(count).map_err(err)?;
                frames.fetch_add((written / usize::from(channels)) as u64, Ordering::Release);
                first = false;
                if written != samples {
                    return Err("Recording buffer overflow; completed audio was retained".into());
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Briefly opens the actual default microphone; deletes the test recording"]
    fn default_microphone_captures_and_resumes_with_backing() {
        let path = std::env::temp_dir().join(format!(
            "sonitrelix-capture-smoke-{}.wav",
            std::process::id()
        ));
        let gate = Arc::new(super::super::StartGate::new());
        let timeline = sonitrelix_audio_io::Timeline::new(
            48000,
            2,
            vec![sonitrelix_audio_io::Segment {
                source: None,
                source_start: 0,
                frames: 240000,
            }],
        )
        .unwrap();
        let mut backing = super::super::Engine::new().unwrap();
        backing
            .play(super::super::PlayRequest {
                gate: Some(gate.clone()),
                timeline,
                start_ms: 0,
                start_frame: None,
                output_id: String::new(),
                buffer_frames: 512,
                paused: false,
            })
            .unwrap();
        let recorder = Recorder::start(RecordRequest {
            gate: Some(gate),
            path: path.clone(),
            input_id: String::new(),
            sample_rate: 48000,
            channels: 1,
            buffer_frames: 512,
        })
        .unwrap();
        let exercise = || -> Result<WavInfo, String> {
            let wait_frames = |target: u64| -> Result<(), String> {
                let deadline = Instant::now() + Duration::from_secs(8);
                while recorder.frames() < target {
                    if let Some(result) = recorder.try_result() {
                        return Err(result
                            .err()
                            .unwrap_or_else(|| "Capture stopped early".into()));
                    }
                    if Instant::now() >= deadline {
                        return Err("Microphone did not deliver frames".into());
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                Ok(())
            };
            wait_frames(9600)?;
            recorder.pause(true);
            backing.pause(true);
            thread::sleep(Duration::from_millis(150));
            let paused = recorder.frames();
            thread::sleep(Duration::from_millis(120));
            if paused != recorder.frames() {
                return Err("Microphone continued capturing while paused".into());
            }
            recorder.pause(false);
            backing.pause(false);
            wait_frames(paused + 9600)?;
            recorder.stop();
            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                if let Some(result) = recorder.try_result() {
                    let result = result?;
                    if let Some(warning) = result.warning {
                        return Err(warning);
                    }
                    return Ok(result.info);
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err("Capture did not finalize".into())
        };
        let result = exercise();
        recorder.stop();
        backing.stop();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !recorder.done() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        let saved = sonitrelix_audio_io::WavReader::open(&path).map(|reader| reader.info);
        std::fs::remove_file(path).unwrap();
        let info = result.unwrap();
        assert_eq!(saved.unwrap(), info);
        assert!(info.frames >= 19200);
        println!(
            "Captured and finalized {} frames with backing playback",
            info.frames
        );
    }
    #[test]
    #[ignore = "Invalid input endpoint only; never activates or captures a microphone"]
    fn invalid_input_does_not_fall_back() {
        let path = std::env::temp_dir().join(format!(
            "sonitrelix-invalid-input-{}.wav",
            std::process::id()
        ));
        let r = Recorder::start(RecordRequest {
            gate: None,
            path: path.clone(),
            input_id: "sonitrelix-nonexistent-input".into(),
            sample_rate: 48000,
            channels: 1,
            buffer_frames: 256,
        })
        .unwrap();
        let end = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(result) = r.try_result() {
                assert!(result.is_err());
                break;
            }
            assert!(Instant::now() < end);
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(r.frames(), 0);
        drop(r);
        assert_eq!(
            sonitrelix_audio_io::WavReader::open(&path)
                .unwrap()
                .info
                .frames,
            0
        );
        std::fs::remove_file(path).unwrap();
    }
}
