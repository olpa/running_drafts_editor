//! Line-oriented developer tooling for listening to recognition chunks.

use std::{
    fmt,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use crate::chunking::SampleRange;
use crate::document::Document;
use crate::recognition::{ChunkBoundaryReason, RecognitionRun};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditionCommand {
    Play(usize),
    Info { paragraph: usize, chunk: usize },
    List,
    Help,
    Quit,
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CommandParseError {
    #[error("unknown command '{0}'; type 'help' for available commands")]
    Unknown(String),
    #[error("play requires a numeric prefix, for example '3play'")]
    MissingChunkNumber,
    #[error("invalid chunk number '{0}'; expected a positive integer")]
    InvalidChunkNumber(String),
    #[error("info requires a marker address, for example '2.3info'")]
    MissingMarkerAddress,
    #[error("invalid marker address '{0}'; expected M.N with positive numbers")]
    InvalidMarkerAddress(String),
    #[error("command accepts no additional arguments")]
    ExtraArguments,
}

pub fn parse_command(input: &str) -> Result<AuditionCommand, CommandParseError> {
    let mut words = input.split_whitespace();
    let Some(command) = words.next() else {
        return Ok(AuditionCommand::Empty);
    };
    if matches!(command, "play" | "p" | "info") {
        return if command == "info" {
            Err(CommandParseError::MissingMarkerAddress)
        } else {
            Err(CommandParseError::MissingChunkNumber)
        };
    }
    if command.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        if let Some(address) = command.strip_suffix("info") {
            if words.next().is_some() {
                return Err(CommandParseError::ExtraArguments);
            }
            let (paragraph, chunk) = parse_marker_address(address)?;
            return Ok(AuditionCommand::Info { paragraph, chunk });
        }
        let value = command
            .strip_suffix("play")
            .or_else(|| command.strip_suffix('p'))
            .ok_or_else(|| CommandParseError::Unknown(command.into()))?;
        if words.next().is_some() {
            return Err(CommandParseError::ExtraArguments);
        }
        let number = value
            .parse::<usize>()
            .ok()
            .filter(|number| *number > 0)
            .ok_or_else(|| CommandParseError::InvalidChunkNumber(value.into()))?;
        return Ok(AuditionCommand::Play(number));
    }
    match command {
        "list" | "l" => no_arguments(words, AuditionCommand::List),
        "help" | "h" => no_arguments(words, AuditionCommand::Help),
        "quit" | "q" => no_arguments(words, AuditionCommand::Quit),
        other => Err(CommandParseError::Unknown(other.into())),
    }
}

fn parse_marker_address(address: &str) -> Result<(usize, usize), CommandParseError> {
    let Some((paragraph, chunk)) = address.split_once('.') else {
        return Err(CommandParseError::InvalidMarkerAddress(address.into()));
    };
    if chunk.contains('.') {
        return Err(CommandParseError::InvalidMarkerAddress(address.into()));
    }
    let parse_part = |part: &str| part.parse::<usize>().ok().filter(|number| *number > 0);
    parse_part(paragraph)
        .zip(parse_part(chunk))
        .ok_or_else(|| CommandParseError::InvalidMarkerAddress(address.into()))
}

fn no_arguments<'a>(
    mut words: impl Iterator<Item = &'a str>,
    command: AuditionCommand,
) -> Result<AuditionCommand, CommandParseError> {
    if words.next().is_some() {
        Err(CommandParseError::ExtraArguments)
    } else {
        Ok(command)
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
    let document = Document::from_chunks(&run.chunks);
    render_recognition_document(run, &document, source, output)?;
    if run.chunks.is_empty() {
        return Ok(());
    }
    render_help(output)?;
    loop {
        write!(output, "chunk> ")?;
        output.flush()?;
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            return Ok(());
        }
        match parse_command(&line) {
            Ok(AuditionCommand::Play(number)) => {
                let Some(chunk) = run.chunks.get(number - 1) else {
                    writeln!(
                        errors,
                        "invalid chunk number {number}; expected 1..={}",
                        run.chunks.len()
                    )?;
                    continue;
                };
                if let Err(error) =
                    player.play(source, run.source.sample_rate_hz, chunk.audio_range)
                {
                    writeln!(errors, "playback failed for chunk {number}: {error}")?;
                }
            }
            Ok(AuditionCommand::Info { paragraph, chunk }) => {
                let Some(marker) = document.chunk_marker(paragraph, chunk) else {
                    writeln!(errors, "unknown chunk marker {paragraph}.{chunk}")?;
                    continue;
                };
                let Some(recognition_chunk) = run
                    .chunks
                    .iter()
                    .find(|candidate| candidate.id == marker.chunk_id())
                else {
                    writeln!(
                        errors,
                        "chunk data is unavailable for marker {paragraph}.{chunk}"
                    )?;
                    continue;
                };
                render_chunk_info(run, recognition_chunk, paragraph, chunk, output)?;
            }
            Ok(AuditionCommand::List) => {
                render_recognition_document(run, &document, source, output)?
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
    let document = Document::from_chunks(&run.chunks);
    render_recognition_document(run, &document, source, output)
}

fn render_recognition_document(
    run: &RecognitionRun,
    document: &Document,
    source: &Path,
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
        let mut start = 0;
        for (chunk_index, marker) in paragraph.chunk_boundaries().iter().enumerate() {
            write!(
                output,
                "{} ⟦{}.{}⟧",
                &paragraph.text()[start..marker.end_offset()],
                paragraph_index + 1,
                chunk_index + 1
            )?;
            start = marker.end_offset();
        }
        writeln!(output)?;
        if paragraph_index + 1 < document.paragraphs().len() {
            writeln!(output)?;
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
        "{}.{}  {} – {}  {:>9}  {:>3} tokens  {}",
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
    writeln!(output, "  Nplay, Np  play chunk N; for example, 3p")?;
    writeln!(
        output,
        "  M.Ninfo    show details for marker M.N; for example, 2.3info"
    )?;
    writeln!(
        output,
        "  list, l    show the recognized text and chunk markers"
    )?;
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
    fn parser_accepts_commands_aliases_and_whitespace() {
        assert_eq!(parse_command(" 12p \n").unwrap(), AuditionCommand::Play(12));
        assert_eq!(parse_command("12play").unwrap(), AuditionCommand::Play(12));
        assert_eq!(
            parse_command(" 2.3info \n").unwrap(),
            AuditionCommand::Info {
                paragraph: 2,
                chunk: 3
            }
        );
        assert_eq!(parse_command("list").unwrap(), AuditionCommand::List);
        assert_eq!(parse_command("h").unwrap(), AuditionCommand::Help);
        assert_eq!(parse_command(" q ").unwrap(), AuditionCommand::Quit);
        assert_eq!(parse_command("  ").unwrap(), AuditionCommand::Empty);
    }

    #[test]
    fn parser_rejects_invalid_commands_and_arguments() {
        assert_eq!(
            parse_command("play").unwrap_err(),
            CommandParseError::MissingChunkNumber
        );
        assert_eq!(
            parse_command("0play").unwrap_err(),
            CommandParseError::InvalidChunkNumber("0".into())
        );
        assert_eq!(
            parse_command("7").unwrap_err(),
            CommandParseError::Unknown("7".into())
        );
        assert_eq!(
            parse_command("1play now").unwrap_err(),
            CommandParseError::ExtraArguments
        );
        assert_eq!(
            parse_command("info").unwrap_err(),
            CommandParseError::MissingMarkerAddress
        );
        assert_eq!(
            parse_command("2info").unwrap_err(),
            CommandParseError::InvalidMarkerAddress("2".into())
        );
        assert_eq!(
            parse_command("0.1info").unwrap_err(),
            CommandParseError::InvalidMarkerAddress("0.1".into())
        );
        assert_eq!(
            parse_command("1.2.3info").unwrap_err(),
            CommandParseError::InvalidMarkerAddress("1.2.3".into())
        );
    }

    #[test]
    fn help_explains_each_session_command_with_examples() {
        let mut output = Vec::new();

        render_help(&mut output).unwrap();

        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("Nplay, Np  play chunk N; for example, 3p"));
        assert!(output.contains("M.Ninfo    show details for marker M.N"));
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
