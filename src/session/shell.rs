use std::{
    env,
    ffi::OsString,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
};

use rustyline::{error::ReadlineError, DefaultEditor};

use crate::{
    document::Document,
    navigation::NavigationState,
    persistence::{export_text, load_document, save_document},
    recognition::{RecognitionConfig, RecognitionRun, RecognizerSession},
};

use super::{
    command::{parse_command, SessionCommand},
    editing::{
        alternative_address, apply_history, apply_paragraph_merge, apply_paragraph_split,
        chunk_prefix, edit_range, preserve_boundary_whitespace, render_alternatives,
        resolve_current_chunk, run_corrected_refresh, run_refresh,
    },
    issues::{self, IssueThresholds},
    playback::{repeat_document_replay, start_document_replay, AudioPlayer, ReplayStart},
    render::{
        render_chunk_info, render_issue_paragraph, render_recognition_document, render_token_range,
        render_tokens,
    },
};

pub struct SessionContext<'a> {
    document_path: Option<&'a Path>,
    recognition_run: Option<&'a RecognitionRun>,
    start: SessionStart<'a>,
    model: Option<&'a Path>,
    recognizer: Option<RecognizerSession>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SessionStart<'a> {
    SavedDocument,
    RecognizedAudio { source: &'a Path },
}

impl<'a> SessionContext<'a> {
    pub fn saved_document(document_path: &'a Path, model: Option<&'a Path>) -> Self {
        Self {
            document_path: Some(document_path),
            recognition_run: None,
            start: SessionStart::SavedDocument,
            model,
            recognizer: None,
        }
    }

    pub fn recognized_audio(
        recognition_run: &'a RecognitionRun,
        source: &'a Path,
        document_path: Option<&'a Path>,
        model: Option<&'a Path>,
    ) -> Self {
        Self {
            document_path,
            recognition_run: Some(recognition_run),
            start: SessionStart::RecognizedAudio { source },
            model,
            recognizer: None,
        }
    }

    pub fn recognized_audio_with_recognizer(
        recognition_run: &'a RecognitionRun,
        source: &'a Path,
        document_path: Option<&'a Path>,
        model: Option<&'a Path>,
        recognizer: RecognizerSession,
    ) -> Self {
        Self {
            document_path,
            recognition_run: Some(recognition_run),
            start: SessionStart::RecognizedAudio { source },
            model,
            recognizer: Some(recognizer),
        }
    }
}

struct SessionState<'a> {
    document: Document,
    document_path: Option<std::path::PathBuf>,
    recognition_run: Option<&'a RecognitionRun>,
    start: SessionStart<'a>,
    navigation: NavigationState,
    last_playback: Option<super::playback::LastPlayback>,
    language: String,
    recognizer: Option<RecognizerSession>,
    model_path: Option<PathBuf>,
    issue_thresholds: IssueThresholds,
    color: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SessionControl {
    Continue,
    Exit,
}

