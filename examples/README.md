# Soundlooper Examples

This directory contains comprehensive examples demonstrating all features of the soundlooper audio engine.

## Examples Overview

### 1. `basic_api.rs` - Basic API Usage
**Simple API usage** - Minimal example showing basic commands:
- Create looper engine
- Send commands directly
- No audio streams (for testing logic)

**Run with:**
```bash
cargo run --example basic_api
```

### 2. `record_playback.rs` - Basic Workflow
**Perfect for beginners** - Shows the core recording workflow:
- Initialize audio system
- Record from microphone for 3 seconds
- Play back the recorded loop
- Export to WAV file

**Run with:**
```bash
cargo run --example record_playback
```

### 3. `feature_demo.rs` - All Features
**Complete feature demonstration** - Shows every capability:
- WAV import/export
- Layer management (volume, mute, solo)
- Tempo control and tap tempo
- Beat sync and metronome
- Count-in mode
- Event monitoring

**Run with:**
```bash
cargo run --example feature_demo
```

### 4. `multi_layer_mix.rs` - Music Production
**Advanced mixing workflow** - Create layered compositions:
- Record multiple layers sequentially
- Adjust individual layer volumes
- Use solo/mute for mixing
- Export final composition

**Run with:**
```bash
cargo run --example multi_layer_mix
```

## Requirements

### Audio Setup
- **Microphone**: Required for recording examples
- **Speakers/Headphones**: Required for playback
- **Audio Drivers**: CPAL will use your system's default audio devices

### Sample Files
- Place sample WAV files in `assets/` directory
- `metronome.wav` is used by feature_demo

## Features Demonstrated

### Core Audio
- Real-time recording and playback
- Multi-layer audio mixing
- WAV file import/export
- Volume control and gain staging

### Advanced Features
- Beat synchronization
- Tempo control (BPM, tap tempo)
- Metronome with click sounds
- Count-in mode for precise timing
- Solo/mute functionality

### Technical Features
- Cross-platform audio (CPAL)
- Lock-free command processing
- Event-driven architecture
- Real-time audio processing

## Troubleshooting

### Common Issues
1. **No audio devices**: Ensure microphone and speakers are connected
2. **Permission denied**: Grant microphone access to your terminal/IDE
3. **Sample rate mismatch**: System will resample automatically
4. **File not found**: Check `assets/` directory for sample files

### Debug Mode
Enable debug logging by modifying examples to pass `true` for debug mode:
```rust
let audio_stream = AudioStream::new(config.clone(), true)?; // Enable debug
```
This creates a `debug.log` file with detailed audio processing information.

## Next Steps

After running these examples:
1. **Modify the examples** to test your own audio workflows
2. **Create custom sample files** for testing different scenarios
3. **Integrate the engine** into your own Rust applications
4. **Explore the API** in `src/audio/mod.rs` for more advanced usage

## Performance Notes

- **Low latency**: Optimized for real-time performance
- **Lock-free**: Uses `try_lock()` to avoid audio thread blocking
- **Memory efficient**: Samples are stored as `Vec<f32>`
- **Cross-platform**: Works on Windows, macOS, and Linux
