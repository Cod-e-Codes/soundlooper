use crossbeam::channel::{Receiver, Sender};
use std::sync::{Arc, Mutex};

use super::{AudioConfig, AudioEvent, AudioLayer, LayerCommand};

pub struct LooperEngine {
    layers: Arc<Vec<Arc<Mutex<AudioLayer>>>>,
    config: AudioConfig,
    master_loop_length: Arc<Mutex<Option<usize>>>,
    input_buffer: Arc<Mutex<Vec<f32>>>,
    output_buffer: Arc<Mutex<Vec<f32>>>,
    is_recording: Arc<Mutex<bool>>,
    recording_layer: Arc<Mutex<Option<usize>>>,
    command_receiver: Arc<Mutex<Option<Receiver<LayerCommand>>>>,
    event_sender: Arc<Mutex<Option<Sender<AudioEvent>>>>,
    debug_mode: Arc<Mutex<bool>>,
}

impl LooperEngine {
    pub fn new(config: AudioConfig) -> Self {
        let mut layers = Vec::with_capacity(config.max_layers);
        for i in 0..config.max_layers {
            layers.push(Arc::new(Mutex::new(AudioLayer::new(i))));
        }

        Self {
            layers: Arc::new(layers),
            config: config.clone(),
            master_loop_length: Arc::new(Mutex::new(None)),
            input_buffer: Arc::new(Mutex::new(Vec::with_capacity(config.buffer_size))),
            output_buffer: Arc::new(Mutex::new(Vec::with_capacity(config.buffer_size))),
            is_recording: Arc::new(Mutex::new(false)),
            recording_layer: Arc::new(Mutex::new(None)),
            command_receiver: Arc::new(Mutex::new(None)),
            event_sender: Arc::new(Mutex::new(None)),
            debug_mode: Arc::new(Mutex::new(false)),
        }
    }

    pub fn process_audio(&self, input: &[f32], output: &mut [f32]) {
        // Debug: Log when process_audio is called (periodically, only in debug mode)
        if let Ok(debug_mode) = self.debug_mode.lock()
            && *debug_mode
        {
            use std::sync::atomic::{AtomicUsize, Ordering};
            static CALL_COUNT: AtomicUsize = AtomicUsize::new(0);
            let count = CALL_COUNT.fetch_add(1, Ordering::Relaxed);
            if count.is_multiple_of(1000) {
                let _ = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open("debug.log")
                    .and_then(|mut file| {
                        use std::io::Write;
                        writeln!(
                            file,
                            "process_audio called #{}: input={} samples, output={} samples",
                            count,
                            input.len(),
                            output.len()
                        )
                    });
            }
        }

        // Clear output buffer
        if let Ok(mut output_buf) = self.output_buffer.lock() {
            output_buf.clear();
            output_buf.resize(output.len(), 0.0);
        }

        // Process commands from UI thread
        self.process_commands();

        // Record input if any layer is recording
        if let Ok(recording_layer) = self.recording_layer.lock()
            && let Some(layer_id) = *recording_layer
            && let Ok(mut layer) = self.layers[layer_id].lock()
        {
            layer.append_samples(input);
        }

        // Mix all layers into output buffer
        if let Ok(mut output_buf) = self.output_buffer.lock() {
            output_buf.fill(0.0);
            Self::mix_layers_static(&self.layers, &mut output_buf);

            // Copy to final output
            output.copy_from_slice(&output_buf);
        }

        // Check if we need to set master loop length
        if let Ok(recording_layer) = self.recording_layer.lock()
            && let Some(layer_id) = *recording_layer
            && let Ok(layer) = self.layers[layer_id].lock()
            && layer.is_recording
            && !layer.buffer.is_empty()
            && let Ok(mut master_len) = self.master_loop_length.lock()
            && master_len.is_none()
        {
            // This is the first layer recording, set it as master
            *master_len = Some(layer.buffer.len());
        }
    }

