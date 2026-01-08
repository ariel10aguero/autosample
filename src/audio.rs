use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Sample, SampleFormat, Stream, StreamConfig};
use crossbeam_channel::{bounded, Receiver, Sender};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{info, warn};

pub fn list_audio_inputs() -> Result<()> {
    let host = cpal::default_host();

    println!("\nAvailable Audio Input Devices:");
    println!("{:-<80}", "");

    let devices: Vec<_> = host
        .input_devices()?
        .enumerate()
        .collect();

    if devices.is_empty() {
        println!("No audio input devices found.");
        return Ok(());
    }

    for (i, device) in devices {
        let name = device.name().unwrap_or_else(|_| 
"Unknown".to_string());
        println!("\n{:3}: {}", i, name);

        if let Ok(config) = device.default_input_config() {
            println!("     Default config:");
            println!("       Sample rate: {} Hz", config.sample_rate().0);
            println!("       Channels: {}", config.channels());
            println!("       Format: {:?}", config.sample_format());
        }

        // List supported configs
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

pub fn find_audio_input(identifier: &str) -> Result<Device> {
    let host = cpal::default_host();
    let devices: Vec<_> = host.input_devices()?.collect();

    // Try as index first
    if let Ok(index) = identifier.parse::<usize>() {
        if index < devices.len() {
            return Ok(devices[index].clone());
        }
    }

    // Try as name (case-insensitive substring match)
    let identifier_lower = identifier.to_lowercase();
    for device in devices {
        if let Ok(name) = device.name() {
            if name.to_lowercase().contains(&identifier_lower) {
                return Ok(device);
            }
        }
    }

    anyhow::bail!("Audio input device not found: {}", identifier)
}

pub struct AudioCapture {
    stream: Stream,
    receiver: Receiver<Vec<f32>>,
    running: Arc<AtomicBool>,
    channels: u16,
    sample_rate: u32,
}

impl AudioCapture {
    pub fn new(device: Device, target_sr: u32, target_channels: u16) -> 
Result<Self> {
        let device_name = device.name().unwrap_or_else(|_| 
"Unknown".to_string());
        info!("Setting up audio capture from: {}", device_name);

        // Try to find a config that matches our requirements
        let config = Self::find_suitable_config(&device, target_sr, 
target_channels)?;

        info!(
            "Using audio config: {} Hz, {} channels, {:?}",
            config.sample_rate.0, config.channels, config.sample_format
        );

        let (sender, receiver) = bounded(1000);
        let running = Arc::new(AtomicBool::new(true));

        let stream = match config.sample_format {
            SampleFormat::F32 => Self::build_stream::<f32>(&device, 
&config.config, sender)?,
            SampleFormat::I16 => Self::build_stream::<i16>(&device, 
&config.config, sender)?,
            SampleFormat::U16 => Self::build_stream::<u16>(&device, 
&config.config, sender)?,
            _ => anyhow::bail!("Unsupported sample format: {:?}", 
config.sample_format),
        };

        stream.play()?;

        Ok(Self {
            stream,
            receiver,
            running,
            channels: config.channels,
            sample_rate: config.sample_rate.0,
        })
    }

    fn find_suitable_config(
        device: &Device,
        target_sr: u32,
        target_channels: u16,
    ) -> Result<cpal::SupportedStreamConfig> {
        // Try exact match first
        if let Ok(configs) = device.supported_input_configs() {
            for config in configs {
                if config.channels() == target_channels {
                    let sr = cpal::SampleRate(target_sr);
                    if config.min_sample_rate() <= sr && sr <= 
config.max_sample_rate() {
                        return Ok(config.with_sample_rate(sr));
                    }
                }
            }
        }

        // Fall back to default
        let default_config = device.default_input_config()?;
        warn!(
            "Could not find exact match, using default: {} Hz, {} 
channels",
            default_config.sample_rate().0,
            default_config.channels()
        );
        Ok(default_config)
    }

    fn build_stream<T>(
        device: &Device,
        config: &StreamConfig,
        sender: Sender<Vec<f32>>,
    ) -> Result<Stream>
    where
        T: Sample + cpal::SizedSample,
        f32: cpal::FromSample<T>,
    {
        let channels = config.channels as usize;

        let stream = device.build_input_stream(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                let samples: Vec<f32> = data.iter().map(|&s| 
s.to_sample()).collect();
                let _ = sender.try_send(samples);
            },
            move |err| {
                warn!("Audio stream error: {}", err);
            },
            None,
        )?;

        Ok(stream)
    }

    pub fn receiver(&self) -> &Receiver<Vec<f32>> {
        &self.receiver
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        self.stop();
    }
}
