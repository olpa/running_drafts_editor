//! Resolve document positions to honest canonical-audio ranges.

use crate::{
    chunking::SampleRange,
    document::AlignmentState,
    navigation::{
        chunks_in_range, item_chunk, item_paragraph, item_token, tokens_in_range, Address,
        ChunkAddress, NavigationError, NavigationState, PositionAddress, TokenAddress,
    },
    project::Project,
};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedReplay {
    pub source_id: String,
    pub range: SampleRange,
    pub alignment: AlignmentState,
    pub partial: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReplayResolutionError {
    #[error("there is no current position or selection")]
    NoCurrentPosition,
    #[error("{0}")]
    InvalidAddress(String),
    #[error("the current position or selection is stale")]
    StalePosition,
    #[error("audio alignment is unavailable for the requested content")]
    Unavailable,
    #[error("the requested content maps to more than one audio source")]
    MultipleSources,
}

impl From<NavigationError> for ReplayResolutionError {
    fn from(value: NavigationError) -> Self {
        match value {
            NavigationError::NoCurrentPosition | NavigationError::NoTokenSelection => {
                Self::NoCurrentPosition
            }
            NavigationError::StaleSelection => Self::StalePosition,
            other => Self::InvalidAddress(other.to_string()),
        }
    }
}

enum ReplayTarget {
    Paragraph(usize),
    Chunk(ChunkAddress),
    Token(TokenAddress),
    Range(PositionAddress, PositionAddress),
}

pub fn resolve(
    document: &Project,
    navigation: &NavigationState,
    address: Option<&Address>,
    context_samples: u64,
) -> Result<ResolvedReplay, ReplayResolutionError> {
    let target = match address {
        Some(Address::Position(PositionAddress::Paragraph(p))) => {
            ReplayTarget::Paragraph(item_paragraph(document, *p)?)
        }
        Some(Address::Position(PositionAddress::Chunk(a))) => {
            ReplayTarget::Chunk(item_chunk(document, *a)?)
        }
        Some(Address::Position(PositionAddress::Token(a))) => {
            ReplayTarget::Token(item_token(document, *a)?)
        }
        Some(Address::Range { start, end }) => ReplayTarget::Range(*start, *end),
        Some(Address::Current) | None => {
            let (start, end) = navigation.current_range(document)?;
            if !navigation.selection_is_empty(document)? {
                ReplayTarget::Range(start, end)
            } else {
                match start {
                    PositionAddress::Paragraph(p) => {
                        ReplayTarget::Paragraph(item_paragraph(document, p)?)
                    }
                    PositionAddress::Chunk(a) => ReplayTarget::Chunk(item_chunk(document, a)?),
                    PositionAddress::Token(a) => ReplayTarget::Token(item_token(document, a)?),
                }
            }
        }
    };
    match target {
        ReplayTarget::Paragraph(number) => resolve_chunks(
            document,
            document
                .paragraph(number)
                .unwrap()
                .chunk_boundaries()
                .iter()
                .map(|m| m.chunk_id()),
        ),
        ReplayTarget::Chunk(a) => resolve_chunks(
            document,
            std::iter::once(
                document
                    .chunk_marker(a.paragraph, a.chunk)
                    .unwrap()
                    .chunk_id(),
            ),
        ),
        ReplayTarget::Token(a) => resolve_tokens(document, [a], context_samples),
        ReplayTarget::Range(start, end) => resolve_range(document, start, end, context_samples),
    }
}

fn resolve_range(
    document: &Project,
    start: PositionAddress,
    end: PositionAddress,
    context_samples: u64,
) -> Result<ResolvedReplay, ReplayResolutionError> {
    let chunks = chunks_in_range(document, start, end)?;
    let tokens = tokens_in_range(document, start, end)?;
    if chunks.is_empty() {
        if tokens.is_empty() {
            return Err(ReplayResolutionError::Unavailable);
        }
        return resolve_tokens(document, tokens, context_samples);
    }
    let complete = chunks
        .iter()
        .map(|a| {
            document
                .chunk_marker(a.paragraph, a.chunk)
                .unwrap()
                .chunk_id()
        })
        .collect::<HashSet<_>>();
    let partial_tokens = tokens
        .into_iter()
        .filter(|a| {
            !complete.contains(
                document
                    .chunk_marker(a.paragraph, a.chunk)
                    .unwrap()
                    .chunk_id(),
            )
        })
        .collect::<Vec<_>>();
    let mut pieces = chunks
        .iter()
        .map(|a| {
            let id = document
                .chunk_marker(a.paragraph, a.chunk)
                .unwrap()
                .chunk_id();
            let m = document
                .chunk_audio_mapping(id)
                .ok_or(ReplayResolutionError::Unavailable)?;
            Ok((m.source_id().to_owned(), m.range(), m.alignment()))
        })
        .collect::<Result<Vec<_>, ReplayResolutionError>>()?;
    let partial_token_count = partial_tokens.len();
    let token_pieces = token_pieces(document, partial_tokens)?;
    let has_partial_tokens = partial_token_count > 0;
    let partial = token_pieces.len() != partial_token_count;
    pieces.extend(token_pieces);
    combine(
        document,
        pieces,
        partial,
        if has_partial_tokens {
            context_samples
        } else {
            0
        },
    )
}

fn resolve_chunks<'a>(
    document: &Project,
    ids: impl Iterator<Item = &'a str>,
) -> Result<ResolvedReplay, ReplayResolutionError> {
    let pieces = ids
        .map(|id| {
            let m = document
                .chunk_audio_mapping(id)
                .ok_or(ReplayResolutionError::Unavailable)?;
            Ok((m.source_id().to_owned(), m.range(), m.alignment()))
        })
        .collect::<Result<Vec<_>, ReplayResolutionError>>()?;
    combine(document, pieces, false, 0)
}

