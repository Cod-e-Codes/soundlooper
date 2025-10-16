#[derive(Debug, Clone)]
pub struct AudioLayer {
    pub id: usize,
    pub buffer: Vec<f32>,
    pub volume: f32,
    pub is_recording: bool,
    pub is_playing: bool,
    pub is_muted: bool,
    pub is_solo: bool,
    pub playback_position: usize,
    pub loop_start: usize,
    pub loop_end: usize,
    pub previous_buffer: Option<Vec<f32>>,
    pub meter: crate::audio::peak_meter::PeakMeter,
}

impl AudioLayer {
    pub fn new(id: usize) -> Self {
        Self {
            id,
            buffer: Vec::new(),
            volume: 1.0,
            is_recording: false,
            is_playing: false,
            is_muted: false,
            is_solo: false,
            playback_position: 0,
            loop_start: 0,
            loop_end: 0,
            previous_buffer: None,
            meter: crate::audio::peak_meter::PeakMeter::new(),
        }
    }

    pub fn start_recording(&mut self) {
        self.is_recording = true;
        self.is_playing = false;
        // Store previous buffer for undo (only if not empty to avoid unnecessary clone)
        if !self.buffer.is_empty() {
            self.previous_buffer = Some(std::mem::take(&mut self.buffer));
        } else {
            self.buffer.clear(); // Ensure it's clear
        }
        self.playback_position = 0;
        self.loop_start = 0;
        self.loop_end = 0;
    }

    pub fn stop_recording(&mut self) {
        self.is_recording = false;
        if !self.buffer.is_empty() {
            self.loop_end = self.buffer.len();
            self.is_playing = true;
        }
    }

    pub fn start_playing(&mut self) {
        if !self.buffer.is_empty() {
            self.is_playing = true;
            self.playback_position = self.loop_start;
        }
    }

    pub fn stop_playing(&mut self) {
        self.is_playing = false;
        self.playback_position = self.loop_start;
    }

    pub fn toggle_mute(&mut self) {
        self.is_muted = !self.is_muted;
    }

    pub fn toggle_solo(&mut self) {
        self.is_solo = !self.is_solo;
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
    }

    pub fn append_samples(&mut self, samples: &[f32]) {
        self.buffer.extend_from_slice(samples);
    }

    pub fn get_next_samples(&mut self, count: usize) -> Vec<f32> {
        if !self.is_playing || self.buffer.is_empty() {
            return vec![0.0; count];
        }

        let mut output = Vec::with_capacity(count);
        let buffer_len = self.buffer.len();
        let loop_len = self.loop_end - self.loop_start;

        if loop_len == 0 {
            return vec![0.0; count];
        }

        for _ in 0..count {
            if self.playback_position >= buffer_len {
                self.playback_position = self.loop_start;
            }

            let sample = self.buffer[self.playback_position];
            let volume_sample = if self.is_muted {
                0.0
            } else {
                sample * self.volume
            };
            output.push(volume_sample);

            self.playback_position += 1;
        }

        // Update peak meter
        self.meter.update(&output);

        output
    }

    pub fn get_loop_length(&self) -> usize {
        self.loop_end - self.loop_start
    }

    pub fn set_loop_points(&mut self, start: usize, end: usize) {
        self.loop_start = start.min(self.buffer.len());
        self.loop_end = end.min(self.buffer.len());
        if self.loop_start >= self.loop_end {
            self.loop_end = self.loop_start + 1;
        }
    }

    pub fn undo(&mut self) -> bool {
        if let Some(prev_buffer) = self.previous_buffer.take() {
            self.buffer = prev_buffer;
            self.loop_end = self.buffer.len();
            self.playback_position = self.loop_start;
            true
        } else {
            false
        }
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
        self.is_recording = false;
        self.is_playing = false;
        self.playback_position = 0;
        self.loop_start = 0;
        self.loop_end = 0;
        self.previous_buffer = None;
        self.meter.reset();
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    pub fn get_buffer_length(&self) -> usize {
        self.buffer.len()
    }
}

impl Default for AudioLayer {
    fn default() -> Self {
        Self::new(0)
    }
}
