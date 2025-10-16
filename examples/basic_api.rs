use soundlooper::audio::{AudioConfig, LayerCommand, LooperEngine};
use std::thread;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Simple Looper Example");
    println!("This example demonstrates basic looper functionality");

    // Create a looper engine with default config
    let config = AudioConfig::default();
    let looper = LooperEngine::new(config);

    // Simulate recording on layer 0
    println!("Starting recording on layer 0...");
    looper.send_command(LayerCommand::Record(0))?;

    // Simulate some recording time
    thread::sleep(Duration::from_millis(500));

    // Stop recording (this automatically starts playback if there's content)
    println!("Stopping recording and starting playback...");
    looper.send_command(LayerCommand::StopRecording(0))?;

    // Let it play for a bit
    thread::sleep(Duration::from_millis(1000));

    // Stop playback
    println!("Stopping playback...");
    looper.send_command(LayerCommand::StopPlaying(0))?;

    // Start playback again
    println!("Starting playback again...");
    looper.send_command(LayerCommand::Play(0))?;

    // Let it play for a bit more
    thread::sleep(Duration::from_millis(500));

    // Stop all layers
    println!("Stopping all layers...");
    looper.send_command(LayerCommand::StopAll)?;

    // Clear the layer
    println!("Clearing layer 0...");
    looper.send_command(LayerCommand::Clear(0))?;

    println!("Example completed successfully!");
    Ok(())
}
