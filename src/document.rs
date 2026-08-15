//! Token-oriented visible document derived from immutable recognition evidence.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::chunking::SampleRange;

use crate::recognition::{ChunkBoundaryReason, DecodedSegment, RecognitionChunk, RecognitionRun};

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
pub struct ChunkSplitOutcome {
    pub paragraph: usize,
    pub marker: Option<usize>,
    pub created: bool,
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
    #[error("unknown token {paragraph}.{token}")]
    UnknownToken { paragraph: usize, token: usize },
    #[error("unknown chunk marker {paragraph}@{marker}")]
    UnknownMarker { paragraph: usize, marker: usize },
    #[error("paragraph {0} has no following paragraph")]
    NoFollowingParagraph(usize),
    #[error("the final chunk marker cannot split a paragraph")]
    FinalMarker,
    #[error("the final chunk marker has no chunk to merge on its right")]
    NoRightChunk,
    #[error("chunk merge requires compatible replay mappings from one audio source")]
    IncompatibleChunkMappings,
    #[error("merged chunk would exceed 480000 canonical samples")]
    ChunkTooLong,
    #[error("paragraph revision cannot be increased")]
    RevisionOverflow,
}

pub const DOCUMENT_SCHEMA: &str = "rde-document/v1-experimental";

fn is_zero(value: &u64) -> bool {
    *value == 0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    schema: String,
    id: String,
    paragraphs: Vec<Paragraph>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    audio_sources: Vec<AudioSource>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    chunk_audio_mappings: Vec<ChunkAudioMapping>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    token_audio_mappings: Vec<TokenAudioMapping>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    replay_chunks: Vec<ReplayChunk>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    recognition_token_evidence: Vec<RecognitionTokenEvidence>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    recognition_runs: Vec<RecognitionRun>,
    #[serde(default, skip_serializing_if = "is_zero")]
    next_structure_id: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    edit_history: Vec<EditHistoryEntry>,
    #[serde(skip)]
    token_fallbacks: Vec<TokenFallback>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct EditHistoryEntry {
    before: EditableDocumentState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct EditableDocumentState {
    paragraphs: Vec<Paragraph>,
    chunk_audio_mappings: Vec<ChunkAudioMapping>,
    token_audio_mappings: Vec<TokenAudioMapping>,
    replay_chunks: Vec<ReplayChunk>,
    next_structure_id: u64,
}

impl Document {
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
            schema: DOCUMENT_SCHEMA.into(),
            id: format!("document:{run_id}"),
            paragraphs,
            audio_sources: Vec::new(),
            chunk_audio_mappings: Vec::new(),
            token_audio_mappings: Vec::new(),
            replay_chunks: Vec::new(),
            recognition_token_evidence: Vec::new(),
            recognition_runs: Vec::new(),
            next_structure_id: 0,
            edit_history: Vec::new(),
            token_fallbacks,
        }
    }

    pub fn schema(&self) -> &str {
        &self.schema
    }

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

    pub fn chunk_marker(&self, paragraph: usize, marker: usize) -> Option<&ChunkBoundaryMarker> {
        self.paragraph(paragraph)?
            .chunk_boundaries
            .get(marker.checked_sub(1)?)
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

    pub fn edit_history_len(&self) -> usize {
        self.edit_history.len()
    }

    fn remember_editable_state(&mut self) {
        self.edit_history.push(EditHistoryEntry {
            before: EditableDocumentState {
                paragraphs: self.paragraphs.clone(),
                chunk_audio_mappings: self.chunk_audio_mappings.clone(),
                token_audio_mappings: self.token_audio_mappings.clone(),
                replay_chunks: self.replay_chunks.clone(),
                next_structure_id: self.next_structure_id,
            },
        });
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

    pub fn install_chunk_recognition(
        &mut self,
        paragraph_number: usize,
        marker_number: usize,
        run: RecognitionRun,
    ) -> Result<(), String> {
        let mut next = self.clone();
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
        let new_ids = tokens.iter().map(|t| t.id.clone()).collect::<Vec<_>>();
        let delta = tokens.len() as isize - (end - start) as isize;
        let paragraph = &mut self.paragraphs[paragraph_number - 1];
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
        self.recognition_token_evidence
            .retain(|e| !removed.contains(&e.token_id));
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

    pub fn split_chunk(
        &mut self,
        paragraph_number: usize,
        token_number: usize,
        after: bool,
    ) -> Result<ChunkSplitOutcome, StructureEditError> {
        let paragraph = self
            .paragraph(paragraph_number)
            .ok_or(StructureEditError::UnknownParagraph(paragraph_number))?;
        if token_number == 0 || token_number > paragraph.tokens.len() {
            return Err(StructureEditError::UnknownToken {
                paragraph: paragraph_number,
                token: token_number,
            });
        }
        let boundary = if after {
            token_number
        } else {
            token_number - 1
        };
        if boundary == 0 {
            return Ok(ChunkSplitOutcome {
                paragraph: paragraph_number,
                marker: None,
                created: false,
            });
        }
        if let Some(marker) = paragraph
            .chunk_boundaries
            .iter()
            .position(|marker| marker.after_tokens == boundary)
        {
            return Ok(ChunkSplitOutcome {
                paragraph: paragraph_number,
                marker: Some(marker + 1),
                created: false,
            });
        }
        let marker_index = paragraph
            .chunk_boundaries
            .iter()
            .position(|marker| marker.after_tokens > boundary)
            .expect("the final marker follows every token boundary");
        if paragraph.revision == u64::MAX {
            return Err(StructureEditError::RevisionOverflow);
        }
        let chunk_start = marker_index
            .checked_sub(1)
            .map_or(0, |index| paragraph.chunk_boundaries[index].after_tokens);
        let chunk_end = paragraph.chunk_boundaries[marker_index].after_tokens;
        let parent_id = paragraph.chunk_boundaries[marker_index].chunk_id.clone();
        let left_ids = paragraph.tokens[chunk_start..boundary]
            .iter()
            .map(|token| token.id.clone())
            .collect::<Vec<_>>();
        let right_ids = paragraph.tokens[boundary..chunk_end]
            .iter()
            .map(|token| token.id.clone())
            .collect::<Vec<_>>();
        let left_edge = paragraph.tokens[boundary - 1].id.clone();
        let right_edge = paragraph.tokens[boundary].id.clone();
        let paragraph_id = paragraph.id.clone();
        let paragraph_revision = paragraph.revision;
        self.remember_editable_state();
        self.ensure_replay_chunks();
        let (left_id, right_id) = (
            self.new_structure_id("chunk"),
            self.new_structure_id("chunk"),
        );

        let mappings = self.split_chunk_mappings(
            &parent_id,
            (&left_id, &right_id),
            (&paragraph_id, paragraph_revision),
            (&left_edge, &right_edge),
        );
        self.chunk_audio_mappings
            .retain(|mapping| mapping.chunk_id != parent_id);
        self.chunk_audio_mappings.extend(mappings);
        self.replay_chunks.push(ReplayChunk {
            id: left_id.clone(),
            parent_ids: vec![parent_id.clone()],
            token_ids: left_ids,
        });
        self.replay_chunks.push(ReplayChunk {
            id: right_id.clone(),
            parent_ids: vec![parent_id],
            token_ids: right_ids,
        });
        let paragraph = &mut self.paragraphs[paragraph_number - 1];
        paragraph.chunk_boundaries[marker_index].chunk_id = right_id;
        paragraph.chunk_boundaries.insert(
            marker_index,
            ChunkBoundaryMarker {
                chunk_id: left_id,
                after_tokens: boundary,
            },
        );
        self.advance_paragraph_revision(paragraph_number)?;
        Ok(ChunkSplitOutcome {
            paragraph: paragraph_number,
            marker: Some(marker_index + 1),
            created: true,
        })
    }

    fn split_chunk_mappings(
        &self,
        parent_id: &str,
        child_ids: (&str, &str),
        paragraph: (&str, u64),
        edge_tokens: (&VisibleTokenId, &VisibleTokenId),
    ) -> Vec<ChunkAudioMapping> {
        let (left_id, right_id) = child_ids;
        let (paragraph_id, revision) = paragraph;
        let (left_token, right_token) = edge_tokens;
        let Some(parent) = self.chunk_audio_mapping(parent_id) else {
            return Vec::new();
        };
        let token_mapping = |id: &VisibleTokenId| {
            self.token_audio_mappings.iter().find(|mapping| {
                mapping.paragraph_id == paragraph_id
                    && mapping.paragraph_revision == revision
                    && &mapping.token_id == id
                    && mapping.source_id == parent.source_id
                    && mapping.alignment <= AlignmentState::Aligned
            })
        };
        let boundary = match (token_mapping(left_token), token_mapping(right_token)) {
            (Some(left), Some(right)) if left.range.end_sample == right.range.start_sample => {
                Some((left.range.end_sample, AlignmentState::Exact))
            }
            (Some(left), Some(right)) if left.range.end_sample < right.range.start_sample => {
                Some((
                    left.range.end_sample + (right.range.start_sample - left.range.end_sample) / 2,
                    AlignmentState::Aligned,
                ))
            }
            _ => None,
        };
        match boundary {
            Some((sample, alignment))
                if parent.range.start_sample < sample && sample < parent.range.end_sample =>
            {
                vec![
                    ChunkAudioMapping {
                        chunk_id: left_id.into(),
                        source_id: parent.source_id.clone(),
                        range: SampleRange {
                            start_sample: parent.range.start_sample,
                            end_sample: sample,
                        },
                        alignment: parent.alignment.max(alignment),
                    },
                    ChunkAudioMapping {
                        chunk_id: right_id.into(),
                        source_id: parent.source_id.clone(),
                        range: SampleRange {
                            start_sample: sample,
                            end_sample: parent.range.end_sample,
                        },
                        alignment: parent.alignment.max(alignment),
                    },
                ]
            }
            _ => vec![
                ChunkAudioMapping {
                    chunk_id: left_id.into(),
                    source_id: parent.source_id.clone(),
                    range: parent.range,
                    alignment: parent.alignment.max(AlignmentState::Inherited),
                },
                ChunkAudioMapping {
                    chunk_id: right_id.into(),
                    source_id: parent.source_id.clone(),
                    range: parent.range,
                    alignment: parent.alignment.max(AlignmentState::Inherited),
                },
            ],
        }
    }

    fn new_structure_id(&mut self, kind: &str) -> String {
        self.next_structure_id = self.next_structure_id.saturating_add(1);
        format!("{kind}:{}:{}", self.id, self.next_structure_id)
    }

    fn advance_paragraph_revision(
        &mut self,
        paragraph_number: usize,
    ) -> Result<(), StructureEditError> {
        let paragraph = &mut self.paragraphs[paragraph_number - 1];
        let old_revision = paragraph.revision;
        paragraph.revision = old_revision
            .checked_add(1)
            .ok_or(StructureEditError::RevisionOverflow)?;
        for mapping in &mut self.token_audio_mappings {
            if mapping.paragraph_id == paragraph.id && mapping.paragraph_revision == old_revision {
                mapping.paragraph_revision = paragraph.revision;
            }
        }
        Ok(())
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
        self.remember_editable_state();
        let left_id = self.new_structure_id("paragraph");
        let right_id = self.new_structure_id("paragraph");
        let left = Paragraph {
            id: left_id.clone(),
            revision: 1,
            tokens: paragraph.tokens[..boundary].to_vec(),
            chunk_boundaries: paragraph.chunk_boundaries[..=marker_index].to_vec(),
        };
        let right = Paragraph {
            id: right_id.clone(),
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
        self.remap_paragraph_tokens(
            &paragraph.id,
            &[(&left_id, &left.tokens), (&right_id, &right.tokens)],
        );
        self.paragraphs.splice(index..=index, [left, right]);
        Ok(ParagraphSplitOutcome {
            right_paragraph: paragraph_number + 1,
        })
    }

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
        self.remember_editable_state();
        let new_id = self.new_structure_id("paragraph");
        let left_count = left.tokens.len();
        let mut tokens = left.tokens.clone();
        tokens.extend(right.tokens.clone());
        let mut markers = left.chunk_boundaries.clone();
        markers.extend(right.chunk_boundaries.iter().cloned().map(|mut marker| {
            marker.after_tokens += left_count;
            marker
        }));
        let merged = Paragraph {
            id: new_id.clone(),
            revision: 1,
            tokens,
            chunk_boundaries: markers,
        };
        self.remap_paragraph_tokens(&left.id, &[(&new_id, &merged.tokens)]);
        self.remap_paragraph_tokens(&right.id, &[(&new_id, &merged.tokens)]);
        self.paragraphs.splice(index..=index + 1, [merged]);
        Ok(ParagraphMergeOutcome {
            paragraph: paragraph_number,
            first_right_token: left_count + 1,
        })
    }

    fn remap_paragraph_tokens(
        &mut self,
        old_paragraph_id: &str,
        destinations: &[(&String, &Vec<VisibleToken>)],
    ) {
        for mapping in &mut self.token_audio_mappings {
            if mapping.paragraph_id != old_paragraph_id {
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
    }

    pub fn merge_chunks(
        &mut self,
        paragraph_number: usize,
        marker_number: usize,
    ) -> Result<usize, StructureEditError> {
        let paragraph = self
            .paragraph(paragraph_number)
            .ok_or(StructureEditError::UnknownParagraph(paragraph_number))?;
        let left_index = marker_number
            .checked_sub(1)
            .filter(|index| *index < paragraph.chunk_boundaries.len())
            .ok_or(StructureEditError::UnknownMarker {
                paragraph: paragraph_number,
                marker: marker_number,
            })?;
        if left_index + 1 >= paragraph.chunk_boundaries.len() {
            return Err(StructureEditError::NoRightChunk);
        }
        if paragraph.revision == u64::MAX {
            return Err(StructureEditError::RevisionOverflow);
        }
        let left_id = paragraph.chunk_boundaries[left_index].chunk_id.clone();
        let right_id = paragraph.chunk_boundaries[left_index + 1].chunk_id.clone();
        let left_mapping = self
            .chunk_audio_mapping(&left_id)
            .ok_or(StructureEditError::IncompatibleChunkMappings)?
            .clone();
        let right_mapping = self
            .chunk_audio_mapping(&right_id)
            .ok_or(StructureEditError::IncompatibleChunkMappings)?
            .clone();
        if left_mapping.source_id != right_mapping.source_id {
            return Err(StructureEditError::IncompatibleChunkMappings);
        }
        let range = SampleRange {
            start_sample: left_mapping
                .range
                .start_sample
                .min(right_mapping.range.start_sample),
            end_sample: left_mapping
                .range
                .end_sample
                .max(right_mapping.range.end_sample),
        };
        if range.len() > 480_000 {
            return Err(StructureEditError::ChunkTooLong);
        }
        self.remember_editable_state();
        self.ensure_replay_chunks();
        let left_tokens = self
            .replay_chunks
            .iter()
            .find(|chunk| chunk.id == left_id)
            .expect("current marker has a replay chunk")
            .token_ids
            .clone();
        let right_tokens = self
            .replay_chunks
            .iter()
            .find(|chunk| chunk.id == right_id)
            .expect("current marker has a replay chunk")
            .token_ids
            .clone();
        let merged_id = self.new_structure_id("chunk");
        let mut token_ids = left_tokens;
        token_ids.extend(right_tokens);
        self.replay_chunks.push(ReplayChunk {
            id: merged_id.clone(),
            parent_ids: vec![left_id.clone(), right_id.clone()],
            token_ids,
        });
        self.chunk_audio_mappings
            .retain(|mapping| mapping.chunk_id != left_id && mapping.chunk_id != right_id);
        self.chunk_audio_mappings.push(ChunkAudioMapping {
            chunk_id: merged_id.clone(),
            source_id: left_mapping.source_id,
            range,
            alignment: left_mapping.alignment.max(right_mapping.alignment),
        });
        let paragraph = &mut self.paragraphs[paragraph_number - 1];
        paragraph.chunk_boundaries.remove(left_index);
        paragraph.chunk_boundaries[left_index].chunk_id = merged_id;
        self.advance_paragraph_revision(paragraph_number)?;
        Ok(marker_number)
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
        let paragraph = self
            .paragraphs
            .get_mut(paragraph_number.checked_sub(1).unwrap_or(usize::MAX))
            .ok_or(DocumentEditError::UnknownParagraph(paragraph_number))?;
        let removed = end_exclusive - start;
        let inserted = usize::from(replacement.is_some());

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
        let replacement = replacement.map(|text| VisibleToken {
            id: VisibleTokenId::Pseudo {
                id: format!("edit:{}:{new_revision}", paragraph.id),
            },
            text,
            origin: VisibleTokenOrigin::Pseudo {
                reason: reason.into(),
            },
        });
        paragraph.tokens.splice(start..end_exclusive, replacement);
        paragraph.revision = new_revision;

        self.token_audio_mappings.retain_mut(|mapping| {
            if mapping.paragraph_id != paragraph.id || mapping.paragraph_revision != old_revision {
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
        chunking::SampleRange,
        recognition::{ChunkBoundary, RecognitionToken},
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

        let document = Document::from_evidence("run", &segments, &chunks);

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

        let document = Document::from_evidence("run", &segments, &chunks);
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

        let document = Document::from_evidence("run", &segments, &chunks);

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
        let mut document = Document::from_evidence("run", &segments, &chunks);

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
        let mut document = Document::from_evidence("run", &segments, &chunks);

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
        let mut document = Document::from_evidence("run", &segments, &chunks);
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
    fn invalid_outer_structure_operations_do_not_change_the_document() {
        let segments = vec![segment("s1", "a", vec![token("a")])];
        let chunks = vec![chunk("a", "s1", "a", ChunkBoundaryReason::SourceEnd)];
        let mut document = Document::from_evidence("run", &segments, &chunks);
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
        assert_eq!(
            document.merge_chunks(1, 1),
            Err(StructureEditError::NoRightChunk)
        );
        assert_eq!(document, original);
    }
}
