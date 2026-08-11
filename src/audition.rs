//! Line-oriented developer tooling for listening to recognition chunks.

use std::{
    fmt,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use crate::chunking::SampleRange;
use crate::document::Document;
use crate::navigation::{
    parse_line, Address, Caret, CommandLine, NavigationState, Selection, SyntaxError,
};
use crate::persistence::save_document;
use crate::recognition::{ChunkBoundaryReason, RecognitionRun};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditionCommand {
    Play { paragraph: usize, chunk: usize },
    Info { paragraph: usize, chunk: usize },
    Print(Option<usize>),
    Move(Address),
    Select(Address),
    Tokens(usize),
    Save(Option<PathBuf>),
    Help,
    Quit,
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CommandParseError {
    #[error(transparent)]
    Syntax(#[from] SyntaxError),
    #[error("unknown command '{0}'; type 'help' for available commands")]
    Unknown(String),
    #[error("{command} requires {expected}")]
    AddressRequired {
        command: String,
        expected: &'static str,
    },
    #[error("{command} does not accept address '{address}'; expected {expected}")]
    InvalidAddress {
        command: String,
        address: Address,
        expected: &'static str,
    },
    #[error("{0} does not accept an address")]
    UnexpectedAddress(String),
    #[error("{0} accepts no additional arguments")]
    ExtraArguments(String),
}

pub fn parse_command(input: &str) -> Result<AuditionCommand, CommandParseError> {
    let (address, name, arguments) = match parse_line(input)? {
        CommandLine::Empty => return Ok(AuditionCommand::Empty),
        CommandLine::Address(address) => return Ok(AuditionCommand::Move(address)),
        CommandLine::Command {
            address,
            name,
            arguments,
        } => (address, name, arguments),
    };

    if name == "save" {
        return no_address(
            address,
            name,
            AuditionCommand::Save((!arguments.is_empty()).then(|| PathBuf::from(arguments))),
        );
    }
    if !arguments.is_empty() {
        return Err(CommandParseError::ExtraArguments(name));
    }

    match name.as_str() {
        "print" | "p" | "list" | "l" => match address {
            None => Ok(AuditionCommand::Print(None)),
            Some(Address::Paragraph(paragraph)) => Ok(AuditionCommand::Print(Some(paragraph))),
            Some(address) => Err(CommandParseError::InvalidAddress {
                command: name,
                address,
                expected: "a paragraph address M",
            }),
        },
        "play" => marker_command(address, name, |paragraph, chunk| AuditionCommand::Play {
            paragraph,
            chunk,
        }),
        "select" | "s" => {
            address
                .map(AuditionCommand::Select)
                .ok_or(CommandParseError::AddressRequired {
                    command: name,
                    expected: "an address M, M.N, M.N,M.U, M@N, or .",
                })
        }
        "tokens" => match address {
            Some(Address::Paragraph(paragraph)) => Ok(AuditionCommand::Tokens(paragraph)),
            Some(address) => Err(CommandParseError::InvalidAddress {
                command: name,
                address,
                expected: "a paragraph address M",
            }),
            None => Err(CommandParseError::AddressRequired {
                command: name,
                expected: "a paragraph address M",
            }),
        },
        "info" | "i" => marker_command(address, name, |paragraph, chunk| AuditionCommand::Info {
            paragraph,
            chunk,
        }),
        "help" | "h" => no_address(address, name, AuditionCommand::Help),
        "quit" | "q" => no_address(address, name, AuditionCommand::Quit),
        _ => Err(CommandParseError::Unknown(name)),
    }
}

fn marker_command(
    address: Option<Address>,
    command: String,
    build: impl FnOnce(usize, usize) -> AuditionCommand,
) -> Result<AuditionCommand, CommandParseError> {
    match address {
        Some(Address::Marker { paragraph, marker }) => Ok(build(paragraph, marker)),
        Some(address) => Err(CommandParseError::InvalidAddress {
            command,
            address,
            expected: "a chunk-marker address M@N",
        }),
        None => Err(CommandParseError::AddressRequired {
            command,
            expected: "a chunk-marker address M@N",
        }),
    }
}

fn no_address(
    address: Option<Address>,
    command: String,
    result: AuditionCommand,
) -> Result<AuditionCommand, CommandParseError> {
    if address.is_some() {
        Err(CommandParseError::UnexpectedAddress(command))
    } else {
        Ok(result)
    }
}

pub trait AudioPlayer {
    fn play(
        &mut self,
        source: &Path,
        sample_rate_hz: u32,
        range: SampleRange,
    ) -> Result<(), PlaybackError>;
}

#[derive(Debug, thiserror::Error)]
pub enum PlaybackError {
    #[error("could not start playback program '{}': {source}", program.display())]
    Start {
        program: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("playback program '{}' exited with {status}", program.display())]
    Failed {
        program: PathBuf,
        status: std::process::ExitStatus,
    },
    #[error("cannot play a range at a zero sample rate")]
    ZeroSampleRate,
    #[error("invalid playback range [{}, {})", .0.start_sample, .0.end_sample)]
    InvalidRange(SampleRange),
    #[error("{0}")]
    Other(String),
}

#[derive(Debug, Clone)]
pub struct Ffplay {
    program: PathBuf,
}

impl Default for Ffplay {
    fn default() -> Self {
        Self::new("ffplay")
    }
}

impl Ffplay {
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
        }
    }
}

impl AudioPlayer for Ffplay {
    fn play(
        &mut self,
        source: &Path,
        sample_rate_hz: u32,
        range: SampleRange,
    ) -> Result<(), PlaybackError> {
        if sample_rate_hz == 0 {
            return Err(PlaybackError::ZeroSampleRate);
        }
        if range.start_sample >= range.end_sample {
            return Err(PlaybackError::InvalidRange(range));
        }
        let start = samples_as_seconds(range.start_sample, sample_rate_hz);
        let duration = samples_as_seconds(range.len(), sample_rate_hz);
        let status = Command::new(&self.program)
            .args([
                "-nodisp",
                "-autoexit",
                "-loglevel",
                "error",
                "-ss",
                &start,
                "-t",
                &duration,
                "-i",
            ])
            .arg(source)
            .stdin(Stdio::null())
            .status()
            .map_err(|source| PlaybackError::Start {
                program: self.program.clone(),
                source,
            })?;
        if status.success() {
            Ok(())
        } else {
            Err(PlaybackError::Failed {
                program: self.program.clone(),
                status,
            })
        }
    }
}

fn samples_as_seconds(samples: u64, sample_rate_hz: u32) -> String {
    let rate = u64::from(sample_rate_hz);
    format!(
        "{}.{:09}",
        samples / rate,
        (samples % rate) * 1_000_000_000 / rate
    )
}

pub fn run_recognition_session(
    run: &RecognitionRun,
    source: &Path,
    input: &mut impl BufRead,
    output: &mut impl Write,
    errors: &mut impl Write,
    player: &mut impl AudioPlayer,
) -> io::Result<()> {
    let document = Document::from_run_with_source(run, Some(source));
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
    render_help(output)?;
    let mut navigation = NavigationState::new(&document);
    loop {
        write!(output, "rde> ")?;
        output.flush()?;
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            return Ok(());
        }
        match parse_command(&line) {
            Ok(AuditionCommand::Play { paragraph, chunk }) => {
                let Some(marker) = document.chunk_marker(paragraph, chunk) else {
                    writeln!(errors, "unknown chunk marker {paragraph}@{chunk}")?;
                    continue;
                };
                let Some(recognition_chunk) = run
                    .chunks
                    .iter()
                    .find(|candidate| candidate.id == marker.chunk_id())
                else {
                    writeln!(
                        errors,
                        "chunk data is unavailable for marker {paragraph}@{chunk}"
                    )?;
                    continue;
                };
                if let Err(error) = player.play(
                    source,
                    run.source.sample_rate_hz,
                    recognition_chunk.audio_range,
                ) {
                    writeln!(
                        errors,
                        "playback failed for marker {paragraph}@{chunk}: {error}"
                    )?;
                }
            }
            Ok(AuditionCommand::Info { paragraph, chunk }) => {
                let Some(marker) = document.chunk_marker(paragraph, chunk) else {
                    writeln!(errors, "unknown chunk marker {paragraph}@{chunk}")?;
                    continue;
                };
                let Some(recognition_chunk) = run
                    .chunks
                    .iter()
                    .find(|candidate| candidate.id == marker.chunk_id())
                else {
                    writeln!(
                        errors,
                        "chunk data is unavailable for marker {paragraph}@{chunk}"
                    )?;
                    continue;
                };
                render_chunk_info(run, recognition_chunk, paragraph, chunk, output)?;
            }
            Ok(AuditionCommand::Print(None)) => render_recognition_document_with_navigation(
                run,
                &document,
                source,
                Some(&navigation),
                output,
            )?,
            Ok(AuditionCommand::Print(Some(paragraph))) => {
                let Some(value) = document.paragraph(paragraph) else {
                    writeln!(
                        errors,
                        "unknown paragraph {paragraph}; expected 1..={}",
                        document.paragraphs().len()
                    )?;
                    continue;
                };
                render_paragraph(value, paragraph, Some(&navigation), output)?;
            }
            Ok(AuditionCommand::Move(address)) => match navigation.move_to(&document, &address) {
                Ok(()) => writeln!(output, "caret {address}")?,
                Err(error) => writeln!(errors, "{error}")?,
            },
            Ok(AuditionCommand::Select(address)) => match navigation.select(&document, &address) {
                Ok(()) => writeln!(output, "selected {address}")?,
                Err(error) => writeln!(errors, "{error}")?,
            },
            Ok(AuditionCommand::Tokens(paragraph)) => {
                let Some(value) = document.paragraph(paragraph) else {
                    writeln!(errors, "unknown paragraph {paragraph}")?;
                    continue;
                };
                render_tokens(value, paragraph, output)?;
            }
            Ok(AuditionCommand::Save(path)) => {
                let Some(path) = path else {
                    writeln!(
                        errors,
                        "save requires a document path in an audition session"
                    )?;
                    continue;
                };
                match save_document(&path, &document) {
                    Ok(()) => writeln!(output, "saved {}", path.display())?,
                    Err(error) => writeln!(errors, "{error}")?,
                }
            }
            Ok(AuditionCommand::Help) => render_help(output)?,

            Ok(AuditionCommand::Quit) => return Ok(()),
            Ok(AuditionCommand::Empty) => {}
            Err(error) => writeln!(errors, "{error}")?,
        }
    }
}

