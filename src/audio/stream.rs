use anyhow::{Result, anyhow};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Host, SampleFormat, Stream, StreamConfig};
use crossbeam::channel::{Receiver, Sender};
use std::sync::{Arc, Mutex};

use super::{AudioConfig, LayerCommand, LooperEngine};

pub struct AudioStream {
    host: Host,
    input_device: Device,
    output_device: Device,
    input_config: StreamConfig,
    output_config: StreamConfig,
    sample_format: SampleFormat,
    // For resampling between different rates
    resample_ratio: f64,
    // Device names for UI display
    input_device_name: String,
    output_device_name: String,
}

impl AudioStream {
    pub fn new(_config: AudioConfig, debug_mode: bool) -> Result<Self> {
        let host = cpal::default_host();

        let input_device = host
            .default_input_device()
            .ok_or_else(|| anyhow!("No input device available"))?;

        let output_device = host
            .default_output_device()
            .ok_or_else(|| anyhow!("No output device available"))?;

        let input_default = input_device.default_input_config()?;
        let output_default = output_device.default_output_config()?;

        // Store device info for UI header (always needed)
        let input_device_name = input_device
            .name()
            .unwrap_or_else(|_| "Unknown".to_string());
        let output_device_name = output_device
            .name()
            .unwrap_or_else(|_| "Unknown".to_string());

        // Log device information to debug file (only in debug mode)
        if debug_mode {
            let _ = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open("debug.log")
                .map(|mut file| {
                    use std::io::Write;
                    let _ = writeln!(
                        file,
                        "═══════════════════════════════════════════════════════"
                    );
                    let _ = writeln!(file, "Input device: {}", input_device_name);
                    let _ = writeln!(
                        file,
                        "  Default: {}Hz, {}ch, {:?}",
                        input_default.sample_rate().0,
                        input_default.channels(),
                        input_default.sample_format()
                    );
                    let _ = writeln!(file, "Output device: {}", output_device_name);
                    let _ = writeln!(
                        file,
                        "  Default: {}Hz, {}ch, {:?}",
                        output_default.sample_rate().0,
                        output_default.channels(),
                        output_default.sample_format()
                    );
                });
        }

        // Use native configs for each device
        let input_config = StreamConfig {
            channels: input_default.channels(),
            sample_rate: input_default.sample_rate(),
            buffer_size: cpal::BufferSize::Default,
        };

        let output_config = StreamConfig {
            channels: output_default.channels(),
            sample_rate: output_default.sample_rate(),
            buffer_size: cpal::BufferSize::Default,
        };

        let resample_ratio =
            output_default.sample_rate().0 as f64 / input_default.sample_rate().0 as f64;

        if debug_mode {
            let _ = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open("debug.log")
                .map(|mut file| {
                    use std::io::Write;
                    let _ = writeln!(
                        file,
                        "═══════════════════════════════════════════════════════"
                    );
                    let _ = writeln!(file, "Configuration:");
                    let _ = writeln!(
                        file,
                        "  Input:  {}Hz, {}ch",
                        input_config.sample_rate.0, input_config.channels
                    );
                    let _ = writeln!(
                        file,
                        "  Output: {}Hz, {}ch",
                        output_config.sample_rate.0, output_config.channels
                    );
                    let _ = writeln!(file, "  Resample ratio: {:.4}", resample_ratio);
                    let _ = writeln!(
                        file,
                        "═══════════════════════════════════════════════════════"
                    );
                });
        }

        Ok(Self {
            host,
            input_device,
            output_device,
            input_config,
            output_config,
            sample_format: output_default.sample_format(),
            resample_ratio,
            input_device_name,
            output_device_name,
        })
    }

