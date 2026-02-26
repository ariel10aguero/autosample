use autosample_core::AudioDeviceInfo;
use crossbeam_channel::Receiver;
use std::thread;

pub struct DeviceScanResult {
    pub audio_devices: Vec<AudioDeviceInfo>,
}

pub type DeviceScanOutcome = Result<DeviceScanResult, String>;

pub fn spawn_device_scan() -> Receiver<DeviceScanOutcome> {
    let (tx, rx) = crossbeam_channel::bounded(1);

    thread::spawn(move || {
        let outcome = autosample_core::audio::get_audio_devices()
            .map(|audio_devices| DeviceScanResult { audio_devices })
            .map_err(|audio_error| format!("Audio scan failed: {}", audio_error));

        let _ = tx.send(outcome);
    });

    rx
}
