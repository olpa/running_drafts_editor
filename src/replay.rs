//! Resolve visible text positions to honest canonical-audio ranges.

use crate::{
    chunking::SampleRange,
    document::{AlignmentState, Document, VisibleTokenId},
    navigation::{Address, Caret, NavigationState, Selection, StableTokenPosition, TokenAddress},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedReplay {
    pub source_id: String,
    pub range: SampleRange,
    pub alignment: AlignmentState,
    pub partial: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReplayResolutionError {
    #[error("there is no current caret or selection")]
    NoCurrentPosition,
    #[error("unknown paragraph {0}")]
    UnknownParagraph(usize),
    #[error("unknown token {paragraph}.{token}")]
    UnknownToken { paragraph: usize, token: usize },
    #[error("unknown chunk marker {paragraph}@{marker}")]
    UnknownMarker { paragraph: usize, marker: usize },
    #[error("the current position is stale")]
    StalePosition,
    #[error("audio alignment is unavailable for the requested text")]
    Unavailable,
    #[error("the requested text maps to more than one audio source")]
    MultipleSources,
}

pub fn resolve(
    document: &Document,
    navigation: &NavigationState,
    address: Option<&Address>,
    context_samples: u64,
) -> Result<ResolvedReplay, ReplayResolutionError> {
    let target = match address.filter(|address| !matches!(address, Address::Current)) {
        Some(address) => Target::Address(address),
        None => match navigation.selection() {
            Some(selection) => Target::Selection(selection),
            None => match navigation.caret() {
                Some(caret) => Target::Caret(caret),
                None => return Err(ReplayResolutionError::NoCurrentPosition),
            },
        },
    };
    if let Some((source_id, range)) = marker_range(document, target)? {
        return Ok(ResolvedReplay {
            source_id,
            range,
            alignment: AlignmentState::Exact,
            partial: false,
        });
    }
    let tokens = target_tokens(document, target)?;
    let mut source_id = None::<String>;
    let mut start = u64::MAX;
    let mut end = 0;
    let mut alignment = AlignmentState::Exact;
    let mut mapped = 0;
    for (paragraph_id, revision, token_id) in &tokens {
        let Some(mapping) = document.token_audio_mappings().iter().find(|mapping| {
            mapping.paragraph_id() == paragraph_id
                && mapping.paragraph_revision() == *revision
                && mapping.token_id() == token_id
        }) else {
            continue;
        };
        if matches!(mapping.alignment(), AlignmentState::Unavailable) {
            continue;
        }
        if source_id
            .as_deref()
            .is_some_and(|id| id != mapping.source_id())
        {
            return Err(ReplayResolutionError::MultipleSources);
        }
        source_id.get_or_insert_with(|| mapping.source_id().to_owned());
        start = start.min(mapping.range().start_sample);
        end = end.max(mapping.range().end_sample);
        alignment = alignment.max(mapping.alignment());
        mapped += 1;
    }
    let Some(source_id) = source_id else {
        return Err(ReplayResolutionError::Unavailable);
    };
    let source = document
        .audio_source(&source_id)
        .ok_or(ReplayResolutionError::Unavailable)?;
    let range = SampleRange {
        start_sample: start.saturating_sub(context_samples),
        end_sample: source
            .canonical_sample_count()
            .map_or(end.saturating_add(context_samples), |count| {
                end.saturating_add(context_samples).min(count)
            }),
    };
    Ok(ResolvedReplay {
        source_id,
        range,
        alignment,
        partial: mapped != tokens.len(),
    })
}

#[derive(Clone, Copy)]
enum Target<'a> {
    Address(&'a Address),
    Selection(&'a Selection),
    Caret(&'a Caret),
}

fn marker_range(
    document: &Document,
    target: Target<'_>,
) -> Result<Option<(String, SampleRange)>, ReplayResolutionError> {
    let marker = match target {
        Target::Address(Address::Marker { paragraph, marker }) => document
            .chunk_marker(*paragraph, *marker)
            .ok_or(ReplayResolutionError::UnknownMarker {
                paragraph: *paragraph,
                marker: *marker,
            })?,
        Target::Selection(Selection::Marker(position)) | Target::Caret(Caret::Marker(position)) => {
            let paragraph = document
                .paragraphs()
                .iter()
                .find(|p| {
                    p.id() == position.paragraph_id && p.revision() == position.paragraph_revision
                })
                .ok_or(ReplayResolutionError::StalePosition)?;
            paragraph
                .chunk_boundaries()
                .iter()
                .find(|m| m.chunk_id() == position.chunk_id)
                .ok_or(ReplayResolutionError::StalePosition)?
        }
        _ => return Ok(None),
    };
    let Some((source, range)) = document.audio_mapping(marker.chunk_id()) else {
        return Err(ReplayResolutionError::Unavailable);
    };
    Ok(Some((source.id().to_owned(), range)))
}

fn target_tokens(
    document: &Document,
    target: Target<'_>,
) -> Result<Vec<(String, u64, VisibleTokenId)>, ReplayResolutionError> {
    match target {
        Target::Address(Address::Token(token)) => address_tokens(document, *token, *token),
        Target::Address(Address::TokenRange { start, end }) => {
            address_tokens(document, *start, *end)
        }
        Target::Address(Address::Paragraph(number)) => paragraph_tokens(document, *number),
        Target::Address(Address::Current) => unreachable!(),
        Target::Address(Address::Marker { .. }) => unreachable!(),
        Target::Caret(Caret::Token(position)) => stable_tokens(document, position, position),
        Target::Caret(Caret::Marker(_)) => unreachable!(),
        Target::Selection(Selection::Tokens {
            start,
            end_inclusive,
            ..
        }) => stable_tokens(document, start, end_inclusive),
        Target::Selection(Selection::Paragraph {
            paragraph_id,
            paragraph_revision,
        }) => {
            let paragraph = document
                .paragraphs()
                .iter()
                .position(|p| p.id() == paragraph_id && p.revision() == *paragraph_revision)
                .ok_or(ReplayResolutionError::StalePosition)?;
            paragraph_tokens(document, paragraph + 1)
        }
        Target::Selection(Selection::Marker(_)) => unreachable!(),
    }
}

fn paragraph_tokens(
    document: &Document,
    number: usize,
) -> Result<Vec<(String, u64, VisibleTokenId)>, ReplayResolutionError> {
    let paragraph = document
        .paragraph(number)
        .ok_or(ReplayResolutionError::UnknownParagraph(number))?;
    Ok(paragraph
        .tokens()
        .iter()
        .map(|token| {
            (
                paragraph.id().to_owned(),
                paragraph.revision(),
                token.id().clone(),
            )
        })
        .collect())
}

fn address_tokens(
    document: &Document,
    start: TokenAddress,
    end: TokenAddress,
) -> Result<Vec<(String, u64, VisibleTokenId)>, ReplayResolutionError> {
    let start_token = document.token(start.paragraph, start.token).ok_or(
        ReplayResolutionError::UnknownToken {
            paragraph: start.paragraph,
            token: start.token,
        },
    )?;
    let end_token =
        document
            .token(end.paragraph, end.token)
            .ok_or(ReplayResolutionError::UnknownToken {
                paragraph: end.paragraph,
                token: end.token,
            })?;
    let start = StableTokenPosition {
        paragraph_id: document.paragraph(start.paragraph).unwrap().id().into(),
        paragraph_revision: document.paragraph(start.paragraph).unwrap().revision(),
        token_id: start_token.id().clone(),
    };
    let end = StableTokenPosition {
        paragraph_id: document.paragraph(end.paragraph).unwrap().id().into(),
        paragraph_revision: document.paragraph(end.paragraph).unwrap().revision(),
        token_id: end_token.id().clone(),
    };
    stable_tokens(document, &start, &end)
}

fn stable_tokens(
    document: &Document,
    start: &StableTokenPosition,
    end: &StableTokenPosition,
) -> Result<Vec<(String, u64, VisibleTokenId)>, ReplayResolutionError> {
    let all = document
        .paragraphs()
        .iter()
        .flat_map(|paragraph| {
            paragraph.tokens().iter().map(move |token| {
                (
                    paragraph.id().to_owned(),
                    paragraph.revision(),
                    token.id().clone(),
                )
            })
        })
        .collect::<Vec<_>>();
    let first = all
        .iter()
        .position(|(p, r, t)| {
            p == &start.paragraph_id && *r == start.paragraph_revision && t == &start.token_id
        })
        .ok_or(ReplayResolutionError::StalePosition)?;
    let last = all
        .iter()
        .position(|(p, r, t)| {
            p == &end.paragraph_id && *r == end.paragraph_revision && t == &end.token_id
        })
        .ok_or(ReplayResolutionError::StalePosition)?;
    if first > last {
        return Err(ReplayResolutionError::StalePosition);
    }
    Ok(all[first..=last].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn document(second_mapping: bool) -> Document {
        let mut mappings = vec![json!({
            "paragraph_id": "p1", "paragraph_revision": 1,
            "token_id": {"kind": "recognition", "run_id": "run", "segment_id": "s1", "token_index": 0},
            "source_id": "audio", "range": {"start_sample": 1_000, "end_sample": 2_000},
            "alignment": "exact"
        })];
        if second_mapping {
            mappings.push(json!({
                "paragraph_id": "p1", "paragraph_revision": 1,
                "token_id": {"kind": "recognition", "run_id": "run", "segment_id": "s1", "token_index": 1},
                "source_id": "audio", "range": {"start_sample": 2_100, "end_sample": 3_000},
                "alignment": "inherited"
            }));
        }
        serde_json::from_value(json!({
            "schema": "rde-document/v1-experimental", "id": "document:test",
            "paragraphs": [{
                "id": "p1", "revision": 1,
                "tokens": [
                    {"id": {"kind": "recognition", "run_id": "run", "segment_id": "s1", "token_index": 0}, "text": "one", "origin": {"kind": "recognition"}},
                    {"id": {"kind": "recognition", "run_id": "run", "segment_id": "s1", "token_index": 1}, "text": " two", "origin": {"kind": "recognition"}}
                ],
                "chunk_boundaries": [{"chunk_id": "c1", "after_tokens": 2}]
            }],
            "audio_sources": [{"id": "audio", "path": "audio.wav", "canonical_sample_count": 3_200}],
            "chunk_audio_mappings": [{"chunk_id": "c1", "source_id": "audio", "range": {"start_sample": 900, "end_sample": 3_100}}],
            "token_audio_mappings": mappings
        })).unwrap()
    }

    #[test]
    fn current_token_replay_adds_and_clamps_context() {
        let document = document(true);
        let navigation = NavigationState::new(&document);
        let resolved = resolve(&document, &navigation, None, 1_500).unwrap();
        assert_eq!(
            resolved.range,
            SampleRange {
                start_sample: 0,
                end_sample: 3_200
            }
        );
        assert_eq!(resolved.alignment, AlignmentState::Exact);
        assert!(!resolved.partial);
        assert_eq!(
            resolve(&document, &navigation, Some(&Address::Current), 1_500).unwrap(),
            resolved
        );
    }

    #[test]
    fn range_reports_partial_coverage_without_inventing_timing() {
        let document = document(false);
        let navigation = NavigationState::new(&document);
        let address = Address::TokenRange {
            start: TokenAddress {
                paragraph: 1,
                token: 1,
            },
            end: TokenAddress {
                paragraph: 1,
                token: 2,
            },
        };
        let resolved = resolve(&document, &navigation, Some(&address), 0).unwrap();
        assert_eq!(
            resolved.range,
            SampleRange {
                start_sample: 1_000,
                end_sample: 2_000
            }
        );
        assert!(resolved.partial);
    }

    #[test]
    fn marker_replay_uses_exact_chunk_without_context() {
        let document = document(true);
        let navigation = NavigationState::new(&document);
        let address = Address::Marker {
            paragraph: 1,
            marker: 1,
        };
        let resolved = resolve(&document, &navigation, Some(&address), 12_000).unwrap();
        assert_eq!(
            resolved.range,
            SampleRange {
                start_sample: 900,
                end_sample: 3_100
            }
        );
    }

    #[test]
    fn range_uses_least_precise_alignment() {
        let document = document(true);
        let navigation = NavigationState::new(&document);
        let address = Address::Paragraph(1);
        let resolved = resolve(&document, &navigation, Some(&address), 0).unwrap();
        assert_eq!(resolved.alignment, AlignmentState::Inherited);
        assert!(!resolved.partial);
    }
}
