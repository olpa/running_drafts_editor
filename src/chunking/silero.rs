use std::{fs::File, io::Read, mem::take, path::Path};

use ndarray::{Array, Array1, Array2, ArrayD};
use ort::{session::Session, value::Value};
use sha2::{Digest, Sha256};

use super::{
    CanonicalAudio, DetectorError, DetectorErrorCode, DetectorIdentity, FrameEvidence,
    SpeechDetector,
};

const SAMPLE_RATE: u32 = 16_000;
const FRAME_SAMPLES: usize = 512;
const CONTEXT_SAMPLES: usize = 64;

#[derive(Debug, Clone)]
pub struct SileroConfig {
    pub model_version: String,
    pub expected_model_sha256: String,
    pub intra_threads: usize,
}

pub struct SileroDetector {
    session: Session,
    identity: DetectorIdentity,
    state: ArrayD<f32>,
    context: Array1<f32>,
}

impl std::fmt::Debug for SileroDetector {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SileroDetector")
            .field("identity", &self.identity)
            .finish_non_exhaustive()
    }
}

impl SileroDetector {
    pub fn load(model_path: impl AsRef<Path>, config: SileroConfig) -> Result<Self, DetectorError> {
        if config.intra_threads == 0 {
            return Err(DetectorError::new(
                DetectorErrorCode::RuntimeInitialization,
                "intra_threads must be non-zero",
            ));
        }
        let actual_hash = hash_file(model_path.as_ref())?;
        if actual_hash != config.expected_model_sha256 {
            return Err(DetectorError::new(
                DetectorErrorCode::ModelHashMismatch,
                "model file does not match the expected SHA-256",
            ));
        }
        let session = Session::builder()
            .and_then(|builder| builder.with_intra_threads(config.intra_threads))
            .and_then(|builder| builder.commit_from_file(model_path.as_ref()))
            .map_err(|_| {
                DetectorError::new(
                    DetectorErrorCode::InvalidModel,
                    "ONNX Runtime could not load the supplied model",
                )
            })?;
        Ok(Self {
            session,
            identity: DetectorIdentity {
                name: "silero-vad-onnx".into(),
                version: config.model_version,
                model_sha256: config.expected_model_sha256,
                frame_samples: FRAME_SAMPLES as u64,
                sample_rate_hz: SAMPLE_RATE,
                runtime: format!("ort-2.0.0-rc.10/cpu/intra-threads-{}", config.intra_threads),
            },
            state: ArrayD::zeros([2, 1, 128].as_slice()),
            context: Array1::zeros(CONTEXT_SAMPLES),
        })
    }

    fn reset(&mut self) {
        self.state = ArrayD::zeros([2, 1, 128].as_slice());
        self.context = Array1::zeros(CONTEXT_SAMPLES);
    }

    fn infer_frame(&mut self, frame: &[f32]) -> Result<f32, DetectorError> {
        let mut input = Vec::with_capacity(CONTEXT_SAMPLES + FRAME_SAMPLES);
        input.extend_from_slice(
            self.context
                .as_slice()
                .ok_or_else(|| malformed("detector context is not contiguous"))?,
        );
        input.extend_from_slice(frame);
        let input = Array2::from_shape_vec([1, input.len()], input)
            .map_err(|_| malformed("could not shape detector input"))?;
        let sample_rate = Array::from_shape_vec([1], vec![i64::from(SAMPLE_RATE)])
            .map_err(|_| malformed("could not shape sample rate input"))?;
        let input_value = Value::from_array(input).map_err(inference)?;
        let state_value = Value::from_array(take(&mut self.state)).map_err(inference)?;
        let sample_rate_value = Value::from_array(sample_rate).map_err(inference)?;
        let outputs = self
            .session
            .run([
                (&input_value).into(),
                (&state_value).into(),
                (&sample_rate_value).into(),
            ])
            .map_err(inference)?;
        let (shape, state) = outputs["stateN"]
            .try_extract_tensor::<f32>()
            .map_err(|_| malformed("model output stateN is missing or malformed"))?;
        let state_shape = shape
            .iter()
            .map(|value| *value as usize)
            .collect::<Vec<_>>();
        self.state = ArrayD::from_shape_vec(state_shape, state.to_vec())
            .map_err(|_| malformed("model output stateN has an invalid shape"))?;
        self.context = Array1::from_vec(frame[FRAME_SAMPLES - CONTEXT_SAMPLES..].to_vec());
        let probability = outputs["output"]
            .try_extract_tensor::<f32>()
            .map_err(|_| malformed("model probability output is missing or malformed"))?
            .1
            .first()
            .copied()
            .ok_or_else(|| malformed("model probability output is empty"))?;
        if !probability.is_finite() || !(0.0..=1.0).contains(&probability) {
            return Err(malformed("model returned an invalid speech probability"));
        }
        Ok(probability)
    }
}

impl SpeechDetector for SileroDetector {
    fn identity(&self) -> DetectorIdentity {
        self.identity.clone()
    }

    fn detect(&mut self, audio: &CanonicalAudio<'_>) -> Result<Vec<FrameEvidence>, DetectorError> {
        self.reset();
        let mut evidence = Vec::with_capacity(audio.samples.len().div_ceil(FRAME_SAMPLES));
        for (index, source_frame) in audio.samples.chunks(FRAME_SAMPLES).enumerate() {
            let mut padded = [0.0; FRAME_SAMPLES];
            padded[..source_frame.len()].copy_from_slice(source_frame);
            let probability = self.infer_frame(&padded)?;
            let start = index
                .checked_mul(FRAME_SAMPLES)
                .and_then(|value| u64::try_from(value).ok())
                .ok_or_else(|| malformed("detector frame position overflow"))?;
            let real_len = u64::try_from(source_frame.len())
                .map_err(|_| malformed("detector frame length overflow"))?;
            evidence.push(FrameEvidence {
                start_sample: start,
                end_sample: start + real_len,
                speech_probability: probability,
            });
        }
        Ok(evidence)
    }
}

fn hash_file(path: &Path) -> Result<String, DetectorError> {
    let mut file = File::open(path).map_err(|error| {
        let code = if error.kind() == std::io::ErrorKind::NotFound {
            DetectorErrorCode::ModelNotFound
        } else {
            DetectorErrorCode::RuntimeInitialization
        };
        DetectorError::new(code, "could not read the supplied model file")
    })?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|_| {
            DetectorError::new(
                DetectorErrorCode::RuntimeInitialization,
                "could not read the supplied model file",
            )
        })?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(hex::encode(digest.finalize()))
}

fn inference(_: ort::Error) -> DetectorError {
    DetectorError::new(
        DetectorErrorCode::InferenceFailed,
        "ONNX Runtime inference failed",
    )
}

fn malformed(summary: &'static str) -> DetectorError {
    DetectorError::new(DetectorErrorCode::MalformedOutput, summary)
}