fn resolve_tokens(
    document: &Project,
    tokens: impl IntoIterator<Item = TokenAddress>,
    context_samples: u64,
) -> Result<ResolvedReplay, ReplayResolutionError> {
    let values = tokens.into_iter().collect::<Vec<_>>();
    let pieces = token_pieces(document, values.clone())?;
    let partial = pieces.len() != values.len();
    combine(document, pieces, partial, context_samples)
}

fn token_pieces(
    document: &Project,
    tokens: impl IntoIterator<Item = TokenAddress>,
) -> Result<Vec<(String, SampleRange, AlignmentState)>, ReplayResolutionError> {
    let mut pieces = Vec::new();
    for a in tokens {
        let paragraph = document.paragraph(a.paragraph).ok_or_else(|| {
            ReplayResolutionError::InvalidAddress(format!("unknown paragraph {}", a.paragraph))
        })?;
        let token = document
            .chunk_token(a.paragraph, a.chunk, a.token)
            .ok_or_else(|| ReplayResolutionError::InvalidAddress(format!("unknown token {a}")))?;
        if let Some(m) = document
            .token_audio_mappings()
            .iter()
            .find(|m| {
                m.paragraph_id() == paragraph.id()
                    && m.paragraph_revision() == paragraph.revision()
                    && m.token_identity() == token.id()
            })
            .filter(|m| !matches!(m.alignment(), AlignmentState::Unavailable))
        {
            pieces.push((m.source_id().to_owned(), m.range(), m.alignment()));
        }
    }
    Ok(pieces)
}

