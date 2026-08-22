//! Editing and recognition-refresh operations for session command execution.

use std::io::{self, Write};

use crate::{
    chunking::{read_canonical_wav, SourceFacts},
    document::Document,
    navigation::{Address, NavigationState, TokenAddress},
    recognition::{ChunkRefreshRequest, RecognizerSession},
};

pub(crate) fn preserve_boundary_whitespace(
    document: &Document,
    start: TokenAddress,
    end: TokenAddress,
    replacement: String,
) -> String {
    let paragraph = document
        .paragraph(start.paragraph)
        .expect("a resolved edit range has a paragraph");
    let selected = paragraph.tokens()[start.token - 1..end.token]
        .iter()
        .map(|token| token.text())
        .collect::<String>();
    if selected.chars().all(char::is_whitespace) {
        return replacement;
    }
    let leading_end = selected
        .char_indices()
        .find(|(_, character)| !character.is_whitespace())
        .map_or(selected.len(), |(index, _)| index);
    let trailing_start = selected
        .char_indices()
        .rev()
        .find(|(_, character)| !character.is_whitespace())
        .map_or(0, |(index, character)| index + character.len_utf8());
    format!(
        "{}{}{}",
        &selected[..leading_end],
        replacement,
        &selected[trailing_start..]
    )
}

pub(crate) fn apply_history(
    document: &mut Document,
    navigation: &mut NavigationState,
    count: usize,
    redo: bool,
    output: &mut impl Write,
) -> io::Result<()> {
    let applied = if redo {
        document.redo(count)
    } else {
        document.undo(count)
    };
    if applied == 0 {
        writeln!(output, "nothing to {}", if redo { "redo" } else { "undo" })
    } else {
        *navigation = NavigationState::new(document);
        writeln!(
            output,
            "{} {applied} edit{}",
            if redo { "redid" } else { "undid" },
            if applied == 1 { "" } else { "s" }
        )
    }
}

pub(crate) fn render_alternatives(
    document: &Document,
    navigation: &NavigationState,
    addressed: Option<TokenAddress>,
    output: &mut impl Write,
    errors: &mut impl Write,
) -> io::Result<()> {
    let address = match alternative_address(document, navigation, addressed) {
        Ok(address) => address,
        Err(error) => return writeln!(errors, "alternatives unavailable: {error}"),
    };
    let Some(alternatives) = document.alternatives(address.paragraph, address.token) else {
        return writeln!(errors, "alternatives unavailable for {address}");
    };
    writeln!(output, "alternatives for {address}:")?;
    for (index, candidate) in alternatives.iter().enumerate() {
        writeln!(
            output,
            "  {}  id={}  probability={:.6}  text={:?}",
            index + 1,
            candidate.token_id(),
            candidate.probability(),
            candidate.text()
        )?;
    }
    Ok(())
}

pub(crate) fn alternative_address(
    document: &Document,
    navigation: &NavigationState,
    addressed: Option<TokenAddress>,
) -> Result<TokenAddress, String> {
    if let Some(address) = addressed {
        return document
            .token(address.paragraph, address.token)
            .map(|_| address)
            .ok_or_else(|| format!("unknown token {address}"));
    }
    if navigation.selection().is_some() {
        let (start, end) = navigation
            .selected_token_range(document)
            .map_err(|error| error.to_string())?;
        if start != end {
            return Err("alternatives require exactly one selected token".into());
        }
        return Ok(start);
    }
    navigation
        .current_token_address(document)
        .map_err(|error| error.to_string())
}

pub(crate) fn edit_range(
    document: &Document,
    navigation: &NavigationState,
    addressed: Option<(TokenAddress, TokenAddress)>,
) -> Result<(TokenAddress, TokenAddress), crate::navigation::NavigationError> {
    addressed.map_or_else(|| navigation.selected_token_range(document), Ok)
}