    fn mix_layers_static(layers: &Arc<Vec<Arc<Mutex<AudioLayer>>>>, output: &mut [f32]) {
        let mut has_solo = false;

        // Check if any layer is soloed
        for layer_arc in layers.iter() {
            if let Ok(layer) = layer_arc.lock()
                && layer.is_solo
            {
                has_solo = true;
                break;
            }
        }

        // Mix layers
        for layer_arc in layers.iter() {
            if let Ok(mut layer) = layer_arc.lock() {
                if !layer.is_playing {
                    continue;
                }

                // Skip if solo is active and this layer is not soloed
                if has_solo && !layer.is_solo {
                    continue;
                }

                // Skip if layer is muted
                if layer.is_muted {
                    continue;
                }

                // Get samples from this layer
                let layer_samples = layer.get_next_samples(output.len());

                // Mix into output buffer
                for (i, sample) in layer_samples.iter().enumerate() {
                    output[i] += sample;
                }
            }
        }

        // Apply master volume and clipping
        for sample in output.iter_mut() {
            *sample = sample.clamp(-1.0, 1.0);
        }
    }

    pub fn set_command_channel(&self, receiver: Receiver<LayerCommand>) {
        let mut cmd_receiver = self.command_receiver.lock().unwrap();
        *cmd_receiver = Some(receiver);
    }

    pub fn set_event_sender(&self, sender: Sender<AudioEvent>) {
        let mut evt_sender = self.event_sender.lock().unwrap();
        *evt_sender = Some(sender);
    }

    pub fn set_debug_mode(&self, debug_mode: bool) {
        let mut debug = self.debug_mode.lock().unwrap();
        *debug = debug_mode;
    }

    fn process_commands(&self) {
        // Process commands from UI thread
        if let Ok(receiver) = self.command_receiver.lock()
            && let Some(ref cmd_receiver) = *receiver
        {
            let mut commands = Vec::new();
            let debug_mode = self.debug_mode.lock().map(|d| *d).unwrap_or(false);
            while let Ok(command) = cmd_receiver.try_recv() {
                if debug_mode {
                    let _ = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open("debug.log")
                        .and_then(|mut file| {
                            use std::io::Write;
                            writeln!(file, "Received command: {:?}", command)
                        });
                }
                commands.push(command);
            }

            // Drop the lock before processing commands
            drop(receiver);

            // Process all commands
            for command in commands {
                if let Err(e) = self.send_command(command) {
                    eprintln!("Command processing error: {}", e);
                }
            }
        } else {
            // Log when we can't get the receiver (only in debug mode)
            if let Ok(debug_mode) = self.debug_mode.lock()
                && *debug_mode
            {
                let _ = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open("debug.log")
                    .and_then(|mut file| {
                        use std::io::Write;
                        writeln!(file, "process_commands: Could not get receiver")
                    });
            }
        }
    }

    fn send_event(&self, event: AudioEvent) {
        if let Ok(sender) = self.event_sender.lock()
            && let Some(ref evt_sender) = *sender
        {
            let _ = evt_sender.try_send(event);
        }
    }

