use anyhow::Result;
use crossbeam::channel;
use soundlooper::audio::{AudioConfig, AudioEvent, AudioStream, LayerCommand, LooperEngine};
use soundlooper::ui::TerminalUI;
use std::sync::Arc;
use std::thread;

fn print_help() {
    println!("Soundlooper - Terminal-based multi-layer audio looper");
    println!();
    println!("USAGE:");
    println!("    soundlooper [OPTIONS]");
    println!();
    println!("OPTIONS:");
    println!("    -h, --help      Print this help message");
    println!("    --debug         Enable debug logging");
    println!();
    println!("DESCRIPTION:");
    println!("    A terminal-based multi-layer audio looper supporting real-time");
    println!("    recording, playback, and mixing of up to 16 audio layers.");
    println!();
    println!("FEATURES:");
    println!("    • 16-layer audio recording and playback");
    println!("    • Real-time audio processing with low latency");
    println!("    • WAV file import/export with validation");
    println!("    • Per-layer volume, mute, and solo controls");
    println!("    • Cross-platform audio support");
    println!("    • Professional terminal UI with syntax highlighting");
    println!();
    println!("CONTROLS:");
    println!("    ↑↓     Select layer");
    println!("    1-9,0  Record/Stop/Play layer 1-10");
    println!("    R      Record on selected layer");
    println!("    S      Stop selected layer");
    println!("    Space  Stop all layers");
    println!("    P      Play selected layer");
    println!("    A      Play all layers");
    println!("    O      Options (select input/output devices)");
    println!("    +/-    Adjust volume");
    println!("    M      Mute/unmute selected layer");
    println!("    L      Solo/unsolo selected layer");
    println!("    C      Clear selected layer");
    println!("    X      Clear all layers");
    println!("    I      Import WAV file to selected layer");
    println!("    E      Export composition as WAV");
    println!("    Q      Quit");
    println!();
    println!("EXAMPLES:");
    println!("    soundlooper              # Start with default settings");
    println!("    soundlooper --debug      # Start with debug logging");
    println!();
    println!("For more information, visit: https://github.com/Cod-e-Codes/soundlooper");
}

fn main() -> Result<()> {
    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();

    // Check for help flag
    if args.contains(&"--help".to_string()) || args.contains(&"-h".to_string()) {
        print_help();
        return Ok(());
    }

    let debug_mode = args.contains(&"--debug".to_string());

    if debug_mode {
        println!("Starting Soundlooper in DEBUG mode...");
    } else {
        println!("Starting Soundlooper...");
    }

    // Create a provisional audio config and audio stream to detect actual device rates
    let provisional_config = AudioConfig::default();
    let audio_stream = AudioStream::new(provisional_config.clone(), debug_mode)?;

    // Build the runtime audio config to MATCH the device input sample rate
    let runtime_config = AudioConfig {
        sample_rate: audio_stream.get_sample_rate(),
        buffer_size: provisional_config.buffer_size,
        max_layers: provisional_config.max_layers,
    };

    if debug_mode {
        println!(
            "Audio config: {}Hz, buffer size: {}, max layers: {}",
            runtime_config.sample_rate, runtime_config.buffer_size, runtime_config.max_layers
        );
    }

    // Create looper engine (now thread-safe) with the ACTUAL processing sample rate
    let looper_engine = Arc::new(LooperEngine::new(runtime_config.clone()));
    let layers = looper_engine.get_layers();

    // Create communication channels
    let (command_sender, command_receiver) = channel::unbounded::<LayerCommand>();
    let (event_sender, event_receiver) = channel::unbounded::<AudioEvent>();

    // Extract device names before moving audio_stream into thread
    let input_device_name = audio_stream.get_input_device_name().to_string();
    let output_device_name = audio_stream.get_output_device_name().to_string();

    // Start audio thread with the SAME looper engine
    let looper_clone = Arc::clone(&looper_engine);

    let _audio_thread = thread::spawn(move || {
        if let Err(e) = run_audio_thread(
            audio_stream,
            looper_clone,
            command_receiver,
            event_sender,
            debug_mode,
        ) {
            eprintln!("Audio thread error: {}", e);
            if debug_mode {
                let _ = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open("debug.log")
                    .and_then(|mut file| {
                        use std::io::Write;
                        writeln!(file, "Audio thread error: {}", e)
                    });
            }
        }
    });

    // Create and run TUI
    let mut ui = TerminalUI::new(
        layers,
        command_sender,
        event_receiver,
        &input_device_name,
        &output_device_name,
    )
    .map_err(|e| anyhow::anyhow!("UI creation failed: {}", e))?;
    ui.run()
        .map_err(|e| anyhow::anyhow!("UI run failed: {}", e))?;

    println!("Soundlooper stopped.");
    Ok(())
}

fn run_audio_thread(
    audio_stream: AudioStream,
    looper_engine: Arc<LooperEngine>,
    command_receiver: channel::Receiver<LayerCommand>,
    event_sender: channel::Sender<AudioEvent>,
    debug_mode: bool,
) -> Result<()> {
    // Start audio streams with proper synchronization
    let (_input_stream, _output_stream) = audio_stream.start_audio_looper(
        looper_engine,
        command_receiver,
        event_sender,
        debug_mode,
    )?;

    // Log that we're in the keep-alive loop (only in debug mode)
    if debug_mode {
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("debug.log")
            .and_then(|mut file| {
                use std::io::Write;
                writeln!(file, "Audio thread: Entering keep-alive loop")
            });
    }

    // Keep both streams alive
    loop {
        thread::sleep(std::time::Duration::from_secs(1));
    }
}