pub fn render_recognition_chunks(
    run: &RecognitionRun,
    source: &Path,
    output: &mut impl Write,
) -> io::Result<()> {
    let document = Document::from_run(run);
    render_recognition_document(run, &document, source, output)
}

fn render_recognition_document(
    run: &RecognitionRun,
    document: &Document,
    source: &Path,
    output: &mut impl Write,
) -> io::Result<()> {
    render_recognition_document_with_navigation(run, document, source, None, output)
}

fn render_recognition_document_with_navigation(
    run: &RecognitionRun,
    document: &Document,
    source: &Path,
    navigation: Option<&NavigationState>,
    output: &mut impl Write,
) -> io::Result<()> {
    writeln!(
        output,
        "Built {} chunks from {}",
        run.chunks.len(),
        source.display()
    )?;
    if run.chunks.is_empty() {
        return Ok(());
    }
    writeln!(output)?;
    for (paragraph_index, paragraph) in document.paragraphs().iter().enumerate() {
        render_paragraph(paragraph, paragraph_index + 1, navigation, output)?;
        if paragraph_index + 1 < document.paragraphs().len() {
            writeln!(output)?;
        }
    }
    Ok(())
}

pub(crate) fn render_paragraph(
    paragraph: &crate::document::Paragraph,
    paragraph_number: usize,
    navigation: Option<&NavigationState>,
    output: &mut impl Write,
) -> io::Result<()> {
    let paragraph_selected = navigation.is_some_and(|state| {
        matches!(state.selection(), Some(Selection::Paragraph { paragraph_id, paragraph_revision })
            if paragraph_id == paragraph.id() && *paragraph_revision == paragraph.revision())
    });
    if paragraph_selected {
        write!(output, "⟪")?;
    }
    let mut marker_index = 0;
    for token_index in 0..=paragraph.tokens().len() {
        while paragraph
            .chunk_boundaries()
            .get(marker_index)
            .is_some_and(|marker| marker.after_tokens() == token_index)
        {
            let marker = &paragraph.chunk_boundaries()[marker_index];
            let marker_selected = navigation.is_some_and(|state| {
                matches!(state.selection(), Some(Selection::Marker(position))
                    if position.paragraph_id == paragraph.id()
                        && position.paragraph_revision == paragraph.revision()
                        && position.chunk_id == marker.chunk_id())
            });
            let marker_caret = navigation.is_some_and(|state| {
                state.selection().is_none()
                    && matches!(state.caret(), Some(Caret::Marker(position))
                        if position.paragraph_id == paragraph.id()
                            && position.paragraph_revision == paragraph.revision()
                            && position.chunk_id == marker.chunk_id())
            });
            write!(output, " ")?;
            if marker_selected {
                write!(output, "⟪")?;
            } else if marker_caret {
                write!(output, "‹")?;
            }
            write!(output, "⟦{}@{}⟧", paragraph_number, marker_index + 1)?;
            if marker_selected {
                write!(output, "⟫")?;
            } else if marker_caret {
                write!(output, "›")?;
            }
            marker_index += 1;
        }
        if let Some(token) = paragraph.tokens().get(token_index) {
            let selection_start = token_selection_edge(
                navigation.and_then(NavigationState::selection),
                paragraph,
                token,
                true,
            );
            let selection_end = token_selection_edge(
                navigation.and_then(NavigationState::selection),
                paragraph,
                token,
                false,
            );
            let token_caret = navigation.is_some_and(|state| {
                state.selection().is_none()
                    && matches!(state.caret(), Some(Caret::Token(position))
                        if position.paragraph_id == paragraph.id()
                            && position.paragraph_revision == paragraph.revision()
                            && position.token_id == *token.id())
            });
            if selection_start {
                write!(output, "⟪")?;
            } else if token_caret {
                write!(output, "‹")?;
            }
            write!(output, "{}", token.text())?;
            if selection_end {
                write!(output, "⟫")?;
            } else if token_caret {
                write!(output, "›")?;
            }
        }
    }
    if paragraph_selected {
        write!(output, "⟫")?;
    }
    writeln!(output)
}

