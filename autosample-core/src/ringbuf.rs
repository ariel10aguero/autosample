use crossbeam_channel::Receiver;
use std::collections::VecDeque;

pub struct RingBuffer {
    buffer: VecDeque<f32>,
    max_size: usize,
}

impl RingBuffer {
    pub fn new(max_size: usize) -> Self {
        Self {
            buffer: VecDeque::with_capacity(max_size),
            max_size,
        }
    }

    pub fn push_samples(&mut self, samples: &[f32]) {
        for &sample in samples {
            if self.buffer.len() >= self.max_size {
                self.buffer.pop_front();
            }
            self.buffer.push_back(sample);
        }
    }

    pub fn get_last_samples(&self, count: usize) -> Vec<f32> {
        let available = self.buffer.len().min(count);
        self.buffer
            .iter()
            .skip(self.buffer.len().saturating_sub(available))
            .copied()
            .collect()
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}

pub fn consume_audio_packets(receiver: &Receiver<Vec<f32>>, ring: &mut RingBuffer) {
    while let Ok(packet) = receiver.try_recv() {
        ring.push_samples(&packet);
    }
}
