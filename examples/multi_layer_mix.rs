// Multi-Layer Mix Example
// This example demonstrates building a multi-layer composition:
// - Record multiple layers sequentially
// - Adjust individual layer volumes
// - Use solo/mute for mixing
// - Export the final mix

use anyhow::Result;
use crossbeam::channel;
use soundlooper::audio::{AudioConfig, AudioEvent, AudioStream, LayerCommand, LooperEngine};
use std::io::{self, Write};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn main() -> Result<()> {
    println!("=== Multi-Layer Mix Example ===\n");
    println!("Create a layered composition by recording multiple tracks!\n");

    // Initialize
    let config = AudioConfig::default();
    let audio_stream = AudioStream::new(config.clone(), false)?;

    let runtime_config = AudioConfig {
        sample_rate: audio_stream.get_sample_rate(),
        buffer_size: config.buffer_size,
        max_layers: config.max_layers,
    };

    let looper = Arc::new(LooperEngine::new(runtime_config.clone()));

    // Channels
    let (cmd_tx, cmd_rx) = channel::unbounded();
    let (evt_tx, evt_rx) = channel::unbounded();

    // Start audio
    let (_input, _output) =
        audio_stream.start_audio_looper(Arc::clone(&looper), cmd_rx, evt_tx.clone(), false)?;

    // Event monitor
    let event_thread = thread::spawn(move || {
        use std::time::{Duration, Instant};
        let timeout = Duration::from_millis(100);
        let mut last_event = Instant::now();

        loop {
            match evt_rx.recv_timeout(timeout) {
                Ok(event) => {
                    last_event = Instant::now();
                    match event {
                        AudioEvent::LayerRecording(id) => {
                            println!("  🔴 Recording Layer {}...", id + 1);
                        }
                        AudioEvent::LayerPlaying(id) => {
                            println!("  ▶️  Layer {} playing", id + 1);
                        }
                        AudioEvent::LayerMuted(id) => {
                            println!("  🔇 Layer {} muted", id + 1);
                        }
                        AudioEvent::LayerUnmuted(id) => {
                            println!("  🔊 Layer {} unmuted", id + 1);
                        }
                        AudioEvent::LayerSoloed(id) => {
                            println!("  🎯 Layer {} soloed", id + 1);
                        }
                        AudioEvent::VolumeChanged(id, vol) => {
                            println!("  🎚️  Layer {} volume: {:.0}%", id + 1, vol * 100.0);
                        }
                        AudioEvent::WavExported(path) => {
                            println!("  💾 Exported: {}", path);
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

    println!("Audio system ready!\n");

    // Helper function to wait for user
    fn wait_for_enter(prompt: &str) -> Result<()> {
        print!("{}", prompt);
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        Ok(())
    }

    // === Recording Phase ===
    println!("=== RECORDING PHASE ===\n");

    // Layer 1 - Bass/Rhythm
    wait_for_enter("Layer 1 (Bass/Rhythm) - Press Enter to record for 3s: ")?;
    cmd_tx.send(LayerCommand::Record(0))?;
    thread::sleep(Duration::from_secs(3));
    cmd_tx.send(LayerCommand::StopRecording(0))?;
    thread::sleep(Duration::from_millis(300));
    println!("  ✓ Layer 1 recorded\n");

    // Layer 2 - Melody
    wait_for_enter("Layer 2 (Melody) - Press Enter to record for 3s: ")?;
    // Start playing layer 1 so user can hear it while recording layer 2
    cmd_tx.send(LayerCommand::Play(0))?;
    thread::sleep(Duration::from_millis(100));
    cmd_tx.send(LayerCommand::Record(1))?;
    thread::sleep(Duration::from_secs(3));
    cmd_tx.send(LayerCommand::StopRecording(1))?;
    thread::sleep(Duration::from_millis(300));
    println!("  ✓ Layer 2 recorded\n");

    // Layer 3 - Harmony/Effects
    wait_for_enter("Layer 3 (Harmony) - Press Enter to record for 3s: ")?;
    cmd_tx.send(LayerCommand::Record(2))?;
    thread::sleep(Duration::from_secs(3));
    cmd_tx.send(LayerCommand::StopRecording(2))?;
    cmd_tx.send(LayerCommand::StopAll)?;
    thread::sleep(Duration::from_millis(300));
    println!("  ✓ Layer 3 recorded\n");

    // === Mixing Phase ===
    println!("\n=== MIXING PHASE ===\n");

    println!("Playing all layers at full volume...");
    cmd_tx.send(LayerCommand::PlayAll)?;
    thread::sleep(Duration::from_secs(3));

    println!("\nAdjusting mix levels:");

    // Adjust volumes for balance
    println!("  Setting Layer 1 (bass) to 80%");
    cmd_tx.send(LayerCommand::SetVolume(0, 0.8))?;
    thread::sleep(Duration::from_millis(100));

    println!("  Setting Layer 2 (melody) to 100%");
    cmd_tx.send(LayerCommand::SetVolume(1, 1.0))?;
    thread::sleep(Duration::from_millis(100));

    println!("  Setting Layer 3 (harmony) to 60%");
    cmd_tx.send(LayerCommand::SetVolume(2, 0.6))?;
    thread::sleep(Duration::from_secs(3));

    // Demonstrate mute
    println!("\nMuting Layer 2 for contrast...");
    cmd_tx.send(LayerCommand::Mute(1))?;
    thread::sleep(Duration::from_secs(2));

    println!("Unmuting Layer 2...");
    cmd_tx.send(LayerCommand::Mute(1))?;
    thread::sleep(Duration::from_secs(2));

    // Demonstrate solo
    println!("\nSoloing Layer 1 (bass only)...");
    cmd_tx.send(LayerCommand::Solo(0))?;
    thread::sleep(Duration::from_secs(2));

    println!("Removing solo (back to full mix)...");
    cmd_tx.send(LayerCommand::Solo(0))?;
    thread::sleep(Duration::from_secs(2));

    cmd_tx.send(LayerCommand::StopAll)?;
    thread::sleep(Duration::from_millis(300));

    // === Export Phase ===
    println!("\n=== EXPORT PHASE ===\n");

    println!("Exporting final mix to 'my_composition.wav'...");
    cmd_tx.send(LayerCommand::ExportWav("my_composition.wav".to_string()))?;
    thread::sleep(Duration::from_millis(500));

    println!("\n=== Composition Complete! ===\n");
    println!("Your multi-layer composition has been saved.");
    println!("\nLayers recorded:");
    println!("  Layer 1: Bass/Rhythm (80% volume)");
    println!("  Layer 2: Melody (100% volume)");
    println!("  Layer 3: Harmony (60% volume)");
    println!("\nOutput file: my_composition.wav\n");

    println!("Mixing tips:");
    println!("  • Use volume levels to balance instruments");
    println!("  • Mute layers to A/B test your mix");
    println!("  • Solo layers to check individual recordings");
    println!("  • Lower background elements to ~60-80%");
    println!("  • Keep lead elements at ~90-100%\n");

    // Cleanup
    drop(_input);
    drop(_output);
    drop(cmd_tx);
    drop(evt_tx); // Close the event sender to signal the thread to exit
    let _ = event_thread.join();

    Ok(())
}
