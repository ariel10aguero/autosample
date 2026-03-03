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
    AudioDeviceInfo, EngineStatus, LogLevel, MidiPortInfo, OutputFormat, OutputOrganization,
    ProgressUpdate, RunConfig, SampleInfo, SampleJob, SessionMetadata,
};

pub use audio::{find_audio_device, get_audio_devices, list_audio_devices};
pub use engine::AutosampleEngine;
pub use midi::{find_midi_port, get_midi_ports, list_midi_ports};