fn token_selection_edge(
    selection: Option<&Selection>,
    paragraph: &crate::document::Paragraph,
    token: &crate::document::VisibleToken,
    start_edge: bool,
) -> bool {
    let Some(Selection::Tokens {
        start,
        end_inclusive,
        ..
    }) = selection
    else {
        return false;
    };
    let position = if start_edge { start } else { end_inclusive };
    position.paragraph_id == paragraph.id()
        && position.paragraph_revision == paragraph.revision()
        && position.token_id == *token.id()
}

pub(crate) fn render_tokens(
    paragraph: &crate::document::Paragraph,
    paragraph_number: usize,
    output: &mut impl Write,
) -> io::Result<()> {
    let mut marker_index = 0;
    for token_index in 0..=paragraph.tokens().len() {
        while paragraph
            .chunk_boundaries()
            .get(marker_index)
            .is_some_and(|marker| marker.after_tokens() == token_index)
        {
            writeln!(
                output,
                "{}@{}  marker  chunk boundary",
                paragraph_number,
                marker_index + 1
            )?;
            marker_index += 1;
        }
        if let Some(token) = paragraph.tokens().get(token_index) {
            writeln!(
                output,
                "{}.{}  {:<6}  {:?}",
                paragraph_number,
                token_index + 1,
                token.kind_label(),
                token.text()
            )?;
        }
    }
    Ok(())
}

