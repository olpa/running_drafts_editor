//! Line-oriented session for a previously saved visible document.

use std::{
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
};

use crate::{
    audition::{parse_command, render_paragraph, render_tokens, AudioPlayer, AuditionCommand},
    document::Document,
    navigation::NavigationState,
    persistence::save_document,
};

pub fn run_editor_session(
    document: &Document,
    document_path: &Path,
    input: &mut impl BufRead,
    output: &mut impl Write,
    errors: &mut impl Write,
    player: &mut impl AudioPlayer,
) -> io::Result<()> {
    render_document(document, output)?;
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
    let mut navigation = NavigationState::new(document);
    loop {
        write!(output, "rde> ")?;
        output.flush()?;
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            return Ok(());
        }
        match parse_command(&line) {
            Ok(AuditionCommand::Print(None)) => {
                render_document_with_navigation(document, Some(&navigation), output)?
            }
            Ok(AuditionCommand::Print(Some(number))) => match document.paragraph(number) {
                Some(paragraph) => render_paragraph(paragraph, number, Some(&navigation), output)?,
                None => writeln!(errors, "unknown paragraph {number}")?,
            },
            Ok(AuditionCommand::Move(address)) => match navigation.move_to(document, &address) {
                Ok(()) => writeln!(output, "caret {address}")?,
                Err(error) => writeln!(errors, "{error}")?,
            },
            Ok(AuditionCommand::Select(address)) => match navigation.select(document, &address) {
                Ok(()) => writeln!(output, "selected {address}")?,
                Err(error) => writeln!(errors, "{error}")?,
            },
            Ok(AuditionCommand::Tokens(number)) => match document.paragraph(number) {
                Some(paragraph) => render_tokens(paragraph, number, output)?,
                None => writeln!(errors, "unknown paragraph {number}")?,
            },
            Ok(AuditionCommand::Play { paragraph, chunk }) => {
                let Some(marker) = document.chunk_marker(paragraph, chunk) else {
                    writeln!(errors, "unknown chunk marker {paragraph}@{chunk}")?;
                    continue;
                };
                let Some((source, range)) = document.audio_mapping(marker.chunk_id()) else {
                    writeln!(
                        errors,
                        "audio mapping is unavailable for marker {paragraph}@{chunk}"
                    )?;
                    continue;
                };
                let Some(path) = source.path() else {
                    writeln!(errors, "audio source '{}' has no local path", source.id())?;
                    continue;
                };
                if !path.is_file() {
                    writeln!(
                        errors,
                        "audio source '{}' is unavailable at {}",
                        source.id(),
                        path.display()
                    )?;
                    continue;
                }
                if let Err(error) = player.play(path, 16_000, range) {
                    writeln!(
                        errors,
                        "playback failed for marker {paragraph}@{chunk}: {error}"
                    )?;
                }
            }
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
                let path = path.unwrap_or_else(|| PathBuf::from(document_path));
                match save_document(&path, document) {
                    Ok(()) => writeln!(output, "saved {}", path.display())?,
                    Err(error) => writeln!(errors, "{error}")?,
                }
            }
            Ok(AuditionCommand::Help) => render_editor_help(output)?,
            Ok(AuditionCommand::Quit) => return Ok(()),
            Ok(AuditionCommand::Empty) => {}
            Err(error) => writeln!(errors, "{error}")?,
        }
    }
}

fn render_document(document: &Document, output: &mut impl Write) -> io::Result<()> {
    render_document_with_navigation(document, None, output)
}

fn render_document_with_navigation(
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
        "Commands:\n  p | print                  print the document\n  Mp                         print paragraph M\n  M.N                        move caret to a token\n  M@N                        move caret to a chunk marker\n  Aselect                    select token/range/paragraph/marker A\n  Mtokens                    list paragraph tokens\n  M@Nplay                    play mapped audio when available\n  M@Ninfo                    report recognition information availability\n  save [PATH]                save atomically; default is the opened file\n  h | help                   show this help\n  q | quit                   leave the session"
    )
}
