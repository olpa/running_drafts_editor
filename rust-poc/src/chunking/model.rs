use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceFacts {
    pub sha256: String,
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub decoded_sample_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SampleRange {
    pub start_sample: u64,
    pub end_sample: u64,
}

impl SampleRange {
    pub fn len(self) -> u64 {
        self.end_sample - self.start_sample
    }

    pub fn is_empty(self) -> bool {
        self.start_sample == self.end_sample
    }
}
