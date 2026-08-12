//! Line-oriented session for a previously saved visible document.

use std::{
    io::{self, BufRead, Write},
    path::Path,
};

use crate::{
    audition::{
        parse_command, render_paragraph, render_tokens, repeat_document_replay,
        start_document_replay, AudioPlayer, AuditionCommand, ReplacementText, ReplayStart,
    },
    document::{Document, EditedTokenPosition},
    navigation::{Address, NavigationState, TokenAddress},
    persistence::{load_document, save_document},
};

pub fn run_editor_session(
    document: &Document,
    document_path: &Path,
    input: &mut impl BufRead,
    output: &mut impl Write,
    errors: &mut impl Write,
    player: &mut impl AudioPlayer,
    replay_context_samples: u64,
) -> io::Result<()> {
    let mut document = document.clone();
    let mut document_path = document_path.to_path_buf();
    render_document(&document, output)?;

    for source in document.audio_sources() {
        match source.path() {
            None => writeln!(
                errors,
                "audio source '{}' has no local path; replay is unavailable",
                source.id()
            )?,
            Some(path) if !path.is_file() => writeln!(
                errors,
                "audio source '{}' is unavailable at {}; text remains editable",
                source.id(),
                path.display()
            )?,
            Some(_) => {}
        }
    }
    writeln!(output, "Type 'help' for session commands.")?;
    let mut navigation = NavigationState::new(&document);
    let mut last_playback = None;
    loop {
        write!(output, "rde> ")?;
        output.flush()?;
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            return Ok(());
        }
        match parse_command(&line) {
            Ok(AuditionCommand::Print(None)) => {
                render_document_with_navigation(&document, Some(&navigation), output)?
            }
            Ok(AuditionCommand::Print(Some(number))) => match document.paragraph(number) {
                Some(paragraph) => render_paragraph(paragraph, number, Some(&navigation), output)?,
                None => writeln!(errors, "unknown paragraph {number}")?,
            },
            Ok(AuditionCommand::Move(address)) => match navigation.move_to(&document, &address) {
                Ok(()) => writeln!(output, "caret {address}")?,
                Err(error) => writeln!(errors, "{error}")?,
            },
            Ok(AuditionCommand::Select(address)) => match navigation.select(&document, &address) {
                Ok(()) => writeln!(output, "selected {address}")?,
                Err(error) => writeln!(errors, "{error}")?,
            },
            Ok(AuditionCommand::Tokens(number)) => match document.paragraph(number) {
                Some(paragraph) => render_tokens(paragraph, number, output)?,
                None => writeln!(errors, "unknown paragraph {number}")?,
            },
            Ok(AuditionCommand::Alternatives { address }) => {
                render_alternatives(&document, &navigation, address, output, errors)?
            }
            Ok(AuditionCommand::ChooseAlternative { address, candidate }) => apply_alternative(
                &mut document,
                &mut navigation,
                address,
                candidate,
                output,
                errors,
            )?,
            Ok(AuditionCommand::Insert { address, text }) => apply_insert(
                &mut document,
                &mut navigation,
                address,
                false,
                text,
                output,
                errors,
            )?,
            Ok(AuditionCommand::Append { address, text }) => apply_insert(
                &mut document,
                &mut navigation,
                address,
                true,
                text,
                output,
                errors,
            )?,
            Ok(AuditionCommand::Replace { range, replacement }) => apply_replace(
                &mut document,
                &mut navigation,
                range,
                replacement,
                output,
                errors,
            )?,
            Ok(AuditionCommand::Delete { range }) => {
                apply_delete(&mut document, &mut navigation, range, output, errors)?
            }
            Ok(AuditionCommand::SplitChunk { address, after }) => apply_chunk_split(
                &mut document,
                &mut navigation,
                address,
                after,
                output,
                errors,
            )?,
            Ok(AuditionCommand::SplitParagraph { marker }) => {
                apply_paragraph_split(&mut document, &mut navigation, marker, output, errors)?
            }
            Ok(AuditionCommand::MergeParagraph(paragraph)) => {
                apply_paragraph_merge(&mut document, &mut navigation, paragraph, output, errors)?
            }
            Ok(AuditionCommand::MergeChunks { paragraph, marker }) => apply_chunk_merge(
                &mut document,
                &mut navigation,
                paragraph,
                marker,
                output,
                errors,
            )?,
            Ok(AuditionCommand::Play { address, speed }) => {
                if let Some(value) = start_document_replay(
                    &document,
                    &navigation,
                    address.as_ref(),
                    ReplayStart {
                        context_samples: replay_context_samples,
                        speed,
                        require_file: true,
                    },
                    player,
                    output,
                    errors,
                )? {
                    last_playback = Some(value);
                }
            }
            Ok(AuditionCommand::Replay { speed }) => repeat_document_replay(
                &document,
                last_playback.as_ref(),
                speed,
                player,
                output,
                errors,
            )?,
            Ok(AuditionCommand::Stop) => match player.stop() {
                Ok(true) => writeln!(output, "playback stopped")?,
                Ok(false) => writeln!(errors, "nothing is playing")?,
                Err(error) => writeln!(errors, "could not stop playback: {error}")?,
            },
            Ok(AuditionCommand::Info { paragraph, chunk }) => {
                if document.chunk_marker(paragraph, chunk).is_none() {
                    writeln!(errors, "unknown chunk marker {paragraph}@{chunk}")?;
                } else {
                    writeln!(
                        errors,
                        "recognition information is not stored in this document baseline"
                    )?;
                }
            }
            Ok(AuditionCommand::Save(path)) => {
                let path = path.unwrap_or_else(|| document_path.clone());
                match save_document(&path, &document) {
                    Ok(()) => {
                        document_path = path.clone();
                        writeln!(output, "saved {}", path.display())?;
                    }

                    Err(error) => writeln!(errors, "{error}")?,
                }
            }
            Ok(AuditionCommand::Load(path)) => match load_document(&path) {
                Ok(loaded) => {
                    document = loaded;
                    document_path = path;
                    navigation = NavigationState::new(&document);
                    last_playback = None;
                    writeln!(output, "loaded {}", document_path.display())?;
                    render_document_with_navigation(&document, Some(&navigation), output)?;
                }
                Err(error) => writeln!(errors, "{error}")?,
            },
            Ok(AuditionCommand::Help) => render_editor_help(output)?,
            Ok(AuditionCommand::Quit) => return Ok(()),
            Ok(AuditionCommand::Empty) => {}
            Err(error) => writeln!(errors, "{error}")?,
        }
    }
}