    pub fn send_command(&self, command: LayerCommand) -> Result<(), Box<dyn std::error::Error>> {
        match command {
            LayerCommand::SwitchInputDevice(_device_name) => {
                // Notify UI; actual device switch is handled in the audio thread
                self.send_event(AudioEvent::DeviceSwitchRequested);
            }
            LayerCommand::SwitchOutputDevice(_device_name) => {
                // Notify UI; actual device switch is handled in the audio thread
                self.send_event(AudioEvent::DeviceSwitchRequested);
            }
            LayerCommand::Record(layer_id) => {
                if layer_id >= self.config.max_layers {
                    return Err("Layer ID out of range".into());
                }

                // Stop any current recording
                {
                    let recording_layer = self.recording_layer.lock().unwrap();
                    if let Some(current_layer) = *recording_layer
                        && let Ok(mut layer) = self.layers[current_layer].lock()
                    {
                        layer.stop_recording();
                    }
                }

                // Start recording on new layer
                if let Ok(mut layer) = self.layers[layer_id].lock() {
                    layer.start_recording();
                    {
                        let mut recording_layer = self.recording_layer.lock().unwrap();
                        *recording_layer = Some(layer_id);
                    }
                    {
                        let mut is_recording = self.is_recording.lock().unwrap();
                        *is_recording = true;
                    }
                    self.send_event(AudioEvent::LayerRecording(layer_id));
                }
            }
            LayerCommand::StopRecording(layer_id) => {
                if layer_id >= self.config.max_layers {
                    return Err("Layer ID out of range".into());
                }

                if let Ok(mut layer) = self.layers[layer_id].lock() {
                    layer.stop_recording(); // This automatically starts playback if there's content
                    self.send_event(AudioEvent::LayerStopped(layer_id));
                }

                {
                    let mut recording_layer = self.recording_layer.lock().unwrap();
                    if *recording_layer == Some(layer_id) {
                        *recording_layer = None;
                    }
                }
                {
                    let mut is_recording = self.is_recording.lock().unwrap();
                    *is_recording = false;
                }
            }
            LayerCommand::StopPlaying(layer_id) => {
                if layer_id >= self.config.max_layers {
                    return Err("Layer ID out of range".into());
                }

                if let Ok(mut layer) = self.layers[layer_id].lock() {
                    layer.stop_playing();
                    self.send_event(AudioEvent::LayerStopped(layer_id));
                }
            }
            LayerCommand::Play(layer_id) => {
                if layer_id >= self.config.max_layers {
                    return Err("Layer ID out of range".into());
                }

                if let Ok(mut layer) = self.layers[layer_id].lock() {
                    layer.start_playing();
                    self.send_event(AudioEvent::LayerPlaying(layer_id));
                }
            }
            LayerCommand::Mute(layer_id) => {
                if layer_id >= self.config.max_layers {
                    return Err("Layer ID out of range".into());
                }

                if let Ok(mut layer) = self.layers[layer_id].lock() {
                    layer.toggle_mute();
                    if layer.is_muted {
                        self.send_event(AudioEvent::LayerMuted(layer_id));
                    } else {
                        self.send_event(AudioEvent::LayerUnmuted(layer_id));
                    }
                }
            }
            LayerCommand::Solo(layer_id) => {
                if layer_id >= self.config.max_layers {
                    return Err("Layer ID out of range".into());
                }

                if let Ok(mut layer) = self.layers[layer_id].lock() {
                    layer.toggle_solo();
                    if layer.is_solo {
                        self.send_event(AudioEvent::LayerSoloed(layer_id));
                    } else {
                        self.send_event(AudioEvent::LayerUnsoloed(layer_id));
                    }
                }
            }
            LayerCommand::SetVolume(layer_id, volume) => {
                if layer_id >= self.config.max_layers {
                    return Err("Layer ID out of range".into());
                }

                if let Ok(mut layer) = self.layers[layer_id].lock() {
                    layer.set_volume(volume);
                    self.send_event(AudioEvent::VolumeChanged(layer_id, volume));
                }
            }
            LayerCommand::StopAll => {
                for layer_arc in self.layers.iter() {
                    if let Ok(mut layer) = layer_arc.lock() {
                        layer.stop_recording();
                        layer.stop_playing();
                    }
                }
                {
                    let mut recording_layer = self.recording_layer.lock().unwrap();
                    *recording_layer = None;
                }
                {
                    let mut is_recording = self.is_recording.lock().unwrap();
                    *is_recording = false;
                }
                self.send_event(AudioEvent::AllStopped);
            }
            LayerCommand::Clear(layer_id) => {
                if layer_id >= self.config.max_layers {
                    return Err("Layer ID out of range".into());
                }

                if let Ok(mut layer) = self.layers[layer_id].lock() {
                    layer.clear();
                    self.send_event(AudioEvent::LayerCleared(layer_id));
                }

                // If this was the recording layer, clear it
                {
                    let mut recording_layer = self.recording_layer.lock().unwrap();
                    if *recording_layer == Some(layer_id) {
                        *recording_layer = None;
                    }
                }
                {
                    let mut is_recording = self.is_recording.lock().unwrap();
                    *is_recording = false;
                }
            }
            LayerCommand::ClearAll => {
                for layer_arc in self.layers.iter() {
                    if let Ok(mut layer) = layer_arc.lock() {
                        layer.clear();
                    }
                }
                {
                    let mut recording_layer = self.recording_layer.lock().unwrap();
                    *recording_layer = None;
                }
                {
                    let mut is_recording = self.is_recording.lock().unwrap();
                    *is_recording = false;
                }
                self.send_event(AudioEvent::AllCleared);
            }
            LayerCommand::PlayAll => {
                for layer_arc in self.layers.iter() {
                    if let Ok(mut layer) = layer_arc.lock()
                        && !layer.buffer.is_empty()
                    {
                        layer.start_playing();
                    }
                }
                self.send_event(AudioEvent::AllPlaying);
            }
            LayerCommand::ImportWav(layer_id, file_path) => {
                if layer_id >= self.config.max_layers {
                    return Err("Layer ID out of range".into());
                }

                match super::io::import_wav(&file_path, self.config.sample_rate) {
                    Ok(samples) => {
                        if let Ok(mut layer) = self.layers[layer_id].lock() {
                            layer.buffer = samples;
                            layer.loop_end = layer.buffer.len();
                            self.send_event(AudioEvent::WavImported(layer_id, file_path));
                        }
                    }
                    Err(e) => {
                        self.send_event(AudioEvent::Error(format!("Failed to import WAV: {}", e)));
                    }
                }
            }
            LayerCommand::ExportWav(file_path) => {
                // Collect all layer buffers
                let layer_buffers: Vec<Vec<f32>> = self
                    .layers
                    .iter()
                    .filter_map(|layer_arc| layer_arc.lock().ok().map(|layer| layer.buffer.clone()))
                    .collect();

                match super::io::export_mixed_wav(
                    &file_path,
                    &layer_buffers,
                    self.config.sample_rate,
                ) {
                    Ok(()) => {
                        self.send_event(AudioEvent::WavExported(file_path));
                    }
                    Err(e) => {
                        self.send_event(AudioEvent::Error(format!("Failed to export WAV: {}", e)));
                    }
                }
            }
        }
        Ok(())
    }

