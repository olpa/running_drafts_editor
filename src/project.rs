//! Complete working state for editing one document.

use std::ops::Deref;

use serde::{Deserialize, Serialize};

use crate::{
    document::{Document, Paragraph},
    transcription::{
        Chunk, DecodeSpan, DecodeSpanItem, InitialTranscriptionEvidence, Transcription,
    },
};

pub use crate::document::{
    AlignmentState, AttentionMark, AudioSource, ChunkAudioMapping, ResolvedIssue,
    TokenAlignmentFailure, TokenAudioMapping,
};

/// Experimental project format; older unreleased formats are not supported.
pub const PROJECT_SCHEMA: &str = "rde-project/v3-experimental";

fn is_zero(value: &u64) -> bool {
    *value == 0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub(crate) schema: String,
    /// Materialized editing projection. Decode-span content is authoritative
    /// for chunk order and paragraph breaks; persistence validates this cache.
    /// Issue #67 will define the final mixed-content history representation.
    #[serde(flatten)]
    pub(crate) document: Document,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) audio_sources: Vec<AudioSource>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) chunk_audio_mappings: Vec<ChunkAudioMapping>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) token_audio_mappings: Vec<TokenAudioMapping>,
    #[serde(default)]
    pub(crate) initial_evidence: Option<InitialTranscriptionEvidence>,
    pub(crate) settings: TranscriptionSettings,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) resolved_issues: Vec<ResolvedIssue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) attention_marks: Vec<AttentionMark>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) edit_history: Vec<EditHistoryEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) redo_history: Vec<EditableProjectState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct EditHistoryEntry {
    pub(crate) before: EditableProjectState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct EditableProjectState {
    pub(crate) settings: TranscriptionSettings,
    /// Temporary history snapshot of the materialized Document projection.
    pub(crate) paragraphs: Vec<Paragraph>,
    #[serde(default)]
    pub(crate) current_transcriptions: Vec<(String, String)>,
    pub(crate) chunk_audio_mappings: Vec<ChunkAudioMapping>,
    pub(crate) token_audio_mappings: Vec<TokenAudioMapping>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub(crate) next_structure_id: u64,
    #[serde(default)]
    pub(crate) resolved_issues: Vec<ResolvedIssue>,
    #[serde(default)]
    pub(crate) attention_marks: Vec<AttentionMark>,
}

/// Settings used for the next transcription, restored with project history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptionSettings {
    pub model: Option<std::path::PathBuf>,
    pub language: String,
}
impl Default for TranscriptionSettings {
    fn default() -> Self {
        Self {
            model: None,
            language: "auto".into(),
        }
    }
}

impl Project {
    pub fn decode_spans(&self) -> &[DecodeSpan] {
        self.initial_evidence
            .as_ref()
            .map_or(&[], |evidence| evidence.decode_spans.as_slice())
    }

    pub fn chunks(&self) -> impl Iterator<Item = &Chunk> {
        self.initial_evidence
            .iter()
            .flat_map(|evidence| &evidence.decode_spans)
            .flat_map(|span| &span.content)
            .filter_map(|item| match item {
                DecodeSpanItem::Chunk(chunk) => Some(chunk),
                DecodeSpanItem::ParagraphBreak(_) => None,
            })
    }

    pub fn chunk(&self, chunk_id: &str) -> Option<&Chunk> {
        self.chunks().find(|chunk| chunk.id == chunk_id)
    }

    pub(crate) fn chunk_mut(&mut self, chunk_id: &str) -> Option<&mut Chunk> {
        self.initial_evidence
            .as_mut()?
            .decode_spans
            .iter_mut()
            .flat_map(|span| &mut span.content)
            .filter_map(|item| match item {
                DecodeSpanItem::Chunk(chunk) => Some(chunk),
                DecodeSpanItem::ParagraphBreak(_) => None,
            })
            .find(|chunk| chunk.id == chunk_id)
    }

    pub fn transcriptions(&self) -> Vec<&Transcription> {
        self.chunks()
            .flat_map(|chunk| &chunk.transcriptions)
            .collect()
    }

    pub fn settings(&self) -> &TranscriptionSettings {
        &self.settings
    }
    /// Configure the initial session before any user actions.
    pub fn configure_initial_settings(
        &mut self,
        model: Option<std::path::PathBuf>,
        language: String,
    ) -> Result<(), String> {
        if self
            .transcriptions()
            .iter()
            .any(|t| t.config.language != language)
        {
            return Err("initial language must match the initial transcriptions".into());
        }
        if !self.edit_history.is_empty()
            || !self.redo_history.is_empty()
            || self
                .transcriptions()
                .iter()
                .any(|t| t.previous_id.is_some())
        {
            return Err("initial settings cannot replace settings after a user action".into());
        }
        self.settings = TranscriptionSettings { model, language };
        Ok(())
    }

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
