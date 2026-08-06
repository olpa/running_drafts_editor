//! Line-oriented developer tooling for listening to recognition chunks.

use std::{
    fmt,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    process::Command,
};

use crate::chunking::{BoundaryKind, RecognitionPlan, SampleRange};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditionCommand {
    Play(usize),
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
    #[error("command accepts no additional arguments")]
    ExtraArguments,
}

pub fn parse_command(input: &str) -> Result<AuditionCommand, CommandParseError> {
    let mut words = input.split_whitespace();
    let Some(command) = words.next() else {
        return Ok(AuditionCommand::Empty);
    };
    if matches!(command, "play" | "p") {
        return Err(CommandParseError::MissingChunkNumber);
    }
    if command.as_bytes().first().is_some_and(u8::is_ascii_digit) {
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
                "-nostdin",
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

pub fn run_session(
    plan: &RecognitionPlan,
    source: &Path,
    input: &mut impl BufRead,
    output: &mut impl Write,
    errors: &mut impl Write,
    player: &mut impl AudioPlayer,
) -> io::Result<()> {
    render_chunks(plan, source, output)?;
    if plan.chunks.is_empty() {
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
                let Some(chunk) = plan.chunks.get(number - 1) else {
                    writeln!(
                        errors,
                        "invalid chunk number {number}; expected 1..={}",
                        plan.chunks.len()
                    )?;
                    continue;
                };
                if let Err(error) = player.play(source, plan.source.sample_rate_hz, chunk.submitted)
                {
                    writeln!(errors, "playback failed for chunk {number}: {error}")?;
                }
            }
            Ok(AuditionCommand::List) => render_chunks(plan, source, output)?,
            Ok(AuditionCommand::Help) => render_help(output)?,
            Ok(AuditionCommand::Quit) => return Ok(()),
            Ok(AuditionCommand::Empty) => {}
            Err(error) => writeln!(errors, "{error}")?,
        }
    }
}

pub fn render_chunks(
    plan: &RecognitionPlan,
    source: &Path,
    output: &mut impl Write,
) -> io::Result<()> {
    writeln!(
        output,
        "Planned {} chunks from {}",
        plan.chunks.len(),
        source.display()
    )?;
    if plan.chunks.is_empty() {
        return Ok(());
    }
    writeln!(output)?;
    for (index, chunk) in plan.chunks.iter().enumerate() {
        writeln!(
            output,
            "{:>3}  {} – {}  {:>9}  {}",
            index + 1,
            Timestamp::new(chunk.submitted.start_sample, plan.source.sample_rate_hz),
            Timestamp::new(chunk.submitted.end_sample, plan.source.sample_rate_hz),
            Duration::new(chunk.submitted.len(), plan.source.sample_rate_hz),
            boundary_label(&chunk.boundary.kind),
        )?;
        if chunk.core != chunk.submitted {
            writeln!(
                output,
                "     core {} – {}",
                Timestamp::new(chunk.core.start_sample, plan.source.sample_rate_hz),
                Timestamp::new(chunk.core.end_sample, plan.source.sample_rate_hz),
            )?;
        }
    }
    Ok(())
}

fn render_help(output: &mut impl Write) -> io::Result<()> {
    writeln!(output)?;
    writeln!(output, "Commands: Nplay (or Np), list, help, quit")
}

fn boundary_label(kind: &BoundaryKind) -> &'static str {
    match kind {
        BoundaryKind::SourceEnd => "source end",
        BoundaryKind::VadValley => "vad valley",
        BoundaryKind::HardLimitNoCandidate => "hard limit (no candidate)",
        BoundaryKind::HardLimitDetectorUnavailable => "hard limit (detector unavailable)",
    }
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
    }

    #[test]
    fn ffplay_seconds_preserve_sample_precision() {
        assert_eq!(samples_as_seconds(1, 16_000), "0.000062500");
        assert_eq!(samples_as_seconds(480_001, 16_000), "30.000062500");
    }
}
