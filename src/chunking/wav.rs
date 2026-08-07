use std::{fs::File, io::Read, path::Path};

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
    #[error("sample {index} is not finite f32 PCM")]
    NonFiniteSample { index: u64 },
    #[error("sample {index} is outside normalized [-1, 1] f32 PCM")]
    UnnormalizedSample { index: u64 },
}

pub fn read_canonical_wav(path: impl AsRef<Path>) -> Result<WavInput, WavError> {
    let path = path.as_ref();
    let source_sha256 = hash_file(path)?;
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    validate_spec(spec)?;
    let samples = reader.samples::<f32>().collect::<Result<Vec<_>, _>>()?;
    for (index, sample) in samples.iter().enumerate() {
        if !sample.is_finite() {
            return Err(WavError::NonFiniteSample {
                index: index as u64,
            });
        }
        if !(-1.0..=1.0).contains(sample) {
            return Err(WavError::UnnormalizedSample {
                index: index as u64,
            });
        }
    }
    Ok(WavInput {
        samples,
        source_sha256,
        sample_rate_hz: spec.sample_rate,
        channels: spec.channels,
    })
}

fn validate_spec(spec: hound::WavSpec) -> Result<(), WavError> {
    if spec.sample_rate != 16_000
        || spec.channels != 1
        || spec.sample_format != hound::SampleFormat::Float
        || spec.bits_per_sample != 32
    {
        return Err(WavError::UnsupportedFormat);
    }
    Ok(())
}

fn hash_file(path: &Path) -> Result<String, std::io::Error> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(hex::encode(digest.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_wav(samples: &[f32]) -> tempfile::NamedTempFile {
        let file = tempfile::NamedTempFile::new().unwrap();
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = hound::WavWriter::create(file.path(), spec).unwrap();
        for sample in samples {
            writer.write_sample(*sample).unwrap();
        }
        writer.finalize().unwrap();
        file
    }

    #[test]
    fn reads_complete_canonical_audio() {
        let samples = vec![0.25; 10_001];
        let file = write_wav(&samples);

        let wav = read_canonical_wav(file.path()).unwrap();

        assert_eq!(wav.samples, samples);
        assert_eq!(wav.sample_rate_hz, 16_000);
        assert_eq!(wav.channels, 1);
    }

    #[test]
    fn rejects_unnormalized_sample_at_source_position() {
        let file = write_wav(&[0.0, 1.01, 0.0]);
        let error = read_canonical_wav(file.path()).unwrap_err();

        assert!(matches!(error, WavError::UnnormalizedSample { index: 1 }));
    }
}