impl<'a> SessionState<'a> {
    fn new(
        document: &Document,
        context: SessionContext<'a>,
        output: &mut impl Write,
        errors: &mut impl Write,
        color: bool,
    ) -> io::Result<Option<Self>> {
        let SessionContext {
            document_path,
            recognition_run,
            start,
            model,
            recognizer,
        } = context;
        let document = document.clone();
        let document_path = document_path.map(Path::to_path_buf);
        match start {
            SessionStart::SavedDocument => {
                render_session_document(
                    &document,
                    None,
                    IssueThresholds::default(),
                    color,
                    output,
                )?;
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
            SessionStart::RecognizedAudio { source } => {
                let run = recognition_run.expect("recognized-audio context has a recognition run");
                if color {
                    writeln!(
                        output,
                        "Built {} chunks from {}",
                        run.chunks.len(),
                        source.display()
                    )?;
                    if !run.chunks.is_empty() {
                        writeln!(output)?;
                        render_session_document(
                            &document,
                            None,
                            IssueThresholds::default(),
                            true,
                            output,
                        )?;
                    }
                } else {
                    render_recognition_document(run, &document, source, output)?;
                }
                for fallback in document.token_fallbacks() {
                    let address = document
                        .marker_address_for_chunk(fallback.chunk_id())
                        .map_or_else(
                            || fallback.chunk_id().to_owned(),
                            |(paragraph, chunk)| format!("{paragraph}.{chunk}"),
                        );
                    writeln!(
                        errors,
                        "token alignment unavailable for chunk {address}: {}; using chunk text as one pseudo-token",
                        fallback.reason()
                    )?;
                }
                if run.chunks.is_empty() {
                    return Ok(None);
                }
            }
        }
        writeln!(output, "Type 'help' for session commands.")?;
        let navigation = NavigationState::new(&document);
        let model_path = model.map(Path::to_path_buf);
        if let Some(path) = &model_path {
            std::fs::File::open(path).map_err(|error| {
                io::Error::other(format!(
                    "could not open model '{}': {error}",
                    path.display()
                ))
            })?;
        }
        Ok(Some(Self {
            document,
            document_path,
            recognition_run,
            start,
            navigation,
            last_playback: None,
            language: "auto".to_string(),
            recognizer,
            model_path,
            issue_thresholds: IssueThresholds::default(),
            color,
        }))
    }

    #[allow(clippy::too_many_arguments)]
    fn execute(
        &mut self,
        command: SessionCommand,
        output: &mut impl Write,
        errors: &mut impl Write,
        player: &mut impl AudioPlayer,
        replay_context_samples: u64,
    ) -> io::Result<SessionControl> {
        let Self {
            document,
            document_path,
            recognition_run,
            start,
            navigation,
            last_playback,
            language,
            recognizer,
            model_path,
            issue_thresholds,
            color,
        } = self;
        let append = matches!(&command, SessionCommand::Append { .. });
        match command {
            SessionCommand::NextIssue => {
                issues::navigate(document, navigation, *issue_thresholds, true, output)?
            }
            SessionCommand::PreviousIssue => {
                issues::navigate(document, navigation, *issue_thresholds, false, output)?
            }
            SessionCommand::Issues => issues::list(document, *issue_thresholds, output)?,
            SessionCommand::IssueProbability { level, value } => match (level, value) {
                (None, None) => writeln!(
                    output,
                    "issue-prob red {} orange {}",
                    issue_thresholds.red, issue_thresholds.orange
                )?,
                (Some(level), Some(value)) => {
                    let parsed = value
                        .parse::<f32>()
                        .ok()
                        .filter(|v| (0.0..=1.0).contains(v));
                    let Some(parsed) = parsed else {
                        writeln!(
                            errors,
                            "issue-prob value must be a probability from 0.0 through 1.0"
                        )?;
                        return Ok(SessionControl::Continue);
                    };
                    let mut changed = *issue_thresholds;
                    match level.as_str() {
                        "red" => changed.red = parsed,
                        "orange" => changed.orange = parsed,
                        _ => {
                            writeln!(errors, "issue-prob level must be red or orange")?;
                            return Ok(SessionControl::Continue);
                        }
                    }
                    if changed.red >= changed.orange {
                        writeln!(errors, "issue-prob red must be less than orange")?;
                        return Ok(SessionControl::Continue);
                    }
                    *issue_thresholds = changed;
                    writeln!(
                        output,
                        "issue-prob red {} orange {}",
                        changed.red, changed.orange
                    )?;
                }
                _ => unreachable!(),
            },
            SessionCommand::Ignore(number) => {
                let values = issues::entries(document, *issue_thresholds);
                let selected = if let Some(number) = number {
                    let Some(issue) = values.get(number - 1) else {
                        writeln!(
                            errors,
                            "unknown issue {number}; run issues for current numbers"
                        )?;
                        return Ok(SessionControl::Continue);
                    };
                    if !issue.is_open() {
                        writeln!(errors, "issue {number} is already resolved")?;
                        return Ok(SessionControl::Continue);
                    }
                    navigation
                        .select(
                            document,
                            &crate::navigation::Address::Range {
                                start: crate::navigation::PositionAddress::Token(issue.start),
                                end: crate::navigation::PositionAddress::Token(
                                    crate::navigation::TokenAddress {
                                        token: issue.end.token + 1,
                                        ..issue.end
                                    },
                                ),
                            },
                        )
                        .unwrap();
                    issue.clone()
                } else {
                    let Ok((start, end)) = navigation.selected_token_range(document) else {
                        writeln!(errors,"ignore requires the current selection to equal one complete open issue")?;
                        return Ok(SessionControl::Continue);
                    };
                    let Some(issue) = values
                        .into_iter()
                        .find(|i| i.is_open() && i.start == start && i.end == end)
                    else {
                        writeln!(errors,"ignore requires the current selection to equal one complete open issue")?;
                        return Ok(SessionControl::Continue);
                    };
                    issue
                };
                document.resolve_issue(selected.token_ids);
                writeln!(
                    output,
                    "resolved {},{}.{}.{}",
                    selected.start,
                    selected.end.paragraph,
                    selected.end.chunk,
                    selected.end.token + 1
                )?;
                issues::navigate(document, navigation, *issue_thresholds, true, output)?;
            }
            SessionCommand::Unignore(number) => {
                let values = issues::entries(document, *issue_thresholds);
                let Some(issue) = values.get(number - 1) else {
                    writeln!(
                        errors,
                        "unknown issue {number}; run issues for current numbers"
                    )?;
                    return Ok(SessionControl::Continue);
                };
                let Some(index) = issue.resolved_index else {
                    writeln!(errors, "issue {number} is open")?;
                    return Ok(SessionControl::Continue);
                };
                let issue = issue.clone();
                navigation
                    .select(
                        document,
                        &crate::navigation::Address::Range {
                            start: crate::navigation::PositionAddress::Token(issue.start),
                            end: crate::navigation::PositionAddress::Token(
                                crate::navigation::TokenAddress {
                                    token: issue.end.token + 1,
                                    ..issue.end
                                },
                            ),
                        },
                    )
                    .unwrap();
                document.reopen_issue(index);
                writeln!(
                    output,
                    "reopened {},{}.{}.{}",
                    issue.start,
                    issue.end.paragraph,
                    issue.end.chunk,
                    issue.end.token + 1
                )?;
            }
            SessionCommand::Print(None) => render_session_document(
                document,
                Some(navigation),
                *issue_thresholds,
                *color,
                output,
            )?,
            SessionCommand::Print(Some(number)) => match document.paragraph(number) {
                Some(paragraph) => render_issue_paragraph(
                    document,
                    paragraph,
                    number,
                    Some(navigation),
                    *issue_thresholds,
                    *color,
                    output,
                )?,
                None => writeln!(errors, "unknown paragraph {number}")?,
            },
            SessionCommand::Move(address) => match navigation.move_to(document, &address) {
                Ok(()) => writeln!(output, "position {address}")?,
                Err(error) => writeln!(errors, "{error}")?,
            },
            SessionCommand::Select(address) => match navigation.select(document, &address) {
                Ok(()) => writeln!(output, "selected {address}")?,
                Err(error) => writeln!(errors, "{error}")?,
            },
            SessionCommand::Tokens(Some(number)) => match document.paragraph(number) {
                Some(paragraph) => render_tokens(
                    document,
                    paragraph,
                    number,
                    *issue_thresholds,
                    *color,
                    output,
                )?,
                None => writeln!(errors, "unknown paragraph {number}")?,
            },
            SessionCommand::Tokens(None) => match navigation.selected_token_endpoints(document) {
                Ok((start, end)) => {
                    render_selected_tokens(document, start, end, *issue_thresholds, *color, output)?
                }
                Err(_) => writeln!(
                    errors,
                    "tokens requires a selection containing tokens or a paragraph position N"
                )?,
            },
            SessionCommand::Alternatives { address } => {
                render_alternatives(document, navigation, address, output, errors)?
            }
            SessionCommand::Mark { address, remove } => {
                let target = address.map_or_else(|| navigation.current_token_address(document), Ok);
                match target {
                    Ok(address) => {
                        let global = document
                            .paragraph_token_number(address.paragraph, address.chunk, address.token)
                            .unwrap();
                        let result = if remove {
                            document.unmark_attention(address.paragraph, global)
                        } else {
                            document.mark_attention(address.paragraph, global)
                        };
                        match result {
                            Ok(()) => writeln!(
                                output,
                                "{} {address}",
                                if remove { "unmarked" } else { "marked" }
                            )?,
                            Err(error) => writeln!(
                                errors,
                                "{} failed: {error}",
                                if remove { "unmark" } else { "mark" }
                            )?,
                        }
                    }
                    Err(error) => writeln!(
                        errors,
                        "{} requires a current token: {error}",
                        if remove { "unmark" } else { "mark" }
                    )?,
                }
            }
            SessionCommand::ChooseAlternative { address, candidate } => {
                let address = match alternative_address(document, navigation, address) {
                    Ok(v) => v,
                    Err(e) => {
                        writeln!(errors, "alternative failed: {e}")?;
                        return Ok(SessionControl::Continue);
                    }
                };
                let global = document
                    .paragraph_token_number(address.paragraph, address.chunk, address.token)
                    .unwrap();
                let Some(token_id) =
                    document.alternative_token_id(address.paragraph, global, candidate)
                else {
                    writeln!(
                        errors,
                        "alternative failed: unknown alternative {candidate}"
                    )?;
                    return Ok(SessionControl::Continue);
                };
                let prefix = chunk_prefix(document, address, address.token - 1).unwrap();
                if !ensure_recognizer(recognizer, model_path, language, errors)? {
                    return Ok(SessionControl::Continue);
                }
                run_corrected_refresh(
                    document,
                    navigation,
                    recognizer,
                    language,
                    address.paragraph,
                    address.chunk,
                    prefix,
                    Some(token_id),
                    output,
                    errors,
                )?;
            }
            SessionCommand::Insert { address, text } | SessionCommand::Append { address, text } => {
                let after = append;
                let Some(count) = document.chunk_token_count(address.paragraph, address.chunk)
                else {
                    writeln!(
                        errors,
                        "insert failed: unknown chunk {}.{}",
                        address.paragraph, address.chunk
                    )?;
                    return Ok(SessionControl::Continue);
                };
                if count == 0 || address.token > count + 1 {
                    writeln!(errors, "insert failed: unknown token position {address}")?;
                    return Ok(SessionControl::Continue);
                }
                if after
                    && document
                        .chunk_token(address.paragraph, address.chunk, address.token)
                        .is_none()
                {
                    writeln!(
                        errors,
                        "append failed: position {address} has no following token"
                    )?;
                    return Ok(SessionControl::Continue);
                }
                let through = if after {
                    address.token
                } else {
                    address.token - 1
                };
                let intended = format!(
                    "{}{}",
                    chunk_prefix(document, address, through).unwrap_or_default(),
                    text
                );
                if !ensure_recognizer(recognizer, model_path, language, errors)? {
                    return Ok(SessionControl::Continue);
                }
                run_corrected_refresh(
                    document,
                    navigation,
                    recognizer,
                    language,
                    address.paragraph,
                    address.chunk,
                    intended,
                    None,
                    output,
                    errors,
                )?;
            }
            SessionCommand::Replace { range, replacement } => {
                let (start, end) = match edit_range(document, navigation, range) {
                    Ok(v) => v,
                    Err(e) => {
                        writeln!(errors, "edit failed: {e}")?;
                        return Ok(SessionControl::Continue);
                    }
                };
                let text = if replacement.exact_boundaries {
                    replacement.text
                } else {
                    preserve_boundary_whitespace(document, start, end, replacement.text)
                };
                let intended = format!(
                    "{}{}",
                    chunk_prefix(document, start, start.token - 1).unwrap_or_default(),
                    text
                );
                if !ensure_recognizer(recognizer, model_path, language, errors)? {
                    return Ok(SessionControl::Continue);
                }
                run_corrected_refresh(
                    document,
                    navigation,
                    recognizer,
                    language,
                    start.paragraph,
                    start.chunk,
                    intended,
                    None,
                    output,
                    errors,
                )?;
            }
            SessionCommand::Delete { range } => {
                if let Err(error) = edit_range(document, navigation, range) {
                    writeln!(errors, "delete failed: {error}")?;
                    return Ok(SessionControl::Continue);
                }
                writeln!(
                    errors,
                    "delete is disabled; deletion of audio-backed text is not implemented"
                )?
            }
            SessionCommand::Refresh { marker } => {
                let resolved = marker.or_else(|| resolve_current_chunk(document, navigation));
                let Some((paragraph, marker)) = resolved else {
                    writeln!(
                        errors,
                        "refresh requires a current chunk or a selection covering one chunk"
                    )?;
                    return Ok(SessionControl::Continue);
                };
                if document.chunk_token_count(paragraph, marker).is_none() {
                    writeln!(errors, "refresh failed: unknown chunk {paragraph}.{marker}")?;
                    return Ok(SessionControl::Continue);
                }
                if !ensure_recognizer(recognizer, model_path, language, errors)? {
                    return Ok(SessionControl::Continue);
                }
                run_refresh(
                    document,
                    navigation,
                    recognizer,
                    language,
                    paragraph,
                    marker,
                    Vec::new(),
                    output,
                    errors,
                )?;
            }
            SessionCommand::Model(path) => match path {
                None => writeln!(
                    output,
                    "model {}",
                    model_path
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "(none)".into())
                )?,
                Some(path) => match std::fs::File::open(&path) {
                    Ok(_) => {
                        *recognizer = None;
                        *model_path = Some(path.clone());
                        writeln!(output, "model {} (loads on first use)", path.display())?;
                    }
                    Err(error) => {
                        writeln!(errors, "could not open model '{}': {error}", path.display())?
                    }
                },
            },
            SessionCommand::Language(value) => match value {
                None => writeln!(output, "language {language}")?,
                Some(value) => {
                    *language = value;
                    if let Some(r) = recognizer {
                        r.set_language(language.clone());
                    }
                    writeln!(output, "language {language}")?;
                }
            },
            SessionCommand::SplitParagraph { marker } => {
                apply_paragraph_split(document, navigation, marker, output, errors)?
            }
            SessionCommand::MergeParagraph(paragraph) => {
                apply_paragraph_merge(document, navigation, paragraph, output, errors)?
            }
            SessionCommand::Undo(count) => {
                apply_history(document, navigation, count, false, output)?
            }
            SessionCommand::Redo(count) => {
                apply_history(document, navigation, count, true, output)?
            }
            SessionCommand::Play { address, speed } => {
                if let Some(value) = start_document_replay(
                    document,
                    navigation,
                    address.as_ref(),
                    ReplayStart {
                        context_samples: replay_context_samples,
                        speed,
                        require_file: matches!(*start, SessionStart::SavedDocument),
                    },
                    player,
                    output,
                    errors,
                )? {
                    *last_playback = Some(value);
                }
            }
            SessionCommand::Replay { speed } => repeat_document_replay(
                document,
                last_playback.as_ref(),
                speed,
                player,
                output,
                errors,
            )?,
            SessionCommand::Stop => match player.stop() {
                Ok(true) => writeln!(output, "playback stopped")?,
                Ok(false) => writeln!(errors, "nothing is playing")?,
                Err(error) => writeln!(errors, "could not stop playback: {error}")?,
            },
            SessionCommand::Info { paragraph, chunk } => {
                let Some(marker) = document.chunk_marker(paragraph, chunk) else {
                    writeln!(errors, "unknown chunk {paragraph}.{chunk}")?;
                    return Ok(SessionControl::Continue);
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
                        None => {
                            writeln!(errors, "chunk data is unavailable for {paragraph}.{chunk}")?
                        }
                    }
                } else {
                    writeln!(
                        errors,
                        "recognition information is not stored in this document baseline"
                    )?;
                }
            }
            SessionCommand::Save(path) => {
                let path = path.or_else(|| document_path.clone());
                let Some(path) = path else {
                    writeln!(errors, "save requires a document path")?;
                    return Ok(SessionControl::Continue);
                };
                match save_document(&path, document) {
                    Ok(()) => {
                        *document_path = Some(path.clone());
                        writeln!(output, "saved {}", path.display())?;
                    }

                    Err(error) => writeln!(errors, "{error}")?,
                }
            }
            SessionCommand::Export(path) => match export_text(&path, document) {
                Ok(()) => writeln!(output, "exported {}", path.display())?,
                Err(error) => writeln!(errors, "{error}")?,
            },
            SessionCommand::Load(path) => match load_document(&path) {
                Ok(loaded) => {
                    *document = loaded;
                    *document_path = Some(path);
                    *recognition_run = None;
                    *start = SessionStart::SavedDocument;
                    *navigation = NavigationState::new(document);
                    *last_playback = None;
                    writeln!(
                        output,
                        "loaded {}",
                        document_path.as_ref().unwrap().display()
                    )?;
                    render_session_document(
                        document,
                        Some(navigation),
                        *issue_thresholds,
                        *color,
                        output,
                    )?;
                }
                Err(error) => writeln!(errors, "{error}")?,
            },
            SessionCommand::Help => render_help(output)?,
            SessionCommand::Quit => return Ok(SessionControl::Exit),
            SessionCommand::Empty => {}
        }
        Ok(SessionControl::Continue)
    }
}