fn combine(
    document: &Project,
    pieces: Vec<(String, SampleRange, AlignmentState)>,
    partial: bool,
    context_samples: u64,
) -> Result<ResolvedReplay, ReplayResolutionError> {
    let Some((source_id, _, first_alignment)) = pieces.first().cloned() else {
        return Err(ReplayResolutionError::Unavailable);
    };
    if pieces.iter().any(|(source, _, _)| source != &source_id) {
        return Err(ReplayResolutionError::MultipleSources);
    }
    let start = pieces
        .iter()
        .map(|(_, range, _)| range.start_sample)
        .min()
        .unwrap();
    let end = pieces
        .iter()
        .map(|(_, range, _)| range.end_sample)
        .max()
        .unwrap();
    let alignment = pieces
        .iter()
        .fold(first_alignment, |value, (_, _, next)| value.max(*next));
    let source = document
        .audio_source(&source_id)
        .ok_or(ReplayResolutionError::Unavailable)?;
    Ok(ResolvedReplay {
        source_id,
        range: SampleRange {
            start_sample: start.saturating_sub(context_samples),
            end_sample: source
                .canonical_sample_count()
                .map_or(end.saturating_add(context_samples), |count| {
                    end.saturating_add(context_samples).min(count)
                }),
        },
        alignment,
        partial,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn empty_chunks() -> Project {
        serde_json::from_value(json!({
            "schema":"rde-project/v1-experimental", "id":"document:empty", "settings":{"model":null,"language":"auto"},
            "paragraphs":[
                {"id":"p1","revision":1,"tokens":[],"chunk_boundaries":[{"chunk_id":"c1","transcription_id":"test","text":"","after_tokens":0},{"chunk_id":"c2","transcription_id":"test","text":"","after_tokens":0}]},
                {"id":"p2","revision":1,"tokens":[],"chunk_boundaries":[{"chunk_id":"c3","transcription_id":"test","text":"","after_tokens":0}]}
            ],
            "audio_sources":[{"id":"audio","canonical_sample_count":300}],
            "chunk_audio_mappings":[
                {"chunk_id":"c1","source_id":"audio","range":{"start_sample":0,"end_sample":100}},
                {"chunk_id":"c2","source_id":"audio","range":{"start_sample":100,"end_sample":200}},
                {"chunk_id":"c3","source_id":"audio","range":{"start_sample":200,"end_sample":300}}
            ]
        })).unwrap()
    }

    #[test]
    fn plays_chunks_and_paragraphs_without_addressable_tokens() {
        let document = empty_chunks();
        let navigation = NavigationState::new(&document);
        let chunk = resolve(
            &document,
            &navigation,
            Some(&Address::Position(PositionAddress::Chunk(ChunkAddress {
                paragraph: 1,
                chunk: 2,
            }))),
            50,
        )
        .unwrap();
        assert_eq!(
            chunk.range,
            SampleRange {
                start_sample: 100,
                end_sample: 200
            }
        );
        let paragraph = resolve(
            &document,
            &navigation,
            Some(&Address::Position(PositionAddress::Paragraph(1))),
            50,
        )
        .unwrap();
        assert_eq!(
            paragraph.range,
            SampleRange {
                start_sample: 0,
                end_sample: 200
            }
        );
        let range = resolve(
            &document,
            &navigation,
            Some(&Address::Range {
                start: PositionAddress::Chunk(ChunkAddress {
                    paragraph: 1,
                    chunk: 1,
                }),
                end: PositionAddress::Chunk(ChunkAddress {
                    paragraph: 1,
                    chunk: 2,
                }),
            }),
            50,
        )
        .unwrap();
        assert_eq!(
            range.range,
            SampleRange {
                start_sample: 0,
                end_sample: 100
            }
        );
    }

    #[test]
    fn item_commands_reject_end_positions() {
        let document = empty_chunks();
        let navigation = NavigationState::new(&document);
        assert!(resolve(
            &document,
            &navigation,
            Some(&Address::Position(PositionAddress::Chunk(ChunkAddress {
                paragraph: 1,
                chunk: 3
            }))),
            0
        )
        .is_err());
        assert!(resolve(
            &document,
            &navigation,
            Some(&Address::Position(PositionAddress::Paragraph(3))),
            0
        )
        .is_err());
    }
}
