use crate::dsp;
use anyhow::{Context, Result};
use hound::{SampleFormat, WavSpec, WavWriter};
use std::fs;
use std::path::Path;
use std::process::Command;
use tracing::{info, warn};

pub fn export_wav(
    samples: &[f32],
    channels: u16,
    sample_rate: u32,
    bit_depth: u16,
    path: &Path,
) -> Result<()> {
    let spec = WavSpec {
        channels,
        sample_rate,
        bits_per_sample: bit_depth,
        sample_format: if bit_depth == 32 {
            SampleFormat::Float
        } else {
            SampleFormat::Int
        },
    };

    let mut writer = WavWriter::create(path, spec)
        .with_context(|| format!("Failed to create WAV file: {}", 
path.display()))?;

    match bit_depth {
        16 => {
            for &sample in samples {
                let s = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as 
i16;
                writer.write_sample(s)?;
            }
        }
        24 => {
            for &sample in samples {
                let s = (sample.clamp(-1.0, 1.0) * 8388607.0) as i32; // 
2^23 - 1
                writer.write_sample(s)?;
            }
        }
        32 => {
            for &sample in samples {
                writer.write_sample(sample)?;
            }
        }
        _ => anyhow::bail!("Unsupported bit depth: {}", bit_depth),
    }

    writer.finalize()?;
    Ok(())
}

pub fn export_mp3(wav_path: &Path, mp3_path: &Path, bitrate: u32) -> 
Result<()> {
    // Check if ffmpeg is available
    let ffmpeg_check = Command::new("ffmpeg").arg("-version").output();

    if ffmpeg_check.is_err() {
        anyhow::bail!(
            "ffmpeg not found. Please install ffmpeg or use WAV format 
only.\n\
             On macOS: brew install ffmpeg\n\
             On Ubuntu: sudo apt install ffmpeg\n\
             On Windows: Download from https://ffmpeg.org/"
        );
    }

    info!(
        "Converting {} to MP3 at {} kbps",
        wav_path.display(),
        bitrate
    );

    let output = Command::new("ffmpeg")
        .arg("-i")
        .arg(wav_path)
        .arg("-codec:a")
        .arg("libmp3lame")
        .arg("-b:a")
        .arg(format!("{}k", bitrate))
        .arg("-y") // Overwrite output
        .arg(mp3_path)
        .arg("-loglevel")
        .arg("error")
        .output()
        .context("Failed to execute ffmpeg")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("ffmpeg conversion failed: {}", stderr);
    }

    Ok(())
}

pub fn process_and_export(
    mut samples: Vec<f32>,
    channels: u16,
    sample_rate: u32,
    bit_depth: u16,
    format: &str,
    output_path: &Path,
    trim_threshold_db: Option<f32>,
    min_tail_ms: u64,
    normalize_mode: &str,
) -> Result<f32> {
    let channels_usize = channels as usize;

    // Trimming
    if let Some(threshold) = trim_threshold_db {
        let min_tail_samples =
            (sample_rate as u64 * min_tail_ms / 1000) as usize * 
channels_usize;
        let (start, end) = dsp::trim_silence(&samples, channels_usize, 
threshold, min_tail_samples);
        samples = samples[start..end].to_vec();
        info!("Trimmed to {} samples", samples.len());
    }

    // Apply fade to avoid clicks
    let fade_samples = (sample_rate as f32 * 0.005) as usize; // 5ms fade
    dsp::apply_fade(&mut samples, channels_usize, fade_samples);

    // Calculate peak before normalization
    let peak_before = dsp::calculate_peak_db(&samples);

    // Normalization
    dsp::normalize(&mut samples, normalize_mode);

    let peak_after = dsp::calculate_peak_db(&samples);
    info!("Peak: {:.2} dB -> {:.2} dB", peak_before, peak_after);

    // Ensure output directory exists
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Export based on format
    match format {
        "wav" => {
            export_wav(&samples, channels, sample_rate, bit_depth, 
output_path)?;
        }
        "mp3" => {
            // Export to temp WAV first
            let temp_wav = output_path.with_extension("tmp.wav");
            export_wav(&samples, channels, sample_rate, bit_depth, 
&temp_wav)?;
            
            // Convert to MP3
            let mp3_path = output_path.with_extension("mp3");
            match export_mp3(&temp_wav, &mp3_path, 320) {
                Ok(_) => {
                    fs::remove_file(&temp_wav)?;
                }
                Err(e) => {
                    fs::remove_file(&temp_wav)?;
                    return Err(e);
                }
            }
        }
        "both" => {
            // Export WAV
            let wav_path = output_path.with_extension("wav");
            export_wav(&samples, channels, sample_rate, bit_depth, 
&wav_path)?;
            
            // Export MP3
            let mp3_path = output_path.with_extension("mp3");
            match export_mp3(&wav_path, &mp3_path, 320) {
                Ok(_) => {}
                Err(e) => {
                    warn!("MP3 export failed: {}. WAV file saved.", e);
                }
            }
        }
        _ => anyhow::bail!("Unsupported format: {}", format),
    }

    Ok(peak_after)
}
