use crate::types::AudioDeviceInfo;

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