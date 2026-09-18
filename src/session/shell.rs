use std::{
    env,
    ffi::OsString,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
};

use rustyline::{error::ReadlineError, DefaultEditor};

use crate::{
    navigation::NavigationState,
    persistence::{export_text, load_project, save_project},
    project::Project,
    transcription::{
        ChunkTranscriber, InitialTranscriptionResult, TranscriberSession, TranscriptionConfig,
        TranscriptionError,
    },
};

use super::{
    command::{parse_command, SessionCommand},
    editing::{
        alternative_address, apply_history, apply_paragraph_merge, apply_paragraph_split,
        chunk_prefix, edit_range, preserve_boundary_whitespace, preserve_text_boundary_whitespace,
        render_alternatives, resolve_current_chunk, run_correction, run_transcription,
    },
    issues::{self, IssueThresholds},
    playback::{repeat_document_replay, start_document_replay, AudioPlayer, ReplayStart},
    render::{
        render_issue_paragraph, render_token_range, render_tokens, render_transcription_document,
    },
};

trait TranscriberFactory {
    fn load(
        &mut self,
        model: &Path,
        config: &TranscriptionConfig,
    ) -> Result<Box<dyn ChunkTranscriber>, TranscriptionError>;
}
struct WhisperFactory;
impl TranscriberFactory for WhisperFactory {
    fn load(
        &mut self,
        model: &Path,
        config: &TranscriptionConfig,
    ) -> Result<Box<dyn ChunkTranscriber>, TranscriptionError> {
        TranscriberSession::load(model, config).map(|s| Box::new(s) as Box<dyn ChunkTranscriber>)
    }
}

pub struct SessionContext<'a> {
    project_path: Option<&'a Path>,
    initial_result: Option<&'a InitialTranscriptionResult>,
    start: SessionStart<'a>,
    model: Option<&'a Path>,
    transcriber: Option<Box<dyn ChunkTranscriber>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SessionStart<'a> {
    SavedDocument,
    TranscribedAudio { source: &'a Path },
}

impl<'a> SessionContext<'a> {
    pub fn saved_project(project_path: &'a Path, model: Option<&'a Path>) -> Self {
        Self {
            project_path: Some(project_path),
            initial_result: None,
            start: SessionStart::SavedDocument,
            model,
            transcriber: None,
        }
    }

    pub fn transcribed_audio(
        initial_result: &'a InitialTranscriptionResult,
        source: &'a Path,
        project_path: Option<&'a Path>,
        model: Option<&'a Path>,
    ) -> Self {
        Self {
            project_path,
            initial_result: Some(initial_result),
            start: SessionStart::TranscribedAudio { source },
            model,
            transcriber: None,
        }
    }

    pub fn transcribed_audio_with_transcriber(
        initial_result: &'a InitialTranscriptionResult,
        source: &'a Path,
        project_path: Option<&'a Path>,
        model: Option<&'a Path>,
        transcriber: TranscriberSession,
    ) -> Self {
        Self {
            project_path,
            initial_result: Some(initial_result),
            start: SessionStart::TranscribedAudio { source },
            model,
            transcriber: Some(Box::new(transcriber)),
        }
    }
}

struct SessionState<'a> {
    project: Project,
    project_path: Option<std::path::PathBuf>,
    initial_result: Option<&'a InitialTranscriptionResult>,
    start: SessionStart<'a>,
    navigation: NavigationState,
    last_playback: Option<super::playback::LastPlayback>,
    language: String,
    transcriber: Option<Box<dyn ChunkTranscriber>>,
    model_path: Option<PathBuf>,
    issue_thresholds: IssueThresholds,
    color: bool,
    factory: Box<dyn TranscriberFactory>,
    startup_model: Option<PathBuf>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SessionControl {
    Continue,
    Exit,
}

