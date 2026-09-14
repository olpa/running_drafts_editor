//! Complete working state for editing one document.

use std::ops::Deref;

use serde::{Deserialize, Serialize};

use crate::{
    document::{Document, Paragraph},
    recognition::RecognitionRun,
};

pub use crate::document::{
    AlignmentState, AttentionMark, AudioSource, ChunkAudioMapping, RecognitionAlternative,
    RecognitionTokenEvidence, ReplayChunk, ResolvedIssue, TokenAudioMapping, TokenFallback,
};

/// The historical on-disk name is retained because the v1 representation is
/// unchanged. `Project` is now the Rust aggregate represented by that schema.
pub const PROJECT_SCHEMA: &str = "rde-document/v1-experimental";

/// Compatibility name for callers that inspect legacy files.
#[deprecated(note = "use project::PROJECT_SCHEMA")]
pub const DOCUMENT_SCHEMA: &str = PROJECT_SCHEMA;

fn is_zero(value: &u64) -> bool {
    *value == 0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub(crate) schema: String,
    #[serde(flatten)]
    pub(crate) document: Document,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) audio_sources: Vec<AudioSource>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) chunk_audio_mappings: Vec<ChunkAudioMapping>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) token_audio_mappings: Vec<TokenAudioMapping>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) replay_chunks: Vec<ReplayChunk>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) recognition_token_evidence: Vec<RecognitionTokenEvidence>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) recognition_runs: Vec<RecognitionRun>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) resolved_issues: Vec<ResolvedIssue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) attention_marks: Vec<AttentionMark>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) edit_history: Vec<EditHistoryEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) redo_history: Vec<EditableProjectState>,
    #[serde(skip)]
    pub(crate) token_fallbacks: Vec<TokenFallback>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct EditHistoryEntry {
    pub(crate) before: EditableProjectState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct EditableProjectState {
    pub(crate) paragraphs: Vec<Paragraph>,
    pub(crate) chunk_audio_mappings: Vec<ChunkAudioMapping>,
    pub(crate) token_audio_mappings: Vec<TokenAudioMapping>,
    pub(crate) replay_chunks: Vec<ReplayChunk>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub(crate) next_structure_id: u64,
    #[serde(default)]
    pub(crate) resolved_issues: Vec<ResolvedIssue>,
    #[serde(default)]
    pub(crate) attention_marks: Vec<AttentionMark>,
}

impl Project {
    pub fn schema(&self) -> &str {
        &self.schema
    }

    pub fn document(&self) -> &Document {
        &self.document
    }
}

impl Deref for Project {
    type Target = Document;

    fn deref(&self) -> &Self::Target {
        &self.document
    }
}
