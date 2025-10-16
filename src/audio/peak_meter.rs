// src/audio/peak_meter.rs
// Real-time audio level metering with ballistics

use std::sync::atomic::{AtomicU32, Ordering};

/// Audio peak meter with proper ballistics (attack/release)
#[derive(Debug)]
pub struct PeakMeter {
    peak_level: AtomicU32,        // Current peak (as f32 bits)
    peak_hold: AtomicU32,         // Peak hold value
    rms_level: AtomicU32,         // RMS level
    peak_hold_counter: AtomicU32, // Frames to hold peak
}

impl PeakMeter {
    const PEAK_HOLD_FRAMES: u32 = 30; // Hold peak for ~0.5s at 60fps
    const RMS_WINDOW_SIZE: usize = 2048;

    pub fn new() -> Self {
        Self {
            peak_level: AtomicU32::new(0),
            peak_hold: AtomicU32::new(0),
            rms_level: AtomicU32::new(0),
            peak_hold_counter: AtomicU32::new(0),
        }
    }

    /// Update meter with new audio samples (call from audio thread)
    pub fn update(&self, samples: &[f32]) {
        if samples.is_empty() {
            return;
        }

        // Calculate peak
        let peak = samples.iter().map(|&s| s.abs()).fold(0.0f32, f32::max);

        // Update peak level with fast attack
        let current_peak = f32::from_bits(self.peak_level.load(Ordering::Relaxed));
        let new_peak = if peak > current_peak {
            peak // Instant attack
        } else {
            current_peak * 0.95 + peak * 0.05 // Slow release
        };
        self.peak_level.store(new_peak.to_bits(), Ordering::Relaxed);

        // Update peak hold
        let current_hold = f32::from_bits(self.peak_hold.load(Ordering::Relaxed));
        if peak > current_hold {
            self.peak_hold.store(peak.to_bits(), Ordering::Relaxed);
            self.peak_hold_counter
                .store(Self::PEAK_HOLD_FRAMES, Ordering::Relaxed);
        } else {
            let counter = self.peak_hold_counter.load(Ordering::Relaxed);
            if counter > 0 {
                self.peak_hold_counter.store(counter - 1, Ordering::Relaxed);
            } else {
                // Release peak hold
                self.peak_hold.store(new_peak.to_bits(), Ordering::Relaxed);
            }
        }

        // Calculate RMS
        let rms_sum: f32 = samples
            .iter()
            .take(Self::RMS_WINDOW_SIZE.min(samples.len()))
            .map(|&s| s * s)
            .sum();
        let rms = (rms_sum / samples.len().min(Self::RMS_WINDOW_SIZE) as f32).sqrt();

        // Smooth RMS
        let current_rms = f32::from_bits(self.rms_level.load(Ordering::Relaxed));
        let new_rms = current_rms * 0.8 + rms * 0.2;
        self.rms_level.store(new_rms.to_bits(), Ordering::Relaxed);
    }

    /// Get current peak level (0.0 - 1.0+)
    pub fn get_peak(&self) -> f32 {
        f32::from_bits(self.peak_level.load(Ordering::Relaxed))
    }

    /// Get peak hold value
    pub fn get_peak_hold(&self) -> f32 {
        f32::from_bits(self.peak_hold.load(Ordering::Relaxed))
    }

    /// Get RMS level
    pub fn get_rms(&self) -> f32 {
        f32::from_bits(self.rms_level.load(Ordering::Relaxed))
    }

    /// Convert linear level to dB
    pub fn to_db(level: f32) -> f32 {
        if level <= 0.0 {
            -96.0 // Silence
        } else {
            20.0 * level.log10()
        }
    }

    /// Get meter color based on level
    pub fn get_color(level: f32) -> MeterColor {
        if level >= 1.0 {
            MeterColor::Clip // Red - clipping
        } else if level >= 0.9 {
            MeterColor::Hot // Orange - very hot
        } else if level >= 0.7 {
            MeterColor::Warn // Yellow - getting hot
        } else {
            MeterColor::Normal // Green - normal
        }
    }

    /// Reset all meter values
    pub fn reset(&self) {
        self.peak_level.store(0, Ordering::Relaxed);
        self.peak_hold.store(0, Ordering::Relaxed);
        self.rms_level.store(0, Ordering::Relaxed);
        self.peak_hold_counter.store(0, Ordering::Relaxed);
    }
}

impl Default for PeakMeter {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for PeakMeter {
    fn clone(&self) -> Self {
        Self {
            peak_level: AtomicU32::new(self.peak_level.load(Ordering::Relaxed)),
            peak_hold: AtomicU32::new(self.peak_hold.load(Ordering::Relaxed)),
            rms_level: AtomicU32::new(self.rms_level.load(Ordering::Relaxed)),
            peak_hold_counter: AtomicU32::new(self.peak_hold_counter.load(Ordering::Relaxed)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeterColor {
    Normal, // Green (0.0 - 0.7)
    Warn,   // Yellow (0.7 - 0.9)
    Hot,    // Orange (0.9 - 1.0)
    Clip,   // Red (>= 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peak_detection() {
        let meter = PeakMeter::new();

        // Silent samples
        let silent = vec![0.0; 100];
        meter.update(&silent);
        assert_eq!(meter.get_peak(), 0.0);

        // Full scale
        let full = vec![1.0; 100];
        meter.update(&full);
        assert!(meter.get_peak() > 0.9);
    }

    #[test]
    fn test_peak_hold() {
        let meter = PeakMeter::new();

        // Send peak
        let peak = vec![0.8; 10];
        meter.update(&peak);
        let initial_hold = meter.get_peak_hold();
        assert!(initial_hold >= 0.8);

        // Send lower level
        let lower = vec![0.2; 10];
        meter.update(&lower);

        // Peak hold should still be high
        assert!(meter.get_peak_hold() >= 0.7);
    }

    #[test]
    fn test_color_mapping() {
        assert_eq!(PeakMeter::get_color(0.5), MeterColor::Normal);
        assert_eq!(PeakMeter::get_color(0.8), MeterColor::Warn);
        assert_eq!(PeakMeter::get_color(0.95), MeterColor::Hot);
        assert_eq!(PeakMeter::get_color(1.1), MeterColor::Clip);
    }

    #[test]
    fn test_db_conversion() {
        assert_eq!(PeakMeter::to_db(1.0), 0.0);
        assert!((PeakMeter::to_db(0.5) - (-6.02)).abs() < 0.1);
        assert_eq!(PeakMeter::to_db(0.0), -96.0);
    }
}
