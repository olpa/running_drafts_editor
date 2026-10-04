//! Complete working state for editing one document.

use std::ops::Deref;

use serde::{Deserialize, Serialize};

use crate::{
    document::DocumentView,
    transcription::{Chunk, DecodeSpan, InitialTranscriptionEvidence, Transcription},
};

pub use crate::document::{
    AlignmentState, AttentionMark, AudioSource, ChunkAudioMapping, ResolvedIssue,
    TokenAlignmentFailure, TokenAudioMapping,
};

/// Experimental project format; older unreleased formats are not supported.
pub const PROJECT_SCHEMA: &str = "rde-project/v4-experimental";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub(crate) schema: String,
    pub(crate) document: ProjectDocument,
    #[serde(skip)]
    pub(crate) view: DocumentView,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) audio_sources: Vec<AudioSource>,
    #[serde(skip)]
    pub(crate) chunk_audio_mappings: Vec<ChunkAudioMapping>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) token_audio_mappings: Vec<TokenAudioMapping>,
    #[serde(
        default,
        rename = "_inspection",
        deserialize_with = "ignore_initial_evidence",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) initial_evidence: Option<InitialTranscriptionEvidence>,
    pub(crate) transcription_profiles: Vec<TranscriptionProfile>,
    pub(crate) active_transcription_profile_id: String,
    #[serde(skip)]
    pub(crate) settings: TranscriptionSettings,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) resolved_issues: Vec<ResolvedIssue>,
    #[serde(skip)]
    pub(crate) attention_marks: Vec<AttentionMark>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) edit_history: Vec<EditHistoryEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) redo_history: Vec<EditableProjectState>,
}

fn ignore_initial_evidence<'de, D>(
    deserializer: D,
) -> Result<Option<InitialTranscriptionEvidence>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    serde::de::IgnoredAny::deserialize(deserializer)?;
    Ok(None)
}

impl PartialEq for Project {
    fn eq(&self, other: &Self) -> bool {
        self.schema == other.schema
            && self.document == other.document
            && self.audio_sources == other.audio_sources
            && self.chunk_audio_mappings == other.chunk_audio_mappings
            && self.token_audio_mappings == other.token_audio_mappings
            && self.transcription_profiles == other.transcription_profiles
            && self.active_transcription_profile_id == other.active_transcription_profile_id
            && self.settings == other.settings
            && self.resolved_issues == other.resolved_issues
            && self.attention_marks == other.attention_marks
            && self.edit_history == other.edit_history
            && self.redo_history == other.redo_history
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProjectDocument {
    pub(crate) id: String,
    pub(crate) content: Vec<DocumentItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum DocumentItem {
    Chunk {
        #[serde(flatten)]
        chunk: Box<Chunk>,
    },
    ParagraphBreak,
}

impl<'de> Deserialize<'de> for DocumentItem {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error as _;

        let mut fields = serde_json::Map::<String, serde_json::Value>::deserialize(deserializer)?;
        let item_type = fields
            .remove("type")
            .and_then(|value| value.as_str().map(str::to_owned))
            .ok_or_else(|| D::Error::custom("Document item requires a string 'type' field"))?;
        match item_type.as_str() {
            "chunk" => serde_json::from_value(serde_json::Value::Object(fields))
                .map(|chunk| Self::Chunk {
                    chunk: Box::new(chunk),
                })
                .map_err(D::Error::custom),
            "paragraph_break" if fields.is_empty() => Ok(Self::ParagraphBreak),
            "paragraph_break" => Err(D::Error::custom(format!(
                "unknown field in ParagraphBreak: {}",
                fields.keys().next().expect("the map is not empty")
            ))),
            other => Err(D::Error::unknown_variant(
                other,
                &["chunk", "paragraph_break"],
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EditHistoryEntry {
    pub(crate) before: EditableProjectState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EditableProjectState {
    pub(crate) settings: TranscriptionSettings,
    pub(crate) active_transcription_profile_id: String,
    pub(crate) document: ProjectDocument,
    pub(crate) token_audio_mappings: Vec<TokenAudioMapping>,
    #[serde(default)]
    pub(crate) resolved_issues: Vec<ResolvedIssue>,
}

/// Settings used for the next transcription, restored with project history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptionSettings {
    pub model: Option<std::path::PathBuf>,
    pub language: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TranscriptionProfile {
    pub id: String,
    pub model: Option<std::path::PathBuf>,
    pub config: crate::transcription::TranscriptionConfig,
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
        self.document.content.iter().filter_map(|item| match item {
            DocumentItem::Chunk { chunk } => Some(chunk.as_ref()),
            DocumentItem::ParagraphBreak => None,
        })
    }

    pub fn chunk(&self, chunk_id: &str) -> Option<&Chunk> {
        self.chunks().find(|chunk| chunk.id == chunk_id)
    }

    pub(crate) fn chunk_mut(&mut self, chunk_id: &str) -> Option<&mut Chunk> {
        self.document
            .content
            .iter_mut()
            .filter_map(|item| match item {
                DocumentItem::Chunk { chunk } => Some(chunk.as_mut()),
                DocumentItem::ParagraphBreak => None,
            })
            .find(|chunk| chunk.id == chunk_id)
    }

    pub fn transcriptions(&self) -> Vec<&Transcription> {
        self.chunks()
            .filter_map(Chunk::current_transcription)
            .collect()
    }

    pub fn settings(&self) -> &TranscriptionSettings {
        &self.settings
    }

    pub fn transcription_profiles(&self) -> &[TranscriptionProfile] {
        &self.transcription_profiles
    }

    pub fn transcription_profile(&self, profile_id: &str) -> Option<&TranscriptionProfile> {
        self.transcription_profiles
            .iter()
            .find(|profile| profile.id == profile_id)
    }

    pub fn active_transcription_profile(&self) -> &TranscriptionProfile {
        self.transcription_profile(&self.active_transcription_profile_id)
            .expect("a valid Project always has its active transcription profile")
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
        let profile = self
            .transcription_profiles
            .iter_mut()
            .find(|profile| profile.id == self.active_transcription_profile_id)
            .ok_or("active transcription profile is missing")?;
        profile.model = self.settings.model.clone();
        profile.config.language.clone_from(&self.settings.language);
        Ok(())
    }

    pub fn schema(&self) -> &str {
        &self.schema
    }

    pub fn document(&self) -> &DocumentView {
        &self.view
    }
}

impl Deref for Project {
    type Target = DocumentView;

    fn deref(&self) -> &Self::Target {
        &self.view
    }
}
