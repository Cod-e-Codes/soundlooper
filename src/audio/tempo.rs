use std::time::Instant;

#[derive(Debug, Clone)]
pub struct TempoEngine {
    pub bpm: f64,
    pub beats_per_measure: u32,
    pub sample_rate: u32,
    pub samples_per_beat: usize,
    pub samples_per_measure: usize,
    pub global_position: usize, // Position in samples since start
    pub last_tap_time: Option<Instant>,
    pub tap_times: Vec<Instant>,
    pub count_in_active: bool,
    pub count_in_remaining_beats: u32,
    pub count_in_layer: Option<usize>,
}

impl TempoEngine {
    pub fn new(sample_rate: u32, bpm: f64, beats_per_measure: u32) -> Self {
        let samples_per_beat = Self::calculate_samples_per_beat(sample_rate, bpm);
        let samples_per_measure = samples_per_beat * beats_per_measure as usize;

        Self {
            bpm,
            beats_per_measure,
            sample_rate,
            samples_per_beat,
            samples_per_measure,
            global_position: 0,
            last_tap_time: None,
            tap_times: Vec::new(),
            count_in_active: false,
            count_in_remaining_beats: 0,
            count_in_layer: None,
        }
    }

    fn calculate_samples_per_beat(sample_rate: u32, bpm: f64) -> usize {
        ((60.0 / bpm) * sample_rate as f64) as usize
    }

    pub fn set_bpm(&mut self, bpm: f64) {
        self.bpm = bpm.clamp(20.0, 300.0); // Clamp to reasonable range
        self.samples_per_beat = Self::calculate_samples_per_beat(self.sample_rate, self.bpm);
        self.samples_per_measure = self.samples_per_beat * self.beats_per_measure as usize;
    }

    pub fn tap_tempo(&mut self) {
        let now = Instant::now();

        if let Some(last_tap) = self.last_tap_time {
            let elapsed = now.duration_since(last_tap).as_secs_f64();

            // If tap is within reasonable range (20-300 BPM equivalent)
            if (0.2..=3.0).contains(&elapsed) {
                self.tap_times.push(now);

                // Keep only the last 4 taps for averaging
                if self.tap_times.len() > 4 {
                    self.tap_times.remove(0);
                }

                // Calculate average BPM from recent taps
                if self.tap_times.len() >= 2 {
                    let total_time = self
                        .tap_times
                        .last()
                        .unwrap()
                        .duration_since(self.tap_times[0])
                        .as_secs_f64();
                    let intervals = (self.tap_times.len() - 1) as f64;
                    let avg_interval = total_time / intervals;
                    let new_bpm = 60.0 / avg_interval;
                    self.set_bpm(new_bpm);
                }
            } else {
                // Reset if too long between taps
                self.tap_times.clear();
                self.tap_times.push(now);
            }
        } else {
            // First tap
            self.tap_times.clear();
            self.tap_times.push(now);
        }

        self.last_tap_time = Some(now);
    }

    pub fn advance(&mut self, sample_count: usize) {
        let previous_position = self.global_position;
        self.global_position = self.global_position.saturating_add(sample_count);

        // Handle count-in
        if self.count_in_active {
            let beats_elapsed = self.global_position / self.samples_per_beat;
            let previous_beats = previous_position / self.samples_per_beat;

            // Check if we've crossed a beat boundary
            if beats_elapsed > previous_beats {
                let beats_crossed = beats_elapsed - previous_beats;
                if self.count_in_remaining_beats >= beats_crossed as u32 {
                    self.count_in_remaining_beats -= beats_crossed as u32;
                } else {
                    self.count_in_remaining_beats = 0;
                }

                // Count-in complete
                if self.count_in_remaining_beats == 0 {
                    self.count_in_active = false;
                }
            }
        }
    }

    pub fn start_count_in(&mut self, layer_id: usize, beats: u32) {
        self.count_in_active = true;
        self.count_in_remaining_beats = beats;
        self.count_in_layer = Some(layer_id);
    }

    pub fn cancel_count_in(&mut self) {
        self.count_in_active = false;
        self.count_in_remaining_beats = 0;
        self.count_in_layer = None;
    }

    pub fn get_next_measure_start(&self) -> usize {
        let current_measure = self.global_position / self.samples_per_measure;
        (current_measure + 1) * self.samples_per_measure
    }

    pub fn get_samples_until_next_measure(&self) -> usize {
        let next_measure = self.get_next_measure_start();
        next_measure.saturating_sub(self.global_position)
    }

    pub fn get_current_beat(&self) -> u32 {
        ((self.global_position / self.samples_per_beat) % self.beats_per_measure as usize) as u32
            + 1
    }

    pub fn get_current_measure(&self) -> usize {
        self.global_position / self.samples_per_measure
    }

    pub fn is_on_measure_boundary(&self, tolerance_samples: usize) -> bool {
        let position_in_measure = self.global_position % self.samples_per_measure;
        position_in_measure <= tolerance_samples
            || position_in_measure >= (self.samples_per_measure - tolerance_samples)
    }

    pub fn reset_position(&mut self) {
        self.global_position = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tempo_calculation() {
        let tempo = TempoEngine::new(44100, 120.0, 4);

        // At 120 BPM, each beat should be 0.5 seconds
        // 0.5 * 44100 = 22050 samples per beat
        assert_eq!(tempo.samples_per_beat, 22050);

        // 4 beats per measure = 88200 samples
        assert_eq!(tempo.samples_per_measure, 88200);
    }

    #[test]
    fn test_beat_tracking() {
        let mut tempo = TempoEngine::new(44100, 120.0, 4);

        assert_eq!(tempo.get_current_beat(), 1);

        // Advance by one beat
        tempo.advance(22050);
        assert_eq!(tempo.get_current_beat(), 2);

        // Advance to beat 4
        tempo.advance(44100);
        assert_eq!(tempo.get_current_beat(), 4);

        // Advance to next measure (should wrap to beat 1)
        tempo.advance(22050);
        assert_eq!(tempo.get_current_beat(), 1);
    }

    #[test]
    fn test_count_in() {
        let mut tempo = TempoEngine::new(44100, 120.0, 4);

        tempo.start_count_in(0, 4);
        assert!(tempo.count_in_active);
        assert_eq!(tempo.count_in_remaining_beats, 4);

        // Advance by one beat
        tempo.advance(22050);
        assert_eq!(tempo.count_in_remaining_beats, 3);

        // Advance by remaining beats
        tempo.advance(66150); // 3 beats
        assert!(!tempo.count_in_active);
        assert_eq!(tempo.count_in_remaining_beats, 0);
    }
}
