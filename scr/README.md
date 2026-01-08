# Autosample - Professional CLI Autosampler

A cross-platform command-line autosampler for capturing hardware and 
software instruments via MIDI triggers and audio recording.

## Features

- 🎹 **MIDI Control**: Send precise MIDI note triggers with configurable 
velocity
- 🎤 **Audio Capture**: Record from any audio input device with low 
latency
- 💾 **Multiple Formats**: Export to WAV, MP3, or both
- 🎚️ **Audio Processing**: Trimming, normalization, and fade-in/out
- 🔄 **Round-Robin Support**: Multiple samples per note/velocity
- 📁 **Smart Organization**: Structured output with metadata
- ⏸️ **Resume Support**: Skip existing files to continue interrupted 
sessions
- 🖥️ **Cross-Platform**: Works on macOS, Windows, and Linux

## Installation

### Prerequisites

- Rust toolchain (1.70+): https://rustup.rs/
- For MP3 export: ffmpeg

#### Installing ffmpeg

**macOS:**
```bash
brew install ffmpeg
