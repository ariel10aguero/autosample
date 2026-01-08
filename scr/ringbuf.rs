use crossbeam_channel::{bounded, Receiver, Sender};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Simple ring buffer for audio capture using channels
pub struct RingBuffer {
    sender: Sender<Vec<f32>>,
    receiver: Receiver<Vec<f32>>,
    capacity_blocks: usize,
}

impl RingBuffer {
    pub fn new(capacity_blocks: usize) -> Self {
        let (sender, receiver) = bounded(capacity_blocks);
        Self {
            sender,
            receiver,
            capacity_blocks,
        }
    }

    pub fn push(&self, block: Vec<f32>) -> Result<(), ()> {
        self.sender.try_send(block).map_err(|_| ())
    }

    pub fn receiver(&self) -> &Receiver<Vec<f32>> {
        &self.receiver
    }
}

pub struct CaptureBuffer {
    buffer: Vec<f32>,
    channels: usize,
    sample_rate: u32,
}

impl CaptureBuffer {
    pub fn new(channels: usize, sample_rate: u32, duration_ms: u64) -> 
Self {
        let samples = (sample_rate as u64 * duration_ms / 1000) as usize * 
channels;
        Self {
            buffer: Vec::with_capacity(samples),
            channels,
            sample_rate,
        }
    }

    pub fn append(&mut self, data: &[f32]) {
        self.buffer.extend_from_slice(data);
    }

    pub fn data(&self) -> &[f32] {
        &self.buffer
    }

    pub fn into_data(self) -> Vec<f32> {
        self.buffer
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn duration_ms(&self) -> f64 {
        let frames = self.buffer.len() / self.channels;
        frames as f64 / self.sample_rate as f64 * 1000.0
    }
}
