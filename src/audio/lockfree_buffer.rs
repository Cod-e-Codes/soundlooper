// src/audio/lockfree_buffer.rs
// Lock-free audio buffer for real-time audio processing
// Add to Cargo.toml: rtrb = "0.3"

use rtrb::{Consumer, Producer, RingBuffer};
use std::sync::atomic::{AtomicBool, Ordering};

/// Lock-free audio buffer for passing audio data between threads
/// without blocking the real-time audio thread
pub struct LockFreeAudioBuffer {
    producer: Producer<f32>,
    consumer: Consumer<f32>,
    capacity: usize,
    overrun: AtomicBool,
}

// SAFETY: The rtrb RingBuffer is designed to be thread-safe for single producer/consumer
unsafe impl Send for LockFreeAudioBuffer {}
unsafe impl Sync for LockFreeAudioBuffer {}

impl LockFreeAudioBuffer {
    /// Create a new lock-free audio buffer with specified capacity
    /// Capacity should be at least 2x your maximum buffer size
    pub fn new(capacity: usize) -> Self {
        let (producer, consumer) = RingBuffer::new(capacity);
        Self {
            producer,
            consumer,
            capacity,
            overrun: AtomicBool::new(false),
        }
    }

    /// Write audio samples (non-blocking, audio thread safe)
    /// Returns true if all samples were written
    pub fn write(&mut self, samples: &[f32]) -> bool {
        let mut all_written = true;
        for &sample in samples {
            match self.producer.push(sample) {
                Ok(()) => {}
                Err(_) => {
                    self.overrun.store(true, Ordering::Relaxed);
                    all_written = false;
                    break;
                }
            }
        }
        all_written
    }

    /// Read audio samples (non-blocking, audio thread safe)
    /// Returns the number of samples actually read
    pub fn read(&mut self, output: &mut [f32]) -> usize {
        let mut count = 0;
        for sample in output.iter_mut() {
            match self.consumer.pop() {
                Ok(value) => {
                    *sample = value;
                    count += 1;
                }
                Err(_) => break,
            }
        }
        count
    }

    /// Check if buffer overrun occurred (samples were dropped)
    pub fn check_and_clear_overrun(&self) -> bool {
        self.overrun.swap(false, Ordering::Relaxed)
    }

    /// Get number of samples available to read
    pub fn available(&self) -> usize {
        self.consumer.slots()
    }

    /// Get capacity
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

/// Thread-safe wrapper for lock-free buffer using `Arc<Mutex>`
/// This allows the buffer to be shared between threads while maintaining
/// the lock-free performance in the audio thread
pub struct SharedLockFreeBuffer {
    buffer: std::sync::Mutex<LockFreeAudioBuffer>,
}

impl SharedLockFreeBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: std::sync::Mutex::new(LockFreeAudioBuffer::new(capacity)),
        }
    }

    /// Write samples (non-blocking, will fail if lock is contended)
    pub fn try_write(&self, samples: &[f32]) -> bool {
        if let Ok(mut buf) = self.buffer.try_lock() {
            buf.write(samples)
        } else {
            false
        }
    }

    /// Read samples (non-blocking, will return 0 if lock is contended)
    pub fn try_read(&self, output: &mut [f32]) -> usize {
        if let Ok(mut buf) = self.buffer.try_lock() {
            buf.read(output)
        } else {
            0
        }
    }

    /// Check for overruns
    pub fn check_overrun(&self) -> bool {
        if let Ok(buf) = self.buffer.try_lock() {
            buf.check_and_clear_overrun()
        } else {
            false
        }
    }
}

/// Bidirectional lock-free audio buffer pair for input/output
pub struct AudioBufferPair {
    pub input: SharedLockFreeBuffer,
    pub output: SharedLockFreeBuffer,
}

impl AudioBufferPair {
    pub fn new(capacity: usize) -> Self {
        Self {
            input: SharedLockFreeBuffer::new(capacity),
            output: SharedLockFreeBuffer::new(capacity),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lockfree_write_read() {
        let mut buffer = LockFreeAudioBuffer::new(1024);

        // Write samples
        let input = vec![0.1, 0.2, 0.3, 0.4];
        assert!(buffer.write(&input));

        // Read samples
        let mut output = vec![0.0; 4];
        assert_eq!(buffer.read(&mut output), 4);
        assert_eq!(output, input);
    }

    #[test]
    fn test_overrun_detection() {
        let mut buffer = LockFreeAudioBuffer::new(8);

        // Fill buffer
        let input = vec![1.0; 8];
        assert!(buffer.write(&input));

        // Try to overfill
        let overflow = vec![2.0; 8];
        assert!(!buffer.write(&overflow));

        // Check overrun flag
        assert!(buffer.check_and_clear_overrun());
        assert!(!buffer.check_and_clear_overrun()); // Should clear
    }

    #[test]
    fn test_concurrent_access() {
        use std::sync::Arc;
        use std::thread;

        let buffer = Arc::new(std::sync::Mutex::new(LockFreeAudioBuffer::new(4096)));

        let buffer_writer = Arc::clone(&buffer);
        let writer = thread::spawn(move || {
            for _ in 0..100 {
                let data = vec![1.0; 64];
                buffer_writer.lock().unwrap().write(&data);
            }
        });

        let buffer_reader = Arc::clone(&buffer);
        let reader = thread::spawn(move || {
            let mut total_read = 0;
            for _ in 0..100 {
                let mut output = vec![0.0; 64];
                total_read += buffer_reader.lock().unwrap().read(&mut output);
                thread::sleep(std::time::Duration::from_micros(100));
            }
            total_read
        });

        writer.join().unwrap();
        let total = reader.join().unwrap();
        assert!(total > 0);
    }
}
