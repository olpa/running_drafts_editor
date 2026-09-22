//! Correction-driven transcriptions and project history for session commands.

use std::io::{self, Write};

use crate::{
    backend::{
        AudioBackend, BackendError, ChunkRecognitionRequest, CorrectionContext, RecognitionBackend,
    },
    navigation::{tokens_in_range, Address, NavigationState, PositionAddress, TokenAddress},
    project::Project,
};

pub(crate) fn preserve_boundary_whitespace(
    document: &Project,
    start: TokenAddress,
    end: TokenAddress,
    replacement: String,
) -> String {
    let paragraph = document
        .paragraph(start.paragraph)
        .expect("a resolved edit range has a paragraph");
    let first = document
        .paragraph_token_number(start.paragraph, start.chunk, start.token)
        .unwrap();
    let last = document
        .paragraph_token_number(end.paragraph, end.chunk, end.token)
        .unwrap();
    let selected = paragraph.tokens()[first - 1..last]
        .iter()
        .map(|token| token.text())
        .collect::<String>();
    preserve_text_boundary_whitespace(&selected, replacement)
}

pub(crate) fn preserve_text_boundary_whitespace(selected: &str, replacement: String) -> String {
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
    document: &mut Project,
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
    document: &Project,
    navigation: &NavigationState,
    addressed: Option<TokenAddress>,
    output: &mut impl Write,
    errors: &mut impl Write,
) -> io::Result<()> {
    let address = match alternative_address(document, navigation, addressed) {
        Ok(address) => address,
        Err(error) => return writeln!(errors, "alternatives unavailable: {error}"),
    };
    let global = document
        .paragraph_token_number(address.paragraph, address.chunk, address.token)
        .unwrap();
    let Some(alternatives) = document.alternatives(address.paragraph, global) else {
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
    document: &Project,
    navigation: &NavigationState,
    addressed: Option<TokenAddress>,
) -> Result<TokenAddress, String> {
    if let Some(address) = addressed {
        return document
            .chunk_token(address.paragraph, address.chunk, address.token)
            .map(|_| address)
            .ok_or_else(|| format!("unknown token {address}"));
    }
    navigation
        .current_token_address(document)
        .map_err(|error| error.to_string())
}

pub(crate) fn edit_range(
    document: &Project,
    navigation: &NavigationState,
    addressed: Option<Address>,
) -> Result<(TokenAddress, TokenAddress), crate::navigation::NavigationError> {
    let (left, right) = match addressed {
        Some(Address::Range { start, end }) => (start, end),
        _ => navigation.current_range(document)?,
    };
    let tokens = tokens_in_range(document, left, right)?;
    let (Some(start), Some(end)) = (tokens.first(), tokens.last()) else {
        return Err(crate::navigation::NavigationError::NoTokenSelection);
    };
    let complete = crate::navigation::chunks_in_range(document, left, right)?;
    if start.paragraph != end.paragraph || complete.iter().any(|c| c.paragraph != start.paragraph) {
        return Err(crate::navigation::NavigationError::CrossParagraphSelection);
    }
    if start.chunk != end.chunk || complete.iter().any(|c| c.chunk != start.chunk) {
        return Err(crate::navigation::NavigationError::CrossChunkSelection);
    }
    Ok((*start, *end))
}

pub(crate) fn chunk_prefix(
    document: &Project,
    address: TokenAddress,
    through: usize,
) -> Option<String> {
    let paragraph = document.paragraph(address.paragraph)?;
    let (start, end) = document.chunk_token_bounds(address.paragraph, address.chunk)?;
    if through > end - start {
        return None;
    }
    Some(
        paragraph.tokens()[start..start + through]
            .iter()
            .map(|token| token.text())
            .collect(),
    )
}

pub(crate) fn resolve_current_chunk(
    document: &Project,
    navigation: &NavigationState,
) -> Option<(usize, usize)> {
    let address = navigation.current_chunk_address(document).ok()?;
    Some((address.paragraph, address.chunk))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_correction(
    document: &mut Project,
    navigation: &mut NavigationState,
    audio: &mut dyn AudioBackend,
    recognition: &mut dyn RecognitionBackend,
    language: &str,
    paragraph: usize,
    chunk: usize,
    intended: String,
    chosen: Option<i32>,
    output: &mut impl Write,
    errors: &mut impl Write,
) -> io::Result<()> {
    let settings = document.settings().clone();
    debug_assert_eq!(settings.language, language);
    run_transcription(
        document,
        navigation,
        audio,
        recognition,
        &settings,
        paragraph,
        chunk,
        Some(CorrectionContext {
            prefix: intended,
            chosen_token_id: chosen,
        }),
        output,
        errors,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_transcription(
    document: &mut Project,
    navigation: &mut NavigationState,
    audio: &mut dyn AudioBackend,
    recognition: &mut dyn RecognitionBackend,
    settings: &crate::project::TranscriptionSettings,
    paragraph: usize,
    marker: usize,
    correction: Option<CorrectionContext>,
    output: &mut impl Write,
    errors: &mut impl Write,
) -> io::Result<()> {
    if settings.model.is_none() {
        return writeln!(
            errors,
            "transcription requires a model: start with --model MODEL or use: model PATH"
        );
    }
    let Some(boundary) = document.chunk_marker(paragraph, marker) else {
        return writeln!(
            errors,
            "transcription failed: unknown chunk {paragraph}.{marker}"
        );
    };
    let chunk_id = boundary.chunk_id().to_owned();
    let Some(mapping) = document.chunk_audio_mapping(&chunk_id) else {
        return writeln!(
            errors,
            "transcription failed: chunk has no usable audio mapping"
        );
    };
    if mapping.alignment() == crate::document::AlignmentState::Unavailable {
        return writeln!(
            errors,
            "transcription failed: chunk audio mapping is unavailable"
        );
    }
    let run = match recognition.transcribe_chunk(
        audio,
        ChunkRecognitionRequest {
            chunk_id,
            previous_id: document
                .current_transcription(paragraph, marker)
                .expect("a current chunk has a transcription")
                .id
                .clone(),
            revision: document.transcriptions().len() as u64 + 1,
            settings: settings.clone(),
            correction,
        },
    ) {
        Ok(run) => run,
        Err(error) => {
            return match error {
                BackendError::Model(error) => writeln!(errors, "could not load model: {error}"),
                BackendError::MissingModel => writeln!(errors, "{error}"),
                BackendError::NoLocalPath(_) => writeln!(
                    errors,
                    "transcription failed: audio source has no local path"
                ),
                BackendError::UnknownRecording(_) => {
                    writeln!(errors, "transcription failed: audio source is missing")
                }
                _ => writeln!(errors, "transcription failed: {error}"),
            }
        }
    };
    match document.install_transcription(paragraph, marker, run, settings.clone()) {
        Ok(()) => {
            *navigation = NavigationState::new(document);
            let position = if document.chunk_has_tokens(paragraph, marker) == Some(true) {
                PositionAddress::Token(TokenAddress {
                    paragraph,
                    chunk: marker,
                    token: 1,
                })
            } else {
                PositionAddress::Chunk(crate::navigation::ChunkAddress {
                    paragraph,
                    chunk: marker,
                })
            };
            let _ = navigation.move_to(document, &Address::Position(position));
            writeln!(output, "transcribed {paragraph}.{marker}")
        }
        Err(e) => writeln!(errors, "transcription failed: {e}"),
    }
}

pub(crate) fn apply_paragraph_split(
    document: &mut Project,
    navigation: &mut NavigationState,
    addressed: Option<(usize, usize)>,
    output: &mut impl Write,
    errors: &mut impl Write,
) -> io::Result<()> {
    let (paragraph, chunk) = match addressed.map_or_else(
        || {
            navigation
                .current_chunk_address(document)
                .map(|a| (a.paragraph, a.chunk))
        },
        Ok,
    ) {
        Ok(address) => address,
        Err(error) => return writeln!(errors, "paragraph split failed: {error}"),
    };
    if document.chunk_token_count(paragraph, chunk).is_none() {
        return writeln!(
            errors,
            "paragraph split failed: unknown chunk {paragraph}.{chunk}"
        );
    }
    let Some(marker) = chunk.checked_sub(1).filter(|m| *m > 0) else {
        return writeln!(
            errors,
            "paragraph split failed: chunk {paragraph}.{chunk} has no preceding boundary"
        );
    };
    match document.split_paragraph(paragraph, marker) {
        Ok(result) => {
            *navigation = NavigationState::new(document);
            if document.token(result.right_paragraph, 1).is_some() {
                navigation
                    .move_to(
                        document,
                        &Address::Position(PositionAddress::Token(TokenAddress {
                            paragraph: result.right_paragraph,
                            chunk: 1,
                            token: 1,
                        })),
                    )
                    .expect("right paragraph begins with a current token");
            }
            writeln!(
                output,
                "split paragraph {paragraph} before {paragraph}.{chunk}"
            )
        }
        Err(error) => writeln!(errors, "paragraph split failed: {error}"),
    }
}

pub(crate) fn apply_paragraph_merge(
    document: &mut Project,
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
                let (chunk, token) = document
                    .chunk_token_address(result.paragraph, result.first_right_token)
                    .unwrap();
                navigation
                    .move_to(
                        document,
                        &Address::Position(PositionAddress::Token(TokenAddress {
                            paragraph: result.paragraph,
                            chunk,
                            token,
                        })),
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn boundary_whitespace_is_preserved_without_requiring_token_surrogates() {
        assert_eq!(
            preserve_text_boundary_whitespace(" \t\u{2003}", "word".into()),
            "word"
        );
        assert_eq!(
            preserve_text_boundary_whitespace("\t old text \u{2003}", "new".into()),
            "\t new \u{2003}"
        );
    }
}
