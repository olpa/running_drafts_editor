use std::path::Path;

use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug)]
pub struct WavInput {
    pub samples: Vec<f32>,
    pub source_sha256: String,
    pub sample_rate_hz: u32,
    pub channels: u16,
}

#[derive(Debug, Error)]
pub enum WavError {
    #[error("could not read WAV: {0}")]
    Read(#[from] hound::Error),
    #[error("could not hash WAV: {0}")]
    Hash(#[from] std::io::Error),
    #[error("WAV must contain mono 16 kHz 32-bit float PCM")]
    UnsupportedFormat,
}

pub fn read_canonical_wav(path: impl AsRef<Path>) -> Result<WavInput, WavError> {
    let bytes = std::fs::read(path.as_ref())?;
    let source_sha256 = hex::encode(Sha256::digest(&bytes));
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    if spec.sample_rate != 16_000
        || spec.channels != 1
        || spec.sample_format != hound::SampleFormat::Float
        || spec.bits_per_sample != 32
    {
        return Err(WavError::UnsupportedFormat);
    }
    let samples = reader.samples::<f32>().collect::<Result<Vec<_>, _>>()?;
    Ok(WavInput {
        samples,
        source_sha256,
        sample_rate_hz: spec.sample_rate,
        channels: spec.channels,
    })
}