pub(crate) fn apply_insert(
    document: &mut Document,
    navigation: &mut NavigationState,
    address: TokenAddress,
    after: bool,
    text: String,
    output: &mut impl Write,
    errors: &mut impl Write,
) -> io::Result<()> {
    match document.insert_text(address.paragraph, address.token, after, text) {
        Ok(position) => {
            refresh_navigation(document, navigation, Some(position));
            writeln!(
                output,
                "{} at {position}",
                if after { "appended" } else { "inserted" }
            )
        }
        Err(error) => writeln!(errors, "edit failed: {error}"),
    }
}

pub(crate) fn apply_replace(
    document: &mut Document,
    navigation: &mut NavigationState,
    range: Option<(TokenAddress, TokenAddress)>,
    replacement: ReplacementText,
    output: &mut impl Write,
    errors: &mut impl Write,
) -> io::Result<()> {
    let (start, end) = match edit_range(document, navigation, range) {
        Ok(range) => range,
        Err(error) => return writeln!(errors, "edit failed: {error}"),
    };
    if start.paragraph != end.paragraph {
        return writeln!(
            errors,
            "edit failed: text-edit ranges cannot cross paragraph boundaries"
        );
    }
    let text = if replacement.exact_boundaries {
        replacement.text
    } else {
        preserve_boundary_whitespace(document, start, end, replacement.text)
    };
    match document.replace_text(start.paragraph, start.token, end.paragraph, end.token, text) {
        Ok(position) => {
            refresh_navigation(document, navigation, Some(position));
            writeln!(output, "replaced {start},{end}")
        }
        Err(error) => writeln!(errors, "edit failed: {error}"),
    }
}

