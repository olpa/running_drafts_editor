//! Line-oriented session for a previously saved visible document.

use std::{
    io::{self, BufRead, Write},
    path::Path,
};

use crate::{
    audition::{
        parse_command, render_chunk_info, render_paragraph, render_tokens, repeat_document_replay,
        start_document_replay, AudioPlayer, AuditionCommand, ReplayStart,
    },
    chunking::{read_canonical_wav, SourceFacts},
    document::Document,
    navigation::{Address, NavigationState, TokenAddress},
    persistence::{load_document, save_document},
    recognition::{ChunkRefreshRequest, RecognitionConfig, RecognitionRun, RecognizerSession},
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
    run_editor_session_with_model(
        document,
        document_path,
        input,
        output,
        errors,
        player,
        replay_context_samples,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn run_editor_session_with_model(
    document: &Document,
    document_path: &Path,
    input: &mut impl BufRead,
    output: &mut impl Write,
    errors: &mut impl Write,
    player: &mut impl AudioPlayer,
    replay_context_samples: u64,
    model: Option<&Path>,
) -> io::Result<()> {
    run_session(
        document,
        Some(document_path),
        None,
        true,
        true,
        input,
        output,
        errors,
        player,
        replay_context_samples,
        model,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn run_session(
    document: &Document,
    document_path: Option<&Path>,
    recognition_run: Option<&RecognitionRun>,
    require_audio_file: bool,
    render_initial_document: bool,
    input: &mut impl BufRead,
    output: &mut impl Write,
    errors: &mut impl Write,
    player: &mut impl AudioPlayer,
    replay_context_samples: u64,
    model: Option<&Path>,
) -> io::Result<()> {
    let mut document = document.clone();
    let mut document_path = document_path.map(Path::to_path_buf);
    let mut recognition_run = recognition_run;
    let mut require_audio_file = require_audio_file;
    if render_initial_document {
        render_document(&document, output)?;
    }

    if require_audio_file {
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
    }
    writeln!(output, "Type 'help' for session commands.")?;
    let mut navigation = NavigationState::new(&document);
    let mut last_playback = None;
    let mut language = "auto".to_string();
    let mut recognizer = match model {
        Some(path) => match RecognizerSession::load(path, &RecognitionConfig::default()) {
            Ok(value) => Some(value),
            Err(error) => {
                return Err(io::Error::other(format!("could not load model: {error}")));
            }
        },
        None => None,
    };
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
            Ok(AuditionCommand::ChooseAlternative { address, candidate }) => {
                let address = match alternative_address(&document, &navigation, address) {
                    Ok(v) => v,
                    Err(e) => {
                        writeln!(errors, "alternative failed: {e}")?;
                        continue;
                    }
                };
                let Some(token_id) =
                    document.alternative_token_id(address.paragraph, address.token, candidate)
                else {
                    writeln!(
                        errors,
                        "alternative failed: unknown alternative {candidate}"
                    )?;
                    continue;
                };
                let prefix = chunk_prefix(&document, address, address.token - 1).unwrap();
                run_corrected_refresh(
                    &mut document,
                    &mut navigation,
                    &mut recognizer,
                    &language,
                    address.paragraph,
                    address.token,
                    prefix,
                    Some(token_id),
                    output,
                    errors,
                )?;
            }
            Ok(AuditionCommand::Insert { address, text })
            | Ok(AuditionCommand::Append { address, text }) => {
                let after = matches!(parse_command(&line), Ok(AuditionCommand::Append { .. }));
                let through = if after {
                    address.token
                } else {
                    address.token - 1
                };
                let intended = format!(
                    "{}{}",
                    chunk_prefix(&document, address, through).unwrap_or_default(),
                    text
                );
                run_corrected_refresh(
                    &mut document,
                    &mut navigation,
                    &mut recognizer,
                    &language,
                    address.paragraph,
                    address.token,
                    intended,
                    None,
                    output,
                    errors,
                )?;
            }
            Ok(AuditionCommand::Replace { range, replacement }) => {
                let (start, end) = match edit_range(&document, &navigation, range) {
                    Ok(v) => v,
                    Err(e) => {
                        writeln!(errors, "edit failed: {e}")?;
                        continue;
                    }
                };
                if document
                    .chunk_for_token(start.paragraph, start.token)
                    .map(|v| v.1)
                    != document
                        .chunk_for_token(end.paragraph, end.token)
                        .map(|v| v.1)
                {
                    writeln!(
                        errors,
                        "edit failed: text-changing ranges cannot cross chunk boundaries"
                    )?;
                    continue;
                }
                let text = if replacement.exact_boundaries {
                    replacement.text
                } else {
                    preserve_boundary_whitespace(&document, start, end, replacement.text)
                };
                let intended = format!(
                    "{}{}",
                    chunk_prefix(&document, start, start.token - 1).unwrap_or_default(),
                    text
                );
                run_corrected_refresh(
                    &mut document,
                    &mut navigation,
                    &mut recognizer,
                    &language,
                    start.paragraph,
                    start.token,
                    intended,
                    None,
                    output,
                    errors,
                )?;
            }
            Ok(AuditionCommand::Delete { range }) => {
                let _ = range;
                writeln!(
                    errors,
                    "delete is disabled; deletion of audio-backed text is not implemented"
                )?
            }
            Ok(AuditionCommand::Refresh { marker }) => {
                let resolved = marker.or_else(|| resolve_current_chunk(&document, &navigation));
                let Some((paragraph, marker)) = resolved else {
                    writeln!(
                        errors,
                        "refresh requires a token caret or token selection in one chunk"
                    )?;
                    continue;
                };
                run_refresh(
                    &mut document,
                    &mut navigation,
                    &mut recognizer,
                    &language,
                    paragraph,
                    marker,
                    Vec::new(),
                    output,
                    errors,
                )?;
            }
            Ok(AuditionCommand::Model(path)) => match path {
                None => writeln!(
                    output,
                    "model {}",
                    recognizer
                        .as_ref()
                        .map(|r| r.model_path().display().to_string())
                        .unwrap_or_else(|| "(none)".into())
                )?,
                Some(path) => match RecognizerSession::load(
                    &path,
                    &RecognitionConfig {
                        language: language.clone(),
                        ..RecognitionConfig::default()
                    },
                ) {
                    Ok(value) => {
                        recognizer = Some(value);
                        writeln!(output, "model {}", path.display())?;
                    }
                    Err(error) => writeln!(errors, "could not load model: {error}")?,
                },
            },
            Ok(AuditionCommand::Language(value)) => match value {
                None => writeln!(output, "language {language}")?,
                Some(value) => {
                    language = value;
                    if let Some(r) = &mut recognizer {
                        r.set_language(language.clone());
                    }
                    writeln!(output, "language {language}")?;
                }
            },
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
            Ok(AuditionCommand::Undo(count)) => {
                apply_history(&mut document, &mut navigation, count, false, output)?
            }
            Ok(AuditionCommand::Redo(count)) => {
                apply_history(&mut document, &mut navigation, count, true, output)?
            }
            Ok(AuditionCommand::Play { address, speed }) => {
                if let Some(value) = start_document_replay(
                    &document,
                    &navigation,
                    address.as_ref(),
                    ReplayStart {
                        context_samples: replay_context_samples,
                        speed,
                        require_file: require_audio_file,
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
                let Some(marker) = document.chunk_marker(paragraph, chunk) else {
                    writeln!(errors, "unknown chunk marker {paragraph}@{chunk}")?;
                    continue;
                };
                if let Some(run) = recognition_run {
                    match run
                        .chunks
                        .iter()
                        .find(|candidate| candidate.id == marker.chunk_id())
                    {
                        Some(recognition_chunk) => {
                            render_chunk_info(run, recognition_chunk, paragraph, chunk, output)?
                        }
                        None => writeln!(
                            errors,
                            "chunk data is unavailable for marker {paragraph}@{chunk}"
                        )?,
                    }
                } else {
                    writeln!(
                        errors,
                        "recognition information is not stored in this document baseline"
                    )?;
                }
            }
            Ok(AuditionCommand::Save(path)) => {
                let path = path.or_else(|| document_path.clone());
                let Some(path) = path else {
                    writeln!(errors, "save requires a document path")?;
                    continue;
                };
                match save_document(&path, &document) {
                    Ok(()) => {
                        document_path = Some(path.clone());
                        writeln!(output, "saved {}", path.display())?;
                    }

                    Err(error) => writeln!(errors, "{error}")?,
                }
            }
            Ok(AuditionCommand::Load(path)) => match load_document(&path) {
                Ok(loaded) => {
                    document = loaded;
                    document_path = Some(path);
                    recognition_run = None;
                    require_audio_file = true;
                    navigation = NavigationState::new(&document);
                    last_playback = None;
                    writeln!(
                        output,
                        "loaded {}",
                        document_path.as_ref().unwrap().display()
                    )?;
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

fn chunk_prefix(document: &Document, address: TokenAddress, through: usize) -> Option<String> {
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

fn resolve_current_chunk(
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
fn run_corrected_refresh(
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
fn run_refresh(
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

pub(crate) fn render_editor_help(output: &mut impl Write) -> io::Result<()> {
    writeln!(
        output,
        "History: undo | Nundo | redo | Nredo (N is a positive maximum count)"
    )?;
    writeln!(
        output,
        "Commands:\n  p | print                  print the document\n  Mp                         print paragraph M\n  M.N                        move caret to a token\n  M@N                        move caret to a chunk marker\n  Aselect | Asel | As        select token/marker range, paragraph, or marker A\n  Mtokens                    list paragraph tokens\n  [M.N]alternatives | alts   list alternatives for one token/current token\n  [M.N]choose N              correct one token and refresh its chunk\n  M.Ninsert TEXT             correct before M.N and refresh its chunk\n  M.Nappend TEXT             correct after M.N and refresh its chunk\n  [M.N,M.U]replace TEXT      replace a one-chunk range and refresh\n                              unquoted keeps selected boundary whitespace\n                              quoted \"TEXT\" controls boundaries exactly\n  [M.N,M.U]delete            disabled pending audio-backed deletion\n  [M@N]refresh               re-recognize one complete replay chunk\n  model [PATH]               show or load the session model\n  language [CODE]            show or set the session language\n  [M.N]split | [M.N]isplit   split chunk before token/current caret\n  [M.N]asplit                split chunk after token/current caret\n  [M@N]parasplit             split paragraph after marker/current marker\n  Mmerge                     merge paragraph M with M+1 exactly\n  M@Nmerge                   merge chunks around marker M@N when legal\n  [A]play | [A]slowplay      play current/addressed text or chunk\n  M@N,M@Uplay                play half-open marker interval [left, right)\n  replay | slowreplay        repeat the last audio range\n  stop                       stop active playback\n  M@Ninfo                    report recognition information availability\n  save [PATH]                save atomically; default is the opened file\n  load PATH | edit PATH      replace the current document and reset navigation\n  h | help                   show this help\n  q | quit                   leave the session"
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
