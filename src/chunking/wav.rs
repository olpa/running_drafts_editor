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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WavFacts {
    pub source_sha256: String,
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub decoded_sample_count: u64,
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
    #[error("WAV sample count overflow")]
    SampleCountOverflow,
}

pub fn stream_canonical_wav(
    path: impl AsRef<Path>,
    mut consume_frame: impl FnMut(u64, &[f32]),
) -> Result<WavFacts, WavError> {
    let path = path.as_ref();
    let source_sha256 = hash_file(path)?;
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    validate_spec(spec)?;

    let mut frame = Vec::with_capacity(512);
    let mut frame_start = 0_u64;
    let mut sample_count = 0_u64;
    for sample in reader.samples::<f32>() {
        let sample = sample?;
        if !sample.is_finite() {
            return Err(WavError::NonFiniteSample {
                index: sample_count,
            });
        }
        if !(-1.0..=1.0).contains(&sample) {
            return Err(WavError::UnnormalizedSample {
                index: sample_count,
            });
        }
        frame.push(sample);
        sample_count = sample_count
            .checked_add(1)
            .ok_or(WavError::SampleCountOverflow)?;
        if frame.len() == 512 {
            consume_frame(frame_start, &frame);
            frame.clear();
            frame_start = sample_count;
        }
    }
    if !frame.is_empty() {
        consume_frame(frame_start, &frame);
    }

    Ok(WavFacts {
        source_sha256,
        sample_rate_hz: spec.sample_rate,
        channels: spec.channels,
        decoded_sample_count: sample_count,
    })
}

pub fn read_canonical_wav(path: impl AsRef<Path>) -> Result<WavInput, WavError> {
    let path = path.as_ref();
    let source_sha256 = hash_file(path)?;
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    validate_spec(spec)?;
    let samples = reader.samples::<f32>().collect::<Result<Vec<_>, _>>()?;
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
    fn streams_bounded_frames_with_complete_coverage() {
        let samples = vec![0.25; 10_001];
        let file = write_wav(&samples);
        let mut starts = Vec::new();
        let mut lengths = Vec::new();

        let facts = stream_canonical_wav(file.path(), |start, frame| {
            starts.push(start);
            lengths.push(frame.len());
        })
        .unwrap();

        assert_eq!(facts.decoded_sample_count, 10_001);
        assert_eq!(lengths.iter().sum::<usize>(), 10_001);
        assert!(lengths.iter().all(|length| *length <= 512));
        assert_eq!(starts, (0..20).map(|index| index * 512).collect::<Vec<_>>());
        assert_eq!(lengths.last(), Some(&273));
    }

    #[test]
    fn rejects_unnormalized_sample_at_stream_position() {
        let file = write_wav(&[0.0, 1.01, 0.0]);
        let error = stream_canonical_wav(file.path(), |_, _| {}).unwrap_err();

        assert!(matches!(error, WavError::UnnormalizedSample { index: 1 }));
    }
}