fn render_session_document(
    document: &Document,
    navigation: Option<&NavigationState>,
    settings: IssueThresholds,
    color: bool,
    output: &mut impl Write,
) -> io::Result<()> {
    for (index, paragraph) in document.paragraphs().iter().enumerate() {
        render_issue_paragraph(
            document,
            paragraph,
            index + 1,
            navigation,
            settings,
            color,
            output,
        )?;
        if index + 1 < document.paragraphs().len() {
            writeln!(output)?;
        }
    }
    Ok(())
}

fn ensure_recognizer(
    recognizer: &mut Option<RecognizerSession>,
    model_path: &Option<PathBuf>,
    language: &str,
    errors: &mut impl Write,
) -> io::Result<bool> {
    if recognizer.is_some() {
        return Ok(true);
    }
    let Some(path) = model_path else {
        writeln!(
            errors,
            "recognition requires a model: start with --model MODEL or use: model PATH"
        )?;
        return Ok(false);
    };
    match RecognizerSession::load(
        path,
        &RecognitionConfig {
            language: language.into(),
            ..RecognitionConfig::default()
        },
    ) {
        Ok(session) => {
            *recognizer = Some(session);
            Ok(true)
        }
        Err(error) => {
            writeln!(errors, "could not load model: {error}")?;
            Ok(false)
        }
    }
}

