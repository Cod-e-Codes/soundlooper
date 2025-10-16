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
        looper_engine.set_command_channel(command_receiver);
        looper_engine.set_event_sender(event_sender);
        looper_engine.set_debug_mode(debug_mode);

        // Build input stream
        let looper_clone = Arc::clone(&looper_engine);
        let input_channels = self.input_config.channels;

        let input_stream = self.input_device.build_input_stream(
            &self.input_config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                // Convert multi-channel to mono by averaging all channels
                let mono_data: Vec<f32> = data
                    .chunks(input_channels as usize)
                    .map(|chunk| chunk.iter().sum::<f32>() / chunk.len() as f32)
                    .collect();

                looper_clone.store_input_samples(&mono_data);
            },
            move |err| eprintln!("❌ Input stream error: {}", err),
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
        let callback_counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let output_stream = self.output_device.build_output_stream(
            &self.output_config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let count = callback_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                // Periodic logging (only in debug mode)
                if debug_mode && count.is_multiple_of(200) {
                    let _ = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open("debug.log")
                        .and_then(|mut file| {
                            use std::io::Write;
                            writeln!(
                                file,
                                "Output callback #{}: {} samples ({}Hz -> {}Hz, ratio: {:.4})",
                                count,
                                data.len(),
                                input_sample_rate,
                                output_sample_rate,
                                resample_ratio
                            )
                        });
                }

                // Get input samples
                let input_samples = looper_clone.get_input_samples();

                // Create buffer at input sample rate
                let mono_len = data.len() / output_channels as usize;

                // Calculate how many input samples we need
                let input_samples_needed = (mono_len as f64 / resample_ratio).ceil() as usize;
                let mut input_buffer = vec![0.0; input_samples_needed];

                // Process audio at input sample rate
                looper_clone.process_audio(&input_samples, &mut input_buffer);

                // Simple linear interpolation resampling
                let mut phase_locked = phase.lock().unwrap();
                for i in 0..mono_len {
                    let input_pos = *phase_locked;
                    let input_idx = input_pos.floor() as usize;
                    let frac = input_pos - input_pos.floor();

                    let sample = if input_idx < input_buffer.len() - 1 {
                        // Linear interpolation
                        let s1 = input_buffer[input_idx];
                        let s2 = input_buffer[input_idx + 1];
                        s1 + (s2 - s1) * frac as f32
                    } else if input_idx < input_buffer.len() {
                        input_buffer[input_idx]
                    } else {
                        0.0
                    };

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
                *phase_locked = (*phase_locked % input_buffer.len() as f64).max(0.0);
            },
            move |err| eprintln!("❌ Output stream error: {}", err),
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