pub(crate) fn chunk_prefix(
    document: &Document,
    address: TokenAddress,
    through: usize,
) -> Option<String> {
    let (marker, _) = document.chunk_for_token(address.paragraph, address.token)?;
    let paragraph = document.paragraph(address.paragraph)?;
    let start = marker
        .checked_sub(2)
        .map_or(0, |i| paragraph.chunk_boundaries()[i].after_tokens());
    Some(
        paragraph.tokens()[start..through]
            .iter()
            .map(|token| token.text())
            .collect(),
    )
}

pub(crate) fn resolve_current_chunk(
    document: &Document,
    navigation: &NavigationState,
) -> Option<(usize, usize)> {
    let (start, end) = if navigation.selection().is_some() {
        navigation.selected_token_range(document).ok()?
    } else {
        let value = navigation.current_token_address(document).ok()?;
        (value, value)
    };
    let left = document.chunk_for_token(start.paragraph, start.token)?;
    let right = document.chunk_for_token(end.paragraph, end.token)?;
    (start.paragraph == end.paragraph && left.0 == right.0).then_some((start.paragraph, left.0))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_corrected_refresh(
    document: &mut Document,
    navigation: &mut NavigationState,
    recognizer: &mut Option<RecognizerSession>,
    language: &str,
    paragraph: usize,
    token: usize,
    intended: String,
    chosen: Option<i32>,
    output: &mut impl Write,
    errors: &mut impl Write,
) -> io::Result<()> {
    let Some(session) = recognizer.as_ref() else {
        return writeln!(
            errors,
            "recognition requires a model: start with --model MODEL or use: model PATH"
        );
    };
    let mut forced = match session.tokenize(&intended) {
        Ok(v) => v,
        Err(e) => return writeln!(errors, "recognition failed: {e}"),
    };
    if let Some(id) = chosen {
        forced.push(id);
    } else {
        match session.render_tokens(&forced) {
            Ok(rendered) if rendered == intended => {}
            Ok(_) => {
                return writeln!(
                    errors,
                    "recognition failed: tokenizer did not reproduce the forced prefix"
                )
            }
            Err(e) => return writeln!(errors, "recognition failed: {e}"),
        }
    }
    let marker = document
        .chunk_for_token(paragraph, token)
        .expect("address was resolved")
        .0;
    run_refresh(
        document, navigation, recognizer, language, paragraph, marker, forced, output, errors,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_refresh(
    document: &mut Document,
    navigation: &mut NavigationState,
    recognizer: &mut Option<RecognizerSession>,
    language: &str,
    paragraph: usize,
    marker: usize,
    forced: Vec<i32>,
    output: &mut impl Write,
    errors: &mut impl Write,
) -> io::Result<()> {
    let Some(session) = recognizer.as_mut() else {
        return writeln!(
            errors,
            "recognition requires a model: start with --model MODEL or use: model PATH"
        );
    };
    let Some(boundary) = document.chunk_marker(paragraph, marker) else {
        return writeln!(
            errors,
            "refresh failed: unknown chunk marker {paragraph}@{marker}"
        );
    };
    let chunk_id = boundary.chunk_id().to_string();
    let Some(mapping) = document.chunk_audio_mapping(&chunk_id) else {
        return writeln!(errors, "refresh failed: chunk has no usable audio mapping");
    };
    if mapping.alignment() == crate::document::AlignmentState::Unavailable {
        return writeln!(errors, "refresh failed: chunk audio mapping is unavailable");
    }
    let range = mapping.range();
    let source_id = mapping.source_id().to_string();
    let Some(source) = document.audio_source(&source_id) else {
        return writeln!(errors, "refresh failed: audio source is missing");
    };
    let Some(path) = source.path() else {
        return writeln!(errors, "refresh failed: audio source has no local path");
    };
    let wav = match read_canonical_wav(path) {
        Ok(v) => v,
        Err(e) => return writeln!(errors, "refresh failed: {e}"),
    };
    if source
        .sha256()
        .is_some_and(|hash| hash != wav.source_sha256)
    {
        return writeln!(errors, "refresh failed: audio source identity changed");
    }
    if source
        .canonical_sample_count()
        .is_some_and(|n| n != wav.samples.len() as u64)
    {
        return writeln!(errors, "refresh failed: canonical audio length changed");
    }
    let facts = SourceFacts {
        sha256: wav.source_sha256,
        sample_rate_hz: wav.sample_rate_hz,
        channels: wav.channels,
        decoded_sample_count: wav.samples.len() as u64,
    };
    let requested = forced.clone();
    let Some(revision) = document
        .recognition_runs()
        .iter()
        .map(|run| run.revision)
        .max()
        .unwrap_or(0)
        .checked_add(1)
    else {
        return writeln!(
            errors,
            "refresh failed: recognition revision cannot be increased"
        );
    };
    let run = match session.refresh_chunk(
        ChunkRefreshRequest {
            source: facts,
            chunk_range: range,
            language: language.into(),
            forced_tokens: forced,
            revision,
        },
        &wav.samples,
    ) {
        Ok(run) => run,
        Err(e) => return writeln!(errors, "refresh failed: {e}"),
    };
    let decoded = run
        .segments
        .iter()
        .flat_map(|s| &s.tokens)
        .map(|t| t.token_id)
        .collect::<Vec<_>>();
    if !requested.is_empty() && !decoded.starts_with(&requested) {
        return writeln!(
            errors,
            "refresh failed: decoder did not preserve the forced prefix"
        );
    }
    match document.install_chunk_recognition(paragraph, marker, run) {
        Ok(()) => {
            *navigation = NavigationState::new(document);
            if document.token(paragraph, 1).is_some() {
                let _ = navigation.move_to(
                    document,
                    &Address::Token(TokenAddress {
                        paragraph,
                        token: 1,
                    }),
                );
            }
            writeln!(output, "refreshed {paragraph}@{marker}")
        }
        Err(e) => writeln!(errors, "refresh failed: {e}"),
    }
}

pub(crate) fn apply_chunk_split(
    document: &mut Document,
    navigation: &mut NavigationState,
    addressed: Option<TokenAddress>,
    after: bool,
    output: &mut impl Write,
    errors: &mut impl Write,
) -> io::Result<()> {
    let address = match addressed.map_or_else(|| navigation.current_token_address(document), Ok) {
        Ok(address) => address,
        Err(error) => return writeln!(errors, "split failed: {error}"),
    };
    match document.split_chunk(address.paragraph, address.token, after) {
        Ok(result) => {
            *navigation = NavigationState::new(document);
            if let Some(marker) = result.marker {
                navigation
                    .move_to(
                        document,
                        &Address::Marker {
                            paragraph: result.paragraph,
                            marker,
                        },
                    )
                    .expect("chunk split reports a current marker");
                if result.created {
                    writeln!(
                        output,
                        "split chunk {} {address}; new boundary {}@{marker}",
                        if after { "after" } else { "before" },
                        result.paragraph
                    )
                } else {
                    writeln!(
                        output,
                        "chunk boundary already exists at {}@{marker}",
                        result.paragraph
                    )
                }
            } else {
                writeln!(output, "chunk boundary already exists at paragraph start")
            }
        }
        Err(error) => writeln!(errors, "split failed: {error}"),
    }
}

pub(crate) fn apply_paragraph_split(
    document: &mut Document,
    navigation: &mut NavigationState,
    addressed: Option<(usize, usize)>,
    output: &mut impl Write,
    errors: &mut impl Write,
) -> io::Result<()> {
    let (paragraph, marker) =
        match addressed.map_or_else(|| navigation.current_marker_address(document), Ok) {
            Ok(address) => address,
            Err(error) => return writeln!(errors, "paragraph split failed: {error}"),
        };
    match document.split_paragraph(paragraph, marker) {
        Ok(result) => {
            *navigation = NavigationState::new(document);
            if document.token(result.right_paragraph, 1).is_some() {
                navigation
                    .move_to(
                        document,
                        &Address::Token(TokenAddress {
                            paragraph: result.right_paragraph,
                            token: 1,
                        }),
                    )
                    .expect("right paragraph begins with a current token");
            }
            writeln!(
                output,
                "split paragraph {paragraph} after {paragraph}@{marker}"
            )
        }
        Err(error) => writeln!(errors, "paragraph split failed: {error}"),
    }
}

pub(crate) fn apply_paragraph_merge(
    document: &mut Document,
    navigation: &mut NavigationState,
    paragraph: usize,
    output: &mut impl Write,
    errors: &mut impl Write,
) -> io::Result<()> {
    match document.merge_paragraphs(paragraph) {
        Ok(result) => {
            *navigation = NavigationState::new(document);
            if document
                .token(result.paragraph, result.first_right_token)
                .is_some()
            {
                navigation
                    .move_to(
                        document,
                        &Address::Token(TokenAddress {
                            paragraph: result.paragraph,
                            token: result.first_right_token,
                        }),
                    )
                    .expect("merged right text has a current token");
            }
            writeln!(
                output,
                "merged paragraphs {paragraph} and {}",
                paragraph + 1
            )
        }
        Err(error) => writeln!(errors, "paragraph merge failed: {error}"),
    }
}

pub(crate) fn apply_chunk_merge(
    document: &mut Document,
    navigation: &mut NavigationState,
    paragraph: usize,
    marker: usize,
    output: &mut impl Write,
    errors: &mut impl Write,
) -> io::Result<()> {
    match document.merge_chunks(paragraph, marker) {
        Ok(merged_marker) => {
            *navigation = NavigationState::new(document);
            navigation
                .move_to(
                    document,
                    &Address::Marker {
                        paragraph,
                        marker: merged_marker,
                    },
                )
                .expect("chunk merge reports its right marker");
            writeln!(output, "merged chunks at {paragraph}@{marker}")
        }
        Err(error) => writeln!(errors, "chunk merge failed: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn all_whitespace_selection_does_not_contribute_boundaries() {
        let document: Document = serde_json::from_value(json!({
            "schema": "rde-document/v1-experimental",
            "id": "document:test",
            "paragraphs": [{
                "id": "paragraph:test",
                "revision": 1,
                "tokens": [{
                    "id": {"kind": "pseudo", "id": "space"},
                    "text": " \t\u{2003}",
                    "origin": {"kind": "pseudo", "reason": "test"}
                }],
                "chunk_boundaries": [{"chunk_id": "chunk", "after_tokens": 1}]
            }]
        }))
        .unwrap();

        assert_eq!(
            preserve_boundary_whitespace(
                &document,
                TokenAddress {
                    paragraph: 1,
                    token: 1,
                },
                TokenAddress {
                    paragraph: 1,
                    token: 1,
                },
                "word".into(),
            ),
            "word"
        );
    }

    #[test]
    fn replacement_keeps_unicode_boundary_whitespace() {
        let document: Document = serde_json::from_value(json!({
            "schema": "rde-document/v1-experimental",
            "id": "document:test",
            "paragraphs": [{
                "id": "paragraph:test",
                "revision": 1,
                "tokens": [{
                    "id": {"kind": "pseudo", "id": "text"},
                    "text": "\t old text \u{2003}",
                    "origin": {"kind": "pseudo", "reason": "test"}
                }],
                "chunk_boundaries": [{"chunk_id": "chunk", "after_tokens": 1}]
            }]
        }))
        .unwrap();
        let address = TokenAddress {
            paragraph: 1,
            token: 1,
        };

        assert_eq!(
            preserve_boundary_whitespace(&document, address, address, "new text".into()),
            "\t new text \u{2003}"
        );
    }
}
