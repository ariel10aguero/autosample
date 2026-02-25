# Autosample

Autosample is a cross-platform MIDI-triggered sampler with:
- a CLI (`autosample`) for scripted runs, and
- a desktop GUI (`autosample-gui`) for interactive setup.

It sends MIDI notes, records audio input, applies optional processing, and exports sample files plus session metadata.

## What You Need Before Running

### 1) Toolchain and dependencies
- Rust toolchain (1.70+): https://rustup.rs/
- `ffmpeg` only if you export `mp3` or `both`

Install `ffmpeg`:

macOS
```bash
brew install ffmpeg
```

Ubuntu / Debian
```bash
sudo apt update && sudo apt install ffmpeg
```

Windows (winget)
```bash
winget install Gyan.FFmpeg
```

### 2) Hardware / routing setup
- A MIDI output destination (hardware instrument or virtual MIDI port)
- An audio input source that captures the instrument output
- Proper monitoring/routing to avoid feedback loops

### 3) OS permissions
- On macOS, grant microphone/input permission to Terminal (CLI) and your app host (GUI), or device enumeration/capture may fail.
- If devices do not appear, restart the app after granting permission.

## Build the Project

From repo root:

```bash
cargo build
```

## Discover Devices First (Recommended)

List MIDI outputs:

```bash
cargo run -p autosample-cli -- list-midi
```

List audio inputs:

```bash
cargo run -p autosample-cli -- list-audio
```

You can pass either device index or partial device name to run commands.

## Run the CLI

Basic required arguments:

```bash
cargo run -p autosample-cli -- run \
  --midi-out "IAC Driver Bus 1" \
  --audio-in "Built-in Microphone" \
  --notes "C4..C5" \
  --vel "127,100,64" \
  --output "./output" \
  --prefix "my_instrument"
```

Stop a run with `Ctrl+C`.

### Useful optional flags
- `--hold-ms 1000` note length
- `--tail-ms 2000` release capture length
- `--preroll-ms 100` pre-capture silence window
- `--sr 48000` requested sample rate
- `--channels 1|2`
- `--format wav|mp3|both`
- `--trim-threshold-db -50`
- `--normalize peak` (or other supported mode)
- `--round-robin N`
- `--resume` skip files that already exist

## Run the GUI

```bash
cargo run -p autosample-gui
```

In the GUI:
- choose MIDI and audio devices,
- fill run parameters,
- use **Start** to begin,
- save/load presets as JSON.

## Input Format Rules

### Notes (`--notes`)
- Single: `C4`
- Range: `C2..C6`
- List: `C4,E4,G4`

Accepted accidental spellings include sharps/flats equivalents such as `C#4`, `Db4`, `Cs4`.

### Velocities (`--vel`)
- Single: `100`
- Range: `64..127`
- List: `127,100,64`
- Range with step: `127..1:8`

## Output Layout

Samples are written to:

```text
<output>/<prefix>/samples/
```

Session metadata is written to:

```text
<output>/<prefix>/session.json
```

File naming pattern:

```text
<prefix>_<NoteName>_<MidiNote>_vel<Velocity>_rr<RoundRobin>.<ext>
```

Example:

```text
piano_C4_060_vel127_rr01.wav
```

## Important Runtime Behavior

- If `--format mp3` is requested and `ffmpeg` is missing, run fails.
- If `--format both` and `ffmpeg` is missing, WAV export continues, MP3 is skipped.
- Audio capture tries to use your requested sample rate/channels when supported; otherwise device default config is used.
- `--resume` checks for existing WAV target files to decide whether to skip.

## Troubleshooting

- **No MIDI/audio devices listed**
  - Check OS permissions and reconnect/restart devices.
  - Verify other apps are not exclusively locking the interface.
- **"MIDI port not found" / "Audio device not found"**
  - Re-run list commands and use exact/partial current names or indexes.
- **MP3 conversion errors**
  - Confirm `ffmpeg -version` works in your shell.
- **Silent or clipped output**
  - Validate routing and gain staging.
  - Adjust `--trim-threshold-db`, `--hold-ms`, and `--tail-ms`.
  - Try `--normalize peak`.

## Development Notes

Workspace crates:
- `autosample-core`: engine, parsing, audio/MIDI, export
- `autosample-cli`: command-line app (`autosample`)
- `autosample-gui`: desktop app (`autosample-gui`)
