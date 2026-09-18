//! The editable document composition and its token-oriented visible projection.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::chunking::SampleRange;

use crate::project::{EditHistoryEntry, EditableProjectState};
use crate::transcription::{
    ChunkBoundaryReason, DecodedSegment, InitialTranscriptionResult, TranscriptionChunk,
};

fn is_zero(value: &u64) -> bool {
    *value == 0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParagraphSplitOutcome {
    pub right_paragraph: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParagraphMergeOutcome {
    pub paragraph: usize,
    pub first_right_token: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StructureEditError {
    #[error("unknown paragraph {0}")]
    UnknownParagraph(usize),
    #[error("unknown chunk {paragraph}.{marker}")]
    UnknownMarker { paragraph: usize, marker: usize },
    #[error("paragraph {0} has no following paragraph")]
    NoFollowingParagraph(usize),
    #[error("the final chunk marker cannot split a paragraph")]
    FinalMarker,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    pub(crate) id: String,
    pub(crate) paragraphs: Vec<Paragraph>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub(crate) next_structure_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttentionMark {
    pub(crate) chunk_id: String,
    pub(crate) token_identity: TokenIdentity,
}

impl AttentionMark {
    pub fn chunk_id(&self) -> &str {
        &self.chunk_id
    }

    pub fn token_identity(&self) -> &TokenIdentity {
        &self.token_identity
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedIssue {
    token_identities: Vec<TokenIdentity>,
}

impl ResolvedIssue {
    pub fn token_identities(&self) -> &[TokenIdentity] {
        &self.token_identities
    }
}

impl Document {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn paragraphs(&self) -> &[Paragraph] {
        &self.paragraphs
    }

    pub fn paragraph(&self, paragraph: usize) -> Option<&Paragraph> {
        self.paragraphs.get(paragraph.checked_sub(1)?)
    }

    pub fn token(&self, paragraph: usize, token: usize) -> Option<&TextToken> {
        self.paragraph(paragraph)?.tokens.get(token.checked_sub(1)?)
    }

    /// Returns the paragraph-wide token bounds of a chunk as a half-open range.
    pub fn chunk_token_bounds(&self, paragraph: usize, chunk: usize) -> Option<(usize, usize)> {
        let paragraph = self.paragraph(paragraph)?;
        let index = chunk.checked_sub(1)?;
        let end = paragraph.chunk_boundaries.get(index)?.after_tokens;
        let start = index.checked_sub(1).map_or(0, |previous| {
            paragraph.chunk_boundaries[previous].after_tokens
        });
        Some((start, end))
    }

    pub fn chunk_token_count(&self, paragraph: usize, chunk: usize) -> Option<usize> {
        self.chunk_token_bounds(paragraph, chunk)
            .map(|(start, end)| end - start)
    }

    pub fn chunk_token(&self, paragraph: usize, chunk: usize, token: usize) -> Option<&TextToken> {
        let (start, end) = self.chunk_token_bounds(paragraph, chunk)?;
        let index = start.checked_add(token.checked_sub(1)?)?;
        (index < end).then(|| &self.paragraph(paragraph).unwrap().tokens[index])
    }

    pub fn chunk_has_tokens(&self, paragraph: usize, chunk: usize) -> Option<bool> {
        self.chunk_token_count(paragraph, chunk)
            .map(|count| count > 0)
    }

    pub fn paragraph_token_number(
        &self,
        paragraph: usize,
        chunk: usize,
        token: usize,
    ) -> Option<usize> {
        let (start, end) = self.chunk_token_bounds(paragraph, chunk)?;
        let index = start.checked_add(token.checked_sub(1)?)?;
        (index < end).then_some(index + 1)
    }

    pub fn chunk_token_address(&self, paragraph: usize, token: usize) -> Option<(usize, usize)> {
        let paragraph_value = self.paragraph(paragraph)?;
        if token == 0 || token > paragraph_value.tokens.len() {
            return None;
        }
        let mut start = 0;
        for (chunk, marker) in paragraph_value.chunk_boundaries.iter().enumerate() {
            if token <= marker.after_tokens {
                return Some((chunk + 1, token - start));
            }
            start = marker.after_tokens;
        }
        None
    }

    pub fn chunk_marker(&self, paragraph: usize, marker: usize) -> Option<&ChunkBoundaryMarker> {
        self.paragraph(paragraph)?
            .chunk_boundaries
            .get(marker.checked_sub(1)?)
    }

    pub fn marker_address_for_chunk(&self, chunk_id: &str) -> Option<(usize, usize)> {
        self.paragraphs
            .iter()
            .enumerate()
            .find_map(|(paragraph, value)| {
                value
                    .chunk_boundaries
                    .iter()
                    .position(|marker| marker.chunk_id == chunk_id)
                    .map(|marker| (paragraph + 1, marker + 1))
            })
    }

    fn new_structure_id(&mut self, kind: &str) -> String {
        self.next_structure_id = self.next_structure_id.saturating_add(1);
        format!("{kind}:{}:{}", self.id, self.next_structure_id)
    }

    /// Split a paragraph between two complete chunks.
    pub fn split_paragraph(
        &mut self,
        paragraph_number: usize,
        marker_number: usize,
    ) -> Result<ParagraphSplitOutcome, StructureEditError> {
        let index = paragraph_number
            .checked_sub(1)
            .ok_or(StructureEditError::UnknownParagraph(paragraph_number))?;
        let paragraph = self
            .paragraphs
            .get(index)
            .ok_or(StructureEditError::UnknownParagraph(paragraph_number))?
            .clone();
        let marker_index = marker_number
            .checked_sub(1)
            .filter(|value| *value < paragraph.chunk_boundaries.len())
            .ok_or(StructureEditError::UnknownMarker {
                paragraph: paragraph_number,
                marker: marker_number,
            })?;
        if marker_index + 1 == paragraph.chunk_boundaries.len() {
            return Err(StructureEditError::FinalMarker);
        }
        let boundary = paragraph.chunk_boundaries[marker_index].after_tokens;
        let left = Paragraph {
            id: self.new_structure_id("paragraph"),
            revision: 1,
            tokens: paragraph.tokens[..boundary].to_vec(),
            chunk_boundaries: paragraph.chunk_boundaries[..=marker_index].to_vec(),
        };
        let right = Paragraph {
            id: self.new_structure_id("paragraph"),
            revision: 1,
            tokens: paragraph.tokens[boundary..].to_vec(),
            chunk_boundaries: paragraph.chunk_boundaries[marker_index + 1..]
                .iter()
                .cloned()
                .map(|mut marker| {
                    marker.after_tokens -= boundary;
                    marker
                })
                .collect(),
        };
        self.paragraphs.splice(index..=index, [left, right]);
        Ok(ParagraphSplitOutcome {
            right_paragraph: paragraph_number + 1,
        })
    }

    /// Merge two adjacent paragraphs without splitting or joining chunks.
    pub fn merge_paragraphs(
        &mut self,
        paragraph_number: usize,
    ) -> Result<ParagraphMergeOutcome, StructureEditError> {
        let index = paragraph_number
            .checked_sub(1)
            .ok_or(StructureEditError::UnknownParagraph(paragraph_number))?;
        let left = self
            .paragraphs
            .get(index)
            .ok_or(StructureEditError::UnknownParagraph(paragraph_number))?
            .clone();
        let right = self
            .paragraphs
            .get(index + 1)
            .ok_or(StructureEditError::NoFollowingParagraph(paragraph_number))?
            .clone();
        let left_count = left.tokens.len();
        let mut tokens = left.tokens;
        tokens.extend(right.tokens);
        let mut markers = left.chunk_boundaries;
        markers.extend(right.chunk_boundaries.into_iter().map(|mut marker| {
            marker.after_tokens += left_count;
            marker
        }));
        let merged = Paragraph {
            id: self.new_structure_id("paragraph"),
            revision: 1,
            tokens,
            chunk_boundaries: markers,
        };
        self.paragraphs.splice(index..=index + 1, [merged]);
        Ok(ParagraphMergeOutcome {
            paragraph: paragraph_number,
            first_right_token: left_count + 1,
        })
    }
}

// Construction and mutations which need transcription, audio, issue, mark, or
// history state belong to `Project`. The implementation remains in this file
// temporarily so the visible projection and its migration helpers stay close.
impl crate::project::Project {
    pub fn from_initial_transcription(run: &InitialTranscriptionResult) -> Self {
        Self::from_initial_transcription_with_source(run, None::<&Path>)
    }

    pub fn from_initial_transcription_with_source(
        run: &InitialTranscriptionResult,
        path: Option<impl AsRef<Path>>,
    ) -> Self {
        let mut document = Self::from_evidence(&run.id, &run.segments, &run.chunks);
        let source_id = format!("audio:{}", run.source.sha256);
        document.audio_sources.push(AudioSource {
            id: source_id.clone(),
            path: path.map(|value| value.as_ref().to_path_buf()),
            sha256: Some(run.source.sha256.clone()),
            canonical_sample_count: Some(run.source.decoded_sample_count),
        });
        document.chunk_audio_mappings = run
            .chunks
            .iter()
            .map(|chunk| ChunkAudioMapping {
                chunk_id: chunk.id.clone(),
                source_id: source_id.clone(),
                range: chunk.audio_range,
                alignment: AlignmentState::Exact,
            })
            .collect();
        document.token_audio_mappings = document
            .paragraphs
            .iter()
            .flat_map(|paragraph| {
                paragraph.tokens.iter().filter_map(|visible| {
                    let TokenIdentity {
                        segment_id,
                        token_index,
                        ..
                    } = &visible.id;
                    let range = run
                        .segments
                        .iter()
                        .find(|segment| &segment.id == segment_id)?
                        .tokens
                        .get(*token_index)?
                        .audio_range?;
                    Some(TokenAudioMapping {
                        paragraph_id: paragraph.id.clone(),
                        paragraph_revision: paragraph.revision,
                        token_identity: visible.id.clone(),
                        source_id: source_id.clone(),
                        range,
                        alignment: AlignmentState::Exact,
                    })
                })
            })
            .collect();
        document.transcriptions = run
            .chunks
            .iter()
            .map(|c| run.transcription_for(c, &c.id, None))
            .collect();
        document.initial_evidence = Some(run.evidence());
        document.settings.language = run.config.language.clone();
        document
    }

    pub(crate) fn from_evidence(
        run_id: &str,
        segments: &[DecodedSegment],
        chunks: &[TranscriptionChunk],
    ) -> Self {
        let segments = segments
            .iter()
            .map(|segment| (segment.id.as_str(), segment))
            .collect::<HashMap<_, _>>();
        let mut paragraphs = Vec::new();
        let mut paragraph_chunks = Vec::new();

        for chunk in chunks {
            paragraph_chunks.push(chunk);
            if matches!(
                chunk.boundary.reason,
                ChunkBoundaryReason::LongPause | ChunkBoundaryReason::SourceEnd
            ) {
                paragraphs.push(Paragraph::from_chunks(run_id, &paragraph_chunks, &segments));
                paragraph_chunks.clear();
            }
        }
        if !paragraph_chunks.is_empty() {
            paragraphs.push(Paragraph::from_chunks(run_id, &paragraph_chunks, &segments));
        }

        Self {
            schema: crate::project::PROJECT_SCHEMA.into(),
            document: Document {
                id: format!("document:{run_id}"),
                paragraphs,
                next_structure_id: 0,
            },
            audio_sources: Vec::new(),
            chunk_audio_mappings: Vec::new(),
            token_audio_mappings: Vec::new(),
            transcriptions: Vec::new(),
            initial_evidence: None,
            settings: crate::project::TranscriptionSettings::default(),
            resolved_issues: Vec::new(),
            attention_marks: Vec::new(),
            edit_history: Vec::new(),
            redo_history: Vec::new(),
        }
    }

    pub fn token_alignment_failures(&self) -> Vec<TokenAlignmentFailure> {
        self.paragraphs()
            .iter()
            .flat_map(|p| p.chunk_boundaries())
            .filter_map(|c| {
                let t = self
                    .transcriptions
                    .iter()
                    .find(|t| t.id == c.transcription_id())?;
                let text = t
                    .segments
                    .iter()
                    .flat_map(|s| &s.tokens)
                    .filter(|t| !t.is_special)
                    .map(|t| t.text.as_str())
                    .collect::<String>();
                (text != t.text).then(|| TokenAlignmentFailure {
                    chunk_id: c.chunk_id.clone(),
                    reason: "normal transcription tokens do not reproduce the chunk text".into(),
                })
            })
            .collect()
    }

    pub fn audio_sources(&self) -> &[AudioSource] {
        &self.audio_sources
    }

    pub fn chunk_audio_mappings(&self) -> &[ChunkAudioMapping] {
        &self.chunk_audio_mappings
    }

    pub fn token_audio_mappings(&self) -> &[TokenAudioMapping] {
        &self.token_audio_mappings
    }
    pub fn token_evidence(
        &self,
        id: &TokenIdentity,
    ) -> Option<&crate::transcription::WhisperToken> {
        self.transcriptions
            .iter()
            .find(|t| t.id == id.transcription_id)?
            .segments
            .iter()
            .find(|s| s.id == id.segment_id)?
            .tokens
            .get(id.token_index)
    }
    pub fn transcriptions(&self) -> &[crate::transcription::Transcription] {
        &self.transcriptions
    }
    pub fn current_transcription(
        &self,
        paragraph: usize,
        chunk: usize,
    ) -> Option<&crate::transcription::Transcription> {
        let id = self
            .paragraph(paragraph)?
            .chunk_boundaries
            .get(chunk.checked_sub(1)?)?
            .transcription_id();
        self.transcriptions.iter().find(|t| t.id == id)
    }
    pub fn resolved_issues(&self) -> &[ResolvedIssue] {
        &self.resolved_issues
    }
    pub fn attention_marks(&self) -> &[AttentionMark] {
        &self.attention_marks
    }

    pub fn is_attention_marked(&self, token_identity: &TokenIdentity) -> bool {
        let Some(chunk_id) = self.chunk_id_for_token(token_identity) else {
            return false;
        };
        self.attention_marks
            .iter()
            .any(|mark| mark.chunk_id == chunk_id && mark.token_identity == *token_identity)
    }

    pub fn mark_attention(&mut self, paragraph: usize, token: usize) -> Result<(), String> {
        let token_identity = self
            .token(paragraph, token)
            .ok_or_else(|| format!("unknown token {paragraph}.{token}"))?
            .id()
            .clone();
        let chunk_id = self
            .chunk_for_token(paragraph, token)
            .expect("a current token belongs to a chunk")
            .1
            .to_owned();
        if self.is_attention_marked(&token_identity) {
            return Err(format!("token {paragraph}.{token} is already marked"));
        }
        self.remember_editable_state();
        self.attention_marks.push(AttentionMark {
            chunk_id,
            token_identity,
        });
        Ok(())
    }

    pub fn unmark_attention(&mut self, paragraph: usize, token: usize) -> Result<(), String> {
        let token_identity = self
            .token(paragraph, token)
            .ok_or_else(|| format!("unknown token {paragraph}.{token}"))?
            .id()
            .clone();
        let chunk_id = self
            .chunk_for_token(paragraph, token)
            .expect("a current token belongs to a chunk")
            .1
            .to_owned();
        let Some(index) = self
            .attention_marks
            .iter()
            .position(|mark| mark.chunk_id == chunk_id && mark.token_identity == token_identity)
        else {
            return Err(format!("token {paragraph}.{token} is not marked"));
        };
        self.remember_editable_state();
        self.attention_marks.remove(index);
        Ok(())
    }

    pub fn resolve_issue(&mut self, token_identities: Vec<TokenIdentity>) {
        self.remember_editable_state();
        self.resolved_issues
            .push(ResolvedIssue { token_identities });
    }

    pub fn reopen_issue(&mut self, index: usize) -> bool {
        if index >= self.resolved_issues.len() {
            return false;
        }
        self.remember_editable_state();
        self.resolved_issues.remove(index);
        true
    }

    pub fn edit_history_len(&self) -> usize {
        self.edit_history.len()
    }

    pub fn redo_history_len(&self) -> usize {
        self.redo_history.len()
    }

    fn editable_state(&self) -> EditableProjectState {
        EditableProjectState {
            settings: self.settings.clone(),
            paragraphs: self.paragraphs.clone(),
            next_structure_id: self.next_structure_id,
            chunk_audio_mappings: self.chunk_audio_mappings.clone(),
            token_audio_mappings: self.token_audio_mappings.clone(),
            resolved_issues: self.resolved_issues.clone(),
            attention_marks: self.attention_marks.clone(),
        }
    }

    fn restore_editable_state(&mut self, state: EditableProjectState) {
        self.settings = state.settings;
        self.document.paragraphs = state.paragraphs;
        self.document.next_structure_id = state.next_structure_id;
        self.chunk_audio_mappings = state.chunk_audio_mappings;
        self.token_audio_mappings = state.token_audio_mappings;
        self.resolved_issues = state.resolved_issues;
        self.attention_marks = state.attention_marks;
    }

    fn remember_editable_state(&mut self) {
        self.edit_history.push(EditHistoryEntry {
            before: self.editable_state(),
        });
        self.redo_history.clear();
    }

    pub fn undo(&mut self, count: usize) -> usize {
        let applied = count.min(self.edit_history.len());
        for _ in 0..applied {
            let before = self
                .edit_history
                .pop()
                .expect("the available undo count was checked");
            self.redo_history.push(self.editable_state());
            self.restore_editable_state(before.before);
        }
        applied
    }

    pub fn redo(&mut self, count: usize) -> usize {
        let applied = count.min(self.redo_history.len());
        for _ in 0..applied {
            let after = self
                .redo_history
                .pop()
                .expect("the available redo count was checked");
            self.edit_history.push(EditHistoryEntry {
                before: self.editable_state(),
            });
            self.restore_editable_state(after);
        }
        applied
    }

    pub fn chunk_for_token(&self, paragraph: usize, token: usize) -> Option<(usize, &str)> {
        let paragraph = self.paragraph(paragraph)?;
        if token == 0 || token > paragraph.tokens.len() {
            return None;
        }
        paragraph
            .chunk_boundaries
            .iter()
            .enumerate()
            .find(|(_, marker)| token <= marker.after_tokens)
            .map(|(index, marker)| (index + 1, marker.chunk_id.as_str()))
    }

    fn chunk_id_for_token(&self, token_identity: &TokenIdentity) -> Option<&str> {
        for paragraph in &self.paragraphs {
            let mut start = 0;
            for marker in &paragraph.chunk_boundaries {
                if paragraph.tokens[start..marker.after_tokens]
                    .iter()
                    .any(|token| token.id == *token_identity)
                {
                    return Some(&marker.chunk_id);
                }
                start = marker.after_tokens;
            }
        }
        None
    }

    pub fn install_transcription(
        &mut self,
        paragraph_number: usize,
        marker_number: usize,
        transcription: crate::transcription::Transcription,
        settings: crate::project::TranscriptionSettings,
    ) -> Result<(), String> {
        if transcription.config.language != settings.language {
            return Err("selected language differs from transcription circumstances".into());
        }
        let mut next = self.clone();
        next.remember_editable_state();
        next.install_transcription_inner(paragraph_number, marker_number, transcription)?;
        next.settings = settings;
        crate::persistence::validate(&next).map_err(|error| error.to_string())?;
        *self = next;
        Ok(())
    }

    fn install_transcription_inner(
        &mut self,
        paragraph_number: usize,
        marker_number: usize,
        mut transcription: crate::transcription::Transcription,
    ) -> Result<(), String> {
        if self
            .transcriptions
            .iter()
            .any(|old| old.id == transcription.id)
        {
            return Err("transcription identity is not unique".into());
        }
        let paragraph = self
            .paragraph(paragraph_number)
            .ok_or_else(|| format!("unknown paragraph {paragraph_number}"))?;
        let marker_index = marker_number
            .checked_sub(1)
            .filter(|i| *i < paragraph.chunk_boundaries.len())
            .ok_or_else(|| format!("unknown chunk {paragraph_number}.{marker_number}"))?;
        if paragraph.revision == u64::MAX {
            return Err("paragraph revision cannot be increased".into());
        }
        let start = marker_index
            .checked_sub(1)
            .map_or(0, |i| paragraph.chunk_boundaries[i].after_tokens);
        let end = paragraph.chunk_boundaries[marker_index].after_tokens;
        let chunk_id = paragraph.chunk_boundaries[marker_index].chunk_id.clone();
        let previous_id = paragraph.chunk_boundaries[marker_index]
            .transcription_id
            .clone();
        if transcription.chunk_id != chunk_id
            || transcription.previous_id.as_deref() != Some(previous_id.as_str())
        {
            return Err(
                "transcription target or predecessor differs from the current chunk".into(),
            );
        }
        let original = self
            .current_transcription(paragraph_number, marker_number)
            .ok_or("current transcription is missing")?;
        if transcription.source != original.source
            || transcription.audio_range != original.audio_range
        {
            return Err("chunk audio identity or boundaries changed".into());
        }
        transcription.boundary = original.boundary.clone();
        let paragraph_id = paragraph.id.clone();
        let old_revision = paragraph.revision;
        let source_id = self
            .chunk_audio_mapping(&chunk_id)
            .ok_or("chunk has no audio mapping")?
            .source_id
            .clone();
        let mut tokens = Vec::new();
        for segment in &transcription.segments {
            for (token_index, token) in segment.tokens.iter().enumerate() {
                if !token.is_special {
                    tokens.push(TextToken {
                        id: TokenIdentity {
                            transcription_id: transcription.id.clone(),
                            segment_id: segment.id.clone(),
                            token_index,
                        },
                        text: token.text.clone(),
                        vocabulary_id: token.token_id,
                    });
                }
            }
        }
        let expected = tokens.iter().map(|t| t.text.as_str()).collect::<String>();
        if expected != transcription.text {
            tokens.clear();
        }
        let removed = self.paragraphs[paragraph_number - 1].tokens[start..end]
            .iter()
            .map(|t| t.id.clone())
            .collect::<Vec<_>>();
        self.resolved_issues
            .retain(|issue| !issue.token_identities.iter().any(|id| removed.contains(id)));
        self.attention_marks
            .retain(|mark| mark.chunk_id != chunk_id || !removed.contains(&mark.token_identity));
        let delta = tokens.len() as isize - (end - start) as isize;
        let paragraph = &mut self.document.paragraphs[paragraph_number - 1];
        paragraph.tokens.splice(start..end, tokens);
        paragraph.chunk_boundaries[marker_index].text = transcription.text.clone();
        paragraph.chunk_boundaries[marker_index].transcription_id = transcription.id.clone();
        for marker in &mut paragraph.chunk_boundaries[marker_index..] {
            marker.after_tokens = marker
                .after_tokens
                .checked_add_signed(delta)
                .ok_or("invalid marker adjustment")?;
        }
        paragraph.revision += 1;
        self.token_audio_mappings
            .retain(|m| !removed.contains(&m.token_identity));
        for mapping in &mut self.token_audio_mappings {
            if mapping.paragraph_id == paragraph_id && mapping.paragraph_revision == old_revision {
                mapping.paragraph_revision += 1;
            }
        }
        for segment in &transcription.segments {
            for (token_index, token) in segment.tokens.iter().enumerate() {
                let id = TokenIdentity {
                    transcription_id: transcription.id.clone(),
                    segment_id: segment.id.clone(),
                    token_index,
                };
                if !token.is_special && expected == transcription.text {
                    if let Some(range) = token.audio_range.filter(|r| {
                        !r.is_empty()
                            && r.start_sample >= transcription.audio_range.start_sample
                            && r.end_sample <= transcription.audio_range.end_sample
                    }) {
                        self.token_audio_mappings.push(TokenAudioMapping {
                            paragraph_id: paragraph_id.clone(),
                            paragraph_revision: old_revision + 1,
                            token_identity: id,
                            source_id: source_id.clone(),
                            range,
                            alignment: AlignmentState::Exact,
                        });
                    }
                }
            }
        }
        self.transcriptions.push(transcription);
        Ok(())
    }

    pub fn alternatives(
        &self,
        paragraph: usize,
        token: usize,
    ) -> Option<&[crate::transcription::TokenAlternative]> {
        Some(
            &self
                .token_evidence(self.token(paragraph, token)?.id())?
                .alternatives,
        )
    }
    pub fn alternative_token_id(
        &self,
        paragraph: usize,
        token: usize,
        candidate: usize,
    ) -> Option<i32> {
        self.alternatives(paragraph, token)?
            .get(candidate.checked_sub(1)?)
            .map(|a| a.token_id)
    }
    pub fn audio_source(&self, source_id: &str) -> Option<&AudioSource> {
        self.audio_sources
            .iter()
            .find(|source| source.id == source_id)
    }

    pub fn audio_mapping(&self, chunk_id: &str) -> Option<(&AudioSource, SampleRange)> {
        let mapping = self
            .chunk_audio_mappings
            .iter()
            .find(|value| value.chunk_id == chunk_id)?;
        let source = self
            .audio_sources
            .iter()
            .find(|value| value.id == mapping.source_id)?;
        Some((source, mapping.range))
    }

    pub fn chunk_audio_mapping(&self, chunk_id: &str) -> Option<&ChunkAudioMapping> {
        self.chunk_audio_mappings
            .iter()
            .find(|mapping| mapping.chunk_id == chunk_id)
    }

    pub fn split_paragraph(
        &mut self,
        paragraph_number: usize,
        marker_number: usize,
    ) -> Result<ParagraphSplitOutcome, StructureEditError> {
        let mut next = self.clone();
        let old_id = next
            .paragraph(paragraph_number)
            .ok_or(StructureEditError::UnknownParagraph(paragraph_number))?
            .id
            .clone();
        next.remember_editable_state();
        let outcome = next
            .document
            .split_paragraph(paragraph_number, marker_number)?;
        let left = &next.document.paragraphs[paragraph_number - 1];
        let right = &next.document.paragraphs[paragraph_number];
        let destinations = [(&left.id, &left.tokens), (&right.id, &right.tokens)];
        for mapping in &mut next.token_audio_mappings {
            if mapping.paragraph_id != old_id {
                continue;
            }
            if let Some((id, _)) = destinations.iter().find(|(_, tokens)| {
                tokens
                    .iter()
                    .any(|token| token.id == mapping.token_identity)
            }) {
                mapping.paragraph_id = (*id).clone();
                mapping.paragraph_revision = 1;
            }
        }
        *self = next;
        Ok(outcome)
    }

    pub fn merge_paragraphs(
        &mut self,
        paragraph_number: usize,
    ) -> Result<ParagraphMergeOutcome, StructureEditError> {
        let mut next = self.clone();
        let index = paragraph_number
            .checked_sub(1)
            .ok_or(StructureEditError::UnknownParagraph(paragraph_number))?;
        let left_id = next
            .document
            .paragraphs
            .get(index)
            .ok_or(StructureEditError::UnknownParagraph(paragraph_number))?
            .id
            .clone();
        let right_id = next
            .document
            .paragraphs
            .get(index + 1)
            .ok_or(StructureEditError::NoFollowingParagraph(paragraph_number))?
            .id
            .clone();
        next.remember_editable_state();
        let outcome = next.document.merge_paragraphs(paragraph_number)?;
        let merged = &next.document.paragraphs[index];
        let merged_id = merged.id.clone();
        for mapping in &mut next.token_audio_mappings {
            if mapping.paragraph_id == left_id || mapping.paragraph_id == right_id {
                mapping.paragraph_id.clone_from(&merged_id);
                mapping.paragraph_revision = 1;
            }
        }
        *self = next;
        Ok(outcome)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Paragraph {
    id: String,
    revision: u64,
    tokens: Vec<TextToken>,
    chunk_boundaries: Vec<ChunkBoundaryMarker>,
}

impl Paragraph {
    fn from_chunks(
        run_id: &str,
        chunks: &[&TranscriptionChunk],
        segments: &HashMap<&str, &DecodedSegment>,
    ) -> Self {
        let mut tokens = Vec::new();
        let mut chunk_boundaries = Vec::with_capacity(chunks.len());
        for chunk in chunks {
            match transcription_tokens(run_id, chunk, segments) {
                Ok(chunk_tokens) => tokens.extend(chunk_tokens),
                Err(_) => {
                    // Keep current text without inventing tokens.
                }
            }
            chunk_boundaries.push(ChunkBoundaryMarker {
                chunk_id: chunk.id.clone(),
                after_tokens: tokens.len(),
                transcription_id: format!("{run_id}:{}", chunk.id),
                text: chunk.text.clone(),
            });
        }
        let first_chunk = chunks
            .first()
            .expect("paragraphs are built from at least one chunk");
        Self {
            id: format!("paragraph:{run_id}:{}", first_chunk.id),
            revision: 1,
            tokens,
            chunk_boundaries,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn text(&self) -> String {
        self.chunk_boundaries
            .iter()
            .map(|c| c.text.as_str())
            .collect()
    }

    pub fn tokens(&self) -> &[TextToken] {
        &self.tokens
    }

    pub fn chunk_boundaries(&self) -> &[ChunkBoundaryMarker] {
        &self.chunk_boundaries
    }
}

fn transcription_tokens(
    run_id: &str,
    chunk: &TranscriptionChunk,
    segments: &HashMap<&str, &DecodedSegment>,
) -> Result<Vec<TextToken>, String> {
    let mut result = Vec::new();
    let mut text = String::new();
    for segment_id in &chunk.segment_ids {
        let segment = segments
            .get(segment_id.as_str())
            .ok_or_else(|| format!("accepted segment '{segment_id}' is unavailable"))?;
        for (token_index, token) in segment.tokens.iter().enumerate() {
            if token.is_special {
                continue;
            }
            text.push_str(&token.text);
            result.push(TextToken {
                id: TokenIdentity {
                    transcription_id: format!("{run_id}:{}", chunk.id),
                    segment_id: segment.id.clone(),
                    token_index,
                },
                text: token.text.clone(),
                vocabulary_id: token.token_id,
            });
        }
    }
    if text != chunk.text {
        return Err("normal transcription tokens do not reproduce the chunk text".into());
    }
    Ok(result)
}

/// Stable identity of one Whisper token occurrence, not its vocabulary ID.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TokenIdentity {
    pub transcription_id: String,
    pub segment_id: String,
    pub token_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextToken {
    id: TokenIdentity,
    text: String,
    vocabulary_id: i32,
}

impl TextToken {
    pub fn id(&self) -> &TokenIdentity {
        &self.id
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn vocabulary_id(&self) -> i32 {
        self.vocabulary_id
    }
    pub fn kind_label(&self) -> &'static str {
        "whisper"
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkBoundaryMarker {
    chunk_id: String,
    after_tokens: usize,
    transcription_id: String,
    text: String,
}

impl ChunkBoundaryMarker {
    pub fn chunk_id(&self) -> &str {
        &self.chunk_id
    }

    pub fn transcription_id(&self) -> &str {
        &self.transcription_id
    }
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn after_tokens(&self) -> usize {
        self.after_tokens
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioSource {
    id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    canonical_sample_count: Option<u64>,
}

impl AudioSource {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn canonical_sample_count(&self) -> Option<u64> {
        self.canonical_sample_count
    }
    pub fn sha256(&self) -> Option<&str> {
        self.sha256.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkAudioMapping {
    chunk_id: String,
    source_id: String,
    range: SampleRange,
    #[serde(default = "exact_alignment")]
    alignment: AlignmentState,
}

fn exact_alignment() -> AlignmentState {
    AlignmentState::Exact
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlignmentState {
    Exact,
    Aligned,
    Inherited,
    Stale,
    Unavailable,
}

impl std::fmt::Display for AlignmentState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Exact => "exact",
            Self::Aligned => "aligned",
            Self::Inherited => "inherited",
            Self::Stale => "stale",
            Self::Unavailable => "unavailable",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenAudioMapping {
    paragraph_id: String,
    paragraph_revision: u64,
    token_identity: TokenIdentity,
    source_id: String,
    range: SampleRange,
    alignment: AlignmentState,
}

impl TokenAudioMapping {
    pub fn paragraph_id(&self) -> &str {
        &self.paragraph_id
    }
    pub fn paragraph_revision(&self) -> u64 {
        self.paragraph_revision
    }
    pub fn token_identity(&self) -> &TokenIdentity {
        &self.token_identity
    }
    pub fn source_id(&self) -> &str {
        &self.source_id
    }
    pub fn range(&self) -> SampleRange {
        self.range
    }
    pub fn alignment(&self) -> AlignmentState {
        self.alignment
    }
}

impl ChunkAudioMapping {
    pub fn chunk_id(&self) -> &str {
        &self.chunk_id
    }

    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    pub fn range(&self) -> SampleRange {
        self.range
    }

    pub fn alignment(&self) -> AlignmentState {
        self.alignment
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenAlignmentFailure {
    chunk_id: String,
    reason: String,
}

impl TokenAlignmentFailure {
    pub fn chunk_id(&self) -> &str {
        &self.chunk_id
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }
}
