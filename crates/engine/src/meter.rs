use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

#[derive(Clone, Copy, Debug, Default)]
pub struct Level {
    pub peak: f32,
    /// Full scale has been reached since this stream started.
    pub clipped: bool,
}

#[derive(Default)]
pub(super) struct PeakMeter {
    peak: AtomicU32,
    clipped: AtomicBool,
}

impl PeakMeter {
    pub(super) fn observe(&self, samples: &[f32]) {
        let mut peak = 0.0_f32;
        for &sample in samples {
            if sample.is_finite() {
                peak = peak.max(sample.abs());
            }
        }
        // Nonnegative finite floats have the same order as their integer bit patterns.
        self.peak.fetch_max(peak.to_bits(), Ordering::Relaxed);
        if peak >= 32767. / 32768. {
            self.clipped.store(true, Ordering::Relaxed);
        }
    }

    pub(super) fn read(&self) -> Level {
        Level {
            peak: f32::from_bits(self.peak.swap(0, Ordering::Relaxed)),
            clipped: self.clipped(),
        }
    }

    pub(super) fn clipped(&self) -> bool {
        self.clipped.load(Ordering::Relaxed)
    }

    pub(super) fn reset(&self) {
        self.peak.store(0, Ordering::Relaxed);
        self.clipped.store(false, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readings_take_the_peak_and_keep_the_clip_indicator() {
        let meter = PeakMeter::default();
        meter.observe(&[0.1, -0.5, f32::NAN]);
        meter.observe(&[0.25]);
        assert_eq!(meter.read().peak, 0.5);
        assert_eq!(meter.read().peak, 0.);
        meter.observe(&[1.25]);
        let level = meter.read();
        assert_eq!(level.peak, 1.25);
        assert!(level.clipped);
        assert!(meter.read().clipped);
        meter.reset();
        assert!(!meter.read().clipped);
    }
}