fn render_chunk_info(
    run: &RecognitionRun,
    chunk: &crate::recognition::RecognitionChunk,
    paragraph: usize,
    chunk_number: usize,
    output: &mut impl Write,
) -> io::Result<()> {
    writeln!(
        output,
        "{}@{}  {} – {}  {:>9}  {:>3} tokens  {}",
        paragraph,
        chunk_number,
        Timestamp::new(chunk.audio_range.start_sample, run.source.sample_rate_hz),
        Timestamp::new(chunk.audio_range.end_sample, run.source.sample_rate_hz),
        Duration::new(chunk.audio_range.len(), run.source.sample_rate_hz),
        chunk.token_count,
        chunk_boundary_label(chunk, run.source.sample_rate_hz),
    )?;
    writeln!(output, "     {}", chunk.text)
}

fn chunk_boundary_label(
    chunk: &crate::recognition::RecognitionChunk,
    sample_rate_hz: u32,
) -> String {
    let reason = match chunk.boundary.reason {
        ChunkBoundaryReason::LongPause => "long pause",
        ChunkBoundaryReason::StrongPause => "strong pause",
        ChunkBoundaryReason::ScoredPause => "best pause",
        ChunkBoundaryReason::MaximumTokens => return "token limit".to_owned(),
        ChunkBoundaryReason::SourceEnd => "source end",
    };
    chunk.boundary.pause_samples.map_or_else(
        || reason.to_owned(),
        |samples| format!("{reason} ({})", Duration::new(samples, sample_rate_hz)),
    )
}

