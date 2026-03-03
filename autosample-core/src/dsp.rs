use std::f32;

pub fn trim_silence(
    samples: &[f32],
    threshold_db: f32,
    channels: usize,
    min_tail_samples: usize,
) -> (Vec<f32>, usize, usize) {
    let threshold = db_to_linear(threshold_db);
    let frame_size = channels;

    // Find start
    let mut start_frame = 0;
    for i in (0..samples.len()).step_by(frame_size) {
        let end_idx = (i + frame_size).min(samples.len());
        let frame = &samples[i..end_idx];
        if frame.iter().any(|&s| s.abs() > threshold) {
            start_frame = i / frame_size;
            break;
        }
    }

    // Find end
    let mut end_frame = samples.len() / frame_size;
    let mut i = (samples.len() / frame_size).saturating_sub(1) * frame_size;
    loop {
        let end_idx = (i + frame_size).min(samples.len());
        let frame = &samples[i..end_idx];
        if frame.iter().any(|&s| s.abs() > threshold) {
            end_frame = (i / frame_size) + 1;
            break;
        }
        if i == 0 {
            break;
        }
        i = i.saturating_sub(frame_size);
    }

    // Apply minimum tail
    let min_tail_frames = min_tail_samples / frame_size;
    end_frame = (end_frame + min_tail_frames).min(samples.len() / frame_size);

    let start_sample = start_frame * frame_size;
    let end_sample = end_frame * frame_size;

    (
        samples[start_sample..end_sample].to_vec(),
        start_frame,
        end_frame,
    )
}

pub fn normalize_audio(samples: &[f32], mode: &str) -> Vec<f32> {
    let peak = samples.iter().map(|&s| s.abs()).fold(0.0f32, f32::max);

    if peak == 0.0 {
        return samples.to_vec();
    }

    let target = match mode {
        "peak" => 0.99,
        "-1dB" | "-1db" => db_to_linear(-1.0),
        _ => return samples.to_vec(),
    };

    let gain = target / peak;
    samples.iter().map(|&s| s * gain).collect()
}

pub fn db_to_linear(db: f32) -> f32 {
    10.0f32.powf(db / 20.0)
}

pub fn linear_to_db(linear: f32) -> f32 {
    if linear <= 0.0 {
        -100.0
    } else {
        20.0 * linear.log10()
    }
}

pub fn get_peak_db(samples: &[f32]) -> f32 {
    let peak = samples.iter().map(|&s| s.abs()).fold(0.0f32, f32::max);
    linear_to_db(peak)
}

pub fn apply_fade(samples: &mut [f32], channels: usize, fade_samples: usize) {
    let frames = samples.len() / channels;
    let fade_frames = fade_samples.min(frames / 2);

    // Fade in
    for i in 0..fade_frames {
        let gain = i as f32 / fade_frames as f32;
        for ch in 0..channels {
            let idx = i * channels + ch;
            if idx < samples.len() {
                samples[idx] *= gain;
            }
        }
    }

    // Fade out
    for i in 0..fade_frames {
        let gain = 1.0 - (i as f32 / fade_frames as f32);
        let frame_idx = frames - fade_frames + i;
        for ch in 0..channels {
            let idx = frame_idx * channels + ch;
            if idx < samples.len() {
                samples[idx] *= gain;
            }
        }
    }
}