    // Create a new AudioStream with specific device names (or defaults)
    pub fn new_with_devices(
        _config: AudioConfig,
        debug_mode: bool,
        input_device_name: Option<String>,
        output_device_name: Option<String>,
    ) -> Result<Self> {
        let host = cpal::default_host();

        // Resolve input device
        let input_device = if let Some(name) = input_device_name.clone() {
            // Search devices by name
            let mut found = None;
            for device in host.input_devices()? {
                if let Ok(device_name) = device.name()
                    && device_name == name
                {
                    found = Some(device);
                    break;
                }
            }
            found.ok_or_else(|| anyhow!("Input device '{}' not found", name))?
        } else {
            host.default_input_device()
                .ok_or_else(|| anyhow!("No input device available"))?
        };

        // Resolve output device
        let output_device = if let Some(name) = output_device_name.clone() {
            let mut found = None;
            for device in host.output_devices()? {
                if let Ok(device_name) = device.name()
                    && device_name == name
                {
                    found = Some(device);
                    break;
                }
            }
            found.ok_or_else(|| anyhow!("Output device '{}' not found", name))?
        } else {
            host.default_output_device()
                .ok_or_else(|| anyhow!("No output device available"))?
        };

        let input_default = input_device.default_input_config()?;
        let output_default = output_device.default_output_config()?;

        let input_device_name = input_device
            .name()
            .unwrap_or_else(|_| "Unknown".to_string());
        let output_device_name = output_device
            .name()
            .unwrap_or_else(|_| "Unknown".to_string());

        if debug_mode {
            let _ = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open("debug.log")
                .map(|mut file| {
                    use std::io::Write;
                    let _ = writeln!(file, "═══ Device Switch ═══");
                    let _ = writeln!(file, "Input: {}", input_device_name);
                    let _ = writeln!(file, "Output: {}", output_device_name);
                });
        }

        let input_config = StreamConfig {
            channels: input_default.channels(),
            sample_rate: input_default.sample_rate(),
            buffer_size: cpal::BufferSize::Default,
        };

        let output_config = StreamConfig {
            channels: output_default.channels(),
            sample_rate: output_default.sample_rate(),
            buffer_size: cpal::BufferSize::Default,
        };

        let resample_ratio =
            output_default.sample_rate().0 as f64 / input_default.sample_rate().0 as f64;

        Ok(Self {
            host,
            input_device,
            output_device,
            input_config,
            output_config,
            sample_format: output_default.sample_format(),
            resample_ratio,
            input_device_name,
            output_device_name,
        })
    }

    pub fn start_audio_looper(
        &self,
        looper_engine: Arc<LooperEngine>,
        command_receiver: Receiver<LayerCommand>,
        event_sender: Sender<super::AudioEvent>,
        debug_mode: bool,
    ) -> Result<(Stream, Stream)>
    where
        LooperEngine: Send + 'static,
    {
        // Set up the looper engine with channels
        // Clone event sender for error callbacks before moving it into the engine
        let input_err_sender = event_sender.clone();
        let output_err_sender = event_sender.clone();
        looper_engine.set_command_channel(command_receiver);
        looper_engine.set_event_sender(event_sender);
        looper_engine.set_debug_mode(debug_mode);

        // Build input stream
        let looper_clone = Arc::clone(&looper_engine);
        let input_channels = self.input_config.channels;

        let input_stream = self.input_device.build_input_stream(
            &self.input_config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                // Convert multi-channel to mono with stack buffer (typical max ~2048 samples)
                let frame_count = data.len() / input_channels as usize;
                let mut mono_buffer = [0.0f32; 4096]; // Stack allocated

                for (i, chunk) in data.chunks(input_channels as usize).enumerate() {
                    if i < mono_buffer.len() {
                        mono_buffer[i] = chunk.iter().sum::<f32>() / chunk.len() as f32;
                    }
                }

                looper_clone.store_input_samples(&mono_buffer[..frame_count]);
            },
            move |_err| {
                // Send error (use owned string to avoid format! allocation in callback)
                // Note: Error callbacks may run in audio thread depending on backend
                let _ = input_err_sender
                    .try_send(super::AudioEvent::Error(String::from("Input stream error")));
                // Try to get a new default input and notify UI
                let new_input = cpal::default_host()
                    .default_input_device()
                    .and_then(|d| d.name().ok());
                let _ =
                    input_err_sender.try_send(super::AudioEvent::DevicesUpdated(new_input, None));
            },
            None,
        )?;

        // Build output stream with resampling support
        let looper_clone = Arc::clone(&looper_engine);
        let output_channels = self.output_config.channels;
        let resample_ratio = self.resample_ratio;
        let input_sample_rate = self.input_config.sample_rate.0;
        let output_sample_rate = self.output_config.sample_rate.0;

        // Accumulator for resampling
        let phase = Arc::new(Mutex::new(0.0_f64));

        // Preallocate buffers for output callback to avoid allocations in RT context
        // Max buffer size: 4096 samples per channel, worst case resampling needs ~8192
        let max_input_buffer_size = 8192;
        let input_buffer_state = Arc::new(Mutex::new(vec![0.0f32; max_input_buffer_size]));
        let input_samples_buffer = Arc::new(Mutex::new(vec![0.0f32; 4096]));

