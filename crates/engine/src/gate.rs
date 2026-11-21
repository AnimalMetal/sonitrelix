//! Cancellable coordination before stream start/resume, outside packet processing.
use std::{
    io,
    sync::atomic::{AtomicU8, AtomicU32, AtomicU64, Ordering},
    time::{Duration, Instant},
};
#[derive(Debug)]
pub struct StartGate {
    ready: [AtomicU32; 2],
    go: AtomicU64,
    epoch: Instant,
    output: AtomicU8,
}
impl Default for StartGate {
    fn default() -> Self {
        Self::new()
    }
}
impl StartGate {
    pub fn new() -> Self {
        Self {
            ready: [AtomicU32::new(0), AtomicU32::new(0)],
            go: AtomicU64::new(0),
            epoch: Instant::now(),
            output: AtomicU8::new(0),
        }
    }
    pub fn finish_output(&self, ended: bool) {
        self.output
            .store(if ended { 1 } else { 2 }, Ordering::Release);
    }
    pub fn wait(&self, side: u8, phase: u32, cancel: impl Fn() -> bool) -> io::Result<()> {
        if ![1, 2].contains(&side) || phase == 0 || phase > 65535 {
            return Err(io::Error::other("Invalid transport phase"));
        }
        self.ready[(side - 1) as usize].store(phase, Ordering::Release);
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if cancel() {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "Transport start cancelled",
                ));
            }
            let output = self.output.load(Ordering::Acquire);
            if output == 2 {
                return Err(io::Error::other("Backing output stopped unexpectedly"));
            }
            if side == 2 && phase > 1 && output == 1 {
                return Ok(());
            }
            if Instant::now() > deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Input/output synchronization timed out",
                ));
            }
            let now = self.epoch.elapsed().as_micros() as u64;
            let mut go = self.go.load(Ordering::Acquire);
            if self
                .ready
                .iter()
                .all(|r| r.load(Ordering::Acquire) >= phase)
                && go >> 48 < u64::from(phase)
            {
                let proposed = (u64::from(phase) << 48) | ((now + 100000) & 0x0000ffffffffffff);
                if self
                    .go
                    .compare_exchange(go, proposed, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    go = proposed;
                }
            }
            if go >> 48 == u64::from(phase) && now >= (go & 0x0000ffffffffffff) {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    #[test]
    fn peers_start_resume_together_cancel_and_handle_backing_end() {
        let g = Arc::new(StartGate::new());
        let other = g.clone();
        let a = std::thread::spawn(move || {
            let mut times = vec![];
            for phase in 1..=3 {
                other.wait(1, phase, || false).unwrap();
                times.push(Instant::now());
            }
            times
        });
        let mut times = vec![];
        for phase in 1..=3 {
            std::thread::sleep(Duration::from_millis(20));
            g.wait(2, phase, || false).unwrap();
            times.push(Instant::now());
        }
        for (a, b) in a.join().unwrap().iter().zip(times) {
            assert!(
                b.checked_duration_since(*a)
                    .unwrap_or_else(|| a.duration_since(b))
                    < Duration::from_millis(50)
            );
        }
        assert!(StartGate::new().wait(1, 1, || true).is_err());
        g.finish_output(true);
        g.wait(2, 4, || false).unwrap();
        g.finish_output(false);
        assert!(g.wait(2, 5, || false).is_err());
    }
}
