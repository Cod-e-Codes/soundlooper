// Comprehensive Soundlooper Demo
// This example demonstrates all major features of the soundlooper engine:
// - Recording and playback
// - Layer management (volume, mute, solo)
// - WAV import/export
// - Beat sync and tempo control
// - Count-in mode
// - Metronome

use anyhow::Result;
use crossbeam::channel;
use soundlooper::audio::{
    AudioConfig, AudioEvent, AudioStream, LayerCommand, LooperEngine, import_wav,
};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn main() -> Result<()> {
    println!("=== Soundlooper Demo ===\n");

    // Step 1: Initialize the audio system
    println!("1. Initializing audio system...");
    let provisional_config = AudioConfig::default();
    let audio_stream = AudioStream::new(provisional_config.clone(), false)?;

    let runtime_config = AudioConfig {
        sample_rate: audio_stream.get_sample_rate(),
        buffer_size: provisional_config.buffer_size,
        max_layers: provisional_config.max_layers,
    };

    println!("   Sample rate: {}Hz", runtime_config.sample_rate);
    println!("   Buffer size: {} samples", runtime_config.buffer_size);
    println!("   Max layers: {}\n", runtime_config.max_layers);

    // Step 2: Create looper engine
    println!("2. Creating looper engine...");
    let looper_engine = Arc::new(LooperEngine::new(runtime_config.clone()));

    // Load metronome sample
    match import_wav("assets/metronome.wav", runtime_config.sample_rate) {
        Ok(samples) => {
            looper_engine.set_metronome_sample(samples);
            println!("   Metronome sample loaded\n");
        }
        Err(e) => {
            println!("   Warning: Could not load metronome: {}\n", e);
        }
    }

    // Step 3: Set up communication channels
    println!("3. Setting up communication channels...");
    let (command_sender, command_receiver) = channel::unbounded::<LayerCommand>();
    let (event_sender, event_receiver) = channel::unbounded::<AudioEvent>();

    // Step 4: Start audio streams
    println!("4. Starting audio streams...");
    let looper_clone = Arc::clone(&looper_engine);
    let (_input_stream, _output_stream) = audio_stream.start_audio_looper(
        looper_clone,
        command_receiver,
        event_sender.clone(),
        false,
    )?;
    println!("   Audio streams active\n");

    // Step 5: Event monitoring thread
    println!("5. Starting event monitor...");
    let event_monitor = thread::spawn(move || {
        use std::time::{Duration, Instant};
        let timeout = Duration::from_millis(100);
        let mut last_event = Instant::now();

        loop {
            match event_receiver.recv_timeout(timeout) {
                Ok(event) => {
                    last_event = Instant::now();
                    match event {
                        AudioEvent::LayerRecording(id) => {
                            println!("   → Layer {} started recording", id + 1);
                        }
                        AudioEvent::LayerStopped(id) => {
                            println!("   → Layer {} stopped", id + 1);
                        }
                        AudioEvent::LayerPlaying(id) => {
                            println!("   → Layer {} playing", id + 1);
                        }
                        AudioEvent::BpmChanged(bpm) => {
                            println!("   → BPM changed to {:.1}", bpm);
                        }
                        AudioEvent::WavImported(id, path) => {
                            println!("   → Layer {} imported: {}", id + 1, path);
                        }
                        AudioEvent::WavExported(path) => {
                            println!("   → Exported to: {}", path);
                        }
                        AudioEvent::Error(msg) => {
                            println!("   ✗ Error: {}", msg);
                        }
                        _ => {}
                    }
                }
                Err(_) => {
                    // Timeout or channel closed - exit after a reasonable delay
                    if last_event.elapsed() > Duration::from_secs(2) {
                        break;
                    }
                }
            }
        }
    });
    println!("   Event monitor running\n");

    // Step 6: Demo sequence
    println!("=== Running Demo Sequence ===\n");

    // Demo 1: Import a WAV file (if available)
    println!("Demo 1: Importing WAV file to Layer 1");
    if std::path::Path::new("assets/metronome.wav").exists() {
        command_sender.send(LayerCommand::ImportWav(
            0,
            "assets/metronome.wav".to_string(),
        ))?;
        thread::sleep(Duration::from_millis(500));
        println!("   Layer 1 now contains metronome sample\n");
    } else {
        println!("   (Skipped - no sample file found)\n");
    }

    // Demo 2: Set BPM
    println!("Demo 2: Setting tempo to 120 BPM");
    command_sender.send(LayerCommand::SetBpm(120.0))?;
    thread::sleep(Duration::from_millis(200));

    // Demo 3: Enable beat sync
    println!("Demo 3: Enabling beat sync mode");
    command_sender.send(LayerCommand::ToggleBeatSync(true))?;
    thread::sleep(Duration::from_millis(200));

    // Demo 4: Enable metronome
    println!("Demo 4: Enabling metronome");
    command_sender.send(LayerCommand::ToggleMetronome(true))?;
    thread::sleep(Duration::from_millis(200));

    // Demo 5: Record on Layer 2 for 2 seconds
    println!("Demo 5: Recording on Layer 2 for 2 seconds");
    println!("   (Recording ambient audio from microphone...)");
    command_sender.send(LayerCommand::Record(1))?;
    thread::sleep(Duration::from_secs(2));
    command_sender.send(LayerCommand::StopRecording(1))?;
    println!("   Recording complete\n");

    // Demo 6: Play Layer 2
    println!("Demo 6: Playing back Layer 2");
    command_sender.send(LayerCommand::Play(1))?;
    thread::sleep(Duration::from_secs(2));

    // Demo 7: Adjust volume
    println!("Demo 7: Adjusting Layer 2 volume to 50%");
    command_sender.send(LayerCommand::SetVolume(1, 0.5))?;
    thread::sleep(Duration::from_millis(500));

    // Demo 8: Mute Layer 2
    println!("Demo 8: Muting Layer 2");
    command_sender.send(LayerCommand::Mute(1))?;
    thread::sleep(Duration::from_secs(1));

    // Demo 9: Unmute Layer 2
    println!("Demo 9: Unmuting Layer 2");
    command_sender.send(LayerCommand::Mute(1))?;
    thread::sleep(Duration::from_secs(1));

    // Demo 10: Play all layers
    println!("Demo 10: Playing all layers together");
    command_sender.send(LayerCommand::PlayAll)?;
    thread::sleep(Duration::from_secs(2));

    // Demo 11: Stop all
    println!("Demo 11: Stopping all layers");
    command_sender.send(LayerCommand::StopAll)?;
    thread::sleep(Duration::from_millis(500));

    // Demo 12: Export composition
    println!("Demo 12: Exporting composition");
    command_sender.send(LayerCommand::ExportWav("demo_output.wav".to_string()))?;
    thread::sleep(Duration::from_millis(500));

    // Demo 13: Count-in mode
    println!("Demo 13: Testing count-in mode");
    command_sender.send(LayerCommand::ToggleCountInMode(true))?;
    thread::sleep(Duration::from_millis(200));
    println!("   Count-in mode enabled");
    println!("   Starting count-in for Layer 3...");
    command_sender.send(LayerCommand::StartCountIn {
        layer_id: 2,
        measures: 1,
    })?;
    thread::sleep(Duration::from_secs(2));

    // Demo 14: Tap tempo
    println!("Demo 14: Tap tempo demonstration");
    println!("   Tapping 4 times at ~140 BPM...");
    for _ in 0..4 {
        command_sender.send(LayerCommand::TapTempo)?;
        thread::sleep(Duration::from_millis(429)); // ~140 BPM
    }
    thread::sleep(Duration::from_millis(500));

    // Demo 15: Clear layers
    println!("Demo 15: Clearing all layers");
    command_sender.send(LayerCommand::ClearAll)?;
    thread::sleep(Duration::from_millis(500));

    // Cleanup
    println!("\n=== Demo Complete ===");
    println!("Shutting down...\n");

    // Give time for final events to process
    thread::sleep(Duration::from_millis(500));

    // Drop streams
    drop(_input_stream);
    drop(_output_stream);

    // Wait for event monitor
    drop(command_sender);
    drop(event_sender); // Close the event sender to signal the thread to exit
    let _ = event_monitor.join();

    println!("Demo finished successfully!");
    println!("\nFeatures demonstrated:");
    println!("  ✓ Audio initialization and configuration");
    println!("  ✓ WAV file import");
    println!("  ✓ Real-time recording and playback");
    println!("  ✓ Layer management (volume, mute)");
    println!("  ✓ Tempo control (BPM, tap tempo)");
    println!("  ✓ Beat sync mode");
    println!("  ✓ Metronome");
    println!("  ✓ Count-in mode");
    println!("  ✓ WAV export");
    println!("  ✓ Event monitoring");

    Ok(())
}
