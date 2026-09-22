//! Audio storage and recognition boundaries. Only implementations and playback
//! adapters handle audio paths and samples; application requests use stable IDs.

pub(crate) mod local;
mod playback;

pub use local::{LocalAudioBackend, LocalRecognitionBackend};
pub use playback::{AudioPlayer, Ffplay, PlaybackError, PlaybackSpeed};

use std::path::Path;

use crate::{
    chunking::{SampleRange, SourceFacts},
    document::{AlignmentState, AudioSource, ChunkAudioMapping},
    project::TranscriptionSettings,
    transcription::{
        InitialTranscriptionResult, Transcription, TranscriptionConfig, TranscriptionError,
    },
};

#[derive(Debug, Clone)]
pub struct RecordingMetadata {
    pub id: String,
    pub sha256: Option<String>,
    pub canonical_sample_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkMetadata {
    pub id: String,
    pub recording_id: String,
    pub range: SampleRange,
    pub alignment: AlignmentState,
}

/// Backend-to-backend data for local recognition. Application callers never
/// need this operation; remote recognition may keep all audio on its server.
pub struct RecordingAudio {
    pub source: SourceFacts,
    pub samples: Vec<f32>,
}

pub struct ChunkAudio {
    pub audio: RecordingAudio,
    pub range: SampleRange,
}

/// Replay keeps canonical source coordinates and includes chunk identities so
/// a backend can serve audio even without retaining the full recording.
pub struct ReplayRequest<'a> {
    pub recording_id: &'a str,
    pub chunk_ids: &'a [String],
    pub range: SampleRange,
    pub speed: PlaybackSpeed,
}

pub trait AudioBackend {
    fn upload(&mut self, path: &Path) -> Result<String, BackendError>;
    fn recording(&self, id: &str) -> Result<RecordingMetadata, BackendError>;
    fn chunk(&self, id: &str) -> Result<ChunkMetadata, BackendError>;
    fn read_recording(&mut self, id: &str) -> Result<RecordingAudio, BackendError>;
    fn read_chunk(&mut self, id: &str) -> Result<ChunkAudio, BackendError>;
    /// Register a complete successful result atomically. Existing chunk IDs
    /// cannot acquire different identities or boundaries.
    fn register_chunks(&mut self, chunks: &[ChunkMetadata]) -> Result<(), BackendError>;
    /// Restore already finalized identities without reading audio. Replaces the
    /// active mappings on load/history movement; it does not create new chunks.
    fn restore(&mut self, sources: &[AudioSource], chunks: &[ChunkAudioMapping]);
    fn availability(&self, id: &str) -> Result<(), BackendError>;
    fn start_replay(
        &mut self,
        request: ReplayRequest<'_>,
        player: &mut dyn AudioPlayer,
    ) -> Result<(), BackendError>;
}

#[derive(Debug, Clone)]
pub struct CorrectionContext {
    pub prefix: String,
    pub chosen_token_id: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct ChunkRecognitionRequest {
    pub chunk_id: String,
    pub previous_id: String,
    pub revision: u64,
    pub settings: TranscriptionSettings,
    pub correction: Option<CorrectionContext>,
}

pub trait RecognitionBackend {
    fn transcribe_recording(
        &mut self,
        audio: &mut dyn AudioBackend,
        recording_id: &str,
        model: &Path,
        config: TranscriptionConfig,
    ) -> Result<InitialTranscriptionResult, BackendError>;
    fn transcribe_chunk(
        &mut self,
        audio: &mut dyn AudioBackend,
        request: ChunkRecognitionRequest,
    ) -> Result<Transcription, BackendError>;
}

#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error("audio source '{0}' is missing")]
    UnknownRecording(String),
    #[error("unknown chunk '{0}'")]
    UnknownChunk(String),
    #[error("audio source '{0}' has no local path")]
    NoLocalPath(String),
    #[error("audio source '{id}' is unavailable at {}", path.display())]
    Unavailable {
        id: String,
        path: std::path::PathBuf,
    },
    #[error("{0}")]
    Audio(#[from] crate::chunking::WavError),
    #[error("{0}")]
    Model(TranscriptionError),
    #[error("transcription requires a model: start with --model MODEL or use: model PATH")]
    MissingModel,
    #[error("{0}")]
    Recognition(#[from] TranscriptionError),
    #[error("playback failed: {0}")]
    Playback(#[from] PlaybackError),
    #[error("{0}")]
    Other(String),
}
