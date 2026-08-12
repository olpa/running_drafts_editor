//! Line-oriented session for a previously saved visible document.

use std::{
    io::{self, BufRead, Write},
    path::Path,
};

use crate::{
    audition::{
        parse_command, render_paragraph, render_tokens, repeat_document_replay,
        start_document_replay, AudioPlayer, AuditionCommand, ReplayStart,
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
            Ok(AuditionCommand::Replace { start, end, text }) => apply_replace(
                &mut document,
                &mut navigation,
                start,
                end,
                text,
                output,
                errors,
            )?,
            Ok(AuditionCommand::Delete { start, end }) => {
                apply_delete(&mut document, &mut navigation, start, end, output, errors)?
            }
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
    start: TokenAddress,
    end: TokenAddress,
    text: String,
    output: &mut impl Write,
    errors: &mut impl Write,
) -> io::Result<()> {
    match document.replace_text(start.paragraph, start.token, end.paragraph, end.token, text) {
        Ok(position) => {
            refresh_navigation(document, navigation, Some(position));
            writeln!(output, "replaced {start},{end}")
        }
        Err(error) => writeln!(errors, "edit failed: {error}"),
    }
}

pub(crate) fn apply_delete(
    document: &mut Document,
    navigation: &mut NavigationState,
    start: TokenAddress,
    end: TokenAddress,
    output: &mut impl Write,
    errors: &mut impl Write,
) -> io::Result<()> {
    match document.delete_text(start.paragraph, start.token, end.paragraph, end.token) {
        Ok(position) => {
            refresh_navigation(document, navigation, position);
            writeln!(output, "deleted {start},{end}")
        }
        Err(error) => writeln!(errors, "edit failed: {error}"),
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
        "Commands:\n  p | print                  print the document\n  Mp                         print paragraph M\n  M.N                        move caret to a token\n  M@N                        move caret to a chunk marker\n  Aselect | Asel | As        select token/marker range, paragraph, or marker A\n  Mtokens                    list paragraph tokens\n  M.Ninsert TEXT             insert one pseudo-token before M.N\n  M.Nappend TEXT             insert one pseudo-token after M.N\n  M.N,M.Ureplace TEXT        replace an inclusive same-paragraph range\n  M.N,M.Udelete              delete an inclusive same-paragraph range\n  [A]play | [A]slowplay      play current/addressed text or chunk\n  M@N,M@Uplay                play half-open marker interval [left, right)\n  replay | slowreplay        repeat the last audio range\n  stop                       stop active playback\n  M@Ninfo                    report recognition information availability\n  save [PATH]                save atomically; default is the opened file\n  load PATH | edit PATH      replace the current document and reset navigation\n  h | help                   show this help\n  q | quit                   leave the session"
    )
}
