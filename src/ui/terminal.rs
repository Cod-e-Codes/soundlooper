use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph},
};
use std::{
    io,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use crate::audio::stream::enumerate_device_names;
use crate::audio::{AudioEvent, AudioLayer, LayerCommand};

#[derive(Debug, Clone, PartialEq)]
enum InputMode {
    FilePicker {
        layer_id: usize,
        current_dir: String,
        entries: Vec<FileEntry>,
        selected_index: usize,
        scroll_offset: usize,
    },
    ExportWav,
    SetBpm,
    DevicePicker {
        inputs: Vec<String>,
        outputs: Vec<String>,
        column: usize,         // 0 = inputs, 1 = outputs
        selected_index: usize, // index within current column
        scroll_offset: usize,  // scroll for the current column
    },
}

#[derive(Debug, Clone, PartialEq)]
enum FileEntry {
    Directory(String),
    WavFile(String),
}

#[derive(Debug, Clone, PartialEq)]
enum HeaderStatus {
    InputPrompt(String, String), // (prompt, current_input)
    Success(String),             // message
    Cancelled,
}

pub struct TerminalUI {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    layers: Arc<Vec<Arc<Mutex<AudioLayer>>>>,
    selected_layer: usize,
    command_sender: crossbeam::channel::Sender<LayerCommand>,
    event_receiver: crossbeam::channel::Receiver<AudioEvent>,
    is_running: bool,
    last_update: Instant,
    last_key_time: Instant,
    // key_debounce_duration: Duration, // Temporarily disabled for debugging
    input_device_name: String,
    output_device_name: String,
    // File input state
    input_mode: Option<InputMode>,
    input_buffer: String,
    input_prompt: String,
    // Header status system
    header_status: Option<HeaderStatus>,
    status_timer: Option<Instant>,
    // File picker overlay
    file_picker_overlay: bool,
    // Tempo/Sync state
    beat_sync_enabled: bool,
    bpm_display: f64,
    current_beat: u32,
    current_measure: usize,
    metronome_enabled: bool,
    count_in_mode_enabled: bool,
    count_in_remaining: Option<(usize, u32)>,
}

impl TerminalUI {
    pub fn new(
        layers: Arc<Vec<Arc<Mutex<AudioLayer>>>>,
        command_sender: crossbeam::channel::Sender<LayerCommand>,
        event_receiver: crossbeam::channel::Receiver<AudioEvent>,
        input_device_name: &str,
        output_device_name: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        Ok(Self {
            terminal,
            layers,
            selected_layer: 0,
            command_sender,
            event_receiver,
            is_running: true,
            last_update: Instant::now(),
            last_key_time: Instant::now(),
            // key_debounce_duration: Duration::from_millis(150), // Temporarily disabled for debugging
            input_device_name: input_device_name.to_string(),
            output_device_name: output_device_name.to_string(),
            // File input state
            input_mode: None,
            input_buffer: String::new(),
            input_prompt: String::new(),
            // Header status system
            header_status: None,
            status_timer: None,
            // File picker overlay
            file_picker_overlay: false,
            // Tempo/Sync state
            beat_sync_enabled: true,
            bpm_display: 120.0,
            current_beat: 1,
            current_measure: 0,
            metronome_enabled: false,
            count_in_mode_enabled: false,
            count_in_remaining: None,
        })
    }

    pub fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        while self.is_running {
            // Process events
            self.process_events()?;

            // Check status timer
            self.check_status_timer();

            // Update display if enough time has passed
            if self.last_update.elapsed() >= Duration::from_millis(50) {
                self.draw()?;
                self.last_update = Instant::now();
            }

            // Small sleep to prevent excessive CPU usage
            std::thread::sleep(Duration::from_millis(1));
        }

        Ok(())
    }

    fn process_events(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if event::poll(Duration::from_millis(0))?
            && let Event::Key(key) = event::read()?
        {
            // Process key presses
            if key.kind == KeyEventKind::Press {
                self.handle_key_event(key)?;
                self.last_key_time = Instant::now();
            }
        }

        // Process audio events
        while let Ok(event) = self.event_receiver.try_recv() {
            self.handle_audio_event(event);
        }

        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<(), Box<dyn std::error::Error>> {
        // Handle input mode first
        if let Some(ref input_mode) = self.input_mode {
            return self.handle_input_key(key, input_mode.clone());
        }

        match key.code {
            KeyCode::Char('q') => {
                self.is_running = false;
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                // Toggle metronome
                self.metronome_enabled = !self.metronome_enabled;
                let new_state = self.metronome_enabled;
                let _ = self
                    .command_sender
                    .send(LayerCommand::ToggleMetronome(new_state));
                self.show_success(if new_state {
                    "Metronome ON"
                } else {
                    "Metronome OFF"
                });
            }
            KeyCode::Char('b') | KeyCode::Char('B') => {
                let _ = self.command_sender.send(LayerCommand::TapTempo);
            }
            KeyCode::Char('t') | KeyCode::Char('T') => {
                self.start_input_mode(InputMode::SetBpm, "Set BPM: ");
            }
            KeyCode::Char('g') | KeyCode::Char('G') => {
                self.beat_sync_enabled = !self.beat_sync_enabled;
                let _ = self
                    .command_sender
                    .send(LayerCommand::ToggleBeatSync(self.beat_sync_enabled));
                self.show_success(if self.beat_sync_enabled {
                    "Beat Sync ON"
                } else {
                    "Beat Sync OFF"
                });
            }
            KeyCode::Char('h') | KeyCode::Char('H') => {
                // Toggle Count-in Mode
                self.count_in_mode_enabled = !self.count_in_mode_enabled;
                let new_state = self.count_in_mode_enabled;
                let _ = self
                    .command_sender
                    .send(LayerCommand::ToggleCountInMode(new_state));
                self.show_success(if new_state {
                    "Count-in Mode ON"
                } else {
                    "Count-in Mode OFF"
                });
            }
            KeyCode::Char('1') => {
                self.handle_layer_key(0);
            }
            KeyCode::Char('2') => {
                self.handle_layer_key(1);
            }
            KeyCode::Char('3') => {
                self.handle_layer_key(2);
            }
            KeyCode::Char('4') => {
                self.handle_layer_key(3);
            }
            KeyCode::Char('5') => {
                self.handle_layer_key(4);
            }
            KeyCode::Char('6') => {
                self.handle_layer_key(5);
            }
            KeyCode::Char('7') => {
                self.handle_layer_key(6);
            }
            KeyCode::Char('8') => {
                self.handle_layer_key(7);
            }
            KeyCode::Char('9') => {
                self.handle_layer_key(8);
            }
            KeyCode::Char('0') => {
                self.handle_layer_key(9);
            }
            KeyCode::Char('r') => {
                // Record on selected layer
                if self.beat_sync_enabled {
                    let _ = self
                        .command_sender
                        .send(LayerCommand::SyncRecord(self.selected_layer));
                } else {
                    self.toggle_layer_record(self.selected_layer);
                }
            }
            KeyCode::Char('s') => {
                // Stop selected layer only (stop recording immediately; stop playback synced if enabled)
                let (is_recording, _is_playing) = match self.layers[self.selected_layer].lock() {
                    Ok(layer) => (layer.is_recording, layer.is_playing),
                    Err(_) => (false, false),
                };
                if is_recording {
                    let _ = self
                        .command_sender
                        .send(LayerCommand::StopRecording(self.selected_layer));
                } else if self.beat_sync_enabled {
                    let _ = self
                        .command_sender
                        .send(LayerCommand::SyncStop(self.selected_layer));
                } else {
                    self.stop_selected_layer();
                }
            }
            KeyCode::Char(' ') => {
                // Stop all
                let _ = self.command_sender.send(LayerCommand::StopAll);
            }
            KeyCode::Up => {
                if self.selected_layer > 0 {
                    self.selected_layer -= 1;
                }
            }
            KeyCode::Down => {
                if self.selected_layer < self.layers.len() - 1 {
                    self.selected_layer += 1;
                }
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                self.adjust_volume(0.1);
            }
            KeyCode::Char('-') => {
                self.adjust_volume(-0.1);
            }
            KeyCode::Char('m') => {
                self.toggle_mute(self.selected_layer);
            }
            KeyCode::Char('l') => {
                self.toggle_solo(self.selected_layer);
            }
            KeyCode::Char('p') => {
                if self.beat_sync_enabled {
                    let _ = self
                        .command_sender
                        .send(LayerCommand::SyncPlay(self.selected_layer));
                } else {
                    self.start_playback(self.selected_layer);
                }
            }
            KeyCode::Char('c') => {
                self.clear_layer(self.selected_layer);
            }
            KeyCode::Char('x') => {
                // Clear all layers
                let _ = self.command_sender.send(LayerCommand::ClearAll);
            }
            KeyCode::Char('a') => {
                // Play all layers
                let _ = self.command_sender.send(LayerCommand::PlayAll);
            }
            KeyCode::Char('i') => {
                // Import WAV to selected layer
                self.import_wav_to_layer(self.selected_layer);
            }
            KeyCode::Char('o') => {
                // Open device picker
                self.open_device_picker();
            }
            KeyCode::Char('e') => {
                // Export composition as WAV
                self.export_composition();
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_audio_event(&mut self, _event: AudioEvent) {
        // Provide immediate user feedback on import/export results
        match _event {
            AudioEvent::WavImported(layer_id, path) => {
                self.show_success(&format!("Imported to Layer {}: {}", layer_id + 1, path));
            }
            AudioEvent::WavExported(path) => {
                self.show_success(&format!("Exported: {}", path));
            }
            AudioEvent::Error(msg) => {
                self.show_success(&format!("Error: {}", msg));
            }
            AudioEvent::BpmChanged(bpm) => {
                self.bpm_display = bpm;
                self.show_success(&format!("BPM: {:.1}", bpm));
            }
            AudioEvent::Beat(beat, measure) => {
                self.current_beat = beat;
                self.current_measure = measure;
            }
            AudioEvent::CountInStarted { layer_id, beats } => {
                self.count_in_remaining = Some((layer_id, beats));
            }
            AudioEvent::CountInFinished { layer_id } => {
                self.count_in_remaining = None;
                self.show_success(&format!("Count-in done L{}", layer_id + 1));
            }
            AudioEvent::CountInTick {
                layer_id,
                remaining_beats,
            } => {
                let _ = layer_id; // we keep for potential per-layer UI later
                self.count_in_remaining = Some((layer_id, remaining_beats));
            }
            AudioEvent::CountInModeToggled(on) => {
                self.show_success(if on {
                    "Count-in Mode ON"
                } else {
                    "Count-in Mode OFF"
                });
            }
            AudioEvent::DeviceSwitchRequested => {
                self.show_success("Switching audio devices...");
            }
            AudioEvent::DeviceSwitchComplete => {
                self.show_success("Device switch complete!");
            }
            AudioEvent::DeviceSwitchFailed(msg) => {
                self.show_success(&format!("Device switch failed: {}", msg));
            }
            AudioEvent::DevicesUpdated(input, output) => {
                match input {
                    Some(name) => self.input_device_name = name,
                    None => self.input_device_name = "No device".to_string(),
                }
                match output {
                    Some(name) => self.output_device_name = name,
                    None => self.output_device_name = "No device".to_string(),
                }
                self.show_success("Devices updated");
            }
            AudioEvent::MetronomeToggled(on) => {
                self.show_success(if on { "Metronome ON" } else { "Metronome OFF" });
            }
            _ => {
                // no-op
            }
        }
    }

    fn toggle_layer_record(&mut self, layer_id: usize) {
        if layer_id < self.layers.len()
            && let Ok(layer) = self.layers[layer_id].lock()
        {
            if layer.is_recording {
                // Stop recording (this will automatically start playback)
                let _ = self
                    .command_sender
                    .send(LayerCommand::StopRecording(layer_id));
            } else if layer.is_playing {
                // Stop playback
                let _ = self
                    .command_sender
                    .send(LayerCommand::StopPlaying(layer_id));
            } else {
                // Start recording
                let _ = self.command_sender.send(LayerCommand::Record(layer_id));
            }
        }
    }

    fn handle_layer_key(&mut self, layer_id: usize) {
        if layer_id >= self.layers.len() {
            return;
        }
        let (is_recording, is_playing) = match self.layers[layer_id].lock() {
            Ok(layer) => (layer.is_recording, layer.is_playing),
            Err(_) => return,
        };

        if is_recording {
            let _ = self
                .command_sender
                .send(LayerCommand::StopRecording(layer_id));
        } else if is_playing {
            if self.beat_sync_enabled {
                let _ = self.command_sender.send(LayerCommand::SyncStop(layer_id));
            } else {
                let _ = self
                    .command_sender
                    .send(LayerCommand::StopPlaying(layer_id));
            }
        } else if self.beat_sync_enabled {
            let _ = self.command_sender.send(LayerCommand::SyncRecord(layer_id));
        } else {
            let _ = self.command_sender.send(LayerCommand::Record(layer_id));
        }
    }

    fn adjust_volume(&mut self, delta: f32) {
        if let Ok(layer) = self.layers[self.selected_layer].lock() {
            let new_volume = (layer.volume + delta).clamp(0.0, 1.0);
            let _ = self
                .command_sender
                .send(LayerCommand::SetVolume(self.selected_layer, new_volume));
        }
    }

    fn toggle_mute(&mut self, layer_id: usize) {
        let _ = self.command_sender.send(LayerCommand::Mute(layer_id));
    }

    fn toggle_solo(&mut self, layer_id: usize) {
        let _ = self.command_sender.send(LayerCommand::Solo(layer_id));
    }

    fn start_playback(&mut self, layer_id: usize) {
        if layer_id < self.layers.len() {
            let _ = self.command_sender.send(LayerCommand::Play(layer_id));
        }
    }

    fn stop_selected_layer(&mut self) {
        if self.selected_layer < self.layers.len() {
            let _ = self
                .command_sender
                .send(LayerCommand::StopPlaying(self.selected_layer));
        }
    }

    fn clear_layer(&mut self, layer_id: usize) {
        if layer_id < self.layers.len() {
            let _ = self.command_sender.send(LayerCommand::Clear(layer_id));
        }
    }

    fn handle_input_key(
        &mut self,
        key: KeyEvent,
        input_mode: InputMode,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match key.code {
            KeyCode::Enter => {
                // Finish input
                match input_mode {
                    InputMode::FilePicker {
                        layer_id,
                        current_dir,
                        entries,
                        selected_index,
                        scroll_offset: _,
                    } => {
                        if selected_index < entries.len() {
                            match &entries[selected_index] {
                                FileEntry::Directory(dir_name) => {
                                    if dir_name == ".." {
                                        // Navigate to parent directory
                                        let current_path = std::path::Path::new(&current_dir);
                                        if let Some(parent) = current_path.parent() {
                                            let new_path = parent.to_string_lossy().to_string();
                                            // Handle empty path case (should go to current directory)
                                            let target_path = if new_path.is_empty() {
                                                ".".to_string()
                                            } else {
                                                new_path
                                            };
                                            if let Err(e) =
                                                self.navigate_to_directory(layer_id, target_path)
                                            {
                                                self.show_success(&format!("Error: {}", e));
                                            }
                                        } else {
                                            // Already at root, stay here
                                            self.show_success("Already at root directory");
                                        }
                                        return Ok(()); // Don't exit input mode, just navigate
                                    } else if dir_name == "🏠 Home" {
                                        // Navigate to home directory
                                        let home_dir = std::env::var("HOME")
                                            .or_else(|_| std::env::var("USERPROFILE"))
                                            .unwrap_or_else(|_| ".".to_string());
                                        if let Err(e) =
                                            self.navigate_to_directory(layer_id, home_dir)
                                        {
                                            self.show_success(&format!("Error: {}", e));
                                        }
                                        return Ok(()); // Don't exit input mode, just navigate
                                    } else {
                                        // Navigate into directory
                                        let new_path = if current_dir == "." {
                                            dir_name.clone()
                                        } else {
                                            std::path::Path::new(&current_dir)
                                                .join(dir_name)
                                                .to_string_lossy()
                                                .to_string()
                                        };
                                        if let Err(e) =
                                            self.navigate_to_directory(layer_id, new_path)
                                        {
                                            self.show_success(&format!("Error: {}", e));
                                        }
                                        return Ok(()); // Don't exit input mode, just navigate
                                    }
                                }
                                FileEntry::WavFile(filename) => {
                                    let full_path = if current_dir == "." {
                                        filename.clone()
                                    } else {
                                        std::path::Path::new(&current_dir)
                                            .join(filename)
                                            .to_string_lossy()
                                            .to_string()
                                    };

                                    // Validate the file before importing
                                    match self.validate_import_file(&full_path) {
                                        Ok(_) => {
                                            // Immediately show importing status
                                            self.show_success(&format!("Importing: {}", full_path));
                                            let _ =
                                                self.command_sender.send(LayerCommand::ImportWav(
                                                    layer_id,
                                                    full_path.clone(),
                                                ));
                                        }
                                        Err(error) => {
                                            self.show_success(&format!("Import failed: {}", error));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    InputMode::DevicePicker {
                        inputs,
                        outputs,
                        column,
                        selected_index,
                        scroll_offset: _,
                    } => {
                        // Apply device change immediately
                        if column == 0 && !inputs.is_empty() && selected_index < inputs.len() {
                            let device_name = inputs[selected_index].clone();
                            self.input_device_name = device_name.clone();
                            let _ = self
                                .command_sender
                                .send(LayerCommand::SwitchInputDevice(device_name.clone()));
                            self.show_success(&format!("Switching to input: {}...", device_name));
                        } else if column == 1
                            && !outputs.is_empty()
                            && selected_index < outputs.len()
                        {
                            let device_name = outputs[selected_index].clone();
                            self.output_device_name = device_name.clone();
                            let _ = self
                                .command_sender
                                .send(LayerCommand::SwitchOutputDevice(device_name.clone()));
                            self.show_success(&format!("Switching to output: {}...", device_name));
                        }

                        // Keep the picker open
                        self.input_mode = Some(InputMode::DevicePicker {
                            inputs,
                            outputs,
                            column,
                            selected_index,
                            scroll_offset: 0,
                        });
                        return Ok(());
                    }
                    InputMode::ExportWav => {
                        let filename = self.ensure_wav_extension(self.input_buffer.clone());

                        // Validate the export path before exporting
                        match self.validate_export_path(&filename) {
                            Ok(_) => {
                                let _ = self
                                    .command_sender
                                    .send(LayerCommand::ExportWav(filename.clone()));
                                self.show_success(&format!("Exported: {}", filename));
                            }
                            Err(error) => {
                                self.show_success(&format!("Export failed: {}", error));
                            }
                        }
                    }
                    InputMode::SetBpm => {
                        let text = self.input_buffer.trim();
                        if let Ok(value) = text.parse::<f64>() {
                            let _ = self.command_sender.send(LayerCommand::SetBpm(value));
                        } else {
                            self.show_success("Invalid BPM");
                        }
                    }
                }
                self.exit_input_mode();
            }
            KeyCode::Esc => {
                // Cancel input
                self.show_cancelled();
                self.exit_input_mode();
            }
            KeyCode::Tab => {
                if let InputMode::DevicePicker {
                    inputs,
                    outputs,
                    column,
                    selected_index,
                    scroll_offset,
                } = input_mode
                {
                    let new_column = 1 - column;
                    // Clamp selection within new column size
                    let max_len = if new_column == 0 {
                        inputs.len()
                    } else {
                        outputs.len()
                    };
                    let new_selected = if max_len == 0 {
                        0
                    } else if selected_index >= max_len {
                        max_len - 1
                    } else {
                        selected_index
                    };
                    let new_scroll =
                        self.calculate_scroll_offset(new_selected, scroll_offset, max_len);
                    self.input_mode = Some(InputMode::DevicePicker {
                        inputs,
                        outputs,
                        column: new_column,
                        selected_index: new_selected,
                        scroll_offset: new_scroll,
                    });
                    return Ok(());
                }
            }
            KeyCode::Backspace => {
                self.input_buffer.pop();
                self.update_input_display();
            }
            KeyCode::Char(c) => {
                self.input_buffer.push(c);
                self.update_input_display();
            }
            KeyCode::Up => {
                if let InputMode::FilePicker {
                    layer_id,
                    current_dir,
                    entries,
                    selected_index,
                    scroll_offset,
                } = input_mode
                {
                    let new_index = if selected_index > 0 {
                        selected_index - 1
                    } else {
                        entries.len().saturating_sub(1)
                    };
                    let new_scroll_offset =
                        self.calculate_scroll_offset(new_index, scroll_offset, entries.len());
                    self.input_mode = Some(InputMode::FilePicker {
                        layer_id,
                        current_dir: current_dir.clone(),
                        entries: entries.clone(),
                        selected_index: new_index,
                        scroll_offset: new_scroll_offset,
                    });
                } else if let InputMode::DevicePicker {
                    inputs,
                    outputs,
                    column,
                    selected_index,
                    scroll_offset,
                } = input_mode
                {
                    let list_len = if column == 0 {
                        inputs.len()
                    } else {
                        outputs.len()
                    };
                    let new_index = if list_len == 0 {
                        0
                    } else if selected_index > 0 {
                        selected_index - 1
                    } else {
                        list_len.saturating_sub(1)
                    };
                    let new_scroll =
                        self.calculate_scroll_offset(new_index, scroll_offset, list_len);
                    self.input_mode = Some(InputMode::DevicePicker {
                        inputs,
                        outputs,
                        column,
                        selected_index: new_index,
                        scroll_offset: new_scroll,
                    });
                }
            }
            KeyCode::Down => {
                if let InputMode::FilePicker {
                    layer_id,
                    current_dir,
                    entries,
                    selected_index,
                    scroll_offset,
                } = input_mode
                {
                    let new_index = if selected_index + 1 < entries.len() {
                        selected_index + 1
                    } else {
                        0
                    };
                    let new_scroll_offset =
                        self.calculate_scroll_offset(new_index, scroll_offset, entries.len());
                    self.input_mode = Some(InputMode::FilePicker {
                        layer_id,
                        current_dir: current_dir.clone(),
                        entries: entries.clone(),
                        selected_index: new_index,
                        scroll_offset: new_scroll_offset,
                    });
                } else if let InputMode::DevicePicker {
                    inputs,
                    outputs,
                    column,
                    selected_index,
                    scroll_offset,
                } = input_mode
                {
                    let list_len = if column == 0 {
                        inputs.len()
                    } else {
                        outputs.len()
                    };
                    let new_index = if list_len == 0 {
                        0
                    } else if selected_index + 1 < list_len {
                        selected_index + 1
                    } else {
                        0
                    };
                    let new_scroll =
                        self.calculate_scroll_offset(new_index, scroll_offset, list_len);
                    self.input_mode = Some(InputMode::DevicePicker {
                        inputs,
                        outputs,
                        column,
                        selected_index: new_index,
                        scroll_offset: new_scroll,
                    });
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn start_input_mode(&mut self, mode: InputMode, prompt: &str) {
        self.input_mode = Some(mode);
        self.input_buffer.clear();
        self.input_prompt = prompt.to_string();
        self.header_status = Some(HeaderStatus::InputPrompt(
            prompt.to_string(),
            "".to_string(),
        ));
    }

    fn start_file_picker(&mut self, layer_id: usize) -> Result<(), Box<dyn std::error::Error>> {
        // Start from user's home directory for better navigation
        let home_dir = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE")) // Windows fallback
            .unwrap_or_else(|_| ".".to_string());
        self.navigate_to_directory(layer_id, home_dir)
    }

    fn navigate_to_directory(
        &mut self,
        layer_id: usize,
        dir_path: String,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut entries = Vec::new();

        // Add parent directory entry if we can navigate up
        let current_path = std::path::Path::new(&dir_path);
        if let Some(_parent) = current_path.parent() {
            entries.push(FileEntry::Directory("..".to_string()));
        }

        // Add home directory entry for quick navigation (if not already at home)
        let home_dir = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".to_string());
        if dir_path != home_dir {
            entries.push(FileEntry::Directory("🏠 Home".to_string()));
        }

        let dir_entries = std::fs::read_dir(&dir_path)?;

        for entry in dir_entries {
            let entry = entry?;
            let path = entry.path();
            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                if path.is_dir() {
                    entries.push(FileEntry::Directory(filename.to_string()));
                } else if filename.to_lowercase().ends_with(".wav") {
                    entries.push(FileEntry::WavFile(filename.to_string()));
                }
            }
        }

        // Sort: special entries first, then directories, then files, all alphabetically
        entries.sort_by(|a, b| match (a, b) {
            (FileEntry::Directory(a), FileEntry::Directory(b)) => {
                // Special entries should come first
                if a == ".." {
                    std::cmp::Ordering::Less
                } else if b == ".." {
                    std::cmp::Ordering::Greater
                } else if a == "🏠 Home" {
                    std::cmp::Ordering::Less
                } else if b == "🏠 Home" {
                    std::cmp::Ordering::Greater
                } else {
                    a.cmp(b)
                }
            }
            (FileEntry::WavFile(a), FileEntry::WavFile(b)) => a.cmp(b),
            (FileEntry::Directory(_), FileEntry::WavFile(_)) => std::cmp::Ordering::Less,
            (FileEntry::WavFile(_), FileEntry::Directory(_)) => std::cmp::Ordering::Greater,
        });

        if entries.is_empty() {
            self.show_success("No directories or WAV files found");
            return Ok(());
        }

        self.input_mode = Some(InputMode::FilePicker {
            layer_id,
            current_dir: dir_path,
            entries,
            selected_index: 0,
            scroll_offset: 0,
        });
        self.file_picker_overlay = true;
        Ok(())
    }

    fn calculate_scroll_offset(
        &self,
        selected_index: usize,
        current_scroll_offset: usize,
        total_entries: usize,
    ) -> usize {
        const VISIBLE_ITEMS: usize = 15; // Maximum items visible in the file picker

        if total_entries <= VISIBLE_ITEMS {
            return 0; // No scrolling needed
        }

        // If selected item is above the visible area, scroll up
        if selected_index < current_scroll_offset {
            return selected_index;
        }

        // If selected item is below the visible area, scroll down
        if selected_index >= current_scroll_offset + VISIBLE_ITEMS {
            return selected_index.saturating_sub(VISIBLE_ITEMS - 1);
        }

        // Keep current scroll offset
        current_scroll_offset
    }

    fn validate_import_file(&self, file_path: &str) -> Result<(), String> {
        let path = std::path::Path::new(file_path);

        // Check if file exists
        if !path.exists() {
            return Err("File does not exist".to_string());
        }

        // Check if it's actually a file (not a directory)
        if !path.is_file() {
            return Err("Path is not a file".to_string());
        }

        // Check file extension
        if let Some(extension) = path.extension() {
            if extension.to_string_lossy().to_lowercase() != "wav" {
                return Err("File must have .wav extension".to_string());
            }
        } else {
            return Err("File must have .wav extension".to_string());
        }

        // Check file size (limit to 100MB)
        const MAX_FILE_SIZE: u64 = 100 * 1024 * 1024; // 100MB
        if let Ok(metadata) = std::fs::metadata(path) {
            if metadata.len() > MAX_FILE_SIZE {
                return Err("File too large (max 100MB)".to_string());
            }
            if metadata.len() == 0 {
                return Err("File is empty".to_string());
            }
        } else {
            return Err("Cannot read file metadata".to_string());
        }

        // Check for dangerous paths (prevent access to system directories)
        let path_str = path.to_string_lossy().to_lowercase();
        let dangerous_paths = [
            "c:\\windows\\",
            "c:\\system32\\",
            "c:\\program files\\",
            "c:\\program files (x86)\\",
            "/system/",
            "/bin/",
            "/sbin/",
            "/usr/bin/",
            "/usr/sbin/",
            "/etc/",
        ];

        for dangerous in &dangerous_paths {
            if path_str.contains(dangerous) {
                return Err("Access to system directories not allowed".to_string());
            }
        }

        Ok(())
    }

    fn validate_export_path(&self, file_path: &str) -> Result<(), String> {
        let path = std::path::Path::new(file_path);

        // Check file extension
        if let Some(extension) = path.extension() {
            if extension.to_string_lossy().to_lowercase() != "wav" {
                return Err("Export file must have .wav extension".to_string());
            }
        } else {
            return Err("Export file must have .wav extension".to_string());
        }

        // Check for dangerous paths
        let path_str = path.to_string_lossy().to_lowercase();
        let dangerous_paths = [
            "c:\\windows\\",
            "c:\\system32\\",
            "c:\\program files\\",
            "c:\\program files (x86)\\",
            "/system/",
            "/bin/",
            "/sbin/",
            "/usr/bin/",
            "/usr/sbin/",
            "/etc/",
        ];

        for dangerous in &dangerous_paths {
            if path_str.contains(dangerous) {
                return Err("Cannot write to system directories".to_string());
            }
        }

        // Check if file already exists and warn
        if path.exists() {
            return Err("File already exists - choose a different name".to_string());
        }

        Ok(())
    }

    fn exit_input_mode(&mut self) {
        self.input_mode = None;
        self.input_buffer.clear();
        self.input_prompt.clear();
        self.file_picker_overlay = false;
        // Keep header_status for success/cancel messages
    }

    fn ensure_wav_extension(&self, filename: String) -> String {
        if filename.to_lowercase().ends_with(".wav") {
            filename
        } else {
            format!("{}.wav", filename)
        }
    }

    fn update_input_display(&mut self) {
        if let Some(HeaderStatus::InputPrompt(ref prompt, _)) = self.header_status {
            let display_input = if prompt.starts_with("Set BPM") {
                self.input_buffer.clone()
            } else if self.input_buffer.is_empty() {
                ".wav".to_string()
            } else {
                format!("{}.wav", self.input_buffer)
            };
            self.header_status = Some(HeaderStatus::InputPrompt(prompt.clone(), display_input));
        }
    }

    fn show_success(&mut self, message: &str) {
        self.header_status = Some(HeaderStatus::Success(message.to_string()));
        self.status_timer = Some(Instant::now());
    }

    fn show_cancelled(&mut self) {
        self.header_status = Some(HeaderStatus::Cancelled);
        self.status_timer = Some(Instant::now());
    }

    fn check_status_timer(&mut self) {
        if let Some(timer) = self.status_timer
            && timer.elapsed() >= Duration::from_secs(3)
        {
            self.header_status = None;
            self.status_timer = None;
        }
    }

    fn import_wav_to_layer(&mut self, layer_id: usize) {
        if let Err(e) = self.start_file_picker(layer_id) {
            self.show_success(&format!("Error: {}", e));
        }
    }

    fn export_composition(&mut self) {
        self.start_input_mode(InputMode::ExportWav, "Export composition as: ");
    }

    fn open_device_picker(&mut self) {
        match enumerate_device_names() {
            Ok((inputs, outputs)) => {
                // Start focused on inputs
                let selected = 0usize;
                let scroll = 0usize;
                self.input_mode = Some(InputMode::DevicePicker {
                    inputs,
                    outputs,
                    column: 0,
                    selected_index: selected,
                    scroll_offset: scroll,
                });
                self.file_picker_overlay = true;
            }
            Err(e) => {
                self.show_success(&format!("Error listing devices: {}", e));
            }
        }
    }

    fn draw(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let selected_layer = self.selected_layer;
        let layers = Arc::clone(&self.layers);
        let countdown = self.count_in_remaining;

        // Extract values to avoid borrow checker issues
        let input_device_name = self.input_device_name.clone();
        let output_device_name = self.output_device_name.clone();
        let header_status = self.header_status.clone();
        let file_picker_overlay = self.file_picker_overlay;
        let input_mode = self.input_mode.clone();

        self.terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3), // Header
                    Constraint::Min(0),    // Layers
                    Constraint::Length(5), // Footer (3 lines of content + borders)
                ])
                .split(f.area());

            Self::draw_header_static(
                f,
                chunks[0],
                &input_device_name,
                &output_device_name,
                &header_status,
            );
            Self::draw_layers_static(f, chunks[1], &layers, selected_layer, countdown);
            Self::draw_footer_static(
                f,
                chunks[2],
                self.bpm_display,
                self.current_beat,
                self.current_measure,
                self.beat_sync_enabled,
                self.metronome_enabled,
            );

            // Draw file picker overlay if active
            if file_picker_overlay {
                match input_mode {
                    Some(InputMode::FilePicker { .. }) => {
                        Self::draw_file_picker_overlay_static(f, f.area(), &input_mode);
                    }
                    Some(InputMode::DevicePicker { .. }) => {
                        Self::draw_device_picker_overlay_static(f, f.area(), &input_mode);
                    }
                    _ => {}
                }
            }
        })?;
        Ok(())
    }

    fn draw_header_static(
        f: &mut Frame,
        area: Rect,
        input_device_name: &str,
        output_device_name: &str,
        header_status: &Option<HeaderStatus>,
    ) {
        let header_text = match header_status {
            Some(HeaderStatus::InputPrompt(prompt, current_input)) => {
                format!("{} {}", prompt, current_input)
            }
            Some(HeaderStatus::Success(message)) => {
                format!("✓ {}", message)
            }
            Some(HeaderStatus::Cancelled) => "✗ Cancelled".to_string(),
            None => {
                format!(
                    "Input: {} | Output: {}",
                    input_device_name, output_device_name
                )
            }
        };

        let header = Paragraph::new(header_text)
            .style(
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(ratatui::layout::Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title("Soundlooper"));
        f.render_widget(header, area);
    }

    fn draw_file_picker_overlay_static(f: &mut Frame, area: Rect, input_mode: &Option<InputMode>) {
        if let Some(InputMode::FilePicker {
            layer_id,
            current_dir,
            entries,
            selected_index,
            scroll_offset,
        }) = input_mode
        {
            // Create a centered overlay
            let overlay_width = 60u16;
            let overlay_height = ((entries.len() + 5).min(20)) as u16; // +5 for title, path, instructions, and borders
            let x = (area.width.saturating_sub(overlay_width)) / 2;
            let y = (area.height.saturating_sub(overlay_height)) / 2;

            let overlay_area = Rect::new(x, y, overlay_width, overlay_height);

            // Draw solid background to prevent corruption
            for row in y..y + overlay_height {
                let bg_line = Paragraph::new(" ".repeat(overlay_width as usize))
                    .style(Style::default().bg(Color::Black));
                let line_area = Rect::new(x, row, overlay_width, 1);
                f.render_widget(bg_line, line_area);
            }

            // Draw border and title
            let bg = Paragraph::new(" ".repeat(overlay_width as usize))
                .style(Style::default().bg(Color::Black))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(format!("Import WAV to Layer {}", layer_id + 1)),
                );
            f.render_widget(bg, overlay_area);

            // Draw current directory path
            let path_text = format!("Path: {}", current_dir);
            let path_area = Rect::new(x + 1, y + 1, overlay_width - 2, 1);
            let path_widget = Paragraph::new(path_text.as_str())
                .style(Style::default().fg(Color::Cyan))
                .alignment(ratatui::layout::Alignment::Left);
            f.render_widget(path_widget, path_area);

            // Draw file list
            let list_area = Rect::new(x + 1, y + 2, overlay_width - 2, overlay_height - 3);
            let mut items = Vec::new();

            // Calculate visible range based on scroll offset
            const VISIBLE_ITEMS: usize = 15;
            let start_index = *scroll_offset;
            let _end_index = (start_index + VISIBLE_ITEMS).min(entries.len());

            for (i, entry) in entries
                .iter()
                .enumerate()
                .skip(start_index)
                .take(VISIBLE_ITEMS)
            {
                let display_index = i; // Original index for selection highlighting
                let (display_text, style) = match entry {
                    FileEntry::Directory(dir_name) => {
                        let (icon, color) = if dir_name == ".." {
                            ("⬆️ ", Color::Cyan)
                        } else if dir_name == "🏠 Home" {
                            ("🏠", Color::Green)
                        } else {
                            ("📁", Color::Yellow)
                        };
                        let text = if dir_name == "🏠 Home" {
                            dir_name.clone() // Already has the emoji
                        } else {
                            format!("{} {}", icon, dir_name)
                        };
                        let base_style = if display_index == *selected_index {
                            Style::default().bg(Color::Blue).fg(Color::White)
                        } else {
                            Style::default().fg(color)
                        };
                        (text, base_style)
                    }
                    FileEntry::WavFile(file_name) => {
                        let text = format!("🎵 {}", file_name);
                        let base_style = if display_index == *selected_index {
                            Style::default().bg(Color::Blue).fg(Color::White)
                        } else {
                            Style::default().fg(Color::White)
                        };
                        (text, base_style)
                    }
                };
                items.push(ListItem::new(display_text).style(style));
            }

            let list = List::new(items)
                .block(Block::default().borders(Borders::NONE))
                .highlight_style(Style::default().bg(Color::Blue).fg(Color::White));

            f.render_widget(list, list_area);

            // Draw instructions at the bottom
            let instructions = "↑↓: Navigate | Enter: Select/Open | Esc: Cancel";
            let instructions_area = Rect::new(x + 1, y + overlay_height - 1, overlay_width - 2, 1);
            let instructions_widget = Paragraph::new(instructions)
                .style(Style::default().fg(Color::Yellow))
                .alignment(ratatui::layout::Alignment::Center);
            f.render_widget(instructions_widget, instructions_area);
        }
    }

    fn draw_device_picker_overlay_static(
        f: &mut Frame,
        area: Rect,
        input_mode: &Option<InputMode>,
    ) {
        if let Some(InputMode::DevicePicker {
            inputs,
            outputs,
            column,
            selected_index,
            scroll_offset,
        }) = input_mode
        {
            let overlay_width = 70u16;
            let overlay_height = 20u16;
            let x = (area.width.saturating_sub(overlay_width)) / 2;
            let y = (area.height.saturating_sub(overlay_height)) / 2;

            let overlay_area = Rect::new(x, y, overlay_width, overlay_height);

            // Background and border
            for row in y..y + overlay_height {
                let bg_line = Paragraph::new(" ".repeat(overlay_width as usize))
                    .style(Style::default().bg(Color::Black));
                let line_area = Rect::new(x, row, overlay_width, 1);
                f.render_widget(bg_line, line_area);
            }
            let bg = Paragraph::new(" ".repeat(overlay_width as usize))
                .style(Style::default().bg(Color::Black))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Select Devices (Options)"),
                );
            f.render_widget(bg, overlay_area);

            // Layout inside overlay: two columns
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(Rect::new(
                    x + 1,
                    y + 1,
                    overlay_width - 2,
                    overlay_height - 3,
                ));

            // Helper to draw a list column
            let draw_list = |f: &mut Frame,
                             area: Rect,
                             title: &str,
                             items: &Vec<String>,
                             selected: usize,
                             active: bool,
                             scroll: usize| {
                // Title
                let title_widget = Paragraph::new(title).style(
                    Style::default()
                        .fg(if active { Color::Yellow } else { Color::Cyan })
                        .add_modifier(Modifier::BOLD),
                );
                let title_area = Rect::new(area.x, area.y, area.width, 1);
                f.render_widget(title_widget, title_area);

                // Items area
                let list_area = Rect::new(
                    area.x,
                    area.y + 1,
                    area.width,
                    area.height.saturating_sub(1),
                );
                const VISIBLE_ITEMS: usize = 15;
                let start_index = scroll;
                for (i, name) in items
                    .iter()
                    .enumerate()
                    .skip(start_index)
                    .take(VISIBLE_ITEMS)
                {
                    let idx = i;
                    let styled = if active && idx == selected {
                        Paragraph::new(format!("> {}", name))
                            .style(Style::default().bg(Color::Blue).fg(Color::White))
                    } else {
                        Paragraph::new(format!("  {}", name))
                            .style(Style::default().fg(Color::White))
                    };
                    // Render each line
                    let row_area = Rect::new(
                        list_area.x,
                        list_area.y + (i - start_index) as u16,
                        list_area.width,
                        1,
                    );
                    f.render_widget(styled, row_area);
                }
            };

            // Draw input list (left)
            draw_list(
                f,
                columns[0],
                "Input Devices",
                inputs,
                *selected_index,
                *column == 0,
                *scroll_offset,
            );
            // Draw output list (right)
            draw_list(
                f,
                columns[1],
                "Output Devices",
                outputs,
                *selected_index,
                *column == 1,
                *scroll_offset,
            );

            // Instructions
            let instructions = "↑↓: Navigate  Tab: Switch  Enter: Select  Esc: Close";
            let instructions_area = Rect::new(x + 1, y + overlay_height - 2, overlay_width - 2, 1);
            let instructions_widget = Paragraph::new(instructions)
                .style(Style::default().fg(Color::Yellow))
                .alignment(ratatui::layout::Alignment::Center);
            f.render_widget(instructions_widget, instructions_area);
        }
    }

    fn draw_layers_static(
        f: &mut Frame,
        area: Rect,
        layers: &Arc<Vec<Arc<Mutex<AudioLayer>>>>,
        selected_layer: usize,
        countdown: Option<(usize, u32)>,
    ) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(area);

        Self::draw_layer_list_static(f, chunks[0], layers, selected_layer, countdown);
        Self::draw_layer_details_static(f, chunks[1], layers, selected_layer);
    }

    fn draw_layer_list_static(
        f: &mut Frame,
        area: Rect,
        layers: &Arc<Vec<Arc<Mutex<AudioLayer>>>>,
        selected_layer: usize,
        countdown: Option<(usize, u32)>,
    ) {
        use ratatui::text::Span;
        use ratatui::widgets::{Cell, Row, Table};

        // Create table rows
        let rows: Vec<Row> = layers
            .iter()
            .enumerate()
            .map(|(i, layer_arc)| {
                let layer = layer_arc.lock().unwrap();

                // Determine status and color; inject count-in countdown if relevant
                let mut status_text = if layer.is_recording {
                    "[REC]".to_string()
                } else if layer.is_playing {
                    "[PLAY]".to_string()
                } else if !layer.is_empty() {
                    "[PAUSE]".to_string()
                } else {
                    "[EMPTY]".to_string()
                };

                let mut status_color = if status_text == "[REC]" {
                    Color::Red
                } else if status_text == "[PLAY]" {
                    Color::Green
                } else if status_text == "[PAUSE]" {
                    Color::Yellow
                } else {
                    Color::Gray
                };
                if let Some((layer_id, beats_left)) = countdown
                    && layer_id == i
                {
                    // Replace status text with countdown 3-2-1 (show 1..n)
                    status_text = format!("[{}]", beats_left);
                    status_color = Color::Cyan;
                }

                // Create status cell with color
                let status_cell = Cell::from(Span::styled(
                    status_text.to_string(),
                    Style::default()
                        .fg(status_color)
                        .add_modifier(Modifier::BOLD),
                ));

                // Volume cell
                let volume_text = format!("{:.0}%", layer.volume * 100.0);
                let volume_cell = Cell::from(volume_text);

                // Samples cell
                let samples_text = if layer.is_empty() {
                    "0".to_string()
                } else {
                    layer.get_buffer_length().to_string()
                };
                let samples_cell = Cell::from(samples_text);

                // Mute/Solo cell
                let mute_solo_text = if layer.is_muted {
                    "MUTED".to_string()
                } else if layer.is_solo {
                    "SOLO".to_string()
                } else {
                    "".to_string()
                };
                let mute_solo_cell = Cell::from(mute_solo_text);

                // Determine row style based on selection
                let row_style = if i == selected_layer {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };

                Row::new(vec![
                    Cell::from(format!("Layer {}", i + 1)),
                    status_cell,
                    volume_cell,
                    samples_cell,
                    mute_solo_cell,
                ])
                .style(row_style)
            })
            .collect();

        // Create table with headers
        let table = Table::new(
            rows,
            &[
                Constraint::Length(8),  // Layer
                Constraint::Length(8),  // Status
                Constraint::Length(8),  // Volume
                Constraint::Length(10), // Samples
                Constraint::Length(10), // Mute/Solo
            ],
        )
        .header(Row::new(vec![
            Cell::from("Layer").style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Cell::from("Status").style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Cell::from("Volume").style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Cell::from("Samples").style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Cell::from("Mute/Solo").style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]))
        .block(Block::default().borders(Borders::ALL).title("Audio Layers"));

        f.render_widget(table, area);
    }

    fn draw_layer_details_static(
        f: &mut Frame,
        area: Rect,
        layers: &Arc<Vec<Arc<Mutex<AudioLayer>>>>,
        selected_layer: usize,
    ) {
        let layer = layers[selected_layer].lock().unwrap();

        let volume_gauge = Gauge::default()
            .block(Block::default().borders(Borders::ALL).title("Volume"))
            .gauge_style(Style::default().fg(Color::Green))
            .ratio(layer.volume as f64);

        let details = Paragraph::new(format!(
            "Layer {} Details:\n\
            Status: {}\n\
            Buffer: {} samples\n\
            Loop: {} - {}\n\
            Position: {}\n\
            Muted: {}\n\
            Solo: {}",
            selected_layer + 1,
            if layer.is_recording {
                "Recording"
            } else if layer.is_playing {
                "Playing"
            } else {
                "Stopped"
            },
            layer.get_buffer_length(),
            layer.loop_start,
            layer.loop_end,
            layer.playback_position,
            layer.is_muted,
            layer.is_solo
        ))
        .block(Block::default().borders(Borders::ALL).title("Details"));

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(area);

        f.render_widget(volume_gauge, chunks[0]);
        f.render_widget(details, chunks[1]);
    }

    fn draw_footer_static(
        f: &mut Frame,
        area: Rect,
        bpm: f64,
        beat: u32,
        measure: usize,
        sync_on: bool,
        metro_on: bool,
    ) {
        use ratatui::text::{Line, Span};

        // Define colors for syntax highlighting
        let key_color = Color::Yellow;
        let desc_color = Color::White;
        let sep_color = Color::DarkGray;

        // Helper function to create a key-description pair
        let key_desc = |key: &str, desc: &str| -> Vec<Span> {
            vec![
                Span::styled(
                    key.to_string(),
                    Style::default().fg(key_color).add_modifier(Modifier::BOLD),
                ),
                Span::styled("=".to_string(), Style::default().fg(sep_color)),
                Span::styled(desc.to_string(), Style::default().fg(desc_color)),
            ]
        };

        // Helper function to create separator
        let separator = || Span::styled(" | ".to_string(), Style::default().fg(sep_color));

        // Build line 1 - navigation, record/stop basics
        let mut line1_spans = Vec::new();
        line1_spans.extend(key_desc("↑↓", "Select"));
        line1_spans.push(separator());
        line1_spans.extend(key_desc("1-9/0", "Record/Stop"));
        line1_spans.push(separator());
        line1_spans.extend(key_desc("R", "Record"));
        line1_spans.push(separator());
        line1_spans.extend(key_desc("S", "Stop"));
        line1_spans.push(separator());
        line1_spans.extend(key_desc("Space", "Stop All"));

        // Build line 2 - playback and file ops
        let mut line2_spans = Vec::new();
        line2_spans.extend(key_desc("P", "Play"));
        line2_spans.push(separator());
        line2_spans.extend(key_desc("A", "Play All"));
        line2_spans.push(separator());
        line2_spans.extend(key_desc("I", "Import"));
        line2_spans.push(separator());
        line2_spans.extend(key_desc("E", "Export"));
        line2_spans.push(separator());
        line2_spans.extend(key_desc("O", "Options"));
        line2_spans.push(separator());
        line2_spans.extend(key_desc("Q", "Quit"));

        // Build line 3 - mixing and tempo/sync
        let mut line3_spans = Vec::new();
        line3_spans.extend(key_desc("+/-", "Volume"));
        line3_spans.push(separator());
        line3_spans.extend(key_desc("M", "Mute"));
        line3_spans.push(separator());
        line3_spans.extend(key_desc("L", "Solo"));
        line3_spans.push(separator());
        line3_spans.extend(key_desc("B", "Tap"));
        line3_spans.push(separator());
        line3_spans.extend(key_desc("T", "BPM"));
        line3_spans.push(separator());
        line3_spans.extend(key_desc("G", if sync_on { "Sync On" } else { "Sync Off" }));
        line3_spans.push(separator());
        line3_spans.extend(key_desc("H", "Count-in"));
        line3_spans.push(separator());
        line3_spans.extend(key_desc(
            "N",
            if metro_on {
                "Metronome On"
            } else {
                "Metronome Off"
            },
        ));

        let status_line = Line::from(vec![
            Span::styled(
                format!(" BPM: {:.1} ", bpm),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" Beat: {}/{} ", beat, measure + 1),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);

        let help_text = vec![
            Line::from(line1_spans),
            Line::from(line2_spans),
            Line::from(line3_spans),
            status_line,
        ];

        let footer = Paragraph::new(help_text)
            .block(Block::default().borders(Borders::ALL).title("Controls"));

        f.render_widget(footer, area);
    }
}

impl Drop for TerminalUI {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        );
    }
}
