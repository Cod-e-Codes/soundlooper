// Simple Loop Example
// This example shows the basic workflow:
// 1. Initialize audio system
// 2. Record a loop
// 3. Play it back
// 4. Export to WAV

use anyhow::Result;
use crossbeam::channel;
use soundlooper::audio::{AudioConfig, AudioEvent, AudioStream, LayerCommand, LooperEngine};
use std::io::{self, Write};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn main() -> Result<()> {
    println!("=== Simple Loop Example ===\n");
    println!("This example will:");
    println!("1. Record audio from your microphone for 3 seconds");
    println!("2. Play it back in a loop");
    println!("3. Export the recording to 'my_loop.wav'\n");

    // Initialize audio
    let config = AudioConfig::default();
    let audio_stream = AudioStream::new(config.clone(), false)?;

    let runtime_config = AudioConfig {
        sample_rate: audio_stream.get_sample_rate(),
        buffer_size: config.buffer_size,
        max_layers: config.max_layers,
    };

    println!(
        "Audio initialized: {}Hz, {} samples buffer\n",
        runtime_config.sample_rate, runtime_config.buffer_size
    );

    // Create looper engine
    let looper = Arc::new(LooperEngine::new(runtime_config.clone()));

    // Set up channels
    let (cmd_tx, cmd_rx) = channel::unbounded();
    let (evt_tx, evt_rx) = channel::unbounded();

    // Start audio
    let (_input, _output) =
        audio_stream.start_audio_looper(Arc::clone(&looper), cmd_rx, evt_tx.clone(), false)?;

    println!("Audio streams started.\n");

    // Event monitor thread
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
                            println!("🔴 Recording on Layer {}...", id + 1);
                        }
                        AudioEvent::LayerStopped(id) => {
                            println!("⏹️  Layer {} stopped", id + 1);
                        }
                        AudioEvent::LayerPlaying(id) => {
                            println!("▶️  Layer {} playing", id + 1);
                        }
                        AudioEvent::WavExported(path) => {
                            println!("💾 Exported to: {}", path);
                        }
                        AudioEvent::Error(msg) => {
                            println!("❌ Error: {}", msg);
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

    // Wait for user to be ready
    print!("Press Enter when ready to record...");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    // Step 1: Record on Layer 0
    println!("\n📢 Recording for 3 seconds. Make some noise!");
    cmd_tx.send(LayerCommand::Record(0))?;

    thread::sleep(Duration::from_secs(3));

    cmd_tx.send(LayerCommand::StopRecording(0))?;
    thread::sleep(Duration::from_millis(500));

    println!("\n✓ Recording complete!");

    // Step 2: Play back the loop
    println!("\n🔄 Playing your loop for 5 seconds...");
    cmd_tx.send(LayerCommand::Play(0))?;

    thread::sleep(Duration::from_secs(5));

    cmd_tx.send(LayerCommand::StopPlaying(0))?;
    thread::sleep(Duration::from_millis(500));

    // Step 3: Export to WAV
    println!("\n💾 Exporting to 'my_loop.wav'...");
    cmd_tx.send(LayerCommand::ExportWav("my_loop.wav".to_string()))?;
    thread::sleep(Duration::from_millis(500));

    println!("\n=== Example Complete! ===");
    println!("\nYour loop has been saved to 'my_loop.wav'");
    println!("You can now:");
    println!("  - Play it in any audio player");
    println!("  - Import it back into soundlooper");
    println!("  - Use it in your music production software\n");

    // Cleanup
    drop(_input);
    drop(_output);
    drop(cmd_tx);
    drop(evt_tx); // Close the event sender to signal the thread to exit
    let _ = event_thread.join();

    Ok(())
}
