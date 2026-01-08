use tracing::info;

pub fn db_to_linear(db: f32) -> f32 {
    10.0f32.powf(db / 20.0)
}

pub fn linear_to_db(linear: f32) -> f32 {
    20.0 * linear.abs().max(1e-10).log10()
}

pub fn calculate_peak_db(samples: &[f32]) -> f32 {
    let peak = samples
        .iter()
        .map(|&s| s.abs())
        .fold(0.0f32, f32::max);
    linear_to_db(peak)
}

pub fn trim_silence(
    samples: &[f32],
    channels: usize,
    threshold_db: f32,
    min_tail_samples: usize,
) -> (usize, usize) {
    if samples.is_empty() {
        return (0, 0);
    }

    let threshold = db_to_linear(threshold_db);
    let frames = samples.len() / channels;

    // Find first frame above threshold
    let mut start_frame = 0;
    'outer_start: for frame in 0..frames {
        for ch in 0..channels {
            if samples[frame * channels + ch].abs() > threshold {
                start_frame = frame;
                break 'outer_start;
            }
        }
    }

    // Find last frame above threshold
    let mut end_frame = frames;
    'outer_end: for frame in (0..frames).rev() {
        for ch in 0..channels {
            if samples[frame * channels + ch].abs() > threshold {
                end_frame = frame + 1;
                break 'outer_end;
            }
        }
    }

    // Ensure minimum tail
    let min_tail_frames = min_tail_samples / channels;
    if end_frame + min_tail_frames <= frames {
        end_frame = (end_frame + min_tail_frames).min(frames);
    } else {
        end_frame = frames;
    }

    (start_frame * channels, end_frame * channels)
}

pub fn normalize(samples: &mut [f32], mode: &str) {
    if samples.is_empty() || mode == "off" {
        return;
    }

    let peak = samples
        .iter()
        .map(|&s| s.abs())
        .fold(0.0f32, f32::max);

    if peak < 1e-10 {
        return;
    }

    let target_peak = match mode {
        "peak" => 0.99, // -0.1 dBFS safety margin
        "-1db" => db_to_linear(-1.0),
        _ => return,
    };

    let gain = target_peak / peak;
    info!("Normalizing with gain: {:.3} ({:.2} dB)", gain, 
linear_to_db(gain));

    for sample in samples.iter_mut() {
        *sample *= gain;
    }
}

pub fn apply_fade(samples: &mut [f32], channels: usize, fade_samples: 
usize) {
    if samples.is_empty() || fade_samples == 0 {
        return;
    }

    let frames = samples.len() / channels;
    let fade_frames = fade_samples.min(frames / 4); // Max 25% of signal

    // Fade in
    for frame in 0..fade_frames {
        let gain = frame as f32 / fade_frames as f32;
        for ch in 0..channels {
            samples[frame * channels + ch] *= gain;
        }
    }

    // Fade out
    for frame in 0..fade_frames {
        let gain = 1.0 - (frame as f32 / fade_frames as f32);
        let abs_frame = frames - fade_frames + frame;
        for ch in 0..channels {
            samples[abs_frame * channels + ch] *= gain;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn test_db_conversion() {
        assert_relative_eq!(db_to_linear(0.0), 1.0, epsilon = 0.001);
        assert_relative_eq!(db_to_linear(-6.0), 0.501, epsilon = 0.01);
        assert_relative_eq!(linear_to_db(1.0), 0.0, epsilon = 0.001);
    }

    #[test]
    fn test_normalize() {
        let mut samples = vec![0.5, -0.5, 0.25, -0.25];
        normalize(&mut samples, "peak");
        
        let peak = samples.iter().map(|&s| s.abs()).fold(0.0f32, 
f32::max);
        assert_relative_eq!(peak, 0.99, epsilon = 0.01);
    }

    #[test]
    fn test_trim_silence() {
        let samples = vec![
            0.0, 0.0, 0.0, 0.0, // silence
            0.1, 0.1, 0.5, 0.5, // signal
            0.0, 0.0, 0.0, 0.0, // silence
        ];
        
        let (start, end) = trim_silence(&samples, 2, -40.0, 0);
        assert_eq!(start, 4); // Start of signal
        assert_eq!(end, 8);   // End of signal
    }
}
