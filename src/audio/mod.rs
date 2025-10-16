pub mod io;
pub mod layer;
pub mod lockfree_buffer;
pub mod looper;
pub mod peak_meter;
pub mod simd_mixer;
pub mod stream;
pub mod tempo;

pub use io::{export_wav, import_wav};
pub use layer::AudioLayer;
pub use lockfree_buffer::{AudioBufferPair, LockFreeAudioBuffer, SharedLockFreeBuffer};
pub use looper::LooperEngine;
pub use peak_meter::{MeterColor, PeakMeter};
pub use simd_mixer::{ScalarMixer, SimdMixer};
pub use stream::AudioStream;
pub use tempo::TempoEngine;

#[derive(Debug, Clone)]
pub struct AudioConfig {
    pub sample_rate: u32,
    pub buffer_size: usize,
    pub max_layers: usize,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            sample_rate: 44100,
            buffer_size: 512,
            max_layers: 16,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum LayerCommand {
    Record(usize),
    StopRecording(usize),
    StopPlaying(usize),
    Play(usize),
    Mute(usize),
    Solo(usize),
    SetVolume(usize, f32),
    StopAll,
    Clear(usize),
    ClearAll,
    PlayAll,
    ImportWav(usize, String),   // layer_id, file_path
    ExportWav(String),          // file_path
    SwitchInputDevice(String),  // device_name
    SwitchOutputDevice(String), // device_name
    // Tempo / Sync controls
    TapTempo,
    SetBpm(f64),
    ToggleBeatSync(bool),
    ToggleCountInMode(bool),
    StartCountIn { layer_id: usize, measures: u32 },
    SyncPlay(usize),
    SyncStop(usize),
    SyncRecord(usize),
    // Metronome
    ToggleMetronome(bool),
}

#[derive(Debug, Clone, PartialEq)]
pub enum AudioEvent {
    LayerRecording(usize),
    LayerStopped(usize),
    LayerPlaying(usize),
    LayerMuted(usize),
    LayerUnmuted(usize),
    LayerSoloed(usize),
    LayerUnsoloed(usize),
    VolumeChanged(usize, f32),
    AllStopped,
    LayerCleared(usize),
    AllCleared,
    AllPlaying,
    WavImported(usize, String),                     // layer_id, file_path
    WavExported(String),                            // file_path
    Error(String),                                  // error message
    DevicesUpdated(Option<String>, Option<String>), // (input_name, output_name)
    DeviceSwitchRequested,
    DeviceSwitchComplete,
    DeviceSwitchFailed(String),
    // Tempo / Sync updates
    BpmChanged(f64),
    Beat(u32, usize), // (beat, measure)
    CountInStarted {
        layer_id: usize,
        beats: u32,
    },
    CountInFinished {
        layer_id: usize,
    },
    CountInTick {
        layer_id: usize,
        remaining_beats: u32,
    },
    CountInModeToggled(bool),
    // Metronome
    MetronomeToggled(bool),
}