fn render_selected_tokens(
    document: &Document,
    start: crate::navigation::TokenAddress,
    end: crate::navigation::TokenAddress,
    settings: IssueThresholds,
    color: bool,
    output: &mut impl Write,
) -> io::Result<()> {
    let offsets = document
        .paragraphs()
        .iter()
        .scan(0usize, |total, paragraph| {
            let offset = *total;
            *total += paragraph.tokens().len();
            Some(offset)
        })
        .collect::<Vec<_>>();
    let total = document
        .paragraphs()
        .iter()
        .map(|p| p.tokens().len())
        .sum::<usize>();
    let first_global = document
        .paragraph_token_number(start.paragraph, start.chunk, start.token)
        .unwrap();
    let last_global = document
        .paragraph_token_number(end.paragraph, end.chunk, end.token)
        .unwrap();
    let first = offsets[start.paragraph - 1] + first_global - 1;
    let last_exclusive = offsets[end.paragraph - 1] + last_global;
    let context_start = first.saturating_sub(5);
    let context_end = last_exclusive.saturating_add(5).min(total);
    for (index, paragraph) in document.paragraphs().iter().enumerate() {
        let paragraph_start = offsets[index];
        let paragraph_end = paragraph_start + paragraph.tokens().len();
        let visible_start = context_start.max(paragraph_start);
        let visible_end = context_end.min(paragraph_end);
        if visible_start < visible_end {
            render_token_range(
                document,
                paragraph,
                index + 1,
                visible_start - paragraph_start,
                visible_end - paragraph_start,
                settings,
                color,
                output,
            )?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn run_session(
    document: &Document,
    context: SessionContext<'_>,
    input: &mut impl BufRead,
    output: &mut impl Write,
    errors: &mut impl Write,
    player: &mut impl AudioPlayer,
    replay_context_samples: u64,
) -> io::Result<()> {
    let Some(mut state) = SessionState::new(document, context, output, errors, false)? else {
        return Ok(());
    };
    loop {
        write!(output, "rde> ")?;
        output.flush()?;
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            return Ok(());
        }
        let command = match parse_command(&line) {
            Ok(command) => command,
            Err(error) => {
                writeln!(errors, "{error}")?;
                continue;
            }
        };
        if state.execute(command, output, errors, player, replay_context_samples)?
            == SessionControl::Exit
        {
            return Ok(());
        }
    }
}

/// Runs a terminal session with editable input and persistent command history.
#[allow(clippy::too_many_arguments)]
pub fn run_readline_session(
    document: &Document,
    context: SessionContext<'_>,
    output: &mut impl Write,
    errors: &mut impl Write,
    player: &mut impl AudioPlayer,
    replay_context_samples: u64,
) -> io::Result<()> {
    let Some(mut state) = SessionState::new(document, context, output, errors, true)? else {
        return Ok(());
    };
    let mut editor = DefaultEditor::new().map_err(readline_io_error)?;
    let history_path = history_path();
    if let Some(path) = &history_path {
        if let Err(error) = editor.load_history(path) {
            if !matches!(error, ReadlineError::Io(ref error) if error.kind() == io::ErrorKind::NotFound)
            {
                writeln!(errors, "could not load command history: {error}")?;
            }
        }
    }

    loop {
        let line = match editor.readline("rde> ") {
            Ok(line) => line,
            Err(ReadlineError::Interrupted) => continue,
            Err(ReadlineError::Eof) => break,
            Err(error) => return Err(readline_io_error(error)),
        };
        if !line.trim().is_empty() {
            let _ = editor.add_history_entry(&line);
        }
        let command = match parse_command(&line) {
            Ok(command) => command,
            Err(error) => {
                writeln!(errors, "{error}")?;
                continue;
            }
        };
        if state.execute(command, output, errors, player, replay_context_samples)?
            == SessionControl::Exit
        {
            break;
        }
    }

    if let Some(path) = history_path {
        let result = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| editor.save_history(&path).map_err(readline_io_error));
        if let Err(error) = result {
            writeln!(errors, "could not save command history: {error}")?;
        }
    }
    Ok(())
}

fn history_path() -> Option<PathBuf> {
    history_path_from(env::var_os("XDG_STATE_HOME"), env::var_os("HOME"))
}

fn history_path_from(xdg_state_home: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    xdg_state_home
        .map(PathBuf::from)
        .or_else(|| home.map(|home| PathBuf::from(home).join(".local/state")))
        .map(|state| state.join("rde/history"))
}

fn readline_io_error(error: ReadlineError) -> io::Error {
    match error {
        ReadlineError::Io(error) => error,
        error => io::Error::other(error),
    }
}

pub(crate) fn render_help(output: &mut impl Write) -> io::Result<()> {
    writeln!(
        output,
        "History: undo | Nundo | redo | Nredo (N is a positive maximum count)"
    )?;
    writeln!(output, "Issues: next | prev | issues | ignore | resolve | Nignore | Nresolve | Nunignore | issue-prob [red|orange VALUE]")?;
    writeln!(output, "Addresses: N is before paragraph N; N.M is before chunk M; N.M.K is before token K. Ranges A,B are half-open.")?;
    writeln!(output, "Token listing: Ntokens lists paragraph N; bare tokens lists the selection plus five tokens on each side")?;
    writeln!(
        output,
        "Document display: print | list | show (short forms: p | l)"
    )?;
    writeln!(output, "Model loading: model [PATH] configures the path; loading waits until recognition is first used")?;
    writeln!(
        output,
        "Alternatives: [N.M.K]choose C and [N.M.K]set C select the same candidate"
    )?;
    writeln!(
        output,
        "Attention: [N.M.K]mark | [N.M.K]unmark; export PATH writes clean paragraph text with flags"
    )?;
    writeln!(
        output,
        "Commands:\n  p | print                  print the document\n  Np                         print paragraph N\n  A                          move to position A\n  A,Bselect | sel | s        select half-open range [A, B)\n  Ntokens                    list paragraph N tokens\n  [N.M.K]alternatives | alts list alternatives for one token/current token\n  [N.M.K]choose C            correct one token and refresh its chunk\n  N.M.Kinsert TEXT           correct before a token (including its end position)\n  N.M.Kappend TEXT           correct after the following token\n  [A,B]replace TEXT          replace a supported one-chunk range and refresh\n                              unquoted keeps selected boundary whitespace\n                              quoted \"TEXT\" controls boundaries exactly\n  [A,B]delete                disabled pending audio-backed deletion\n  [N.M]refresh               re-recognize one complete chunk\n  model [PATH]               show or load the session model\n  language [CODE]            show or set the session language\n  [N.M]parasplit             split paragraph before a chunk/current chunk\n  Nmerge                     merge paragraph N with N+1 exactly\n  [A]play | [A]slowplay      play current/addressed item or range\n  replay | slowreplay        repeat the last audio range\n  stop                       stop active playback\n  N.Minfo                    report recognition information availability\n  save [PATH]                save atomically; default is the opened file\n  load PATH | edit PATH      replace the current document and reset navigation\n  h | help                   show this help\n  q | quit                   leave the session"
    )
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, path::PathBuf};

    use super::{history_path_from, render_help};

    #[test]
    fn command_history_uses_xdg_state_home_when_set() {
        assert_eq!(
            history_path_from(
                Some(OsString::from("/state")),
                Some(OsString::from("/home/user"))
            ),
            Some(PathBuf::from("/state/rde/history"))
        );
    }

    #[test]
    fn command_history_falls_back_to_home_local_state() {
        assert_eq!(
            history_path_from(None, Some(OsString::from("/home/user"))),
            Some(PathBuf::from("/home/user/.local/state/rde/history"))
        );
        assert_eq!(history_path_from(None, None), None);
    }

    #[test]
    fn help_explains_each_session_command_with_examples() {
        let mut output = Vec::new();

        render_help(&mut output).unwrap();

        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("p | print"));
        assert!(output.contains("Np"));
        assert!(output.contains("N.M.K"));
        assert!(output.contains("A,Bselect | sel | s"));
        assert!(output.contains("Ntokens"));
        assert!(output.contains("[A,B]replace TEXT"));
        assert!(output.contains("unquoted keeps selected boundary whitespace"));
        assert!(output.contains("quoted \"TEXT\" controls boundaries exactly"));
        assert!(output.contains("parasplit"));
        assert!(!output.contains("isplit"));
        assert!(!output.contains("@"));
        assert!(output.contains("[A]play | [A]slowplay"));
        assert!(output.contains("[A]slowplay"));
        assert!(output.contains("replay"));
        assert!(output.contains("stop"));
        assert!(output.contains("N.Minfo"));
        assert!(output.contains("save [PATH]"));
        assert!(output.contains("load PATH"));
        assert!(output.contains("edit PATH"));
        assert!(output.contains("h | help"));
        assert!(output.contains("q | quit"));
    }
}
