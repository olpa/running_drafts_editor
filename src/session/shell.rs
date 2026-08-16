use std::{
    io::{self, BufRead, Write},
    path::Path,
};

use crate::{
    document::Document,
    editor::{
        alternative_address, apply_chunk_merge, apply_chunk_split, apply_history,
        apply_paragraph_merge, apply_paragraph_split, chunk_prefix, edit_range,
        preserve_boundary_whitespace, render_alternatives, render_document,
        render_document_with_navigation, resolve_current_chunk, run_corrected_refresh, run_refresh,
    },
    navigation::NavigationState,
    persistence::{load_document, save_document},
    recognition::{RecognitionConfig, RecognitionRun, RecognizerSession},
};

use super::{
    command::{parse_command, SessionCommand},
    playback::{repeat_document_replay, start_document_replay, AudioPlayer, ReplayStart},
    render::{render_chunk_info, render_paragraph, render_recognition_document, render_tokens},
};

pub struct SessionContext<'a> {
    document_path: Option<&'a Path>,
    recognition_run: Option<&'a RecognitionRun>,
    start: SessionStart<'a>,
    model: Option<&'a Path>,
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
        }
    }
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
    let SessionContext {
        document_path,
        recognition_run,
        mut start,
        model,
    } = context;
    let mut document = document.clone();
    let mut document_path = document_path.map(Path::to_path_buf);
    let mut recognition_run = recognition_run;
    match start {
        SessionStart::SavedDocument => {
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
        }
        SessionStart::RecognizedAudio { source } => {
            let run = recognition_run.expect("recognized-audio context has a recognition run");
            render_recognition_document(run, &document, source, output)?;
            for fallback in document.token_fallbacks() {
                let address = document
                    .marker_address_for_chunk(fallback.chunk_id())
                    .map_or_else(
                        || fallback.chunk_id().to_owned(),
                        |(paragraph, marker)| format!("{paragraph}@{marker}"),
                    );
                writeln!(
                    errors,
                    "token alignment unavailable for marker {address}: {}; using chunk text as one pseudo-token",
                    fallback.reason()
                )?;
            }
            if run.chunks.is_empty() {
                return Ok(());
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
            Ok(SessionCommand::Print(None)) => {
                render_document_with_navigation(&document, Some(&navigation), output)?
            }
            Ok(SessionCommand::Print(Some(number))) => match document.paragraph(number) {
                Some(paragraph) => render_paragraph(paragraph, number, Some(&navigation), output)?,
                None => writeln!(errors, "unknown paragraph {number}")?,
            },
            Ok(SessionCommand::Move(address)) => match navigation.move_to(&document, &address) {
                Ok(()) => writeln!(output, "caret {address}")?,
                Err(error) => writeln!(errors, "{error}")?,
            },
            Ok(SessionCommand::Select(address)) => match navigation.select(&document, &address) {
                Ok(()) => writeln!(output, "selected {address}")?,
                Err(error) => writeln!(errors, "{error}")?,
            },
            Ok(SessionCommand::Tokens(number)) => match document.paragraph(number) {
                Some(paragraph) => render_tokens(paragraph, number, output)?,
                None => writeln!(errors, "unknown paragraph {number}")?,
            },
            Ok(SessionCommand::Alternatives { address }) => {
                render_alternatives(&document, &navigation, address, output, errors)?
            }
            Ok(SessionCommand::ChooseAlternative { address, candidate }) => {
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
            Ok(SessionCommand::Insert { address, text })
            | Ok(SessionCommand::Append { address, text }) => {
                let after = matches!(parse_command(&line), Ok(SessionCommand::Append { .. }));
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
            Ok(SessionCommand::Replace { range, replacement }) => {
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
            Ok(SessionCommand::Delete { range }) => {
                let _ = range;
                writeln!(
                    errors,
                    "delete is disabled; deletion of audio-backed text is not implemented"
                )?
            }
            Ok(SessionCommand::Refresh { marker }) => {
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
            Ok(SessionCommand::Model(path)) => match path {
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
            Ok(SessionCommand::Language(value)) => match value {
                None => writeln!(output, "language {language}")?,
                Some(value) => {
                    language = value;
                    if let Some(r) = &mut recognizer {
                        r.set_language(language.clone());
                    }
                    writeln!(output, "language {language}")?;
                }
            },
            Ok(SessionCommand::SplitChunk { address, after }) => apply_chunk_split(
                &mut document,
                &mut navigation,
                address,
                after,
                output,
                errors,
            )?,
            Ok(SessionCommand::SplitParagraph { marker }) => {
                apply_paragraph_split(&mut document, &mut navigation, marker, output, errors)?
            }
            Ok(SessionCommand::MergeParagraph(paragraph)) => {
                apply_paragraph_merge(&mut document, &mut navigation, paragraph, output, errors)?
            }
            Ok(SessionCommand::MergeChunks { paragraph, marker }) => apply_chunk_merge(
                &mut document,
                &mut navigation,
                paragraph,
                marker,
                output,
                errors,
            )?,
            Ok(SessionCommand::Undo(count)) => {
                apply_history(&mut document, &mut navigation, count, false, output)?
            }
            Ok(SessionCommand::Redo(count)) => {
                apply_history(&mut document, &mut navigation, count, true, output)?
            }
            Ok(SessionCommand::Play { address, speed }) => {
                if let Some(value) = start_document_replay(
                    &document,
                    &navigation,
                    address.as_ref(),
                    ReplayStart {
                        context_samples: replay_context_samples,
                        speed,
                        require_file: matches!(start, SessionStart::SavedDocument),
                    },
                    player,
                    output,
                    errors,
                )? {
                    last_playback = Some(value);
                }
            }
            Ok(SessionCommand::Replay { speed }) => repeat_document_replay(
                &document,
                last_playback.as_ref(),
                speed,
                player,
                output,
                errors,
            )?,
            Ok(SessionCommand::Stop) => match player.stop() {
                Ok(true) => writeln!(output, "playback stopped")?,
                Ok(false) => writeln!(errors, "nothing is playing")?,
                Err(error) => writeln!(errors, "could not stop playback: {error}")?,
            },
            Ok(SessionCommand::Info { paragraph, chunk }) => {
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
            Ok(SessionCommand::Save(path)) => {
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
            Ok(SessionCommand::Load(path)) => match load_document(&path) {
                Ok(loaded) => {
                    document = loaded;
                    document_path = Some(path);
                    recognition_run = None;
                    start = SessionStart::SavedDocument;
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
            Ok(SessionCommand::Help) => render_help(output)?,
            Ok(SessionCommand::Quit) => return Ok(()),
            Ok(SessionCommand::Empty) => {}
            Err(error) => writeln!(errors, "{error}")?,
        }
    }
}

pub(crate) fn render_help(output: &mut impl Write) -> io::Result<()> {
    writeln!(
        output,
        "History: undo | Nundo | redo | Nredo (N is a positive maximum count)"
    )?;
    writeln!(
        output,
        "Commands:\n  p | print                  print the document\n  Mp                         print paragraph M\n  M.N                        move caret to a token\n  M@N                        move caret to a chunk marker\n  Aselect | Asel | As        select token/marker range, paragraph, or marker A\n  Mtokens                    list paragraph tokens\n  [M.N]alternatives | alts   list alternatives for one token/current token\n  [M.N]choose N              correct one token and refresh its chunk\n  M.Ninsert TEXT             correct before M.N and refresh its chunk\n  M.Nappend TEXT             correct after M.N and refresh its chunk\n  [M.N,M.U]replace TEXT      replace a one-chunk range and refresh\n                              unquoted keeps selected boundary whitespace\n                              quoted \"TEXT\" controls boundaries exactly\n  [M.N,M.U]delete            disabled pending audio-backed deletion\n  [M@N]refresh               re-recognize one complete replay chunk\n  model [PATH]               show or load the session model\n  language [CODE]            show or set the session language\n  [M.N]split | [M.N]isplit   split chunk before token/current caret\n  [M.N]asplit                split chunk after token/current caret\n  [M@N]parasplit             split paragraph after marker/current marker\n  Mmerge                     merge paragraph M with M+1 exactly\n  M@Nmerge                   merge chunks around marker M@N when legal\n  [A]play | [A]slowplay      play current/addressed text or chunk\n  M@N,M@Uplay                play half-open marker interval [left, right)\n  replay | slowreplay        repeat the last audio range\n  stop                       stop active playback\n  M@Ninfo                    report recognition information availability\n  save [PATH]                save atomically; default is the opened file\n  load PATH | edit PATH      replace the current document and reset navigation\n  h | help                   show this help\n  q | quit                   leave the session"
    )
}
