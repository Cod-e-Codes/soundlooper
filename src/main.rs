use anyhow::Result;
use crossbeam::channel;
use soundlooper::audio::{AudioConfig, AudioEvent, AudioStream, LayerCommand, LooperEngine};
use soundlooper::ui::TerminalUI;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
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

    // Prepare restart mechanism and shared device names
    let restart_audio = Arc::new(AtomicBool::new(false));
    let restart_audio_clone = Arc::clone(&restart_audio);

    let current_input_device = Arc::new(Mutex::new(input_device_name.clone()));
    let current_output_device = Arc::new(Mutex::new(output_device_name.clone()));

    let input_device_clone = Arc::clone(&current_input_device);
    let output_device_clone = Arc::clone(&current_output_device);

    // Start audio thread with the SAME looper engine
    let looper_clone = Arc::clone(&looper_engine);

    let _audio_thread = thread::spawn(move || {
        loop {
            // Read current desired device names
            let input_name = input_device_clone.lock().unwrap().clone();
            let output_name = output_device_clone.lock().unwrap().clone();

            // Build a fresh stream with selected devices
            let audio_stream = match AudioStream::new_with_devices(
                runtime_config.clone(),
                debug_mode,
                Some(input_name.clone()),
                Some(output_name.clone()),
            ) {
                Ok(stream) => stream,
                Err(e) => {
                    eprintln!("Failed to create audio stream: {}", e);
                    let _ = event_sender.try_send(AudioEvent::DeviceSwitchFailed(format!(
                        "Failed to switch devices: {}",
                        e
                    )));
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    continue;
                }
            };

            // Inform UI
            let _ = event_sender.try_send(AudioEvent::DevicesUpdated(
                Some(audio_stream.get_input_device_name().to_string()),
                Some(audio_stream.get_output_device_name().to_string()),
            ));
            let _ = event_sender.try_send(AudioEvent::DeviceSwitchComplete);

            // Create a forwarding channel so we can intercept device switch commands
            let (forward_tx, forward_rx) = channel::unbounded::<LayerCommand>();

            // Forwarder thread: intercept switch commands to update device names and trigger restart
            let restart_for_forwarder = Arc::clone(&restart_audio_clone);
            let input_for_forwarder = Arc::clone(&input_device_clone);
            let output_for_forwarder = Arc::clone(&output_device_clone);
            let event_sender_for_forwarder = event_sender.clone();
            let cmd_receiver_for_forwarder = command_receiver.clone();
            let _forwarder = std::thread::spawn(move || {
                while let Ok(cmd) = cmd_receiver_for_forwarder.recv() {
                    match &cmd {
                        LayerCommand::SwitchInputDevice(new_name) => {
                            if let Ok(mut name) = input_for_forwarder.lock() {
                                *name = new_name.clone();
                            }
                            let _ = event_sender_for_forwarder
                                .try_send(AudioEvent::DeviceSwitchRequested);
                            restart_for_forwarder.store(true, Ordering::Relaxed);
                        }
                        LayerCommand::SwitchOutputDevice(new_name) => {
                            if let Ok(mut name) = output_for_forwarder.lock() {
                                *name = new_name.clone();
                            }
                            let _ = event_sender_for_forwarder
                                .try_send(AudioEvent::DeviceSwitchRequested);
                            restart_for_forwarder.store(true, Ordering::Relaxed);
                        }
                        _ => {}
                    }
                    // Always forward the command to the looper engine
                    if forward_tx.send(cmd).is_err() {
                        break;
                    }
                    // If a restart was requested, exit to allow streams to be rebuilt
                    if restart_for_forwarder.load(Ordering::Relaxed) {
                        break;
                    }
                }
            });

            // Run the inner thread which owns the active streams
            if let Err(e) = run_audio_thread_inner(
                audio_stream,
                Arc::clone(&looper_clone),
                forward_rx,
                event_sender.clone(),
                debug_mode,
                Arc::clone(&restart_audio_clone),
            ) {
                eprintln!("Audio thread error: {}", e);
            }

            // Exit if not restarting
            if !restart_audio_clone.load(Ordering::Relaxed) {
                break;
            }
            restart_audio_clone.store(false, Ordering::Relaxed);
            if debug_mode {
                println!("Restarting audio with new devices...");
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

fn run_audio_thread_inner(
    audio_stream: AudioStream,
    looper_engine: Arc<LooperEngine>,
    command_receiver: channel::Receiver<LayerCommand>,
    event_sender: channel::Sender<AudioEvent>,
    debug_mode: bool,
    restart_flag: Arc<AtomicBool>,
) -> Result<()> {
    let (_input_stream, _output_stream) = audio_stream.start_audio_looper(
        looper_engine,
        command_receiver,
        event_sender,
        debug_mode,
    )?;

    // Keep streams alive and watch for restart
    loop {
        if restart_flag.load(Ordering::Relaxed) {
            break;
        }
        thread::sleep(std::time::Duration::from_millis(100));
    }
    Ok(())
}
