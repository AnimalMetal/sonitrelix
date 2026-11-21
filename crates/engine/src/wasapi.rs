use super::{
    Command, Event, PlayRequest, Progress, State,
    ring::{self, Consumer},
};
use sonitrelix_audio_io::{TimelineReader, ms_to_frames};
use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{Receiver, RecvTimeoutError, Sender},
    },
    thread::{self, JoinHandle},
    time::Duration,
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE, WAIT_FAILED},
        Media::Audio::*,
        System::{
            Com::{
                CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
            },
            Threading::{CreateEventW, SetEvent, WaitForSingleObject},
        },
    },
    core::PCWSTR,
};
pub(super) struct Signal(pub(super) HANDLE);
// Windows event handles may be signaled/waited from any thread. Closed only when
// the last Arc drops after all session workers release it.
unsafe impl Send for Signal {}
unsafe impl Sync for Signal {}
impl Signal {
    pub fn new() -> io::Result<Self> {
        unsafe {
            CreateEventW(None, false, false, None)
                .map(Self)
                .map_err(io::Error::other)
        }
    }
    pub fn set(&self) {
        unsafe {
            let _ = SetEvent(self.0);
        }
    }
}
impl Drop for Signal {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}
struct Mmcss(HANDLE);
impl Drop for Mmcss {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::System::Threading::AvRevertMmThreadCharacteristics(self.0);
        }
    }
}
#[derive(Debug)]
struct Failure {
    operation: &'static str,
    code: i32,
}
type Result<T> = std::result::Result<T, Failure>;
fn api<T>(result: windows::core::Result<T>, operation: &'static str) -> Result<T> {
    result.map_err(|e| Failure {
        operation,
        code: e.code().0,
    })
}
struct Stream {
    client: IAudioClient,
    render: IAudioRenderClient,
    capacity: u32,
}
impl Stream {
    unsafe fn open(
        output_id: &str,
        channels: u16,
        rate: u32,
        buffer: u32,
        wake: &Signal,
    ) -> Result<Self> {
        unsafe {
            let enumerator: IMMDeviceEnumerator = api(
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL),
                "Create output enumerator",
            )?;
            let device = if output_id.is_empty() {
                api(
                    enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia),
                    "Find default output",
                )?
            } else {
                let id: Vec<_> = output_id.encode_utf16().chain(Some(0)).collect();
                api(
                    enumerator.GetDevice(PCWSTR(id.as_ptr())),
                    "Find selected output (no fallback)",
                )?
            };
            let client: IAudioClient =
                api(device.Activate(CLSCTX_ALL, None), "Activate WASAPI output")?;
            let format = WAVEFORMATEX {
                wFormatTag: 3,
                nChannels: channels,
                nSamplesPerSec: rate,
                nAvgBytesPerSec: rate * u32::from(channels) * 4,
                nBlockAlign: channels * 4,
                wBitsPerSample: 32,
                cbSize: 0,
            };
            let duration = i64::from(buffer.clamp(128, 2048)) * 10000000 / i64::from(rate);
            api(
                client.Initialize(
                    AUDCLNT_SHAREMODE_SHARED,
                    AUDCLNT_STREAMFLAGS_EVENTCALLBACK
                        | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
                        | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY
                        | AUDCLNT_STREAMFLAGS_NOPERSIST,
                    duration,
                    0,
                    &format,
                    None,
                ),
                "Initialize shared WASAPI stream",
            )?;
            api(client.SetEventHandle(wake.0), "Set WASAPI event")?;
            let capacity = api(client.GetBufferSize(), "Get WASAPI buffer size")?;
            if capacity == 0 || capacity > rate * 2 {
                return Err(Failure {
                    operation: "Invalid WASAPI buffer size",
                    code: 0,
                });
            }
            let render: IAudioRenderClient = api(client.GetService(), "Get WASAPI render service")?;
            Ok(Self {
                client,
                render,
                capacity,
            })
        }
    }
}
impl Drop for Stream {
    fn drop(&mut self) {
        unsafe {
            let _ = self.client.Stop();
        }
    }
}