    pub fn get_layer(&self, id: usize) -> Option<Arc<Mutex<AudioLayer>>> {
        if id < self.layers.len() {
            Some(Arc::clone(&self.layers[id]))
        } else {
            None
        }
    }

    pub fn get_layers(&self) -> Arc<Vec<Arc<Mutex<AudioLayer>>>> {
        Arc::clone(&self.layers)
    }

    pub fn get_master_loop_length(&self) -> Option<usize> {
        *self.master_loop_length.lock().unwrap()
    }

    pub fn is_recording(&self) -> bool {
        *self.is_recording.lock().unwrap()
    }

    pub fn get_recording_layer(&self) -> Option<usize> {
        *self.recording_layer.lock().unwrap()
    }

    pub fn get_config(&self) -> &AudioConfig {
        &self.config
    }

    pub fn store_input_samples(&self, samples: &[f32]) {
        if let Ok(mut input_buf) = self.input_buffer.lock() {
            input_buf.clear();
            input_buf.extend_from_slice(samples);
        } else {
            eprintln!("Warning: Failed to lock input buffer for writing");
        }
    }

    pub fn get_input_samples(&self) -> Vec<f32> {
        if let Ok(input_buf) = self.input_buffer.lock() {
            input_buf.clone()
        } else {
            vec![]
        }
    }

    pub fn load_audio_to_layer(
        &self,
        layer_id: usize,
        samples: Vec<f32>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if layer_id >= self.config.max_layers {
            return Err("Layer ID out of range".into());
        }

        if let Ok(mut layer) = self.layers[layer_id].lock() {
            layer.buffer = samples;
            layer.loop_end = layer.buffer.len();

            // Set as master if it's the first layer with content
            {
                let mut master_len = self.master_loop_length.lock().unwrap();
                if master_len.is_none() && !layer.buffer.is_empty() {
                    *master_len = Some(layer.buffer.len());
                }
            }
        }

        Ok(())
    }
}