fn render_help(output: &mut impl Write) -> io::Result<()> {
    writeln!(output)?;
    writeln!(output, "Session commands:")?;
    writeln!(output, "  print, p       show the whole document")?;
    writeln!(output, "  Mprint, Mp     show paragraph M; for example, 2p")?;
    writeln!(
        output,
        "  M.N, M@N        move the caret to a token or marker"
    )?;
    writeln!(
        output,
        "  Aselect, As     select token, token range, paragraph, or marker A"
    )?;
    writeln!(
        output,
        "  Mtokens         list paragraph M tokens and markers"
    )?;
    writeln!(
        output,
        "  M@Nplay        play the chunk ending at M@N; for example, 2@3play"
    )?;
    writeln!(
        output,
        "  M@Ninfo, M@Ni show details for marker M@N; for example, 2@3info"
    )?;
    writeln!(output, "  list, l        aliases for print")?;
    writeln!(output, "  help, h    show this help")?;
    writeln!(output, "  quit, q    exit")
}

struct Timestamp {
    samples: u64,
    sample_rate_hz: u32,
}

impl Timestamp {
    fn new(samples: u64, sample_rate_hz: u32) -> Self {
        Self {
            samples,
            sample_rate_hz,
        }
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let milliseconds = u128::from(self.samples) * 1_000 / u128::from(self.sample_rate_hz);
        write!(
            formatter,
            "{:02}:{:02}:{:02}.{:03}",
            milliseconds / 3_600_000,
            milliseconds / 60_000 % 60,
            milliseconds / 1_000 % 60,
            milliseconds % 1_000
        )
    }
}

struct Duration {
    samples: u64,
    sample_rate_hz: u32,
}

impl Duration {
    fn new(samples: u64, sample_rate_hz: u32) -> Self {
        Self {
            samples,
            sample_rate_hz,
        }
    }
}

