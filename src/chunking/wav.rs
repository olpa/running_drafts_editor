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
    #[error("unsupported WAV format: {sample_format:?} with {bits_per_sample} bits per sample")]
    UnsupportedFormat {
        sample_format: hound::SampleFormat,
        bits_per_sample: u16,
    },
    #[error("WAV must have at least one channel and a positive sample rate")]
    InvalidSpec,
    #[error("WAV sample count is not complete for {channels} channels")]
    IncompleteFrame { channels: u16 },
    #[error("decoded WAV is too long to process on this system")]
    TooLong,
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
    let samples = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<Vec<_>, _>>()?,
        hound::SampleFormat::Int => {
            let scale = 2_f64.powi(i32::from(spec.bits_per_sample) - 1);
            reader
                .samples::<i32>()
                .map(|sample| sample.map(|sample| (f64::from(sample) / scale) as f32))
                .collect::<Result<Vec<_>, _>>()?
        }
    };
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
    let mono = downmix_to_mono(&samples, spec.channels)?;
    let samples = resample(&mono, spec.sample_rate, 16_000)?;
    Ok(WavInput {
        samples,
        source_sha256,
        sample_rate_hz: 16_000,
        channels: 1,
    })
}

fn validate_spec(spec: hound::WavSpec) -> Result<(), WavError> {
    if spec.sample_rate == 0 || spec.channels == 0 {
        return Err(WavError::InvalidSpec);
    }
    let supported = match spec.sample_format {
        hound::SampleFormat::Float => spec.bits_per_sample == 32,
        hound::SampleFormat::Int => matches!(spec.bits_per_sample, 8 | 16 | 24 | 32),
    };
    if !supported {
        return Err(WavError::UnsupportedFormat {
            sample_format: spec.sample_format,
            bits_per_sample: spec.bits_per_sample,
        });
    }
    Ok(())
}

fn downmix_to_mono(samples: &[f32], channels: u16) -> Result<Vec<f32>, WavError> {
    let channels = usize::from(channels);
    if !samples.len().is_multiple_of(channels) {
        return Err(WavError::IncompleteFrame {
            channels: channels as u16,
        });
    }
    Ok(samples
        .chunks_exact(channels)
        .map(|frame| {
            let sum = frame.iter().map(|sample| f64::from(*sample)).sum::<f64>();
            (sum / channels as f64) as f32
        })
        .collect())
}

fn resample(samples: &[f32], source_rate: u32, target_rate: u32) -> Result<Vec<f32>, WavError> {
    if samples.is_empty() || source_rate == target_rate {
        return Ok(samples.to_vec());
    }
    let output_len = (samples.len() as u128 * u128::from(target_rate)
        + u128::from(source_rate) / 2)
        / u128::from(source_rate);
    let output_len = usize::try_from(output_len).map_err(|_| WavError::TooLong)?;
    let mut output = Vec::with_capacity(output_len);
    for output_index in 0..output_len {
        let position = output_index as u128 * u128::from(source_rate);
        let left = usize::try_from(position / u128::from(target_rate))
            .map_err(|_| WavError::TooLong)?
            .min(samples.len() - 1);
        let right = (left + 1).min(samples.len() - 1);
        let fraction = (position % u128::from(target_rate)) as f32 / target_rate as f32;
        output.push(samples[left] + (samples[right] - samples[left]) * fraction);
    }
    Ok(output)
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

    fn write_integer_wav(
        samples: &[i32],
        channels: u16,
        sample_rate: u32,
        bits_per_sample: u16,
    ) -> tempfile::NamedTempFile {
        let file = tempfile::NamedTempFile::new().unwrap();
        let spec = hound::WavSpec {
            channels,
            sample_rate,
            bits_per_sample,
            sample_format: hound::SampleFormat::Int,
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

    #[test]
    fn converts_48_khz_16_bit_pcm_to_canonical_audio() {
        let file = write_integer_wav(&vec![16_384; 480], 1, 48_000, 16);

        let wav = read_canonical_wav(file.path()).unwrap();

        assert_eq!(wav.sample_rate_hz, 16_000);
        assert_eq!(wav.channels, 1);
        assert_eq!(wav.samples.len(), 160);
        assert!(wav.samples.iter().all(|sample| *sample == 0.5));
    }

    #[test]
    fn averages_channels_and_upsamples() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 8_000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = hound::WavWriter::create(file.path(), spec).unwrap();
        for sample in [0.75, 0.25, -0.5, 0.5] {
            writer.write_sample(sample).unwrap();
        }
        writer.finalize().unwrap();

        let wav = read_canonical_wav(file.path()).unwrap();

        assert_eq!(wav.samples, vec![0.5, 0.25, 0.0, 0.0]);
    }

    #[test]
    fn reads_24_bit_integer_pcm() {
        let file = write_integer_wav(&[-8_388_608, 0, 8_388_607], 1, 16_000, 24);

        let wav = read_canonical_wav(file.path()).unwrap();

        assert_eq!(wav.samples[0], -1.0);
        assert_eq!(wav.samples[1], 0.0);
        assert!(wav.samples[2] < 1.0);
        assert!(wav.samples[2] > 0.999_999);
    }

    #[test]
    fn reads_8_and_32_bit_integer_pcm() {
        for (bits, minimum) in [(8, -128), (32, i32::MIN)] {
            let file = write_integer_wav(&[minimum, 0], 1, 16_000, bits);

            let wav = read_canonical_wav(file.path()).unwrap();

            assert_eq!(wav.samples, vec![-1.0, 0.0]);
        }
    }
}
