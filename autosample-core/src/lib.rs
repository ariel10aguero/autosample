//! Autosample Core Library
//! 
//! This crate contains all the core autosampling logic shared between
//! the CLI and GUI interfaces.

pub mod audio;
pub mod dsp;
pub mod engine;
pub mod export;
pub mod midi;
pub mod parse;
pub mod ringbuf;
pub mod types;

// Re-export commonly used types
pub use types::{
    OutputFormat, RunConfig, SampleJob, SessionMetadata, SampleInfo,
    ProgressUpdate, EngineStatus,
};

pub use engine::{AutosampleEngine, EngineEvent};
pub use audio::{list_audio_devices, find_audio_device, AudioDeviceInfo};
pub use midi::{list_midi_ports, find_midi_port, MidiPortInfo};