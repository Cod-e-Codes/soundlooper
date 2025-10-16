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
            std::thread::sleep(Duration::from_millis(10));
        }

        Ok(())
    }

    fn process_events(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if event::poll(Duration::from_millis(1))?
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
            KeyCode::Char('1') => {
                self.toggle_layer_record(0);
            }
            KeyCode::Char('2') => {
                self.toggle_layer_record(1);
            }
            KeyCode::Char('3') => {
                self.toggle_layer_record(2);
            }
            KeyCode::Char('4') => {
                self.toggle_layer_record(3);
            }
            KeyCode::Char('5') => {
                self.toggle_layer_record(4);
            }
            KeyCode::Char('6') => {
                self.toggle_layer_record(5);
            }
            KeyCode::Char('7') => {
                self.toggle_layer_record(6);
            }
            KeyCode::Char('8') => {
                self.toggle_layer_record(7);
            }
            KeyCode::Char('9') => {
                self.toggle_layer_record(8);
            }
            KeyCode::Char('0') => {
                self.toggle_layer_record(9);
            }
            KeyCode::Char('r') => {
                // Record on selected layer
                self.toggle_layer_record(self.selected_layer);
            }
            KeyCode::Char('s') => {
                // Stop selected layer only
                self.stop_selected_layer();
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
                self.start_playback(self.selected_layer);
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
                }
                self.exit_input_mode();
            }
            KeyCode::Esc => {
                // Cancel input
                self.show_cancelled();
                self.exit_input_mode();
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

        // Check if parent directory exists and is writable
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                return Err("Directory does not exist".to_string());
            }
            if !parent.is_dir() {
                return Err("Parent path is not a directory".to_string());
            }
        } else {
            return Err("Invalid file path".to_string());
        }

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
            let display_input = if self.input_buffer.is_empty() {
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

    fn draw(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let selected_layer = self.selected_layer;
        let layers = Arc::clone(&self.layers);

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
                    Constraint::Length(4), // Footer (2 lines of content + borders)
                ])
                .split(f.area());

            Self::draw_header_static(
                f,
                chunks[0],
                &input_device_name,
                &output_device_name,
                &header_status,
            );
            Self::draw_layers_static(f, chunks[1], &layers, selected_layer);
            Self::draw_footer_static(f, chunks[2]);

            // Draw file picker overlay if active
            if file_picker_overlay {
                Self::draw_file_picker_overlay_static(f, f.area(), &input_mode);
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

    fn draw_layers_static(
        f: &mut Frame,
        area: Rect,
        layers: &Arc<Vec<Arc<Mutex<AudioLayer>>>>,
        selected_layer: usize,
    ) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(area);

        Self::draw_layer_list_static(f, chunks[0], layers, selected_layer);
        Self::draw_layer_details_static(f, chunks[1], layers, selected_layer);
    }

    fn draw_layer_list_static(
        f: &mut Frame,
        area: Rect,
        layers: &Arc<Vec<Arc<Mutex<AudioLayer>>>>,
        selected_layer: usize,
    ) {
        use ratatui::text::Span;
        use ratatui::widgets::{Cell, Row, Table};

        // Create table rows
        let rows: Vec<Row> = layers
            .iter()
            .enumerate()
            .map(|(i, layer_arc)| {
                let layer = layer_arc.lock().unwrap();

                // Determine status and color
                let (status_text, status_color) = if layer.is_recording {
                    ("[REC]", Color::Red)
                } else if layer.is_playing {
                    ("[PLAY]", Color::Green)
                } else if !layer.is_empty() {
                    ("[PAUSE]", Color::Yellow)
                } else {
                    ("[EMPTY]", Color::Gray)
                };

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

    fn draw_footer_static(f: &mut Frame, area: Rect) {
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

        // Build line 1 with syntax highlighting - organized logically
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
        line1_spans.push(separator());
        line1_spans.extend(key_desc("P", "Play"));
        line1_spans.push(separator());
        line1_spans.extend(key_desc("A", "Play All"));

        // Build line 2 with syntax highlighting - organized logically
        let mut line2_spans = Vec::new();
        line2_spans.extend(key_desc("+/-", "Volume"));
        line2_spans.push(separator());
        line2_spans.extend(key_desc("M", "Mute"));
        line2_spans.push(separator());
        line2_spans.extend(key_desc("L", "Solo"));
        line2_spans.push(separator());
        line2_spans.extend(key_desc("C", "Clear"));
        line2_spans.push(separator());
        line2_spans.extend(key_desc("X", "Clear All"));
        line2_spans.push(separator());
        line2_spans.extend(key_desc("I", "Import"));
        line2_spans.push(separator());
        line2_spans.extend(key_desc("E", "Export"));
        line2_spans.push(separator());
        line2_spans.extend(key_desc("Q", "Quit"));

        let help_text = vec![Line::from(line1_spans), Line::from(line2_spans)];

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
