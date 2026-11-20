//! Single-producer/single-consumer bounded PCM queue. Endpoints cannot be cloned.
use std::{
    cell::UnsafeCell,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
struct Inner {
    samples: Box<[UnsafeCell<f32>]>,
    read: AtomicUsize,
    write: AtomicUsize,
}
// SAFETY: each slot is owned by the producer until its release-published write index,
// then by the consumer until its release-published read index. Acquire loads transfer
// ownership. Only one non-clonable endpoint of each kind exists.
unsafe impl Sync for Inner {}
pub(crate) struct Producer(Arc<Inner>);
pub(crate) struct Consumer(Arc<Inner>);
pub(crate) fn ring(capacity: usize) -> (Producer, Consumer) {
    assert!(capacity.is_power_of_two());
    let inner = Arc::new(Inner {
        samples: (0..capacity).map(|_| UnsafeCell::new(0.)).collect(),
        read: AtomicUsize::new(0),
        write: AtomicUsize::new(0),
    });
    (Producer(inner.clone()), Consumer(inner))
}
impl Producer {
    pub fn push(&mut self, input: &[f32]) -> usize {
        let write = self.0.write.load(Ordering::Relaxed);
        let read = self.0.read.load(Ordering::Acquire);
        let n = input
            .len()
            .min(self.0.samples.len() - write.wrapping_sub(read));
        let mask = self.0.samples.len() - 1;
        for (i, value) in input[..n].iter().enumerate() {
            unsafe {
                self.0.samples[write.wrapping_add(i) & mask]
                    .get()
                    .write(*value);
            }
        }
        self.0.write.store(write.wrapping_add(n), Ordering::Release);
        n
    }
}
impl Consumer {
    pub fn available(&self) -> usize {
        self.0
            .write
            .load(Ordering::Acquire)
            .wrapping_sub(self.0.read.load(Ordering::Relaxed))
    }
    pub fn pop(&mut self, output: &mut [f32]) -> usize {
        let read = self.0.read.load(Ordering::Relaxed);
        let n = output
            .len()
            .min(self.0.write.load(Ordering::Acquire).wrapping_sub(read));
        let mask = self.0.samples.len() - 1;
        for (i, value) in output[..n].iter_mut().enumerate() {
            unsafe {
                *value = self.0.samples[read.wrapping_add(i) & mask].get().read();
            }
        }
        self.0.read.store(read.wrapping_add(n), Ordering::Release);
        n
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concurrent_order_and_backpressure() {
        let (mut p, mut c) = ring(128);
        let writer = std::thread::spawn(move || {
            for i in 0..100000 {
                while p.push(&[i as f32]) == 0 {
                    std::thread::yield_now();
                }
            }
        });
        for i in 0..100000 {
            let mut value = [0.];
            while c.pop(&mut value) == 0 {
                std::thread::yield_now();
            }
            assert_eq!(value[0], i as f32);
        }
        writer.join().unwrap();
        assert_eq!(c.available(), 0);
    }
    #[test]
    fn bounded_wrap() {
        let (mut p, mut c) = ring(4);
        assert_eq!(p.push(&[1., 2., 3., 4., 5.]), 4);
        assert_eq!(p.push(&[6.]), 0);
        let mut a = [0.; 3];
        c.pop(&mut a);
        assert_eq!(a, [1., 2., 3.]);
        assert_eq!(p.push(&[5., 6., 7.]), 3);
        let mut b = [0.; 4];
        c.pop(&mut b);
        assert_eq!(b, [4., 5., 6., 7.]);
    }
}
