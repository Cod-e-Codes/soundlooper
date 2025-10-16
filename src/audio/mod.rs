pub mod io;
pub mod layer;
pub mod looper;
pub mod stream;

pub use io::{export_wav, import_wav};
pub use layer::AudioLayer;
pub use looper::LooperEngine;
pub use stream::AudioStream;

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
    ImportWav(usize, String), // layer_id, file_path
    ExportWav(String),        // file_path
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
}
