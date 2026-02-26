use crate::types::AudioDeviceInfo;
use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, Stream};
use crossbeam_channel::Sender;
use tracing::info;

pub struct AudioCapture {
    pub stream: Stream,
    pub config: CaptureConfig,
}

#[derive(Debug, Clone)]
pub struct CaptureConfig {
    pub channels: u16,
    pub sample_rate: u32,
    pub sample_format: SampleFormat,
}

pub fn list_audio_devices() -> Result<()> {
    let host = cpal::default_host();
    println!("\nAvailable Audio Input Devices:");
    println!("{}", "-".repeat(80));
    
    for (idx, device) in host.input_devices()?.enumerate() {
        let name = device.name().unwrap_or_else(|_| "Unknown".to_string());
        println!("\n  {}: {}", idx, name);

        if let Ok(config) = device.default_input_config() {
            println!("     Default config:");
            println!("       Sample rate: {} Hz", config.sample_rate().0);
            println!("       Channels: {}", config.channels());
            println!("       Format: {:?}", config.sample_format());
        }

        if let Ok(configs) = device.supported_input_configs() {
            println!("     Supported configs:");
            for cfg in configs {
                println!(
                    "       Rate: {}-{} Hz, Channels: {}, Format: {:?}",
                    cfg.min_sample_rate().0,
                    cfg.max_sample_rate().0,
                    cfg.channels(),
                    cfg.sample_format()
                );
            }
        }
    }
    
    println!();
    Ok(())
}

pub fn get_audio_devices() -> Result<Vec<AudioDeviceInfo>> {
    let host = cpal::default_host();
    let mut devices = Vec::new();

    for (idx, device) in host.input_devices()?.enumerate() {
        let name = device.name().unwrap_or_else(|_| "Unknown".to_string());

        if let Ok(config) = device.default_input_config() {
            devices.push(AudioDeviceInfo {
                index: idx,
                name,
                sample_rate: config.sample_rate().0,
                channels: config.channels(),
            });
        }
    }

    Ok(devices)
}

pub fn find_audio_device(name_or_id: &str) -> Result<Device> {
    let host = cpal::default_host();

    // Try parsing as index
    if let Ok(idx) = name_or_id.parse::<usize>() {
        if let Some(device) = host.input_devices()?.nth(idx) {
            return Ok(device);
        }
    }

    let mut devices: Vec<(Device, String)> = host
        .input_devices()?
        .filter_map(|d| d.name().ok().map(|n| (d, n)))
        .collect();

    // Prefer exact match first so similarly named devices do not collide.
    if let Some(pos) = devices.iter().position(|(_, name)| name == name_or_id) {
        return Ok(devices.swap_remove(pos).0);
    }

    let needle_lower = name_or_id.to_lowercase();
    if let Some(pos) = devices
        .iter()
        .position(|(_, name)| name.to_lowercase() == needle_lower)
    {
        return Ok(devices.swap_remove(pos).0);
    }

    // Then allow substring match.
    let mut contains_positions = devices
        .iter()
        .enumerate()
        .filter_map(|(idx, (_, name))| name.to_lowercase().contains(&needle_lower).then_some(idx));
    if let Some(first_idx) = contains_positions.next() {
        if contains_positions.next().is_some() {
            let matches: Vec<String> = devices
                .iter()
                .filter_map(|(_, name)| name.to_lowercase().contains(&needle_lower).then_some(name.clone()))
                .collect();
            anyhow::bail!(
                "Audio device name '{}' is ambiguous. Matches: {}. Select by numeric index instead.",
                name_or_id,
                matches.join(", ")
            );
        }
        return Ok(devices.swap_remove(first_idx).0);
    }

    let available: Vec<String> = devices.into_iter().map(|(_, name)| name).collect();
    anyhow::bail!(
        "Audio device not found: '{}'. Available inputs: {}",
        name_or_id,
        if available.is_empty() {
            "(none)".to_string()
        } else {
            available.join(", ")
        }
    )
}

pub fn start_audio_capture(
    device: Device,
    requested_sr: u32,
    requested_channels: u16,
    sender: Sender<Vec<f32>>,
) -> Result<AudioCapture> {
    let device_name = device.name().unwrap_or_else(|_| "Unknown".to_string());
    let mut config = device
        .default_input_config()
        .map_err(|e| anyhow::anyhow!("Failed to get default input config: {}", e))?;

    // Try to match requested sample rate
    if let Ok(supported_configs) = device.supported_input_configs() {
        for supported_config in supported_configs {
            if supported_config.channels() == requested_channels
                && supported_config.min_sample_rate().0 <= requested_sr
                && supported_config.max_sample_rate().0 >= requested_sr
            {
                config = supported_config.with_sample_rate(cpal::SampleRate(requested_sr));
                break;
            }
        }
    }

    info!(
        "Audio capture using '{}': {} Hz, {} ch, {:?}",
        device_name,
        config.sample_rate().0,
        config.channels(),
        config.sample_format()
    );

    let stream = match config.sample_format() {
        SampleFormat::F32 => build_input_stream::<f32>(&device, &config.config(), sender)?,
        SampleFormat::I16 => build_input_stream::<i16>(&device, &config.config(), sender)?,
        SampleFormat::U16 => build_input_stream::<u16>(&device, &config.config(), sender)?,
        format => {
            anyhow::bail!("Unsupported sample format: {:?}", format)
        }
    };

    stream.play()?;

    Ok(AudioCapture {
        stream,
        config: CaptureConfig {
            channels: config.channels(),
            sample_rate: config.sample_rate().0,
            sample_format: config.sample_format(),
        },
    })
}

fn build_input_stream<T>(
    device: &Device,
    config: &cpal::StreamConfig,
    sender: Sender<Vec<f32>>,
) -> Result<Stream>
where
    T: cpal::Sample + cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    let stream = device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            let samples: Vec<f32> = data
                .iter()
                .map(|&s| cpal::Sample::to_sample::<f32>(s))
                .collect();
            let _ = sender.try_send(samples);
        },
        move |err| {
            eprintln!("Audio stream error: {}", err);
        },
        None,
    )?;

    Ok(stream)
}