/// The monitor has a separate shared-mode output stream so it can continue after backing ends.
pub(super) fn monitor(
    (output_id, rate, channels, buffer): (&str, u32, u16, u32),
    (stop, paused): (&AtomicBool, &AtomicBool),
    mut pcm: Consumer,
) -> std::result::Result<(), String> {
    let wake = Signal::new().map_err(|e| e.to_string())?;
    let mut run = || -> Result<()> {
        unsafe {
            api(
                CoInitializeEx(None, COINIT_MULTITHREADED).ok(),
                "Initialize monitor COM",
            )?;
            let _apartment = Apartment;
            let stream = Stream::open(output_id, channels, rate, buffer, &wake)?;
            let mut task_index = 0;
            let _mmcss = windows::Win32::System::Threading::AvSetMmThreadCharacteristicsW(
                windows::core::w!("Audio"),
                &mut task_index,
            )
            .ok()
            .map(Mmcss);
            let channels = usize::from(channels);
            let backlog = stream.capacity as usize * channels * 2;
            let mut running = false;
            while !stop.load(Ordering::Acquire) {
                if paused.load(Ordering::Acquire) {
                    if running {
                        api(stream.client.Stop(), "Pause microphone monitor")?;
                        api(stream.client.Reset(), "Flush microphone monitor")?;
                        running = false;
                    }
                    pcm.discard(usize::MAX);
                } else {
                    // Capture cannot wait for a slow output device. Drop stale monitor audio,
                    // and leave the separate recording queue untouched.
                    pcm.discard(pcm.available().saturating_sub(backlog));
                    if running || pcm.available() >= channels {
                        let padding = api(
                            stream.client.GetCurrentPadding(),
                            "Read microphone monitor padding",
                        )?;
                        let available = stream.capacity.saturating_sub(padding);
                        if available != 0 {
                            let buffer = api(
                                stream.render.GetBuffer(available),
                                "Acquire microphone monitor buffer",
                            )?;
                            if buffer.is_null() {
                                return Err(Failure {
                                    operation: "Null microphone monitor buffer",
                                    code: 0,
                                });
                            }
                            let output = std::slice::from_raw_parts_mut(
                                buffer.cast::<f32>(),
                                available as usize * channels,
                            );
                            let n = pcm.pop(output);
                            for sample in &mut output[..n] {
                                *sample = if sample.is_finite() {
                                    sample.clamp(-1., 1.) * 0.25
                                } else {
                                    0.
                                };
                            }
                            output[n..].fill(0.);
                            api(
                                stream.render.ReleaseBuffer(available, 0),
                                "Release microphone monitor buffer",
                            )?;
                        }
                        if !running {
                            api(stream.client.Start(), "Start microphone monitor")?;
                            running = true;
                        }
                    }
                }
                if WaitForSingleObject(wake.0, if running { 20 } else { 5 }) == WAIT_FAILED {
                    return Err(Failure {
                        operation: "Wait for microphone monitor event",
                        code: -1,
                    });
                }
            }
            Ok(())
        }
    };
    run().map_err(|e| format!("{} (0x{:08x})", e.operation, e.code))
}
struct Control {
    ticket: u64,
    latest: Arc<AtomicU64>,
    cancel: AtomicBool,
    paused: Arc<AtomicBool>,
    eof: AtomicBool,
    failed: AtomicBool,
}
impl Control {
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Acquire) || self.latest.load(Ordering::Acquire) != self.ticket
    }
}
struct DecoderCompletion(Arc<Control>);
impl Drop for DecoderCompletion {
    fn drop(&mut self) {
        if thread::panicking() {
            self.0.failed.store(true, Ordering::Release);
        }
        self.0.eof.store(true, Ordering::Release);
    }
}
struct Session {
    control: Arc<Control>,
    wake: Arc<Signal>,
    render: JoinHandle<Result<bool>>,
    decoder: JoinHandle<io::Result<()>>,
}
impl Session {
    fn finish(self) -> std::result::Result<bool, String> {
        self.control.cancel.store(true, Ordering::Release);
        self.wake.set();
        let render = self
            .render
            .join()
            .map_err(|_| "WASAPI worker panicked".to_owned());
        let decoder = self
            .decoder
            .join()
            .map_err(|_| "WAV decoder worker panicked".to_owned());
        let decoded = decoder?;
        let rendered = render?;
        if let Err(e) = decoded {
            return Err(format!("WAV read failed: {e}"));
        }
        rendered.map_err(|e| format!("{}: Windows error 0x{:08X}", e.operation, e.code as u32))
    }
}
fn start(
    ticket: u64,
    request: PlayRequest,
    progress: Arc<Progress>,
    latest: Arc<AtomicU64>,
    paused: Arc<AtomicBool>,
    wake: Arc<Signal>,
) -> std::result::Result<Session, String> {
    let mut reader = TimelineReader::new(request.timeline.clone());
    let start = request
        .start_frame
        .unwrap_or_else(|| ms_to_frames(request.start_ms, reader.info().sample_rate))
        .min(reader.info().frames);
    reader.seek_frame(start).map_err(|e| e.to_string())?;
    progress
        .rate
        .store(reader.info().sample_rate, Ordering::Release);
    progress
        .total
        .store(reader.info().frames, Ordering::Release);
    progress.frame.store(start, Ordering::Release);
    progress.underruns.store(0, Ordering::Relaxed);
    progress.meter.reset();
    progress.device_buffer.store(0, Ordering::Relaxed);
    let channels = usize::from(reader.info().channels);
    // Keep less premixed audio queued for interactive mute/solo.
    let (queue_frames, block_frames) = if request.timeline.is_live_mix() {
        let rate = reader.info().sample_rate as usize;
        ((rate / 12).next_power_of_two(), rate / 100)
    } else {
        (32768, 8192)
    };
    let (mut producer, consumer) = ring::ring(queue_frames * channels);
    let control = Arc::new(Control {
        ticket,
        latest,
        cancel: AtomicBool::new(false),
        paused,
        eof: AtomicBool::new(false),
        failed: AtomicBool::new(false),
    });
    let c = control.clone();
    let decoder = thread::Builder::new()
        .name("wav-stream-reader".into())
        .spawn(move || {
            let _completion = DecoderCompletion(c.clone());
            let result = (|| {
                let mut block = vec![0f32; block_frames * channels];
                while !c.cancelled() {
                    let frames = reader.read_frames(&mut block)?;
                    if frames == 0 {
                        break;
                    }
                    let n = frames * channels;
                    let mut offset = 0;
                    while offset < n && !c.cancelled() {
                        let wrote = producer.push(&block[offset..n]);
                        offset += wrote;
                        if wrote == 0 {
                            thread::sleep(Duration::from_millis(1));
                        }
                    }
                }
                Ok(())
            })();
            c.failed.store(result.is_err(), Ordering::Release);
            c.eof.store(true, Ordering::Release);
            result
        })
        .map_err(|e| e.to_string())?;
    let c = control.clone();
    let w = wake.clone();
    let render = match thread::Builder::new()
        .name("wasapi-render".into())
        .spawn(move || unsafe {
            let gate = request.gate.clone();
            let result = render(request, start, consumer, c, w, progress);
            if let Some(gate) = gate {
                gate.finish_output(result.as_ref().is_ok_and(|ended| *ended));
            }
            result
        }) {
        Ok(thread) => thread,
        Err(e) => {
            control.cancel.store(true, Ordering::Release);
            let _ = decoder.join();
            return Err(e.to_string());
        }
    };
    Ok(Session {
        control,
        wake,
        render,
        decoder,
    })
}
unsafe fn render(
    request: PlayRequest,
    start: u64,
    mut pcm: Consumer,
    control: Arc<Control>,
    wake: Arc<Signal>,
    progress: Arc<Progress>,
) -> Result<bool> {
    unsafe {
        api(
            CoInitializeEx(None, COINIT_MULTITHREADED).ok(),
            "Initialize audio COM",
        )?;
        let _apartment = Apartment;
        let format = request.timeline.info();
        let stream = Stream::open(
            &request.output_id,
            format.channels,
            format.sample_rate,
            request.buffer_frames,
            &wake,
        )?;
        let mut task_index = 0;
        let _mmcss = windows::Win32::System::Threading::AvSetMmThreadCharacteristicsW(
            windows::core::w!("Audio"),
            &mut task_index,
        )
        .ok()
        .map(Mmcss);
        progress
            .device_buffer
            .store(stream.capacity, Ordering::Relaxed);
        let channels = usize::from(request.timeline.info().channels);
        let mut submitted = 0u64;
        let mut running = false;
        let mut start_phase = 0u32;
        let mut underrun = false;
        loop {
            if control.cancelled() {
                return Ok(false);
            }
            if control.failed.load(Ordering::Acquire) {
                return Err(Failure {
                    operation: "Decoder failed",
                    code: 0,
                });
            }
            let paused = control.paused.load(Ordering::Acquire);
            if paused {
                if running {
                    api(stream.client.Stop(), "Pause output")?;
                    running = false;
                }
                progress.state(State::Paused);
            }
            let padding = api(
                stream.client.GetCurrentPadding(),
                "Read output position (device may be disconnected)",
            )?;
            progress.frame.store(
                start + submitted.saturating_sub(u64::from(padding)),
                Ordering::Release,
            );
            if !paused {
                let available = stream
                    .capacity
                    .saturating_sub(padding)
                    .min((pcm.available() / channels) as u32);
                if available > 0 {
                    // Deadline-sensitive section: bounded queue reads, copies, and metering.
                    // No file I/O, mutex, allocation, decode, log or UI notification.
                    let buffer = api(stream.render.GetBuffer(available), "Acquire output buffer")?;
                    if buffer.is_null() {
                        return Err(Failure {
                            operation: "Null WASAPI output buffer",
                            code: 0,
                        });
                    }
                    let output = std::slice::from_raw_parts_mut(
                        buffer.cast::<f32>(),
                        available as usize * channels,
                    );
                    let n = pcm.pop(output);
                    debug_assert_eq!(n, output.len());
                    progress.meter.observe(output);
                    api(
                        stream.render.ReleaseBuffer(available, 0),
                        "Release output buffer",
                    )?;
                    submitted += u64::from(available);
                    underrun = false;
                }
                if control.eof.load(Ordering::Acquire)
                    && pcm.available() == 0
                    && padding == 0
                    && available == 0
                {
                    progress
                        .frame
                        .store(request.timeline.info().frames, Ordering::Release);
                    return Ok(true);
                }
                if !running && submitted > 0 {
                    start_phase += 1;
                    if let Some(gate) = &request.gate {
                        gate.wait(1, start_phase, || control.cancelled())
                            .map_err(|_| Failure {
                                operation: "Input/output synchronization cancelled or timed out",
                                code: 0,
                            })?;
                    }
                    api(stream.client.Start(), "Start output")?;
                    running = true;
                }
                if running && padding == 0 && available == 0 && !control.eof.load(Ordering::Acquire)
                {
                    if !underrun {
                        progress.underruns.fetch_add(1, Ordering::Relaxed);
                        underrun = true;
                    }
                    progress.state(State::Buffering);
                } else if running {
                    progress.state(State::Playing);
                } else {
                    progress.state(State::Starting);
                }
            }
            if WaitForSingleObject(wake.0, 50) == WAIT_FAILED {
                return Err(Failure {
                    operation: "Wait for audio event",
                    code: -1,
                });
            }
        }
    }
}
pub(super) fn coordinate(
    commands: Receiver<Command>,
    events: Sender<Event>,
    progress: Arc<Progress>,
    latest: Arc<AtomicU64>,
    paused: Arc<AtomicBool>,
    wake: Arc<Signal>,
) {
    let mut active: Option<(u64, Session)> = None;
    loop {
        if active.as_ref().is_some_and(|(_, s)| s.render.is_finished())
            && let Some((id, session)) = active.take()
        {
            match session.finish() {
                Ok(true) => progress.state(State::Ended),
                Ok(false) => progress.state(State::Stopped),
                Err(message) => {
                    progress.state(State::Error);
                    let _ = events.send(Event::Error { id, message });
                }
            }
        }
        match commands.recv_timeout(Duration::from_millis(10)) {
            Ok(Command::Play(id, request)) => {
                if id != latest.load(Ordering::Acquire) {
                    continue;
                }
                if let Some((_, s)) = active.take() {
                    let _ = s.finish();
                }
                progress.state(State::Starting);
                progress
                    .rate
                    .store(request.timeline.info().sample_rate, Ordering::Release);
                progress
                    .total
                    .store(request.timeline.info().frames, Ordering::Release);
                progress.frame.store(
                    request
                        .start_frame
                        .unwrap_or_else(|| {
                            ms_to_frames(request.start_ms, request.timeline.info().sample_rate)
                        })
                        .min(request.timeline.info().frames),
                    Ordering::Release,
                );
                progress.id.store(id, Ordering::Release);
                match start(
                    id,
                    request,
                    progress.clone(),
                    latest.clone(),
                    paused.clone(),
                    wake.clone(),
                ) {
                    Ok(session) => active = Some((id, session)),
                    Err(message) => {
                        progress.state(State::Error);
                        let _ = events.send(Event::Error { id, message });
                    }
                }
            }
            Ok(Command::Stop) => {
                if let Some((_, s)) = active.take() {
                    let _ = s.finish();
                }
                progress.state(State::Stopped);
            }
            Ok(Command::Shutdown) | Err(RecvTimeoutError::Disconnected) => {
                if let Some((_, s)) = active.take() {
                    let _ = s.finish();
                }
                break;
            }
            Err(RecvTimeoutError::Timeout) => (),
        }
    }
}