impl<'a> SessionState<'a> {
    fn new(
        project: &Project,
        context: SessionContext<'a>,
        output: &mut impl Write,
        errors: &mut impl Write,
        color: bool,
    ) -> io::Result<Option<Self>> {
        let SessionContext {
            project_path,
            initial_result,
            start,
            model,
            transcriber,
        } = context;
        let document = project.clone();
        let initial_model = document.settings().model.clone();
        let startup_model = model
            .map(Path::to_path_buf)
            .filter(|path| Some(path) != initial_model.as_ref());
        let initial_language = document.settings().language.clone();
        let project_path = project_path.map(Path::to_path_buf);
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
            SessionStart::TranscribedAudio { source } => {
                let run = initial_result
                    .expect("transcribed-audio context has an initial transcription result");
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
                    render_transcription_document(run, &document, source, output)?;
                }
                for failure in document.token_alignment_failures() {
                    let address = document
                        .marker_address_for_chunk(failure.chunk_id())
                        .map_or_else(
                            || failure.chunk_id().to_owned(),
                            |(paragraph, chunk)| format!("{paragraph}.{chunk}"),
                        );
                    writeln!(
                        errors,
                        "token alignment unavailable for chunk {address}: {}; preserving transcription text without token positions",
                        failure.reason()
                    )?;
                }
                if run.chunks.is_empty() {
                    return Ok(None);
                }
            }
        }
        writeln!(output, "Type 'help' for session commands.")?;
        let navigation = NavigationState::new(&document);
        let model_path = initial_model;
        Ok(Some(Self {
            project: document,
            project_path,
            initial_result,
            start,
            navigation,
            last_playback: None,
            language: initial_language,
            transcriber,
            model_path,
            issue_thresholds: IssueThresholds::default(),
            color,
            factory: Box::new(WhisperFactory),
            startup_model,
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
        self.sync_settings();
        let Self {
            project: document,
            project_path,
            initial_result,
            start,
            navigation,
            last_playback,
            language,
            transcriber,
            model_path,
            issue_thresholds,
            color,
            factory,
            startup_model: _,
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
                document.resolve_issue(selected.token_identities);
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
                if !ensure_transcriber(transcriber, model_path, language, factory.as_mut(), errors)?
                {
                    return Ok(SessionControl::Continue);
                }
                run_correction(
                    document,
                    navigation,
                    transcriber,
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
                if !ensure_transcriber(transcriber, model_path, language, factory.as_mut(), errors)?
                {
                    return Ok(SessionControl::Continue);
                }
                run_correction(
                    document,
                    navigation,
                    transcriber,
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
                let structural = match &range {
                    Some(crate::navigation::Address::Range { start, end }) => Some((*start, *end)),
                    None => navigation.current_range(document).ok(),
                    _ => None,
                };
                let full = structural
                    .filter(|(a, b)| {
                        crate::navigation::tokens_in_range(document, *a, *b)
                            .is_ok_and(|tokens| tokens.is_empty())
                    })
                    .and_then(|(a, b)| crate::navigation::chunks_in_range(document, a, b).ok())
                    .filter(|chunks| chunks.len() == 1)
                    .and_then(|chunks| {
                        let c = chunks[0];
                        (!document.chunk_has_tokens(c.paragraph, c.chunk)?).then_some(c)
                    });
                if let Some(c) = full {
                    let intended = if replacement.exact_boundaries {
                        replacement.text
                    } else {
                        preserve_text_boundary_whitespace(
                            &document
                                .current_transcription(c.paragraph, c.chunk)
                                .unwrap()
                                .text,
                            replacement.text,
                        )
                    };
                    if !ensure_transcriber(
                        transcriber,
                        model_path,
                        language,
                        factory.as_mut(),
                        errors,
                    )? {
                        return Ok(SessionControl::Continue);
                    }
                    run_correction(
                        document,
                        navigation,
                        transcriber,
                        language,
                        c.paragraph,
                        c.chunk,
                        intended,
                        None,
                        output,
                        errors,
                    )?;
                    return Ok(SessionControl::Continue);
                }

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
                if !ensure_transcriber(transcriber, model_path, language, factory.as_mut(), errors)?
                {
                    return Ok(SessionControl::Continue);
                }
                run_correction(
                    document,
                    navigation,
                    transcriber,
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
            SessionCommand::Model(None) => writeln!(
                output,
                "model {}",
                model_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "(none)".into())
            )?,
            SessionCommand::Language(None) => writeln!(output, "language {language}")?,
            SessionCommand::Model(Some(_)) | SessionCommand::Language(Some(_)) => {
                let Some((paragraph, chunk)) = resolve_current_chunk(document, navigation) else {
                    writeln!(errors, "setting change requires exactly one current chunk")?;
                    return Ok(SessionControl::Continue);
                };
                let mut settings = document.settings().clone();
                match command {
                    SessionCommand::Model(Some(path)) => settings.model = Some(path),
                    SessionCommand::Language(Some(value)) => settings.language = value,
                    _ => unreachable!(),
                }
                if &settings == document.settings() {
                    writeln!(output, "transcription settings unchanged")?;
                    return Ok(SessionControl::Continue);
                }
                let Some(path) = settings.model.as_ref() else {
                    writeln!(
                        errors,
                        "transcription requires a model: start with --model MODEL"
                    )?;
                    return Ok(SessionControl::Continue);
                };
                let config = TranscriptionConfig {
                    language: settings.language.clone(),
                    ..TranscriptionConfig::default()
                };
                match factory.load(path, &config) {
                    Err(error) => writeln!(errors, "could not load model: {error}")?,
                    Ok(engine) => {
                        let mut candidate = Some(engine);
                        let history = document.edit_history_len();
                        run_transcription(
                            document,
                            navigation,
                            &mut candidate,
                            &settings,
                            paragraph,
                            chunk,
                            Vec::new(),
                            output,
                            errors,
                        )?;
                        if document.edit_history_len() > history {
                            *model_path = settings.model;
                            *language = settings.language;
                            *transcriber = candidate;
                        }
                    }
                }
            }
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
                let _ = marker;
                if let Some(t) = document.current_transcription(paragraph, chunk) {
                    super::render::render_transcription_info(t, paragraph, chunk, output)?;
                } else {
                    writeln!(errors, "transcription information unavailable")?;
                }
            }
            SessionCommand::Save(path) => {
                let path = path.or_else(|| project_path.clone());
                let Some(path) = path else {
                    writeln!(errors, "save requires a document path")?;
                    return Ok(SessionControl::Continue);
                };
                match save_project(&path, document) {
                    Ok(()) => {
                        *project_path = Some(path.clone());
                        writeln!(output, "saved {}", path.display())?;
                    }

                    Err(error) => writeln!(errors, "{error}")?,
                }
            }
            SessionCommand::Export(path) => match export_text(&path, document) {
                Ok(()) => writeln!(output, "exported {}", path.display())?,
                Err(error) => writeln!(errors, "{error}")?,
            },
            SessionCommand::Load(path) => match load_project(&path) {
                Ok(loaded) => {
                    *document = loaded;
                    *project_path = Some(path);
                    *initial_result = None;
                    *start = SessionStart::SavedDocument;
                    *navigation = NavigationState::new(document);
                    *last_playback = None;
                    writeln!(
                        output,
                        "loaded {}",
                        project_path.as_ref().unwrap().display()
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

    fn sync_settings(&mut self) {
        let settings = self.project.settings();
        if self.model_path != settings.model || self.language != settings.language {
            self.transcriber = None;
            self.model_path = settings.model.clone();
            self.language = settings.language.clone();
        }
    }
}

fn render_session_document(
    document: &Project,
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

fn ensure_transcriber(
    transcriber: &mut Option<Box<dyn ChunkTranscriber>>,
    model_path: &Option<PathBuf>,
    language: &str,
    factory: &mut dyn TranscriberFactory,
    errors: &mut impl Write,
) -> io::Result<bool> {
    if transcriber.is_some() {
        return Ok(true);
    }
    let Some(path) = model_path else {
        writeln!(
            errors,
            "transcription requires a model: start with --model MODEL or use: model PATH"
        )?;
        return Ok(false);
    };
    match factory.load(
        path,
        &TranscriptionConfig {
            language: language.into(),
            ..TranscriptionConfig::default()
        },
    ) {
        Ok(session) => {
            *transcriber = Some(session);
            Ok(true)
        }
        Err(error) => {
            writeln!(errors, "could not load model: {error}")?;
            Ok(false)
        }
    }
}

fn render_selected_tokens(
    document: &Project,
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
    project: &Project,
    context: SessionContext<'_>,
    input: &mut impl BufRead,
    output: &mut impl Write,
    errors: &mut impl Write,
    player: &mut impl AudioPlayer,
    replay_context_samples: u64,
) -> io::Result<()> {
    let Some(mut state) = SessionState::new(project, context, output, errors, false)? else {
        return Ok(());
    };
    if let Some(model) = state.startup_model.take() {
        state.execute(
            SessionCommand::Model(Some(model)),
            output,
            errors,
            player,
            replay_context_samples,
        )?;
    }
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
    project: &Project,
    context: SessionContext<'_>,
    output: &mut impl Write,
    errors: &mut impl Write,
    player: &mut impl AudioPlayer,
    replay_context_samples: u64,
) -> io::Result<()> {
    let Some(mut state) = SessionState::new(project, context, output, errors, true)? else {
        return Ok(());
    };
    if let Some(model) = state.startup_model.take() {
        state.execute(
            SessionCommand::Model(Some(model)),
            output,
            errors,
            player,
            replay_context_samples,
        )?;
    }
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
    writeln!(output, "Model loading: model [PATH] configures the path; startup model loads on first correction; model/language changes transcribe one current chunk")?;
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
        "Commands:\n  p | print                  print the document\n  Np                         print paragraph N\n  A                          move to position A\n  A,Bselect | sel | s        select half-open range [A, B)\n  Ntokens                    list paragraph N tokens\n  [N.M.K]alternatives | alts list alternatives for one token/current token\n  [N.M.K]choose C            correct one token and produce another chunk transcription\n  N.M.Kinsert TEXT           correct before a token (including its end position)\n  N.M.Kappend TEXT           correct after the following token\n  [A,B]replace TEXT          replace a supported one-chunk range and produce another transcription\n                              unquoted keeps selected boundary whitespace\n                              quoted \"TEXT\" controls boundaries exactly\n  [A,B]delete                disabled pending audio-backed deletion\n  model [PATH]               show model; changing it transcribes the current chunk\n  language [CODE]            show language; changing it transcribes the current chunk\n  [N.M]parasplit             split paragraph before a chunk/current chunk\n  Nmerge                     merge paragraph N with N+1 exactly\n  [A]play | [A]slowplay      play current/addressed item or range\n  replay | slowreplay        repeat the last audio range\n  stop                       stop active playback\n  N.Minfo                    report transcription information availability\n  save [PATH]                save atomically; default is the opened file\n  load PATH | edit PATH      replace the current document and reset navigation\n  h | help                   show this help\n  q | quit                   leave the session"
    )
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, path::PathBuf};

    use super::*;
    use std::{cell::RefCell, rc::Rc};
    type LoadLog = Rc<RefCell<Vec<(PathBuf, String)>>>;

    struct FakeFactory {
        loads: LoadLog,
        fail_load: bool,
        fail_decode: bool,
    }
    impl TranscriberFactory for FakeFactory {
        fn load(
            &mut self,
            model: &Path,
            config: &TranscriptionConfig,
        ) -> Result<Box<dyn ChunkTranscriber>, TranscriptionError> {
            self.loads
                .borrow_mut()
                .push((model.into(), config.language.clone()));
            if self.fail_load {
                return Err(TranscriptionError::Model("synthetic load failure".into()));
            }
            Ok(Box::new(FakeEngine {
                model: model.into(),
                fail: self.fail_decode,
            }))
        }
    }
    struct FakeEngine {
        model: PathBuf,
        fail: bool,
    }
    impl ChunkTranscriber for FakeEngine {
        fn tokenize(&self, text: &str) -> Result<Vec<i32>, TranscriptionError> {
            Ok(text.chars().map(|c| c as i32).collect())
        }
        fn render_tokens(&self, tokens: &[i32]) -> Result<String, TranscriptionError> {
            Ok(tokens
                .iter()
                .filter_map(|id| char::from_u32(*id as u32))
                .collect())
        }
        fn beginning_timestamp_token(&self) -> i32 {
            0
        }
        fn transcribe_chunk(
            &mut self,
            request: crate::transcription::ChunkTranscriptionRequest,
            _: &[f32],
        ) -> Result<crate::transcription::Transcription, TranscriptionError> {
            if self.fail {
                return Err(TranscriptionError::Model("synthetic decode failure".into()));
            }
            let text = if request.forced_tokens.is_empty() {
                format!("{}:{}", self.model.display(), request.language)
            } else {
                format!(
                    "{} suffix",
                    self.render_tokens(&request.forced_tokens[1..])?
                )
            };
            let mut result =
                crate::test_support::batch(&format!("decode-{}", request.revision), &[&text]);
            result.source = request.source;
            result.config.language = request.language;
            result.chunks[0].audio_range = request.chunk_range;
            result.segments[0].audio_range = request.chunk_range;
            result.segments[0].tokens[0].audio_range = Some(request.chunk_range);
            if !request.forced_tokens.is_empty() {
                result.segments[0].tokens = request
                    .forced_tokens
                    .iter()
                    .map(|id| crate::transcription::WhisperToken {
                        token_id: *id,
                        text: if *id == 0 {
                            String::new()
                        } else {
                            char::from_u32(*id as u32).unwrap().to_string()
                        },
                        probability: 0.1,
                        is_special: *id == 0,
                        audio_range: Some(request.chunk_range),
                        alternatives: Vec::new(),
                    })
                    .collect();
                result.segments[0]
                    .tokens
                    .push(crate::transcription::WhisperToken {
                        token_id: 999,
                        text: " suffix".into(),
                        probability: 0.1,
                        is_special: false,
                        audio_range: Some(request.chunk_range),
                        alternatives: Vec::new(),
                    });
            }
            result.windows[0].hypotheses = result.segments.clone();
            result.windows[0].prompt_token_ids = request.forced_tokens;
            Ok(result.transcription_for(
                &result.chunks[0],
                &request.chunk_id,
                Some(request.previous_id),
            ))
        }
    }
    #[derive(Default)]
    struct SilentPlayer;
    impl AudioPlayer for SilentPlayer {
        fn play(
            &mut self,
            _: &Path,
            _: u32,
            _: crate::chunking::SampleRange,
        ) -> Result<(), super::super::playback::PlaybackError> {
            Ok(())
        }
    }
    fn state(texts: &[&str]) -> (tempfile::TempDir, SessionState<'static>, LoadLog) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audio.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        for _ in 0..texts.len() * 100 {
            writer.write_sample(0i16).unwrap();
        }
        writer.finalize().unwrap();
        let wav = crate::chunking::read_canonical_wav(&path).unwrap();
        let mut batch = crate::test_support::batch("initial", texts);
        batch.source.sha256 = wav.source_sha256;
        batch.config.language = "en".into();
        let mut project = Project::from_initial_transcription_with_source(&batch, Some(&path));
        project
            .configure_initial_settings(Some("old-model".into()), "en".into())
            .unwrap();
        let mut state = SessionState::new(
            &project,
            SessionContext::saved_project(Path::new("unused"), None),
            &mut Vec::new(),
            &mut Vec::new(),
            false,
        )
        .unwrap()
        .unwrap();
        let loads = Rc::new(RefCell::new(Vec::new()));
        state.factory = Box::new(FakeFactory {
            loads: loads.clone(),
            fail_load: false,
            fail_decode: false,
        });
        (dir, state, loads)
    }
    fn execute(state: &mut SessionState<'_>, command: &str) -> String {
        let mut errors = Vec::new();
        state
            .execute(
                parse_command(command).unwrap(),
                &mut Vec::new(),
                &mut errors,
                &mut SilentPlayer,
                0,
            )
            .unwrap();
        String::from_utf8(errors).unwrap()
    }

    #[test]
    fn model_and_language_changes_install_one_transaction_and_history_restores_settings_without_decoding(
    ) {
        let (_dir, mut state, loads) = state(&["old"]);
        assert!(execute(&mut state, "language de").is_empty());
        assert_eq!(state.project.edit_history_len(), 1);
        assert_eq!(state.project.paragraph(1).unwrap().text(), "old-model:de");
        assert_eq!(
            state
                .project
                .current_transcription(1, 1)
                .unwrap()
                .config
                .language,
            "de"
        );
        assert!(execute(&mut state, "model new-model").is_empty());
        assert_eq!(
            state.project.settings().model.as_deref(),
            Some(Path::new("new-model"))
        );
        assert_eq!(state.project.edit_history_len(), 2);
        execute(&mut state, "2undo");
        assert_eq!(state.project.settings().language, "en");
        assert_eq!(
            state.project.settings().model.as_deref(),
            Some(Path::new("old-model"))
        );
        assert_eq!(state.project.paragraph(1).unwrap().text(), "old");
        execute(&mut state, "2redo");
        assert_eq!(state.project.settings().language, "de");
        assert_eq!(
            state.project.settings().model.as_deref(),
            Some(Path::new("new-model"))
        );
        assert_eq!(loads.borrow().len(), 2);
    }

    #[test]
    fn load_and_decode_failures_preserve_settings_text_selection_and_redo() {
        for fail_load in [true, false] {
            let (_dir, mut state, loads) = state(&["old", "two"]);
            state.project.split_paragraph(1, 1).unwrap();
            state.project.undo(1);
            state.navigation = NavigationState::new(&state.project);
            let project = state.project.clone();
            let navigation = state.navigation.clone();
            state.factory = Box::new(FakeFactory {
                loads,
                fail_load,
                fail_decode: !fail_load,
            });
            let errors = execute(&mut state, "language de");
            assert!(errors.contains("failure"), "{errors}");
            assert_eq!(state.project, project);
            assert_eq!(state.navigation, navigation);
        }
    }

    #[test]
    fn unchanged_settings_are_not_a_standalone_transcription_request() {
        let (_dir, mut state, loads) = state(&["old"]);
        let before = state.project.clone();
        assert!(execute(&mut state, "language en").is_empty());
        assert!(execute(&mut state, "model old-model").is_empty());
        assert_eq!(state.project, before);
        assert!(loads.borrow().is_empty());
    }

    #[test]
    fn startup_model_override_is_a_normal_atomic_model_change() {
        let (_dir, state, loads) = state(&["old"]);
        let context =
            SessionContext::saved_project(Path::new("unused"), Some(Path::new("new-model")));
        let mut reopened = SessionState::new(
            &state.project,
            context,
            &mut Vec::new(),
            &mut Vec::new(),
            false,
        )
        .unwrap()
        .unwrap();
        assert_eq!(reopened.project, state.project);
        let model = reopened.startup_model.take().unwrap();
        reopened.factory = Box::new(FakeFactory {
            loads: loads.clone(),
            fail_load: false,
            fail_decode: false,
        });
        assert!(execute(&mut reopened, &format!("model {}", model.display())).is_empty());
        assert_eq!(
            reopened.project.settings().model.as_deref(),
            Some(Path::new("new-model"))
        );
        assert_eq!(reopened.project.edit_history_len(), 1);
        execute(&mut reopened, "undo");
        assert_eq!(
            reopened.project.settings().model.as_deref(),
            Some(Path::new("old-model"))
        );
        assert_eq!(loads.borrow().len(), 1);
    }

    #[test]
    fn choosing_a_whisper_alternative_forces_its_vocabulary_id_and_undo_restores_the_mark() {
        let (_dir, mut state, _) = state(&["old"]);
        let source = state.project.transcriptions()[0].source.clone();
        let path = state.project.audio_sources()[0]
            .path()
            .unwrap()
            .to_path_buf();
        let mut batch = crate::test_support::batch("alternatives", &["old"]);
        batch.source = source;
        batch.config.language = "en".into();
        batch.segments[0].tokens[0]
            .alternatives
            .push(crate::transcription::TokenAlternative {
                token_id: 90,
                text: "Z".into(),
                probability: 0.2,
            });
        state.project = Project::from_initial_transcription_with_source(&batch, Some(&path));
        state
            .project
            .configure_initial_settings(Some("old-model".into()), "en".into())
            .unwrap();
        state.navigation = NavigationState::new(&state.project);
        execute(&mut state, "mark");
        let marked = state.project.attention_marks()[0].clone();
        assert!(execute(&mut state, "choose 1").is_empty());
        assert_eq!(state.project.paragraph(1).unwrap().text(), "Z suffix");
        assert_eq!(
            state
                .project
                .current_transcription(1, 1)
                .unwrap()
                .forced_token_ids,
            vec![0, 90]
        );
        assert!(state.project.attention_marks().is_empty());
        execute(&mut state, "undo");
        assert_eq!(state.project.attention_marks(), &[marked]);
    }

    #[test]
    fn partially_crossing_a_second_chunk_is_not_a_single_chunk_setting_target() {
        let (_dir, mut state, loads) = state(&["one", "two"]);
        execute(&mut state, "1.1,1.2.1select");
        // Boundary before the second chunk still describes one complete chunk.
        assert!(execute(&mut state, "language de").is_empty());
        execute(&mut state, "1.1.1,1.2.2select");
        let before = state.project.clone();
        assert!(execute(&mut state, "language fr").contains("exactly one current chunk"));
        assert_eq!(state.project, before);
        assert_eq!(loads.borrow().len(), 1);
    }

    #[test]
    fn complete_chunk_correction_without_token_alignment_produces_only_whisper_tokens() {
        let (_dir, mut state, _) = state(&["old"]);
        // Build the same valid mismatch through the public initial-transcription path.
        let source = state.project.transcriptions()[0].source.clone();
        let path = state.project.audio_sources()[0]
            .path()
            .unwrap()
            .to_path_buf();
        let mut batch = crate::test_support::batch("mismatch", &["old"]);
        batch.source = source;
        batch.config.language = "en".into();
        batch.segments[0].tokens[0].text = "different evidence".into();
        state.project = Project::from_initial_transcription_with_source(&batch, Some(&path));
        state
            .project
            .configure_initial_settings(Some("old-model".into()), "en".into())
            .unwrap();
        state.navigation = NavigationState::new(&state.project);
        assert_eq!(state.project.chunk_has_tokens(1, 1), Some(false));
        execute(&mut state, "1.1,1.2select");
        assert!(execute(&mut state, "replace corrected").is_empty());
        assert_eq!(
            state.project.paragraph(1).unwrap().text(),
            "corrected suffix"
        );
        assert_eq!(state.project.chunk_has_tokens(1, 1), Some(true));
        assert_eq!(
            state
                .project
                .current_transcription(1, 1)
                .unwrap()
                .forced_token_ids[0],
            0
        );
        execute(&mut state, "undo");
        assert_eq!(state.project.paragraph(1).unwrap().text(), "old");
        assert_eq!(state.project.chunk_has_tokens(1, 1), Some(false));
    }

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