        let output_stream = self.output_device.build_output_stream(
            &self.output_config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                // NOTE: File I/O removed from audio callback for real-time safety
                // Debug logging should use lock-free channel to separate thread

                // Create buffer at input sample rate
                let mono_len = data.len() / output_channels as usize;

                // Calculate how many input samples we need
                let input_samples_needed = (mono_len as f64 / resample_ratio).ceil() as usize;

                // Work directly with preallocated heap buffers (no stack allocation, no copy)
                // All locks held for entire operation to minimize contention window
                if let (Ok(mut input_samples_buf), Ok(mut input_buf), Ok(mut phase_locked)) = (
                    input_samples_buffer.try_lock(),
                    input_buffer_state.try_lock(),
                    phase.try_lock(),
                ) {
                    // Read input samples
                    let input_samples_read = looper_clone
                        .read_input_samples(&mut input_samples_buf)
                        .min(4096);

                    let process_len = input_samples_needed.min(input_buf.len());

                    // Process audio at input sample rate directly into input_buf
                    looper_clone.process_audio(
                        &input_samples_buf[..input_samples_read],
                        &mut input_buf[..process_len],
                    );

                    // Resample directly from input_buf (no copy needed)
                    for i in 0..mono_len {
                        let input_pos = *phase_locked;
                        let input_idx = input_pos.floor() as usize;
                        let frac = (input_pos - input_pos.floor()) as f32;

                        // Branchless interpolation with bounds checking
                        let idx_curr = input_idx.min(process_len.saturating_sub(1));
                        let idx_next = (input_idx + 1).min(process_len.saturating_sub(1));
                        let s1 = input_buf[idx_curr];
                        let s2 = input_buf[idx_next];
                        let sample = s1 + (s2 - s1) * frac;

                        // Copy to all channels
                        for channel in 0..output_channels as usize {
                            if let Some(output_sample) =
                                data.get_mut(i * output_channels as usize + channel)
                            {
                                *output_sample = sample;
                            }
                        }

                        *phase_locked += 1.0 / resample_ratio;
                    }

                    // Reset phase when it gets too large
                    if process_len > 0 {
                        *phase_locked = (*phase_locked % process_len as f64).max(0.0);
                    }
                } else {
                    // Fallback: output silence if any lock fails
                    data[..mono_len].fill(0.0);
                }
            },
            move |_err| {
                // Send error (use owned string to avoid format! allocation in callback)
                // Note: Error callbacks may run in audio thread depending on backend
                let _ = output_err_sender.try_send(super::AudioEvent::Error(String::from(
                    "Output stream error",
                )));
                // Try to get a new default output and notify UI
                let new_output = cpal::default_host()
                    .default_output_device()
                    .and_then(|d| d.name().ok());
                let _ =
                    output_err_sender.try_send(super::AudioEvent::DevicesUpdated(None, new_output));
            },
            None,
        )?;

        // Start both streams
        input_stream.play()?;
        output_stream.play()?;

        if debug_mode {
            let _ = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open("debug.log")
                .and_then(|mut file| {
                    use std::io::Write;
                    writeln!(
                        file,
                        "═══ Audio streams started: {}Hz input -> {}Hz output ═══",
                        input_sample_rate, output_sample_rate
                    )
                });
        }

        Ok((input_stream, output_stream))
    }

    pub fn get_sample_rate(&self) -> u32 {
        self.input_config.sample_rate.0
    }

    pub fn get_buffer_size(&self) -> usize {
        match self.output_config.buffer_size {
            cpal::BufferSize::Fixed(size) => size as usize,
            cpal::BufferSize::Default => 512,
        }
    }

    pub fn get_sample_format(&self) -> SampleFormat {
        self.sample_format
    }

    pub fn get_channels(&self) -> u16 {
        self.output_config.channels
    }

    pub fn get_input_device_name(&self) -> &str {
        &self.input_device_name
    }

    pub fn get_output_device_name(&self) -> &str {
        &self.output_device_name
    }

    pub fn list_devices(&self) -> Result<()> {
        println!("Available input devices:");
        let input_devices = self.host.input_devices()?;
        for (i, device) in input_devices.enumerate() {
            println!(
                "  {}: {}",
                i,
                device.name().unwrap_or_else(|_| "Unknown".to_string())
            );
        }

        println!("\nAvailable output devices:");
        let output_devices = self.host.output_devices()?;
        for (i, device) in output_devices.enumerate() {
            println!(
                "  {}: {}",
                i,
                device.name().unwrap_or_else(|_| "Unknown".to_string())
            );
        }

        Ok(())
    }
}

// Public helper to enumerate device names for UI consumption
pub fn enumerate_device_names() -> Result<(Vec<String>, Vec<String>)> {
    let host = cpal::default_host();

    let mut inputs = Vec::new();
    let mut outputs = Vec::new();

    for device in host.input_devices()? {
        inputs.push(device.name().unwrap_or_else(|_| "Unknown".to_string()));
    }

    for device in host.output_devices()? {
        outputs.push(device.name().unwrap_or_else(|_| "Unknown".to_string()));
    }

    Ok((inputs, outputs))
}
