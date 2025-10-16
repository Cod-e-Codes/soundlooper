# Soundlooper

A real-time multi-layer audio looper built in Rust with a terminal UI. Record, mix, and loop up to 16 audio tracks with per-layer controls, WAV import/export, and low-latency playback. Cross-platform support for Windows, macOS, and Linux.

![Soundlooper Screenshot](assets/screenshot.png)

## Features

- **Multi-layer Recording**: Record up to 16 simultaneous audio layers
- **Real-time Playback**: Low-latency audio processing with looping
- **Per-layer Controls**: Individual volume, mute, and solo controls
- **5-Level Undo/Redo**: Navigate through up to 5 previous states per layer
- **Real-time Peak Meters**: Color-coded dB level monitoring with peak hold
- **SIMD-Accelerated Mixing**: Fast multi-layer mixing performance
- **Lock-Free Audio Buffers**: Eliminates mutex contention for lower latency
- **WAV Import/Export**: Import WAV files into layers and export compositions
- **Terminal UI**: Clean, responsive TUI with device information display
- **Options Panel**: Choose input/output audio devices directly from the TUI
- **Beat Sync & Count‑In Mode**: Start/stop/record aligned to measures; optional 3‑2‑1 count‑in
- **Tap Tempo & BPM**: Tap to detect BPM or set BPM numerically
- **Metronome**: Click at each beat, synced to BPM
- **Cross-platform**: Works on Windows, macOS, and Linux
- **Debug Mode**: Optional debug logging with `--debug` flag (logs written to `debug.log`)

## Quick Start

```bash
# Clone and build
git clone https://github.com/Cod-e-Codes/soundlooper.git
cd soundlooper
cargo build --release

# Run the application
cargo run --release

# Show help
cargo run --release -- --help

# Run with debug logging
cargo run --release -- --debug
```

## Controls

| Key | Action |
|-----|--------|
| `↑↓` | Select layer |
| `1-9`, `0` | Record/Stop/Play layer 1-10 (beat‑sync aware) |
| `R` | Record on selected layer |
| `S` | Stop selected layer |
| `Space` | Stop all layers |
| `P` | Play selected layer |
| `A` | Play all layers |
| `+/-` | Adjust volume |
| `M` | Mute/unmute selected layer |
| `L` | Solo/unsolo selected layer |
| `C` | Clear selected layer |
| `X` | Clear all layers |
| `I` | Import WAV file to selected layer |
| `E` | Export composition as WAV |
| `Z` | Undo on selected layer |
| `Y` | Redo on selected layer |
| `O` | Options (select input/output devices) |
| `B` | Tap tempo |
| `T` | Set BPM |
| `G` | Toggle beat sync |
| `H` | Toggle count‑in mode |
| `N` | Toggle metronome |
| `Q` | Quit |

## Architecture

The application is built with a modular architecture:

- **Audio Engine** (`src/audio/`): Core audio processing, mixing, and layer management
- **Terminal UI** (`src/ui/`): User interface built with ratatui
- **Cross-platform Audio**: Uses CPAL for audio I/O across platforms

### Key Components

- `AudioLayer`: Individual audio layer with recording, playback, and control capabilities
- `LooperEngine`: Manages all layers and handles real-time mixing
- `TempoEngine`: BPM tracking, beat synchronization, and count-in functionality
- `AudioStream`: CPAL-based audio input/output handling with resampling
- `LockFreeAudioBuffer`: High-performance, non-blocking audio data transfer
- `SimdMixer`: SIMD-accelerated multi-layer audio mixing
- `PeakMeter`: Real-time audio level monitoring with color-coded display
- `UndoHistory`: 5-level circular buffer for layer state management
- `TerminalUI`: Terminal-based user interface

## Building

```bash
# Debug build
cargo build

# Release build
cargo build --release

## Examples

See the [examples/README.md](examples/README.md) for detailed examples and usage instructions.

```bash
cargo run --example basic_api           # Basic API usage
cargo run --example record_playback     # Interactive recording workflow
cargo run --example feature_demo        # Automated feature demonstration
cargo run --example multi_layer_mix     # Advanced mixing workflow
```

## Requirements

- Rust 1.89+
- Audio input/output device
- Terminal with UTF-8 support

## Dependencies

- `cpal` - Cross-platform audio I/O
- `hound` - WAV file reading/writing
- `rubato` - Sample rate conversion
- `ratatui` - Terminal UI framework
- `crossbeam` - Thread-safe communication
- `anyhow` - Error handling
- `crossterm` - Terminal control
- `ringbuf` - Lock-free ring buffer
- `serde` - Serialization framework
- `toml` - TOML configuration parsing

## Contributing

Contributions are welcome! Please feel free to submit issues and pull requests.

## License

This project is licensed under the MIT License.