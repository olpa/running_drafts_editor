//! The editable document composition and its token-oriented visible projection.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::chunking::SampleRange;

use crate::project::{EditHistoryEntry, EditableProjectState};
use crate::recognition::{ChunkBoundaryReason, DecodedSegment, RecognitionChunk, RecognitionRun};

fn is_zero(value: &u64) -> bool {
    *value == 0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditedTokenPosition {
    pub paragraph: usize,
    pub token: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DocumentEditError {
    #[error("inserted or replacement text cannot be empty")]
    EmptyText,
    #[error("unknown paragraph {0}")]
    UnknownParagraph(usize),
    #[error("unknown token {paragraph}.{token}")]
    UnknownToken { paragraph: usize, token: usize },
    #[error("text-edit ranges cannot cross paragraph boundaries")]
    CrossParagraphRange,
    #[error("token range '{start_paragraph}.{start_token},{end_paragraph}.{end_token}' ends before it starts")]
    ReversedRange {
        start_paragraph: usize,
        start_token: usize,
        end_paragraph: usize,
        end_token: usize,
    },
    #[error("paragraph revision cannot be increased")]
    RevisionOverflow,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AlternativeEditError {
    #[error(transparent)]
    Edit(#[from] DocumentEditError),
    #[error("recognition alternatives are unavailable for this token")]
    Unavailable,
    #[error("unknown alternative {0}")]
    UnknownCandidate(usize),
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
    #[error("unknown chunk marker {paragraph}@{marker}")]
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
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) chunk_id: String,
    pub(crate) token_id: VisibleTokenId,
}

impl AttentionMark {
    pub fn chunk_id(&self) -> &str {
        &self.chunk_id
    }

    pub fn token_id(&self) -> &VisibleTokenId {
        &self.token_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedIssue {
    token_ids: Vec<VisibleTokenId>,
}

impl ResolvedIssue {
    pub fn token_ids(&self) -> &[VisibleTokenId] {
        &self.token_ids
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

    pub fn token(&self, paragraph: usize, token: usize) -> Option<&VisibleToken> {
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

    pub fn chunk_token(
        &self,
        paragraph: usize,
        chunk: usize,
        token: usize,
    ) -> Option<&VisibleToken> {
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
    pub fn from_run(run: &RecognitionRun) -> Self {
        Self::from_run_with_source(run, None::<&Path>)
    }

    pub fn from_run_with_source(run: &RecognitionRun, path: Option<impl AsRef<Path>>) -> Self {
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
                    let VisibleTokenId::Recognition {
                        segment_id,
                        token_index,
                        ..
                    } = &visible.id
                    else {
                        return None;
                    };
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
                        token_id: visible.id.clone(),
                        source_id: source_id.clone(),
                        range,
                        alignment: AlignmentState::Exact,
                    })
                })
            })
            .collect();
        document.recognition_token_evidence = run
            .segments
            .iter()
            .flat_map(|segment| {
                segment
                    .tokens
                    .iter()
                    .enumerate()
                    .map(|(token_index, token)| RecognitionTokenEvidence {
                        token_id: VisibleTokenId::Recognition {
                            run_id: run.id.clone(),
                            segment_id: segment.id.clone(),
                            token_index,
                        },
                        recognition_token_id: token.token_id,
                        probability: token.probability,
                        alternatives: token
                            .alternatives
                            .iter()
                            .map(|candidate| RecognitionAlternative {
                                token_id: candidate.token_id,
                                text: candidate.text.clone(),
                                probability: candidate.probability,
                            })
                            .collect(),
                    })
            })
            .collect();
        document.recognition_runs.push(run.clone());
        document
    }

    pub(crate) fn from_evidence(
        run_id: &str,
        segments: &[DecodedSegment],
        chunks: &[RecognitionChunk],
    ) -> Self {
        let segments = segments
            .iter()
            .map(|segment| (segment.id.as_str(), segment))
            .collect::<HashMap<_, _>>();
        let mut paragraphs = Vec::new();
        let mut paragraph_chunks = Vec::new();
        let mut token_fallbacks = Vec::new();

        for chunk in chunks {
            paragraph_chunks.push(chunk);
            if matches!(
                chunk.boundary.reason,
                ChunkBoundaryReason::LongPause | ChunkBoundaryReason::SourceEnd
            ) {
                paragraphs.push(Paragraph::from_chunks(
                    run_id,
                    &paragraph_chunks,
                    &segments,
                    &mut token_fallbacks,
                ));
                paragraph_chunks.clear();
            }
        }
        if !paragraph_chunks.is_empty() {
            paragraphs.push(Paragraph::from_chunks(
                run_id,
                &paragraph_chunks,
                &segments,
                &mut token_fallbacks,
            ));
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
            replay_chunks: Vec::new(),
            recognition_token_evidence: Vec::new(),
            recognition_runs: Vec::new(),
            resolved_issues: Vec::new(),
            attention_marks: Vec::new(),
            edit_history: Vec::new(),
            redo_history: Vec::new(),
            token_fallbacks,
        }
    }

    pub fn token_fallbacks(&self) -> &[TokenFallback] {
        &self.token_fallbacks
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
    pub fn replay_chunks(&self) -> &[ReplayChunk] {
        &self.replay_chunks
    }
    pub fn recognition_token_evidence(&self) -> &[RecognitionTokenEvidence] {
        &self.recognition_token_evidence
    }
    pub fn recognition_runs(&self) -> &[RecognitionRun] {
        &self.recognition_runs
    }
    pub fn resolved_issues(&self) -> &[ResolvedIssue] {
        &self.resolved_issues
    }
    pub fn attention_marks(&self) -> &[AttentionMark] {
        &self.attention_marks
    }

    pub fn is_attention_marked(&self, token_id: &VisibleTokenId) -> bool {
        let Some(chunk_id) = self.chunk_id_for_visible_token(token_id) else {
            return false;
        };
        self.attention_marks
            .iter()
            .any(|mark| mark.chunk_id == chunk_id && mark.token_id == *token_id)
    }

    pub fn mark_attention(&mut self, paragraph: usize, token: usize) -> Result<(), String> {
        let token_id = self
            .token(paragraph, token)
            .ok_or_else(|| format!("unknown token {paragraph}.{token}"))?
            .id()
            .clone();
        let chunk_id = self
            .chunk_for_token(paragraph, token)
            .expect("a current token belongs to a chunk")
            .1
            .to_owned();
        if self.is_attention_marked(&token_id) {
            return Err(format!("token {paragraph}.{token} is already marked"));
        }
        self.remember_editable_state();
        self.attention_marks
            .push(AttentionMark { chunk_id, token_id });
        Ok(())
    }

    pub fn unmark_attention(&mut self, paragraph: usize, token: usize) -> Result<(), String> {
        let token_id = self
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
            .position(|mark| mark.chunk_id == chunk_id && mark.token_id == token_id)
        else {
            return Err(format!("token {paragraph}.{token} is not marked"));
        };
        self.remember_editable_state();
        self.attention_marks.remove(index);
        Ok(())
    }

    pub fn resolve_issue(&mut self, token_ids: Vec<VisibleTokenId>) {
        self.remember_editable_state();
        self.resolved_issues.push(ResolvedIssue { token_ids });
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
            paragraphs: self.paragraphs.clone(),
            next_structure_id: self.next_structure_id,
            chunk_audio_mappings: self.chunk_audio_mappings.clone(),
            token_audio_mappings: self.token_audio_mappings.clone(),
            replay_chunks: self.replay_chunks.clone(),
            resolved_issues: self.resolved_issues.clone(),
            attention_marks: self.attention_marks.clone(),
        }
    }

    fn restore_editable_state(&mut self, state: EditableProjectState) {
        self.document.paragraphs = state.paragraphs;
        self.document.next_structure_id = state.next_structure_id;
        self.chunk_audio_mappings = state.chunk_audio_mappings;
        self.token_audio_mappings = state.token_audio_mappings;
        self.replay_chunks = state.replay_chunks;
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

    fn chunk_id_for_visible_token(&self, token_id: &VisibleTokenId) -> Option<&str> {
        for paragraph in &self.paragraphs {
            let mut start = 0;
            for marker in &paragraph.chunk_boundaries {
                if paragraph.tokens[start..marker.after_tokens]
                    .iter()
                    .any(|token| token.id == *token_id)
                {
                    return Some(&marker.chunk_id);
                }
                start = marker.after_tokens;
            }
        }
        None
    }

    pub fn install_chunk_recognition(
        &mut self,
        paragraph_number: usize,
        marker_number: usize,
        run: RecognitionRun,
    ) -> Result<(), String> {
        let mut next = self.clone();
        next.remember_editable_state();
        next.install_chunk_recognition_inner(paragraph_number, marker_number, run)?;
        crate::persistence::validate(&next).map_err(|error| error.to_string())?;
        *self = next;
        Ok(())
    }

    fn install_chunk_recognition_inner(
        &mut self,
        paragraph_number: usize,
        marker_number: usize,
        run: RecognitionRun,
    ) -> Result<(), String> {
        if run.chunks.len() != 1 {
            return Err("chunk refresh must contain exactly one chunk".into());
        }
        if self.recognition_runs.iter().any(|old| old.id == run.id) {
            return Err("recognition run ID is not unique".into());
        }
        let paragraph = self
            .paragraph(paragraph_number)
            .ok_or_else(|| format!("unknown paragraph {paragraph_number}"))?;
        let marker_index = marker_number
            .checked_sub(1)
            .filter(|i| *i < paragraph.chunk_boundaries.len())
            .ok_or_else(|| format!("unknown chunk marker {paragraph_number}@{marker_number}"))?;
        if paragraph.revision == u64::MAX {
            return Err("paragraph revision cannot be increased".into());
        }
        let start = marker_index
            .checked_sub(1)
            .map_or(0, |i| paragraph.chunk_boundaries[i].after_tokens);
        let end = paragraph.chunk_boundaries[marker_index].after_tokens;
        let chunk_id = paragraph.chunk_boundaries[marker_index].chunk_id.clone();
        let paragraph_id = paragraph.id.clone();
        let old_revision = paragraph.revision;
        let source_id = self
            .chunk_audio_mapping(&chunk_id)
            .ok_or("chunk has no audio mapping")?
            .source_id
            .clone();
        let mut tokens = Vec::new();
        for segment in &run.segments {
            for (token_index, token) in segment.tokens.iter().enumerate() {
                if !token.is_special {
                    tokens.push(VisibleToken {
                        id: VisibleTokenId::Recognition {
                            run_id: run.id.clone(),
                            segment_id: segment.id.clone(),
                            token_index,
                        },
                        text: token.text.clone(),
                        origin: VisibleTokenOrigin::Recognition,
                    });
                }
            }
        }
        let expected = tokens.iter().map(|t| t.text.as_str()).collect::<String>();
        if expected != run.chunks[0].text {
            return Err("normal recognition tokens do not reproduce refreshed chunk text".into());
        }
        let removed = self.paragraphs[paragraph_number - 1].tokens[start..end]
            .iter()
            .map(|t| t.id.clone())
            .collect::<Vec<_>>();
        self.resolved_issues
            .retain(|issue| !issue.token_ids.iter().any(|id| removed.contains(id)));
        self.attention_marks
            .retain(|mark| mark.chunk_id != chunk_id || !removed.contains(&mark.token_id));
        let new_ids = tokens.iter().map(|t| t.id.clone()).collect::<Vec<_>>();
        let delta = tokens.len() as isize - (end - start) as isize;
        let paragraph = &mut self.document.paragraphs[paragraph_number - 1];
        paragraph.tokens.splice(start..end, tokens);
        for marker in &mut paragraph.chunk_boundaries[marker_index..] {
            marker.after_tokens = marker
                .after_tokens
                .checked_add_signed(delta)
                .ok_or("invalid marker adjustment")?;
        }
        paragraph.revision += 1;
        self.ensure_replay_chunks();
        let replay = self
            .replay_chunks
            .iter_mut()
            .find(|c| c.id == chunk_id)
            .ok_or("chunk has no replay record")?;
        replay.token_ids = new_ids.clone();
        self.token_audio_mappings
            .retain(|m| !removed.contains(&m.token_id));
        for mapping in &mut self.token_audio_mappings {
            if mapping.paragraph_id == paragraph_id && mapping.paragraph_revision == old_revision {
                mapping.paragraph_revision += 1;
            }
        }
        for segment in &run.segments {
            for (token_index, token) in segment.tokens.iter().enumerate() {
                let id = VisibleTokenId::Recognition {
                    run_id: run.id.clone(),
                    segment_id: segment.id.clone(),
                    token_index,
                };
                self.recognition_token_evidence
                    .push(RecognitionTokenEvidence {
                        token_id: id.clone(),
                        recognition_token_id: token.token_id,
                        probability: token.probability,
                        alternatives: token
                            .alternatives
                            .iter()
                            .map(|a| RecognitionAlternative {
                                token_id: a.token_id,
                                text: a.text.clone(),
                                probability: a.probability,
                            })
                            .collect(),
                    });
                if !token.is_special {
                    if let Some(range) = token.audio_range.filter(|r| {
                        !r.is_empty()
                            && r.start_sample >= run.chunks[0].audio_range.start_sample
                            && r.end_sample <= run.chunks[0].audio_range.end_sample
                    }) {
                        self.token_audio_mappings.push(TokenAudioMapping {
                            paragraph_id: paragraph_id.clone(),
                            paragraph_revision: old_revision + 1,
                            token_id: id,
                            source_id: source_id.clone(),
                            range,
                            alignment: AlignmentState::Exact,
                        });
                    }
                }
            }
        }
        self.recognition_runs.push(run);
        Ok(())
    }

    pub fn alternatives(
        &self,
        paragraph: usize,
        token: usize,
    ) -> Option<&[RecognitionAlternative]> {
        let visible = self.token(paragraph, token)?;
        let evidence = self
            .recognition_token_evidence
            .iter()
            .find(|evidence| evidence.token_id == *visible.id())?;
        Some(&evidence.alternatives)
    }
    pub fn alternative_token_id(
        &self,
        paragraph: usize,
        token: usize,
        candidate: usize,
    ) -> Option<i32> {
        let visible = self.token(paragraph, token)?;
        self.recognition_token_evidence
            .iter()
            .find(|e| e.token_id == *visible.id())?
            .alternatives
            .get(candidate.checked_sub(1)?)
            .map(|a| a.token_id)
    }

    pub fn choose_alternative(
        &mut self,
        paragraph: usize,
        token: usize,
        candidate: usize,
    ) -> Result<EditedTokenPosition, AlternativeEditError> {
        let visible = self
            .token(paragraph, token)
            .ok_or(DocumentEditError::UnknownToken { paragraph, token })?;
        let evidence = self
            .recognition_token_evidence
            .iter()
            .find(|evidence| evidence.token_id == *visible.id())
            .ok_or(AlternativeEditError::Unavailable)?;
        let text = evidence
            .alternatives
            .get(candidate.checked_sub(1).unwrap_or(usize::MAX))
            .ok_or(AlternativeEditError::UnknownCandidate(candidate))?
            .text
            .clone();
        self.apply_edit_with_reason(
            paragraph,
            token - 1,
            token,
            Some(text),
            false,
            "recognition alternative",
        )?;
        Ok(EditedTokenPosition { paragraph, token })
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

    fn ensure_replay_chunks(&mut self) {
        if !self.replay_chunks.is_empty() {
            return;
        }
        self.replay_chunks = self
            .paragraphs
            .iter()
            .flat_map(|paragraph| {
                let mut start = 0;
                paragraph.chunk_boundaries.iter().map(move |marker| {
                    let chunk = ReplayChunk {
                        id: marker.chunk_id.clone(),
                        parent_ids: Vec::new(),
                        token_ids: paragraph.tokens[start..marker.after_tokens]
                            .iter()
                            .map(|token| token.id.clone())
                            .collect(),
                    };
                    start = marker.after_tokens;
                    chunk
                })
            })
            .collect();
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
            if let Some((id, _)) = destinations
                .iter()
                .find(|(_, tokens)| tokens.iter().any(|token| token.id == mapping.token_id))
            {
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

    pub fn insert_text(
        &mut self,
        paragraph: usize,
        token: usize,
        after: bool,
        text: String,
    ) -> Result<EditedTokenPosition, DocumentEditError> {
        if text.is_empty() {
            return Err(DocumentEditError::EmptyText);
        }
        let token_count = self.checked_token_count(paragraph, token)?;
        let position = if after { token } else { token - 1 };
        let shift_marker_at_position = after;
        self.apply_edit(
            paragraph,
            position,
            position,
            Some(text),
            shift_marker_at_position,
        )?;
        debug_assert_eq!(
            self.paragraph(paragraph).unwrap().tokens().len(),
            token_count + 1
        );
        Ok(EditedTokenPosition {
            paragraph,
            token: position + 1,
        })
    }

    pub fn replace_text(
        &mut self,
        start_paragraph: usize,
        start_token: usize,
        end_paragraph: usize,
        end_token: usize,
        text: String,
    ) -> Result<EditedTokenPosition, DocumentEditError> {
        if text.is_empty() {
            return Err(DocumentEditError::EmptyText);
        }
        self.checked_range(start_paragraph, start_token, end_paragraph, end_token)?;
        self.apply_edit(
            start_paragraph,
            start_token - 1,
            end_token,
            Some(text),
            false,
        )?;
        Ok(EditedTokenPosition {
            paragraph: start_paragraph,
            token: start_token,
        })
    }

    pub fn delete_text(
        &mut self,
        start_paragraph: usize,
        start_token: usize,
        end_paragraph: usize,
        end_token: usize,
    ) -> Result<Option<EditedTokenPosition>, DocumentEditError> {
        self.checked_range(start_paragraph, start_token, end_paragraph, end_token)?;
        self.apply_edit(start_paragraph, start_token - 1, end_token, None, false)?;
        let remaining = self.paragraph(start_paragraph).unwrap().tokens().len();
        Ok((remaining > 0).then_some(EditedTokenPosition {
            paragraph: start_paragraph,
            token: start_token.min(remaining),
        }))
    }

    fn checked_token_count(
        &self,
        paragraph: usize,
        token: usize,
    ) -> Result<usize, DocumentEditError> {
        let value = self
            .paragraph(paragraph)
            .ok_or(DocumentEditError::UnknownParagraph(paragraph))?;
        if token == 0 || token > value.tokens.len() {
            return Err(DocumentEditError::UnknownToken { paragraph, token });
        }
        Ok(value.tokens.len())
    }

    fn checked_range(
        &self,
        start_paragraph: usize,
        start_token: usize,
        end_paragraph: usize,
        end_token: usize,
    ) -> Result<(), DocumentEditError> {
        if start_paragraph != end_paragraph {
            return Err(DocumentEditError::CrossParagraphRange);
        }
        if start_token > end_token {
            return Err(DocumentEditError::ReversedRange {
                start_paragraph,
                start_token,
                end_paragraph,
                end_token,
            });
        }
        self.checked_token_count(start_paragraph, start_token)?;
        self.checked_token_count(end_paragraph, end_token)?;
        Ok(())
    }

    fn apply_edit(
        &mut self,
        paragraph_number: usize,
        start: usize,
        end_exclusive: usize,
        replacement: Option<String>,
        shift_marker_at_start: bool,
    ) -> Result<(), DocumentEditError> {
        self.apply_edit_with_reason(
            paragraph_number,
            start,
            end_exclusive,
            replacement,
            shift_marker_at_start,
            "user text",
        )
    }

    fn apply_edit_with_reason(
        &mut self,
        paragraph_number: usize,
        start: usize,
        end_exclusive: usize,
        replacement: Option<String>,
        shift_marker_at_start: bool,
        reason: &str,
    ) -> Result<(), DocumentEditError> {
        let old_revision = self
            .paragraphs
            .get(paragraph_number.checked_sub(1).unwrap_or(usize::MAX))
            .ok_or(DocumentEditError::UnknownParagraph(paragraph_number))?
            .revision;
        let new_revision = old_revision
            .checked_add(1)
            .ok_or(DocumentEditError::RevisionOverflow)?;
        self.remember_editable_state();
        let removed = end_exclusive - start;
        let inserted = usize::from(replacement.is_some());
        let (paragraph_id, removed_ids) = {
            let paragraph = self
                .document
                .paragraphs
                .get_mut(paragraph_number.checked_sub(1).unwrap_or(usize::MAX))
                .ok_or(DocumentEditError::UnknownParagraph(paragraph_number))?;

            for marker in &mut paragraph.chunk_boundaries {
                marker.after_tokens = if removed == 0 {
                    if marker.after_tokens > start
                        || (shift_marker_at_start && marker.after_tokens == start)
                    {
                        marker.after_tokens + inserted
                    } else {
                        marker.after_tokens
                    }
                } else if marker.after_tokens <= start {
                    marker.after_tokens
                } else if marker.after_tokens <= end_exclusive {
                    start + inserted
                } else {
                    marker.after_tokens - removed + inserted
                };
            }

            let removed_ids = paragraph.tokens[start..end_exclusive]
                .iter()
                .map(|token| token.id.clone())
                .collect::<std::collections::HashSet<_>>();
            let paragraph_id = paragraph.id.clone();
            let replacement = replacement.map(|text| VisibleToken {
                id: VisibleTokenId::Pseudo {
                    id: format!("edit:{paragraph_id}:{new_revision}"),
                },
                text,
                origin: VisibleTokenOrigin::Pseudo {
                    reason: reason.into(),
                },
            });
            paragraph.tokens.splice(start..end_exclusive, replacement);
            paragraph.revision = new_revision;
            (paragraph_id, removed_ids)
        };
        self.resolved_issues
            .retain(|issue| !issue.token_ids.iter().any(|id| removed_ids.contains(id)));
        self.attention_marks
            .retain(|mark| !removed_ids.contains(&mark.token_id));

        self.token_audio_mappings.retain_mut(|mapping| {
            if mapping.paragraph_id != paragraph_id || mapping.paragraph_revision != old_revision {
                return true;
            }
            if removed_ids.contains(&mapping.token_id) {
                return false;
            }
            mapping.paragraph_revision = new_revision;
            true
        });
        self.sync_current_chunk_memberships();
        Ok(())
    }

    fn sync_current_chunk_memberships(&mut self) {
        if self.replay_chunks.is_empty() {
            return;
        }
        let memberships = self
            .paragraphs
            .iter()
            .flat_map(|paragraph| {
                let mut start = 0;
                paragraph.chunk_boundaries.iter().map(move |marker| {
                    let ids = paragraph.tokens[start..marker.after_tokens]
                        .iter()
                        .map(|token| token.id.clone())
                        .collect::<Vec<_>>();
                    start = marker.after_tokens;
                    (marker.chunk_id.clone(), ids)
                })
            })
            .collect::<HashMap<_, _>>();
        for chunk in &mut self.replay_chunks {
            if let Some(tokens) = memberships.get(&chunk.id) {
                chunk.token_ids.clone_from(tokens);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Paragraph {
    id: String,
    revision: u64,
    tokens: Vec<VisibleToken>,
    chunk_boundaries: Vec<ChunkBoundaryMarker>,
}

impl Paragraph {
    fn from_chunks(
        run_id: &str,
        chunks: &[&RecognitionChunk],
        segments: &HashMap<&str, &DecodedSegment>,
        fallbacks: &mut Vec<TokenFallback>,
    ) -> Self {
        let mut tokens = Vec::new();
        let mut chunk_boundaries = Vec::with_capacity(chunks.len());
        for chunk in chunks {
            match recognition_tokens(run_id, chunk, segments) {
                Ok(chunk_tokens) => tokens.extend(chunk_tokens),
                Err(reason) => {
                    fallbacks.push(TokenFallback {
                        chunk_id: chunk.id.clone(),
                        reason: reason.clone(),
                    });
                    tokens.push(VisibleToken {
                        id: VisibleTokenId::Pseudo {
                            id: format!("fallback:{run_id}:{}", chunk.id),
                        },
                        text: chunk.text.clone(),
                        origin: VisibleTokenOrigin::Pseudo { reason },
                    });
                }
            }
            chunk_boundaries.push(ChunkBoundaryMarker {
                chunk_id: chunk.id.clone(),
                after_tokens: tokens.len(),
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
        self.tokens.iter().map(VisibleToken::text).collect()
    }

    pub fn tokens(&self) -> &[VisibleToken] {
        &self.tokens
    }

    pub fn chunk_boundaries(&self) -> &[ChunkBoundaryMarker] {
        &self.chunk_boundaries
    }
}

fn recognition_tokens(
    run_id: &str,
    chunk: &RecognitionChunk,
    segments: &HashMap<&str, &DecodedSegment>,
) -> Result<Vec<VisibleToken>, String> {
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
            result.push(VisibleToken {
                id: VisibleTokenId::Recognition {
                    run_id: run_id.into(),
                    segment_id: segment.id.clone(),
                    token_index,
                },
                text: token.text.clone(),
                origin: VisibleTokenOrigin::Recognition,
            });
        }
    }
    if text != chunk.text {
        return Err("normal recognition tokens do not reproduce the chunk text".into());
    }
    Ok(result)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VisibleTokenId {
    Recognition {
        run_id: String,
        segment_id: String,
        token_index: usize,
    },
    Pseudo {
        id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VisibleTokenOrigin {
    Recognition,
    Pseudo { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisibleToken {
    id: VisibleTokenId,
    text: String,
    origin: VisibleTokenOrigin,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecognitionAlternative {
    token_id: i32,
    text: String,
    probability: f32,
}

impl Eq for RecognitionAlternative {}

impl RecognitionAlternative {
    pub fn token_id(&self) -> i32 {
        self.token_id
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn probability(&self) -> f32 {
        self.probability
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecognitionTokenEvidence {
    token_id: VisibleTokenId,
    recognition_token_id: i32,
    probability: f32,
    alternatives: Vec<RecognitionAlternative>,
}

impl Eq for RecognitionTokenEvidence {}

impl RecognitionTokenEvidence {
    pub fn token_id(&self) -> &VisibleTokenId {
        &self.token_id
    }
    pub fn probability(&self) -> f32 {
        self.probability
    }
}

impl VisibleToken {
    pub fn id(&self) -> &VisibleTokenId {
        &self.id
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn origin(&self) -> &VisibleTokenOrigin {
        &self.origin
    }

    pub fn kind_label(&self) -> &'static str {
        match self.origin {
            VisibleTokenOrigin::Recognition => "rec",
            VisibleTokenOrigin::Pseudo { .. } => "pseudo",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkBoundaryMarker {
    chunk_id: String,
    after_tokens: usize,
}

impl ChunkBoundaryMarker {
    pub fn chunk_id(&self) -> &str {
        &self.chunk_id
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayChunk {
    id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    parent_ids: Vec<String>,
    token_ids: Vec<VisibleTokenId>,
}

impl ReplayChunk {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn parent_ids(&self) -> &[String] {
        &self.parent_ids
    }
    pub fn token_ids(&self) -> &[VisibleTokenId] {
        &self.token_ids
    }
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
    token_id: VisibleTokenId,
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
    pub fn token_id(&self) -> &VisibleTokenId {
        &self.token_id
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
pub struct TokenFallback {
    chunk_id: String,
    reason: String,
}

impl TokenFallback {
    pub fn chunk_id(&self) -> &str {
        &self.chunk_id
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        chunking::{SampleRange, SourceFacts},
        project::Project,
        recognition::{
            ChunkBoundary, RecognitionConfig, RecognitionRun, RecognitionStatus, RecognitionToken,
            RecognizerIdentity, RECOGNITION_RUN_SCHEMA,
        },
    };

    fn token(text: &str) -> RecognitionToken {
        RecognitionToken {
            token_id: 1,
            text: text.into(),
            probability: 1.0,
            is_special: false,
            audio_range: None,
            alternatives: Vec::new(),
        }
    }

    fn segment(id: &str, text: &str, tokens: Vec<RecognitionToken>) -> DecodedSegment {
        DecodedSegment {
            id: id.into(),
            audio_range: SampleRange {
                start_sample: 0,
                end_sample: 1,
            },
            text: text.into(),
            no_speech_probability: 0.0,
            tokens,
        }
    }

    fn chunk(
        id: &str,
        segment_id: &str,
        text: &str,
        reason: ChunkBoundaryReason,
    ) -> RecognitionChunk {
        RecognitionChunk {
            id: id.into(),
            ordinal: 1,
            segment_ids: vec![segment_id.into()],
            audio_range: SampleRange {
                start_sample: 0,
                end_sample: 1,
            },
            text: text.into(),
            token_count: 1,
            boundary: ChunkBoundary {
                reason,
                pause_samples: None,
            },
        }
    }

    fn run(id: &str, chunk_id: &str, segment_id: &str, text: &str) -> RecognitionRun {
        let segment = segment(segment_id, text, vec![token(text)]);
        RecognitionRun {
            schema: RECOGNITION_RUN_SCHEMA.into(),
            id: id.into(),
            revision: 1,
            source: SourceFacts {
                sha256: "source".into(),
                sample_rate_hz: 16_000,
                channels: 1,
                decoded_sample_count: 1,
            },
            recognizer: RecognizerIdentity {
                name: "test".into(),
                implementation: "test".into(),
                model_sha256: "model".into(),
            },
            config: RecognitionConfig::default(),
            status: RecognitionStatus::Succeeded,
            windows: Vec::new(),
            segments: vec![segment],
            chunks: vec![chunk(
                chunk_id,
                segment_id,
                text,
                ChunkBoundaryReason::SourceEnd,
            )],
        }
    }

    #[test]
    fn builds_paragraphs_from_complete_normal_tokens() {
        let segments = vec![
            segment("s1", "one", vec![token("one")]),
            segment("s2", " two", vec![token(" "), token("two")]),
            segment("s3", " three", vec![token(" three")]),
        ];
        let chunks = vec![
            chunk("a", "s1", "one", ChunkBoundaryReason::StrongPause),
            chunk("b", "s2", " two", ChunkBoundaryReason::LongPause),
            chunk("c", "s3", " three", ChunkBoundaryReason::SourceEnd),
        ];

        let document = Project::from_evidence("run", &segments, &chunks);

        assert_eq!(document.paragraphs().len(), 2);
        assert_eq!(document.paragraphs()[0].text(), "one two");
        assert_eq!(document.paragraphs()[0].tokens().len(), 3);
        assert_eq!(
            document.paragraphs()[0].chunk_boundaries()[0].after_tokens(),
            1
        );
        assert_eq!(
            document.paragraphs()[0].chunk_boundaries()[1].after_tokens(),
            3
        );
        assert!(document.token_fallbacks().is_empty());
    }

    #[test]
    fn mismatched_or_missing_evidence_becomes_one_pseudo_token_per_chunk() {
        let segments = vec![segment("s1", "wrong", vec![token("wrong")])];
        let chunks = vec![
            chunk("a", "s1", "authoritative", ChunkBoundaryReason::StrongPause),
            chunk("b", "missing", " text", ChunkBoundaryReason::SourceEnd),
        ];

        let document = Project::from_evidence("run", &segments, &chunks);
        let paragraph = &document.paragraphs()[0];

        assert_eq!(paragraph.text(), "authoritative text");
        assert_eq!(paragraph.tokens().len(), 2);
        assert!(paragraph
            .tokens()
            .iter()
            .all(|token| matches!(token.origin(), VisibleTokenOrigin::Pseudo { .. })));
        assert_eq!(document.token_fallbacks().len(), 2);
        assert_eq!(paragraph.chunk_boundaries()[0].after_tokens(), 1);
        assert_eq!(paragraph.chunk_boundaries()[1].after_tokens(), 2);
    }

    #[test]
    fn special_tokens_remain_evidence_but_not_visible_tokens() {
        let mut special = token("ignored");
        special.is_special = true;
        let segments = vec![segment("s1", "shown", vec![special, token("shown")])];
        let chunks = vec![chunk("a", "s1", "shown", ChunkBoundaryReason::SourceEnd)];

        let document = Project::from_evidence("run", &segments, &chunks);

        assert_eq!(document.paragraphs()[0].tokens().len(), 1);
        assert_eq!(document.paragraphs()[0].tokens()[0].text(), "shown");
    }

    #[test]
    fn edits_create_one_pseudo_token_and_keep_markers_on_token_boundaries() {
        let segments = vec![
            segment("s1", "a", vec![token("a")]),
            segment("s2", " b c", vec![token(" b"), token(" c")]),
        ];
        let chunks = vec![
            chunk("a", "s1", "a", ChunkBoundaryReason::StrongPause),
            chunk("b", "s2", " b c", ChunkBoundaryReason::SourceEnd),
        ];
        let mut document = Project::from_evidence("run", &segments, &chunks);

        let inserted = document
            .insert_text(1, 2, false, " inserted words".into())
            .unwrap();
        assert_eq!(
            inserted,
            EditedTokenPosition {
                paragraph: 1,
                token: 2
            }
        );
        assert_eq!(document.paragraphs()[0].text(), "a inserted words b c");
        assert_eq!(document.paragraphs()[0].tokens().len(), 4);
        assert_eq!(
            document.paragraphs()[0].chunk_boundaries()[0].after_tokens(),
            1
        );
        assert_eq!(
            document.paragraphs()[0].chunk_boundaries()[1].after_tokens(),
            4
        );

        document
            .replace_text(1, 2, 1, 3, " replacement span".into())
            .unwrap();
        let paragraph = &document.paragraphs()[0];
        assert_eq!(paragraph.text(), "a replacement span c");
        assert_eq!(paragraph.tokens().len(), 3);
        assert_eq!(paragraph.revision(), 3);
        assert!(matches!(
            paragraph.tokens()[1].origin(),
            VisibleTokenOrigin::Pseudo { reason } if reason == "user text"
        ));
        assert_eq!(paragraph.chunk_boundaries()[0].after_tokens(), 1);
        assert_eq!(paragraph.chunk_boundaries()[1].after_tokens(), 3);
    }

    #[test]
    fn append_stays_immediately_before_a_marker_and_delete_can_empty_a_paragraph() {
        let segments = vec![segment("s1", "a", vec![token("a")])];
        let chunks = vec![chunk("a", "s1", "a", ChunkBoundaryReason::SourceEnd)];
        let mut document = Project::from_evidence("run", &segments, &chunks);

        document.insert_text(1, 1, true, " tail".into()).unwrap();
        assert_eq!(document.paragraphs()[0].text(), "a tail");
        assert_eq!(
            document.paragraphs()[0].chunk_boundaries()[0].after_tokens(),
            2
        );

        assert_eq!(document.delete_text(1, 1, 1, 2).unwrap(), None);
        assert!(document.paragraphs()[0].tokens().is_empty());
        assert_eq!(
            document.paragraphs()[0].chunk_boundaries()[0].after_tokens(),
            0
        );
    }

    #[test]
    fn edit_preserves_retained_mappings_at_the_new_revision_and_rejects_cross_paragraphs() {
        let segments = vec![
            segment("s1", "a", vec![token("a")]),
            segment("s2", " b", vec![token(" b")]),
        ];
        let chunks = vec![
            chunk("a", "s1", "a", ChunkBoundaryReason::LongPause),
            chunk("b", "s2", " b", ChunkBoundaryReason::SourceEnd),
        ];
        let mut document = Project::from_evidence("run", &segments, &chunks);
        let first_id = document.paragraphs[0].tokens[0].id.clone();
        document.token_audio_mappings.push(TokenAudioMapping {
            paragraph_id: document.paragraphs[0].id.clone(),
            paragraph_revision: 1,
            token_id: first_id.clone(),
            source_id: "audio".into(),
            range: SampleRange {
                start_sample: 1,
                end_sample: 2,
            },
            alignment: AlignmentState::Exact,
        });
        let before = document.clone();

        assert_eq!(
            document.replace_text(1, 1, 2, 1, "no".into()),
            Err(DocumentEditError::CrossParagraphRange)
        );
        assert_eq!(document, before);

        document.insert_text(1, 1, true, " added".into()).unwrap();
        assert_eq!(document.token_audio_mappings.len(), 1);
        assert_eq!(document.token_audio_mappings[0].token_id, first_id);
        assert_eq!(document.token_audio_mappings[0].paragraph_revision, 2);
        assert!(document
            .token_audio_mappings
            .iter()
            .all(|mapping| !matches!(mapping.token_id, VisibleTokenId::Pseudo { .. })));
    }

    #[test]
    fn invalid_paragraph_structure_operations_do_not_change_the_document() {
        let segments = vec![segment("s1", "a", vec![token("a")])];
        let chunks = vec![chunk("a", "s1", "a", ChunkBoundaryReason::SourceEnd)];
        let mut document = Project::from_evidence("run", &segments, &chunks);
        let original = document.clone();

        assert_eq!(
            document.split_paragraph(1, 1),
            Err(StructureEditError::FinalMarker)
        );
        assert_eq!(document, original);
        assert_eq!(
            document.merge_paragraphs(1),
            Err(StructureEditError::NoFollowingParagraph(1))
        );
        assert_eq!(document, original);
    }

    #[test]
    fn paragraph_structure_edits_preserve_chunk_identity_and_evidence() {
        let segments = vec![
            segment("s1", "one", vec![token("one")]),
            segment("s2", " two", vec![token(" two")]),
        ];
        let chunks = vec![
            chunk("a", "s1", "one", ChunkBoundaryReason::StrongPause),
            chunk("b", "s2", " two", ChunkBoundaryReason::SourceEnd),
        ];
        let mut document = Project::from_evidence("run", &segments, &chunks);
        let evidence = document.recognition_token_evidence.clone();

        document.split_paragraph(1, 1).unwrap();
        assert_eq!(document.paragraphs[0].chunk_boundaries[0].chunk_id, "a");
        assert_eq!(document.paragraphs[1].chunk_boundaries[0].chunk_id, "b");
        assert_eq!(document.recognition_token_evidence, evidence);

        document.merge_paragraphs(1).unwrap();
        assert_eq!(
            document.paragraphs[0]
                .chunk_boundaries
                .iter()
                .map(|marker| marker.chunk_id.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b"]
        );
        assert_eq!(document.recognition_token_evidence, evidence);
    }

    #[test]
    fn installing_another_transcription_preserves_chunk_identity() {
        let initial = run("initial", "stable", "s1", "old");
        let mut document = Project::from_run(&initial);
        let mapping = document.chunk_audio_mapping("stable").unwrap().clone();
        document.mark_attention(1, 1).unwrap();

        document
            .install_chunk_recognition(1, 1, run("refresh", "ignored", "s2", "new"))
            .unwrap();

        assert_eq!(document.paragraphs[0].text(), "new");
        assert_eq!(
            document.paragraphs[0].chunk_boundaries[0].chunk_id,
            "stable"
        );
        assert_eq!(document.chunk_audio_mapping("stable"), Some(&mapping));
        assert!(document.attention_marks().is_empty());
        assert_eq!(document.undo(1), 1);
        assert_eq!(document.attention_marks()[0].chunk_id(), "stable");
        assert!(document.is_attention_marked(document.token(1, 1).unwrap().id()));
        assert_eq!(document.redo(1), 1);
        assert!(document.attention_marks().is_empty());
    }

    #[test]
    fn counted_undo_and_redo_apply_available_steps_and_new_edits_clear_redo() {
        let segments = vec![segment("s1", "a b", vec![token("a"), token(" b")])];
        let chunks = vec![chunk("a", "s1", "a b", ChunkBoundaryReason::SourceEnd)];
        let mut document = Project::from_evidence("run", &segments, &chunks);

        document.insert_text(1, 1, true, " x".into()).unwrap();
        document.replace_text(1, 1, 1, 1, "first".into()).unwrap();
        assert_eq!(document.paragraphs()[0].text(), "first x b");
        assert_eq!(document.undo(5), 2);
        assert_eq!(document.paragraphs()[0].text(), "a b");
        assert_eq!(document.edit_history_len(), 0);
        assert_eq!(document.redo_history_len(), 2);

        assert_eq!(document.redo(1), 1);
        assert_eq!(document.paragraphs()[0].text(), "a x b");
        document.insert_text(1, 1, false, "new ".into()).unwrap();
        assert_eq!(document.redo_history_len(), 0);
        assert_eq!(document.redo(5), 0);
    }

    #[test]
    fn attention_mark_address_is_derived_after_an_insertion() {
        let segments = vec![segment("s1", "a b", vec![token("a"), token(" b")])];
        let chunks = vec![chunk("stable", "s1", "a b", ChunkBoundaryReason::SourceEnd)];
        let mut project = Project::from_evidence("run", &segments, &chunks);
        let marked_id = project.token(1, 2).unwrap().id().clone();
        project.mark_attention(1, 2).unwrap();

        project.insert_text(1, 1, false, "new ".into()).unwrap();

        assert_eq!(project.attention_marks()[0].chunk_id(), "stable");
        assert_eq!(project.token(1, 3).unwrap().id(), &marked_id);
        assert!(project.is_attention_marked(project.token(1, 3).unwrap().id()));
    }

    #[test]
    fn document_rearranges_chunks_without_project_support_state() {
        let initial = run("initial", "first", "s1", "one");
        let mut project = crate::project::Project::from_run(&initial);
        let second = chunk("second", "s2", " two", ChunkBoundaryReason::SourceEnd);
        project.document.paragraphs[0].tokens.push(VisibleToken {
            id: VisibleTokenId::Recognition {
                run_id: "initial".into(),
                segment_id: "s2".into(),
                token_index: 0,
            },
            text: " two".into(),
            origin: VisibleTokenOrigin::Recognition,
        });
        project.document.paragraphs[0]
            .chunk_boundaries
            .push(ChunkBoundaryMarker {
                chunk_id: second.id,
                after_tokens: 2,
            });

        let mut composition = project.document().clone();
        composition.split_paragraph(1, 1).unwrap();

        assert_eq!(composition.paragraphs().len(), 2);
        assert_eq!(
            composition.paragraphs()[0].chunk_boundaries()[0].chunk_id(),
            "first"
        );
        assert_eq!(
            composition.paragraphs()[1].chunk_boundaries()[0].chunk_id(),
            "second"
        );
        assert_eq!(project.recognition_runs().len(), 1);
    }

    #[test]
    fn transcription_install_after_undo_follows_restored_state_and_failure_keeps_redo() {
        let initial = run("initial", "stable", "s1", "old");
        let mut project = crate::project::Project::from_run(&initial);
        project
            .install_chunk_recognition(1, 1, run("discarded", "ignored", "s2", "discarded"))
            .unwrap();
        assert_eq!(project.undo(1), 1);
        assert_eq!(project.paragraphs()[0].text(), "old");
        assert_eq!(project.redo_history_len(), 1);

        let before_failure = project.clone();
        let error = project
            .install_chunk_recognition(1, 1, run("initial", "ignored", "s3", "failed"))
            .unwrap_err();
        assert!(error.contains("not unique"));
        assert_eq!(project, before_failure);
        assert_eq!(project.redo_history_len(), 1);

        project
            .install_chunk_recognition(1, 1, run("replacement", "ignored", "s4", "new"))
            .unwrap();
        assert_eq!(project.paragraphs()[0].text(), "new");
        assert_eq!(project.redo_history_len(), 0);
        assert_eq!(project.undo(1), 1);
        assert_eq!(project.paragraphs()[0].text(), "old");
        assert_eq!(project.redo(1), 1);
        assert_eq!(project.paragraphs()[0].text(), "new");
    }
}