impl fmt::Display for Duration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let milliseconds = u128::from(self.samples) * 1_000 / u128::from(self.sample_rate_hz);
        write!(
            formatter,
            "{}.{:03} s",
            milliseconds / 1_000,
            milliseconds % 1_000
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_accepts_addressed_commands_and_aliases() {
        assert_eq!(parse_command(" p ").unwrap(), AuditionCommand::Print(None));
        assert_eq!(
            parse_command("2print").unwrap(),
            AuditionCommand::Print(Some(2))
        );
        assert_eq!(
            parse_command(" 2@3play ").unwrap(),
            AuditionCommand::Play {
                paragraph: 2,
                chunk: 3
            }
        );
        assert_eq!(
            parse_command("2@3 i").unwrap(),
            AuditionCommand::Info {
                paragraph: 2,
                chunk: 3
            }
        );
        assert_eq!(parse_command("list").unwrap(), AuditionCommand::Print(None));
        assert_eq!(parse_command("h").unwrap(), AuditionCommand::Help);
        assert_eq!(
            parse_command("save document.rde.json").unwrap(),
            AuditionCommand::Save(Some(PathBuf::from("document.rde.json")))
        );
        assert_eq!(parse_command("save").unwrap(), AuditionCommand::Save(None));
        assert_eq!(parse_command(" q ").unwrap(), AuditionCommand::Quit);

        assert_eq!(parse_command("  ").unwrap(), AuditionCommand::Empty);
    }

    #[test]
    fn parser_reports_command_specific_address_errors() {
        assert_eq!(
            parse_command("play").unwrap_err(),
            CommandParseError::AddressRequired {
                command: "play".into(),
                expected: "a chunk-marker address M@N",
            }
        );
        assert_eq!(
            parse_command("1play").unwrap_err(),
            CommandParseError::InvalidAddress {
                command: "play".into(),
                address: Address::Paragraph(1),
                expected: "a chunk-marker address M@N",
            }
        );
        assert_eq!(
            parse_command("7").unwrap(),
            AuditionCommand::Move(Address::Paragraph(7))
        );
        assert_eq!(
            parse_command("1@1play now").unwrap_err(),
            CommandParseError::ExtraArguments("play".into())
        );
        assert_eq!(
            parse_command("info").unwrap_err(),
            CommandParseError::AddressRequired {
                command: "info".into(),
                expected: "a chunk-marker address M@N",
            }
        );
        assert_eq!(
            parse_command("2info").unwrap_err(),
            CommandParseError::InvalidAddress {
                command: "info".into(),
                address: Address::Paragraph(2),
                expected: "a chunk-marker address M@N",
            }
        );
        assert_eq!(
            parse_command("0@1info").unwrap_err(),
            CommandParseError::Syntax(SyntaxError::ZeroAddress("0@1".into()))
        );
        assert_eq!(
            parse_command("2.4,3.2select").unwrap(),
            AuditionCommand::Select(Address::TokenRange {
                start: crate::navigation::TokenAddress {
                    paragraph: 2,
                    token: 4
                },
                end: crate::navigation::TokenAddress {
                    paragraph: 3,
                    token: 2
                },
            })
        );
        assert_eq!(
            parse_command("2tokens").unwrap(),
            AuditionCommand::Tokens(2)
        );
        assert_eq!(
            parse_command("2help").unwrap_err(),
            CommandParseError::UnexpectedAddress("help".into())
        );
    }

    #[test]
    fn help_explains_each_session_command_with_examples() {
        let mut output = Vec::new();

        render_help(&mut output).unwrap();

        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("print, p"));
        assert!(output.contains("Mprint, Mp"));
        assert!(output.contains("M.N, M@N        move the caret"));
        assert!(output.contains("Aselect, As"));
        assert!(output.contains("Mtokens"));
        assert!(output.contains("M@Nplay        play the chunk ending at M@N"));
        assert!(output.contains("M@Ninfo, M@Ni"));
        assert!(output.contains("list, l"));
        assert!(output.contains("help, h"));
        assert!(output.contains("quit, q"));
    }

    #[test]
    fn ffplay_seconds_preserve_sample_precision() {
        assert_eq!(samples_as_seconds(1, 16_000), "0.000062500");
        assert_eq!(samples_as_seconds(480_001, 16_000), "30.000062500");
    }
}
