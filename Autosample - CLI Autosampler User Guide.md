# Autosample - CLI Autosampler User Guide

## Quick Start

### 1) List devices

```bash
autosample list-midi
autosample list-audio
```

### 2) Run a test session

```bash
autosample run \
  --midi-out 0 \
  --audio-in 0 \
  --notes "C4" \
  --vel "127" \
  --output "./test" \
  --prefix "test"
```

## Notes and Velocities Syntax

### Notes (`--notes`)

- Single note: `"C4"`
- Range: `"C2..C6"` (ascending only)
- List: `"C4,E4,G4,C5"`
- Accidentals in input must use `#` or `b` notation, for example: `"C#4"`, `"Db4"`

### Velocities (`--vel`)

- Single value: `"127"`
- List: `"127,100,64,32"`
- Ascending range: `"1..127"`
- Descending range requires a step: `"127..1:8"`

## Options Reference

### Required options

| Option | Description | Example |
|---|---|---|
| `--midi-out` | MIDI output device name or index | `0`, `"Scarlett 2i4 USB"` |
| `--audio-in` | Audio input device name or index | `0`, `"Scarlett 2i4 USB"` |
| `--notes` | Notes to sample | `"C2..C6"`, `"C4,E4,G4"` |
| `--vel` | Velocities to sample | `"127..1:8"`, `"127,100,64"` |
| `--output` | Base output directory | `"./output"` |
| `--prefix` | Instrument/prefix name | `"Piano"` |

### Optional options

| Option | Description | Default | Example |
|---|---|---|---|
| `--hold-ms` | Note-on hold duration (ms) | `1000` | `1500` |
| `--tail-ms` | Recording tail after note-off (ms) | `2000` | `3000` |
| `--preroll-ms` | Recording preroll before note-on (ms) | `100` | `150` |
| `--sr` | Sample rate | `48000` | `44100`, `96000` |
| `--channels` | Output channels | `2` | `1`, `2` |
| `--format` | Output format | `wav` | `wav`, `mp3`, `both` |
| `--trim-threshold-db` | Trim silence threshold (dBFS) | none | `-50` |
| `--normalize` | Normalize mode | none | `peak`, `-1dB` |
| `--round-robin` | Takes per note/velocity | `1` | `3` |
| `--resume` | Skip already existing target WAV files | `false` | flag |
| `--output-organization` | Directory organization | `flat` | `flat`, `by-note` |

## Timing Model

Total recording time per sample:

```text
total_ms = preroll_ms + hold_ms + tail_ms
```

Example:

- `--preroll-ms 100`
- `--hold-ms 1000`
- `--tail-ms 2000`
- Total = `3100 ms` per sample

## Common Use Cases

### 1) Quick test

```bash
autosample run \
  --midi-out 0 --audio-in 0 \
  --notes "C4" --vel "127" \
  --output ./test --prefix test
```

### 2) Simple instrument

```bash
autosample run \
  --midi-out 0 --audio-in 0 \
  --notes "C3..C6" \
  --vel "127,100,64,32" \
  --hold-ms 1000 --tail-ms 2000 --preroll-ms 100 \
  --sr 48000 --channels 2 --format wav \
  --output ./output --prefix Piano
```

### 3) Full multi-sample with round-robin

```bash
autosample run \
  --midi-out 0 --audio-in 0 \
  --notes "A0..C8" \
  --vel "127..1:8" \
  --hold-ms 1500 --tail-ms 3000 --preroll-ms 150 \
  --sr 48000 --channels 2 --format both \
  --trim-threshold-db -50 \
  --normalize peak \
  --round-robin 3 \
  --resume \
  --output ./output --prefix GrandPiano
```

## Output Structure

Autosample writes into:

```text
<output>/<prefix>/
```

With `--output-organization flat`:

```text
output/Piano/
  Piano_C4_vel127.wav
  Piano_Cs4_vel127.wav
  session.json
```

With `--output-organization by-note`:

```text
output/Piano/
  C4_060/
    Piano_C4_vel127.wav
  Cs4_061/
    Piano_Cs4_vel127.wav
  session.json
```

Notes:

- `rrNN` is included in filenames only when `--round-robin` is greater than 1.
- Filenames use `Cs`, `Ds`, etc. for sharp note names.
- `session.json` is written at `<output>/<prefix>/session.json`.

## Troubleshooting

| Problem | Suggested fix |
|---|---|
| MIDI device not found | Run `autosample list-midi` and use exact name or index |
| Audio device not found | Run `autosample list-audio` and use exact name or index |
| Start of sample is cut | Increase `--preroll-ms` (for example `150`) |
| Release is cut off | Increase `--tail-ms` (for example `3000`) |
| Files too quiet | Use `--normalize peak` |
| Files clip/too loud | Use `--normalize -1dB` |
| Need to continue interrupted run | Re-run same command with `--resume` |

