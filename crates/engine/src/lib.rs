//! Streaming selected-file transport. Disk decoding and WASAPI rendering are separate.
#[cfg(windows)]
pub mod calibration;
mod gate;
mod meter;
#[cfg(windows)]
pub mod recording;
mod ring;
pub use gate::StartGate;
pub use meter::Level;
#[cfg(windows)]
mod wasapi;
use sonitrelix_audio_io::Timeline;
use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender},
    },
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum State {
    Stopped = 0,
    Starting = 1,
    Playing = 2,
    Paused = 3,
    Buffering = 4,
    Ended = 5,
    Error = 6,
}
impl State {
    fn decode(value: u8) -> Self {
        match value {
            1 => Self::Starting,
            2 => Self::Playing,
            3 => Self::Paused,
            4 => Self::Buffering,
            5 => Self::Ended,
            6 => Self::Error,
            _ => Self::Stopped,
        }
    }
}
#[derive(Clone, Debug)]
pub struct PlayRequest {
    pub gate: Option<Arc<StartGate>>,
    pub timeline: Timeline,
    pub start_ms: u64,
    pub start_frame: Option<u64>,
    pub loop_frames: Option<std::ops::Range<u64>>,
    pub output_id: String,
    pub buffer_frames: u32,
    pub paused: bool,
}
impl PlayRequest {
    fn first_frame(&self) -> u64 {
        let info = self.timeline.info();
        self.loop_frames.as_ref().map_or_else(
            || {
                self.start_frame
                    .unwrap_or_else(|| {
                        sonitrelix_audio_io::ms_to_frames(self.start_ms, info.sample_rate)
                    })
                    .min(info.frames)
            },
            |range| range.start,
        )
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Snapshot {
    pub id: u64,
    pub state: State,
    pub frame: u64,
    pub total: u64,
    pub sample_rate: u32,
    pub underruns: u64,
    pub device_buffer_frames: u32,
}
impl Snapshot {
    pub fn position_ms(&self) -> u64 {
        sonitrelix_audio_io::frames_to_ms(self.frame, self.sample_rate)
    }
}
pub enum Event {
    Error { id: u64, message: String },
}
struct Progress {
    id: AtomicU64,
    state: AtomicU8,
    frame: AtomicU64,
    total: AtomicU64,
    rate: AtomicU32,
    underruns: AtomicU64,
    device_buffer: AtomicU32,
    meter: meter::PeakMeter,
}
struct Completion(Arc<AtomicBool>);
impl Drop for Completion {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}
impl Progress {
    fn new() -> Self {
        Self {
            id: AtomicU64::new(0),
            state: AtomicU8::new(0),
            frame: AtomicU64::new(0),
            total: AtomicU64::new(0),
            rate: AtomicU32::new(48000),
            underruns: AtomicU64::new(0),
            device_buffer: AtomicU32::new(0),
            meter: Default::default(),
        }
    }
    fn state(&self, state: State) {
        self.state.store(state as u8, Ordering::Release);
    }
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            id: self.id.load(Ordering::Acquire),
            state: State::decode(self.state.load(Ordering::Acquire)),
            frame: self.frame.load(Ordering::Acquire),
            total: self.total.load(Ordering::Acquire),
            sample_rate: self.rate.load(Ordering::Acquire),
            underruns: self.underruns.load(Ordering::Relaxed),
            device_buffer_frames: self.device_buffer.load(Ordering::Relaxed),
        }
    }
}
enum Command {
    Play(u64, PlayRequest),
    Stop,
    Shutdown,
}
pub struct Engine {
    tx: Sender<Command>,
    events: Receiver<Event>,
    progress: Arc<Progress>,
    latest: Arc<AtomicU64>,
    paused: Arc<AtomicBool>,
    done: Arc<AtomicBool>,
    closing: bool,
    #[cfg(windows)]
    wake: Arc<wasapi::Signal>,
}
impl Engine {
    pub fn new() -> io::Result<Self> {
        let (tx, rx) = mpsc::channel();
        let (events_tx, events) = mpsc::channel();
        let progress = Arc::new(Progress::new());
        let latest = Arc::new(AtomicU64::new(0));
        let paused = Arc::new(AtomicBool::new(false));
        let done = Arc::new(AtomicBool::new(false));
        #[cfg(windows)]
        {
            let wake = Arc::new(wasapi::Signal::new()?);
            let (p, l, b, d, w) = (
                progress.clone(),
                latest.clone(),
                paused.clone(),
                done.clone(),
                wake.clone(),
            );
            std::thread::Builder::new()
                .name("audio-coordinator".into())
                .spawn(move || {
                    let _completion = Completion(d);
                    wasapi::coordinate(rx, events_tx, p, l, b, w);
                })?;
            Ok(Self {
                tx,
                events,
                progress,
                latest,
                paused,
                done,
                closing: false,
                wake,
            })
        }
        #[cfg(not(windows))]
        {
            let _ = (tx, rx, events_tx, events, progress, latest, paused, done);
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "WASAPI requires Windows",
            ))
        }
    }
    pub fn play(&mut self, request: PlayRequest) -> io::Result<u64> {
        if request.loop_frames.as_ref().is_some_and(|range| {
            range.start >= range.end || range.end > request.timeline.info().frames
        }) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid loop range",
            ));
        }
        if self.closing {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "Audio engine is shutting down",
            ));
        }
        self.paused.store(request.paused, Ordering::Release);
        let id = self.latest.fetch_add(1, Ordering::AcqRel) + 1;
        self.tx
            .send(Command::Play(id, request))
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "Audio worker stopped"))?;
        self.wake();
        Ok(id)
    }
    pub fn pause(&self, paused: bool) {
        self.paused.store(paused, Ordering::Release);
        self.wake();
    }
    pub fn stop(&self) {
        self.latest.fetch_add(1, Ordering::AcqRel);
        let _ = self.tx.send(Command::Stop);
        self.wake();
    }
    pub fn snapshot(&self) -> Snapshot {
        self.progress.snapshot()
    }
    pub fn output_level(&self) -> Level {
        self.progress.meter.read()
    }
    pub fn try_event(&self) -> Option<Event> {
        self.events.try_recv().ok()
    }
    pub fn shutdown(&mut self) {
        if !self.closing {
            self.closing = true;
            self.latest.fetch_add(1, Ordering::AcqRel);
            let _ = self.tx.send(Command::Shutdown);
            self.wake();
        }
    }
    pub fn shutdown_complete(&self) -> bool {
        self.done.load(Ordering::Acquire)
    }
    fn wake(&self) {
        #[cfg(windows)]
        self.wake.set();
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::{
        fs, thread,
        time::{Duration, Instant},
    };
    fn fixture(path: &std::path::Path, rate: u32, channels: u16) {
        let frames = rate * 3;
        let align = channels * 2;
        let bytes = frames * u32::from(align);
        let mut f = fs::File::create(path).unwrap();
        use std::io::Write;
        f.write_all(b"RIFF").unwrap();
        f.write_all(&(36u32 + bytes).to_le_bytes()).unwrap();
        f.write_all(b"WAVEfmt ").unwrap();
        f.write_all(&16u32.to_le_bytes()).unwrap();
        f.write_all(&1u16.to_le_bytes()).unwrap();
        f.write_all(&channels.to_le_bytes()).unwrap();
        f.write_all(&rate.to_le_bytes()).unwrap();
        f.write_all(&(rate * u32::from(align)).to_le_bytes())
            .unwrap();
        f.write_all(&align.to_le_bytes()).unwrap();
        f.write_all(&16u16.to_le_bytes()).unwrap();
        f.write_all(b"data").unwrap();
        f.write_all(&bytes.to_le_bytes()).unwrap();
        f.write_all(&vec![0; bytes as usize]).unwrap();
    }
    fn wait(engine: &Engine, id: u64, predicate: impl Fn(Snapshot) -> bool) -> Snapshot {
        let deadline = Instant::now() + Duration::from_secs(6);
        loop {
            if let Some(Event::Error { message, .. }) = engine.try_event() {
                panic!("{message}");
            }
            let s = engine.snapshot();
            if s.id == id && predicate(s) {
                return s;
            }
            assert!(Instant::now() < deadline, "Timed out: {s:?}");
            thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    #[ignore = "Opens the selected real WASAPI output and renders a short silence fixture; no microphone"]
    fn real_wasapi_play_pause_seek_end_and_no_device_fallback() {
        let dir = std::env::temp_dir().join(format!("sonitrelix-wasapi-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("silence.wav");
        fixture(&path, 44100, 2);
        let catalog = sonitrelix_devices::enumerate().unwrap();
        let device = catalog
            .outputs
            .iter()
            .find(|d| d.is_default)
            .expect("An active multimedia output is required");
        let expected = sonitrelix_audio_io::WavReader::open(&path).unwrap().info;
        let mut engine = Engine::new().unwrap();
        let mix = sonitrelix_audio_io::LiveMix::new(vec![true]);
        let timeline = Timeline::mix(
            expected.sample_rate,
            vec![sonitrelix_audio_io::MixTrack {
                timeline: Timeline::single(path, expected).unwrap(),
                gain: 1.0,
                pan: 0.0,
            }],
        )
        .unwrap()
        .with_live_mix(mix.clone())
        .unwrap();
        let mut request = PlayRequest {
            gate: None,
            timeline,
            start_ms: 0,
            start_frame: None,
            loop_frames: None,
            output_id: device.id.clone(),
            buffer_frames: 256,
            paused: false,
        };
        let id = engine.play(request.clone()).unwrap();
        let playing = wait(&engine, id, |s| {
            s.state == State::Playing && s.position_ms() > 100
        });
        assert!(playing.device_buffer_frames > 0);
        mix.update([false].into_iter());
        let muted = wait(&engine, id, |s| {
            s.state == State::Playing && s.position_ms() > playing.position_ms() + 150
        });
        mix.update([true].into_iter());
        wait(&engine, id, |s| {
            s.state == State::Playing && s.position_ms() > muted.position_ms() + 150
        });
        engine.pause(true);
        wait(&engine, id, |s| s.state == State::Paused);
        thread::sleep(Duration::from_millis(60));
        let paused = engine.snapshot().frame;
        thread::sleep(Duration::from_millis(120));
        assert_eq!(engine.snapshot().frame, paused);
        request.start_ms = 1200;
        request.paused = true;
        let id = engine.play(request.clone()).unwrap();
        let seek = wait(&engine, id, |s| s.state == State::Paused);
        assert_eq!(seek.frame, 52920);
        request.start_frame = Some(52921);
        let id = engine.play(request.clone()).unwrap();
        let seek = wait(&engine, id, |s| s.state == State::Paused);
        assert_eq!(seek.frame, 52921);
        request.start_frame = None;
        engine.pause(false);
        wait(&engine, id, |s| {
            s.state == State::Playing && s.frame > 52920
        });
        request.start_ms = 2900;
        request.paused = false;
        let id = engine.play(request.clone()).unwrap();
        let end = wait(&engine, id, |s| s.state == State::Ended);
        assert_eq!(end.frame, expected.frames);
        let range = 52920..57330;
        request.loop_frames = Some(range.clone());
        let id = engine.play(request.clone()).unwrap();
        wait(&engine, id, |s| s.state == State::Playing);
        let repeat_deadline = Instant::now() + Duration::from_millis(420);
        let mut previous = engine.snapshot().frame;
        let mut wraps = 0;
        while Instant::now() < repeat_deadline {
            let snapshot = engine.snapshot();
            assert!(range.contains(&snapshot.frame), "{snapshot:?}");
            assert_eq!(snapshot.underruns, 0);
            if snapshot.frame < previous {
                wraps += 1;
            }
            previous = snapshot.frame;
            thread::sleep(Duration::from_millis(5));
        }
        assert!(
            wraps >= 3,
            "audio should stay active across repeated loop boundaries"
        );
        request.loop_frames = None;
        request.output_id = "sonitrelix-nonexistent-endpoint".into();
        let id = engine.play(request.clone()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(Event::Error {
                id: failed,
                message,
            }) = engine.try_event()
            {
                assert_eq!(failed, id);
                assert!(message.contains("selected output"));
                break;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
        let mono = dir.join("mono.wav");
        fixture(&mono, 48000, 1);
        let format = sonitrelix_audio_io::WavReader::open(&mono).unwrap().info;
        request.timeline = Timeline::single(mono, format).unwrap();
        request.output_id = device.id.clone();
        request.start_ms = 2900;
        let id = engine.play(request.clone()).unwrap();
        let end = wait(&engine, id, |s| s.state == State::Ended);
        assert_eq!(end.sample_rate, 48000);
        assert_eq!(end.frame, request.timeline.info().frames);
        engine.shutdown();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !engine.shutdown_complete() {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        drop(engine);
        fs::remove_dir_all(dir).unwrap();
    }
}
