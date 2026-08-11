//! Token-oriented visible document derived from immutable recognition evidence.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::chunking::SampleRange;

use crate::recognition::{ChunkBoundaryReason, DecodedSegment, RecognitionChunk, RecognitionRun};

pub const DOCUMENT_SCHEMA: &str = "rde-document/v1-experimental";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    #[serde(skip)]
    token_fallbacks: Vec<TokenFallback>,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkAudioMapping {
    chunk_id: String,
    source_id: String,
    range: SampleRange,
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
}
