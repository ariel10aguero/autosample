use anyhow::Result;
use hound::{WavSpec, WavWriter};
use std::path::Path;
use tracing::info;

pub fn write_wav(path: &Path, samples: &[f32], spec: WavSpec) -> Result<()> {
    let mut writer = WavWriter::create(path, spec)?;

    match spec.bits_per_sample {
        16 => {
            for &s in samples {
                let sample = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
                writer.write_sample(sample)?;
            }
        }
        24 => {
            for &s in samples {
                let sample = (s.clamp(-1.0, 1.0) * 8388607.0) as i32;
                writer.write_sample(sample)?;
            }
        }
        32 => {
            for &s in samples {
                writer.write_sample(s)?;
            }
        }
        _ => anyhow::bail!("Unsupported bit depth: {}", spec.bits_per_sample),
    }

    writer.finalize()?;
    Ok(())
}

pub fn convert_to_mp3(wav_path: &Path, mp3_path: &Path) -> Result<()> {
    use std::process::Command;

    let output = Command::new("ffmpeg")
        .arg("-y")
        .arg("-i")
        .arg(wav_path)
        .arg("-b:a")
        .arg("320k")
        .arg(mp3_path)
        .output()?;

    if !output.status.success() {
        anyhow::bail!("ffmpeg failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    info!("Converted to MP3: {:?}", mp3_path);
    Ok(())
}

pub fn check_ffmpeg_available() -> bool {
    use std::process::Command;
    Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
