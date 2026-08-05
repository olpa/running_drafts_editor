use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const PLAN_SCHEMA: &str = "recognition-plan/v1-experimental";
pub const PLANNER_VERSION: &str = "rde-silero-v1";

#[derive(Debug, Clone, Copy)]
pub struct CanonicalAudio<'a> {
    pub samples: &'a [f32],
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub source_sha256: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceFacts {
    pub sha256: String,
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub decoded_sample_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecognizerContract {
    pub name: String,
    pub version: String,
    pub max_submitted_samples: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlannerConfig {
    pub search_back_samples: u64,
    pub minimum_chunk_samples: u64,
    pub speech_threshold: f32,
    pub minimum_low_speech_samples: u64,
    pub left_padding_samples: u64,
    pub right_padding_samples: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectorIdentity {
    pub name: String,
    pub version: String,
    pub model_sha256: String,
    pub frame_samples: u64,
    pub sample_rate_hz: u32,
    pub runtime: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameEvidence {
    pub start_sample: u64,
    pub end_sample: u64,
    pub speech_probability: f32,
}

pub trait SpeechDetector {
    fn identity(&self) -> DetectorIdentity;
    fn detect(&mut self, audio: &CanonicalAudio<'_>) -> Result<Vec<FrameEvidence>, DetectorError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectorStatus {
    Available,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DetectorRun {
    pub identity: DetectorIdentity,
    pub status: DetectorStatus,
    pub evidence: Vec<FrameEvidence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<DetectorErrorCode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlannerRun {
    pub version: String,
    pub config: PlannerConfig,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundaryKind {
    SourceEnd,
    VadValley,
    HardLimitNoCandidate,
    HardLimitDetectorUnavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValleyEvidence {
    pub search: SampleRange,
    pub selected_run: SampleRange,
    pub mean_speech_probability: f32,
    pub frame_count: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoundaryDecision {
    pub kind: BoundaryKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valley: Option<ValleyEvidence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detector_error_code: Option<DetectorErrorCode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreflightResult {
    Ready,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecognitionChunk {
    pub id: String,
    pub ordinal: u32,
    pub core: SampleRange,
    pub submitted: SampleRange,
    pub boundary: BoundaryDecision,
    pub preflight: PreflightResult,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanFailure {
    pub code: String,
    pub recoverable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecognitionPlan {
    pub schema: String,
    pub id: String,
    pub plan_inputs_hash: String,
    pub revision: u64,
    pub source: SourceFacts,
    pub recognizer: RecognizerContract,
    pub detector: DetectorRun,
    pub planner: PlannerRun,
    pub chunks: Vec<RecognitionChunk>,
    pub failures: Vec<PlanFailure>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectorErrorCode {
    ModelNotFound,
    ModelHashMismatch,
    InvalidModel,
    RuntimeInitialization,
    InferenceFailed,
    MalformedOutput,
    InvalidEvidence,
}

#[derive(Debug, Error)]
#[error("detector failed ({code:?}): {summary}")]
pub struct DetectorError {
    pub code: DetectorErrorCode,
    pub summary: String,
}

impl DetectorError {
    pub fn new(code: DetectorErrorCode, summary: impl Into<String>) -> Self {
        Self {
            code,
            summary: summary.into(),
        }
    }
}

#[derive(Debug, Error)]
pub enum PlanError {
    #[error("invalid canonical audio: {0}")]
    InvalidAudio(String),
    #[error("invalid planner configuration: {0}")]
    InvalidConfiguration(String),
    #[error("integer conversion or arithmetic overflow")]
    IntegerOverflow,
    #[error("invalid detector evidence: {0}")]
    InvalidDetectorEvidence(String),
    #[error("plan invariant failed: {0}")]
    Invariant(String),
    #[error("serialization or hashing failed: {0}")]
    Serialization(String),
}