fn preserve_boundary_whitespace(
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

pub(crate) fn apply_delete(
    document: &mut Document,
    navigation: &mut NavigationState,
    range: Option<(TokenAddress, TokenAddress)>,
    output: &mut impl Write,
    errors: &mut impl Write,
) -> io::Result<()> {
    let (start, end) = match edit_range(document, navigation, range) {
        Ok(range) => range,
        Err(error) => return writeln!(errors, "edit failed: {error}"),
    };
    match document.delete_text(start.paragraph, start.token, end.paragraph, end.token) {
        Ok(position) => {
            refresh_navigation(document, navigation, position);
            writeln!(output, "deleted {start},{end}")
        }
        Err(error) => writeln!(errors, "edit failed: {error}"),
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

pub(crate) fn apply_alternative(
    document: &mut Document,
    navigation: &mut NavigationState,
    addressed: Option<TokenAddress>,
    candidate: usize,
    output: &mut impl Write,
    errors: &mut impl Write,
) -> io::Result<()> {
    let address = match alternative_address(document, navigation, addressed) {
        Ok(address) => address,
        Err(error) => return writeln!(errors, "alternative failed: {error}"),
    };
    match document.choose_alternative(address.paragraph, address.token, candidate) {
        Ok(position) => {
            refresh_navigation(document, navigation, Some(position));
            writeln!(output, "chose alternative {candidate} for {address}")
        }
        Err(error) => writeln!(errors, "alternative failed: {error}"),
    }
}

fn alternative_address(
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

fn edit_range(
    document: &Document,
    navigation: &NavigationState,
    addressed: Option<(TokenAddress, TokenAddress)>,
) -> Result<(TokenAddress, TokenAddress), crate::navigation::NavigationError> {
    addressed.map_or_else(|| navigation.selected_token_range(document), Ok)
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

fn refresh_navigation(
    document: &Document,
    navigation: &mut NavigationState,
    preferred: Option<EditedTokenPosition>,
) {
    *navigation = NavigationState::new(document);
    if let Some(position) = preferred {
        navigation
            .move_to(
                document,
                &Address::Token(TokenAddress {
                    paragraph: position.paragraph,
                    token: position.token,
                }),
            )
            .expect("an edit outcome points to its new document revision");
    }
}

impl std::fmt::Display for EditedTokenPosition {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}.{}", self.paragraph, self.token)
    }
}

fn render_document(document: &Document, output: &mut impl Write) -> io::Result<()> {
    render_document_with_navigation(document, None, output)
}

pub(crate) fn render_document_with_navigation(
    document: &Document,
    navigation: Option<&NavigationState>,
    output: &mut impl Write,
) -> io::Result<()> {
    for (index, paragraph) in document.paragraphs().iter().enumerate() {
        render_paragraph(paragraph, index + 1, navigation, output)?;
        if index + 1 < document.paragraphs().len() {
            writeln!(output)?;
        }
    }
    Ok(())
}

fn render_editor_help(output: &mut impl Write) -> io::Result<()> {
    writeln!(
        output,
        "Commands:\n  p | print                  print the document\n  Mp                         print paragraph M\n  M.N                        move caret to a token\n  M@N                        move caret to a chunk marker\n  Aselect | Asel | As        select token/marker range, paragraph, or marker A\n  Mtokens                    list paragraph tokens\n  [M.N]alternatives | alts   list alternatives for one token/current token\n  [M.N]choose N              replace one token with alternative N\n  M.Ninsert TEXT             insert one pseudo-token before M.N\n  M.Nappend TEXT             insert one pseudo-token after M.N\n  [M.N,M.U]replace TEXT      replace range or current token selection\n                              unquoted keeps selected boundary whitespace\n                              quoted \"TEXT\" controls boundaries exactly\n  [M.N,M.U]delete            delete range or current token selection\n  [M.N]split | [M.N]isplit   split chunk before token/current caret\n  [M.N]asplit                split chunk after token/current caret\n  [M@N]parasplit             split paragraph after marker/current marker\n  Mmerge                     merge paragraph M with M+1 exactly\n  M@Nmerge                   merge chunks around marker M@N when legal\n  [A]play | [A]slowplay      play current/addressed text or chunk\n  M@N,M@Uplay                play half-open marker interval [left, right)\n  replay | slowreplay        repeat the last audio range\n  stop                       stop active playback\n  M@Ninfo                    report recognition information availability\n  save [PATH]                save atomically; default is the opened file\n  load PATH | edit PATH      replace the current document and reset navigation\n  h | help                   show this help\n  q | quit                   leave the session"
    )
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
