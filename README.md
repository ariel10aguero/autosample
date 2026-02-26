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
```

## Output Organization

Autosample now supports configurable sample directory organization under `output/prefix/samples`.

- `Flat`  
  `output/prefix/samples/<file>.wav`
- `ByNote`  
  `output/prefix/samples/<NoteName_Midi>/<file>.wav`
- `ByNoteVelocity`  
  `output/prefix/samples/<NoteName_Midi>/velXXX/<file>.wav`

The filename format remains unchanged:

`<prefix>_<NoteName>_<MidiNote>_vel<Velocity>_rr<RoundRobin>.<ext>`

Examples:
- `sample_C4_060_vel100_rr01.wav`
- `sample_Cs4_061_vel127_rr02.mp3`

## CLI

Use `--output-organization` to select the directory strategy:

```bash
autosample run \
  --midi-out "Your MIDI Device" \
  --audio-in "Your Audio Device" \
  --notes C4..C5 \
  --vel 127,100,64 \
  --output ./output \
  --prefix sample \
  --output-organization by-note-velocity
```

Accepted values:
- `flat`
- `by-note`
- `by-note-velocity`

## Resume Behavior

Resume mode (`--resume` / GUI Resume) skips jobs when the target WAV file for that job already exists at the path implied by the selected output organization mode.
