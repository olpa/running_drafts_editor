//! Line-oriented developer tooling for listening to recognition chunks.

use std::{
    fmt,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

use crate::chunking::SampleRange;
use crate::document::Document;
use crate::editor::{
    apply_alternative, apply_chunk_merge, apply_chunk_split, apply_delete, apply_insert,
    apply_paragraph_merge, apply_paragraph_split, apply_replace, render_alternatives,
    render_document_with_navigation,
};
use crate::navigation::{
    parse_line, Address, Caret, CommandLine, NavigationState, Selection, SyntaxError, TokenAddress,
};
use crate::persistence::{load_document, save_document};
use crate::recognition::{ChunkBoundaryReason, RecognitionRun};
use crate::replay::{resolve as resolve_replay, ResolvedReplay};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditionCommand {
    Play {
        address: Option<Address>,
        speed: PlaybackSpeed,
    },
    Replay {
        speed: PlaybackSpeed,
    },
    Stop,
    Info {
        paragraph: usize,
        chunk: usize,
    },
    Refresh {
        marker: Option<(usize, usize)>,
    },
    Model(Option<PathBuf>),
    Language(Option<String>),
    Print(Option<usize>),
    Move(Address),
    Select(Address),
    Tokens(usize),
    Alternatives {
        address: Option<TokenAddress>,
    },
    ChooseAlternative {
        address: Option<TokenAddress>,
        candidate: usize,
    },
    Insert {
        address: TokenAddress,
        text: String,
    },
    Append {
        address: TokenAddress,
        text: String,
    },
    Replace {
        range: Option<(TokenAddress, TokenAddress)>,
        replacement: ReplacementText,
    },
    Delete {
        range: Option<(TokenAddress, TokenAddress)>,
    },
    SplitChunk {
        address: Option<TokenAddress>,
        after: bool,
    },
    SplitParagraph {
        marker: Option<(usize, usize)>,
    },
    MergeParagraph(usize),
    MergeChunks {
        paragraph: usize,
        marker: usize,
    },
    Save(Option<PathBuf>),
    Load(PathBuf),
    Help,
    Quit,
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplacementText {
    pub text: String,
    pub exact_boundaries: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackSpeed {
    Normal,
    Slow,
}

impl PlaybackSpeed {
    fn atempo(self) -> &'static str {
        match self {
            Self::Normal => "1.0",
            Self::Slow => "0.75",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LastPlayback {
    source_id: String,
    range: SampleRange,
    require_file: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct ReplayStart {
    pub context_samples: u64,
    pub speed: PlaybackSpeed,
    pub require_file: bool,
}

pub(crate) fn start_document_replay(
    document: &Document,
    navigation: &NavigationState,
    address: Option<&Address>,
    start: ReplayStart,
    player: &mut impl AudioPlayer,
    output: &mut impl Write,
    errors: &mut impl Write,
) -> io::Result<Option<LastPlayback>> {
    let resolved = match resolve_replay(document, navigation, address, start.context_samples) {
        Ok(resolved) => resolved,
        Err(error) => {
            writeln!(errors, "replay unavailable: {error}")?;
            return Ok(None);
        }
    };
    start_resolved(
        document,
        &resolved,
        start.speed,
        start.require_file,
        player,
        output,
        errors,
    )
}

fn start_resolved(
    document: &Document,
    resolved: &ResolvedReplay,
    speed: PlaybackSpeed,
    require_file: bool,
    player: &mut impl AudioPlayer,
    output: &mut impl Write,
    errors: &mut impl Write,
) -> io::Result<Option<LastPlayback>> {
    let Some(source) = document.audio_source(&resolved.source_id) else {
        writeln!(
            errors,
            "replay unavailable: audio source '{}' is missing",
            resolved.source_id
        )?;
        return Ok(None);
    };
    let Some(path) = source.path() else {
        writeln!(errors, "audio source '{}' has no local path", source.id())?;
        return Ok(None);
    };
    if require_file && !path.is_file() {
        writeln!(
            errors,
            "audio source '{}' is unavailable at {}",
            source.id(),
            path.display()
        )?;
        return Ok(None);
    }
    if resolved.partial {
        writeln!(errors, "replay uses partial token alignment")?;
    }
    if resolved.alignment != crate::document::AlignmentState::Exact {
        writeln!(errors, "replay alignment is {}", resolved.alignment)?;
    }
    match player.start(path, 16_000, resolved.range, speed) {
        Ok(()) => {
            writeln!(
                output,
                "playing [{}, {}) at {} speed",
                resolved.range.start_sample,
                resolved.range.end_sample,
                speed.atempo()
            )?;
            Ok(Some(LastPlayback {
                source_id: resolved.source_id.clone(),
                range: resolved.range,
                require_file,
            }))
        }
        Err(error) => {
            writeln!(errors, "playback failed: {error}")?;
            Ok(None)
        }
    }
}

pub(crate) fn repeat_document_replay(
    document: &Document,
    last: Option<&LastPlayback>,
    speed: PlaybackSpeed,
    player: &mut impl AudioPlayer,
    output: &mut impl Write,
    errors: &mut impl Write,
) -> io::Result<()> {
    let Some(last) = last else {
        writeln!(errors, "there is no previous replay")?;
        return Ok(());
    };
    let resolved = ResolvedReplay {
        source_id: last.source_id.clone(),
        range: last.range,
        alignment: crate::document::AlignmentState::Exact,
        partial: false,
    };
    let _ = start_resolved(
        document,
        &resolved,
        speed,
        last.require_file,
        player,
        output,
        errors,
    )?;
    Ok(())
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
    #[error("{0} requires a document path")]
    PathRequired(String),
    #[error("{0} requires text after the command")]
    TextRequired(String),
    #[error("invalid quoted replacement: {0}")]
    InvalidQuotedReplacement(String),
    #[error("{0} requires a positive alternative number")]
    AlternativeNumberRequired(String),
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

    match name.as_str() {
        "model" => no_address(
            address,
            name,
            AuditionCommand::Model((!arguments.is_empty()).then(|| PathBuf::from(arguments))),
        ),
        "language" => no_address(
            address,
            name,
            AuditionCommand::Language((!arguments.is_empty()).then_some(arguments)),
        ),
        "refresh" => {
            reject_arguments(&name, &arguments)?;
            let marker = match address {
                Some(Address::Marker { paragraph, marker }) => Some((paragraph, marker)),
                None => None,
                Some(address) => {
                    return Err(CommandParseError::InvalidAddress {
                        command: name,
                        address,
                        expected: "a chunk-marker address M@N",
                    })
                }
            };
            Ok(AuditionCommand::Refresh { marker })
        }
        "save" => no_address(
            address,
            name,
            AuditionCommand::Save((!arguments.is_empty()).then(|| PathBuf::from(arguments))),
        ),
        "load" | "edit" => {
            if arguments.is_empty() {
                return Err(CommandParseError::PathRequired(name));
            }
            no_address(
                address,
                name,
                AuditionCommand::Load(PathBuf::from(arguments)),
            )
        }
        "print" | "p" | "list" | "l" => {
            reject_arguments(&name, &arguments)?;
            match address {
                None => Ok(AuditionCommand::Print(None)),
                Some(Address::Paragraph(paragraph)) => Ok(AuditionCommand::Print(Some(paragraph))),
                Some(address) => Err(CommandParseError::InvalidAddress {
                    command: name,
                    address,
                    expected: "a paragraph address M",
                }),
            }
        }
        "play" | "slowplay" => {
            reject_arguments(&name, &arguments)?;
            Ok(AuditionCommand::Play {
                address,
                speed: if name == "slowplay" {
                    PlaybackSpeed::Slow
                } else {
                    PlaybackSpeed::Normal
                },
            })
        }
        "replay" | "slowreplay" => {
            reject_arguments(&name, &arguments)?;
            no_address(
                address,
                name.clone(),
                AuditionCommand::Replay {
                    speed: if name == "slowreplay" {
                        PlaybackSpeed::Slow
                    } else {
                        PlaybackSpeed::Normal
                    },
                },
            )
        }
        "stop" => {
            reject_arguments(&name, &arguments)?;
            no_address(address, name, AuditionCommand::Stop)
        }
        "select" | "sel" | "s" => {
            reject_arguments(&name, &arguments)?;
            address
                .map(AuditionCommand::Select)
                .ok_or(CommandParseError::AddressRequired {
                    command: name,
                    expected: "an address M, M.N, M.N,M.U, M@N, M@N,M@U, or .",
                })
        }
        "tokens" => {
            reject_arguments(&name, &arguments)?;
            match address {
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
            }
        }
        "alternatives" | "alts" => {
            reject_arguments(&name, &arguments)?;
            optional_token(address, name).map(|address| AuditionCommand::Alternatives { address })
        }
        "choose" => {
            let candidate = arguments
                .parse::<usize>()
                .ok()
                .filter(|value| *value > 0)
                .ok_or_else(|| CommandParseError::AlternativeNumberRequired(name.clone()))?;
            optional_token(address, name)
                .map(|address| AuditionCommand::ChooseAlternative { address, candidate })
        }
        "insert" | "append" => {
            if arguments.is_empty() {
                return Err(CommandParseError::TextRequired(name));
            }
            match address {
                Some(Address::Token(address)) if name == "insert" => Ok(AuditionCommand::Insert {
                    address,
                    text: arguments,
                }),
                Some(Address::Token(address)) => Ok(AuditionCommand::Append {
                    address,
                    text: arguments,
                }),
                Some(address) => Err(CommandParseError::InvalidAddress {
                    command: name,
                    address,
                    expected: "a token address M.N",
                }),
                None => Err(CommandParseError::AddressRequired {
                    command: name,
                    expected: "a token address M.N",
                }),
            }
        }
        "replace" => {
            if arguments.is_empty() {
                return Err(CommandParseError::TextRequired(name));
            }
            let replacement = parse_replacement(&arguments)?;
            optional_token_range(address, name)
                .map(|range| AuditionCommand::Replace { range, replacement })
        }
        "delete" => {
            reject_arguments(&name, &arguments)?;
            optional_token_range(address, name).map(|range| AuditionCommand::Delete { range })
        }
        "split" | "isplit" | "asplit" => {
            reject_arguments(&name, &arguments)?;
            let token = match address {
                Some(Address::Token(token)) => Some(token),
                None => None,
                Some(address) => {
                    return Err(CommandParseError::InvalidAddress {
                        command: name,
                        address,
                        expected: "a token address M.N",
                    })
                }
            };
            Ok(AuditionCommand::SplitChunk {
                address: token,
                after: name == "asplit",
            })
        }
        "parasplit" => {
            reject_arguments(&name, &arguments)?;
            let marker = match address {
                Some(Address::Marker { paragraph, marker }) => Some((paragraph, marker)),
                None => None,
                Some(address) => {
                    return Err(CommandParseError::InvalidAddress {
                        command: name,
                        address,
                        expected: "a chunk-marker address M@N",
                    })
                }
            };
            Ok(AuditionCommand::SplitParagraph { marker })
        }
        "merge" => {
            reject_arguments(&name, &arguments)?;
            match address {
                Some(Address::Paragraph(paragraph)) => {
                    Ok(AuditionCommand::MergeParagraph(paragraph))
                }
                Some(Address::Marker { paragraph, marker }) => {
                    Ok(AuditionCommand::MergeChunks { paragraph, marker })
                }
                Some(address) => Err(CommandParseError::InvalidAddress {
                    command: name,
                    address,
                    expected: "a paragraph M or chunk-marker M@N address",
                }),
                None => Err(CommandParseError::AddressRequired {
                    command: name,
                    expected: "a paragraph M or chunk-marker M@N address",
                }),
            }
        }
        "info" | "i" => {
            reject_arguments(&name, &arguments)?;
            marker_command(address, name, |paragraph, chunk| AuditionCommand::Info {
                paragraph,
                chunk,
            })
        }
        "help" | "h" => {
            reject_arguments(&name, &arguments)?;
            no_address(address, name, AuditionCommand::Help)
        }
        "quit" | "q" => {
            reject_arguments(&name, &arguments)?;
            no_address(address, name, AuditionCommand::Quit)
        }
        _ => Err(CommandParseError::Unknown(name)),
    }
}

fn parse_replacement(input: &str) -> Result<ReplacementText, CommandParseError> {
    if !input.starts_with('"') {
        return Ok(ReplacementText {
            text: input.into(),
            exact_boundaries: false,
        });
    }
    let mut text = String::new();
    let mut characters = input[1..].chars();
    while let Some(character) = characters.next() {
        match character {
            '"' if characters.as_str().is_empty() => {
                if text.is_empty() {
                    return Err(CommandParseError::InvalidQuotedReplacement(
                        "empty text is not allowed; use delete".into(),
                    ));
                }
                return Ok(ReplacementText {
                    text,
                    exact_boundaries: true,
                });
            }
            '"' => {
                return Err(CommandParseError::InvalidQuotedReplacement(
                    "unexpected text after the closing quote".into(),
                ));
            }
            '\\' => match characters.next() {
                Some('"') => text.push('"'),
                Some('\\') => text.push('\\'),
                Some(other) => {
                    return Err(CommandParseError::InvalidQuotedReplacement(format!(
                        "unsupported escape '\\{other}'"
                    )));
                }
                None => {
                    return Err(CommandParseError::InvalidQuotedReplacement(
                        "unfinished escape".into(),
                    ));
                }
            },
            other => text.push(other),
        }
    }
    Err(CommandParseError::InvalidQuotedReplacement(
        "missing closing quote".into(),
    ))
}

fn reject_arguments(command: &str, arguments: &str) -> Result<(), CommandParseError> {
    if arguments.is_empty() {
        Ok(())
    } else {
        Err(CommandParseError::ExtraArguments(command.into()))
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

fn optional_token_range(
    address: Option<Address>,
    command: String,
) -> Result<Option<(TokenAddress, TokenAddress)>, CommandParseError> {
    match address {
        Some(Address::TokenRange { start, end }) => Ok(Some((start, end))),
        Some(address) => Err(CommandParseError::InvalidAddress {
            command,
            address,
            expected: "an inclusive token range M.N,M.U",
        }),
        None => Ok(None),
    }
}

fn optional_token(
    address: Option<Address>,
    command: String,
) -> Result<Option<TokenAddress>, CommandParseError> {
    match address {
        Some(Address::Token(token)) => Ok(Some(token)),
        Some(address) => Err(CommandParseError::InvalidAddress {
            command,
            address,
            expected: "a token address M.N",
        }),
        None => Ok(None),
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

    fn start(
        &mut self,
        source: &Path,
        sample_rate_hz: u32,
        range: SampleRange,
        _speed: PlaybackSpeed,
    ) -> Result<(), PlaybackError> {
        self.play(source, sample_rate_hz, range)
    }

    fn stop(&mut self) -> Result<bool, PlaybackError> {
        Ok(false)
    }
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

#[derive(Debug)]
pub struct Ffplay {
    program: PathBuf,
    child: Option<Child>,
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
            child: None,
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

    fn start(
        &mut self,
        source: &Path,
        sample_rate_hz: u32,
        range: SampleRange,
        speed: PlaybackSpeed,
    ) -> Result<(), PlaybackError> {
        self.stop()?;
        if sample_rate_hz == 0 {
            return Err(PlaybackError::ZeroSampleRate);
        }
        if range.start_sample >= range.end_sample {
            return Err(PlaybackError::InvalidRange(range));
        }
        let start = samples_as_seconds(range.start_sample, sample_rate_hz);
        let duration = samples_as_seconds(range.len(), sample_rate_hz);
        let child = Command::new(&self.program)
            .args([
                "-nodisp",
                "-autoexit",
                "-loglevel",
                "error",
                "-ss",
                &start,
                "-t",
                &duration,
                "-af",
            ])
            .arg(format!("atempo={}", speed.atempo()))
            .arg("-i")
            .arg(source)
            .stdin(Stdio::null())
            .spawn()
            .map_err(|source| PlaybackError::Start {
                program: self.program.clone(),
                source,
            })?;
        self.child = Some(child);
        Ok(())
    }

    fn stop(&mut self) -> Result<bool, PlaybackError> {
        let Some(mut child) = self.child.take() else {
            return Ok(false);
        };
        if child
            .try_wait()
            .map_err(|error| PlaybackError::Other(error.to_string()))?
            .is_some()
        {
            return Ok(false);
        }
        child
            .kill()
            .map_err(|error| PlaybackError::Other(error.to_string()))?;
        child
            .wait()
            .map_err(|error| PlaybackError::Other(error.to_string()))?;
        Ok(true)
    }
}

impl Drop for Ffplay {
    fn drop(&mut self) {
        let _ = self.stop();
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
    replay_context_samples: u64,
) -> io::Result<()> {
    let mut document = Document::from_run_with_source(run, Some(source));
    let mut document_path = None::<PathBuf>;
    let mut loaded_document = false;
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
    let mut last_playback = None;
    loop {
        write!(output, "rde> ")?;
        output.flush()?;
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            return Ok(());
        }
        match parse_command(&line) {
            Ok(AuditionCommand::Play { address, speed }) => {
                if let Some(value) = start_document_replay(
                    &document,
                    &navigation,
                    address.as_ref(),
                    ReplayStart {
                        context_samples: replay_context_samples,
                        speed,
                        require_file: loaded_document,
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
            Ok(
                AuditionCommand::Refresh { .. }
                | AuditionCommand::Model(_)
                | AuditionCommand::Language(_),
            ) => {
                writeln!(errors, "recognition settings and refresh are available in: rde edit DOCUMENT --model MODEL")?;
            }
            Ok(AuditionCommand::Print(None)) => {
                if loaded_document {
                    render_document_with_navigation(&document, Some(&navigation), output)?;
                } else {
                    render_recognition_document_with_navigation(
                        run,
                        &document,
                        source,
                        Some(&navigation),
                        output,
                    )?;
                }
            }
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
            Ok(AuditionCommand::Save(path)) => {
                let path = path.or_else(|| document_path.clone());
                let Some(path) = path else {
                    writeln!(
                        errors,
                        "save requires a document path in an audition session"
                    )?;
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
                    document_path = Some(path.clone());
                    loaded_document = true;
                    navigation = NavigationState::new(&document);
                    writeln!(output, "loaded {}", path.display())?;
                    render_document_with_navigation(&document, Some(&navigation), output)?;
                }
                Err(error) => writeln!(errors, "{error}")?,
            },
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
            let marker_range_start = navigation.is_some_and(|state| {
                matches!(state.selection(), Some(Selection::MarkerRange { start, .. })
                    if start.paragraph_id == paragraph.id()
                        && start.paragraph_revision == paragraph.revision()
                        && start.chunk_id == marker.chunk_id())
            });
            let marker_range_end = navigation.is_some_and(|state| {
                matches!(state.selection(), Some(Selection::MarkerRange { end_exclusive, .. })
                    if end_exclusive.paragraph_id == paragraph.id()
                        && end_exclusive.paragraph_revision == paragraph.revision()
                        && end_exclusive.chunk_id == marker.chunk_id())
            });
            let marker_caret = navigation.is_some_and(|state| {
                state.selection().is_none()
                    && matches!(state.caret(), Some(Caret::Marker(position))
                        if position.paragraph_id == paragraph.id()
                            && position.paragraph_revision == paragraph.revision()
                            && position.chunk_id == marker.chunk_id())
            });
            if marker_range_end {
                write!(output, "⟫")?;
            }
            write!(output, " ")?;
            if marker_range_start {
                write!(output, "⟪")?;
            }
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
        "  Aselect, Asel, As select token/marker range, paragraph, or marker A"
    )?;
    writeln!(
        output,
        "  Mtokens         list paragraph M tokens and markers"
    )?;
    writeln!(
        output,
        "  [M.N]alternatives, alts list alternatives for one token/current token"
    )?;
    writeln!(
        output,
        "  [M.N]choose N   replace one token with alternative N"
    )?;
    writeln!(
        output,
        "  M.Ninsert TEXT insert one pseudo-token before M.N"
    )?;
    writeln!(output, "  M.Nappend TEXT insert one pseudo-token after M.N")?;
    writeln!(
        output,
        "  [M.N,M.U]replace TEXT replace range or current token selection"
    )?;
    writeln!(
        output,
        "                    unquoted keeps selected boundary whitespace"
    )?;
    writeln!(
        output,
        "                    quoted \"TEXT\" controls boundaries exactly"
    )?;
    writeln!(
        output,
        "  [M.N,M.U]delete delete range or current token selection"
    )?;
    writeln!(
        output,
        "  [M.N]split/isplit split chunk before token/current caret"
    )?;
    writeln!(
        output,
        "  [M.N]asplit split chunk after token/current caret"
    )?;
    writeln!(
        output,
        "  [M@N]parasplit split paragraph after marker/current marker"
    )?;
    writeln!(
        output,
        "  Mmerge         merge paragraph M with M+1 exactly"
    )?;
    writeln!(
        output,
        "  M@Nmerge       merge chunks around marker M@N when legal"
    )?;
    writeln!(
        output,
        "  [A]play        play current/addressed token range, paragraph, or chunk"
    )?;
    writeln!(
        output,
        "  M@N,M@Uplay   play marker interval; left included, right excluded"
    )?;
    writeln!(
        output,
        "  [A]slowplay    play the same target at 0.75 speed"
    )?;
    writeln!(
        output,
        "  replay         play the last resolved range again"
    )?;
    writeln!(
        output,
        "  slowreplay     play the last range again at 0.75 speed"
    )?;
    writeln!(output, "  stop           stop active playback")?;
    writeln!(
        output,
        "  M@Ninfo, M@Ni show details for marker M@N; for example, 2@3info"
    )?;
    writeln!(output, "  list, l        aliases for print")?;
    writeln!(
        output,
        "  save [PATH]    save the visible document; for example, save draft.rde.json"
    )?;
    writeln!(
        output,
        "  load PATH      replace the document and reset navigation"
    )?;
    writeln!(output, "  edit PATH      alias for load PATH")?;
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
                address: Some(Address::Marker {
                    paragraph: 2,
                    marker: 3
                }),
                speed: PlaybackSpeed::Normal,
            }
        );
        assert_eq!(
            parse_command("play").unwrap(),
            AuditionCommand::Play {
                address: None,
                speed: PlaybackSpeed::Normal
            }
        );
        assert_eq!(
            parse_command("2slowplay").unwrap(),
            AuditionCommand::Play {
                address: Some(Address::Paragraph(2)),
                speed: PlaybackSpeed::Slow
            }
        );
        assert_eq!(
            parse_command("replay").unwrap(),
            AuditionCommand::Replay {
                speed: PlaybackSpeed::Normal
            }
        );
        assert_eq!(parse_command("stop").unwrap(), AuditionCommand::Stop);
        assert_eq!(
            parse_command("1.2split").unwrap(),
            AuditionCommand::SplitChunk {
                address: Some(TokenAddress {
                    paragraph: 1,
                    token: 2,
                }),
                after: false,
            }
        );
        assert_eq!(
            parse_command("asplit").unwrap(),
            AuditionCommand::SplitChunk {
                address: None,
                after: true,
            }
        );
        assert_eq!(
            parse_command("1@2parasplit").unwrap(),
            AuditionCommand::SplitParagraph {
                marker: Some((1, 2)),
            }
        );
        assert_eq!(
            parse_command("1merge").unwrap(),
            AuditionCommand::MergeParagraph(1)
        );
        assert_eq!(
            parse_command("1@2merge").unwrap(),
            AuditionCommand::MergeChunks {
                paragraph: 1,
                marker: 2,
            }
        );
        assert_eq!(
            parse_command("1.2insert  typed text  ").unwrap(),
            AuditionCommand::Insert {
                address: TokenAddress {
                    paragraph: 1,
                    token: 2,
                },
                text: " typed text  ".into(),
            }
        );
        assert_eq!(
            parse_command("1.2 append text").unwrap(),
            AuditionCommand::Append {
                address: TokenAddress {
                    paragraph: 1,
                    token: 2,
                },
                text: "text".into(),
            }
        );
        assert_eq!(
            parse_command("1.2,1.4replace new text").unwrap(),
            AuditionCommand::Replace {
                range: Some((
                    TokenAddress {
                        paragraph: 1,
                        token: 2,
                    },
                    TokenAddress {
                        paragraph: 1,
                        token: 4,
                    },
                )),
                replacement: ReplacementText {
                    text: "new text".into(),
                    exact_boundaries: false,
                },
            }
        );
        assert_eq!(
            parse_command("1.2,1.4delete").unwrap(),
            AuditionCommand::Delete {
                range: Some((
                    TokenAddress {
                        paragraph: 1,
                        token: 2,
                    },
                    TokenAddress {
                        paragraph: 1,
                        token: 4,
                    },
                )),
            }
        );
        assert_eq!(
            parse_command("1@1,1@2sel").unwrap(),
            AuditionCommand::Select(Address::MarkerRange {
                start_paragraph: 1,
                start_marker: 1,
                end_paragraph: 1,
                end_marker_exclusive: 2,
            })
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
        assert_eq!(
            parse_command("load document.rde.json").unwrap(),
            AuditionCommand::Load(PathBuf::from("document.rde.json"))
        );
        assert_eq!(
            parse_command("edit other document.json").unwrap(),
            AuditionCommand::Load(PathBuf::from("other document.json"))
        );
        assert_eq!(parse_command(" q ").unwrap(), AuditionCommand::Quit);

        assert_eq!(parse_command("  ").unwrap(), AuditionCommand::Empty);
    }

    #[test]
    fn parser_reports_command_specific_address_errors() {
        assert_eq!(
            parse_command("1play").unwrap(),
            AuditionCommand::Play {
                address: Some(Address::Paragraph(1)),
                speed: PlaybackSpeed::Normal
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
            parse_command("unknown argument").unwrap_err(),
            CommandParseError::Unknown("unknown".into())
        );
        assert_eq!(
            parse_command("load").unwrap_err(),
            CommandParseError::PathRequired("load".into())
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
        assert_eq!(
            parse_command("1.2insert").unwrap_err(),
            CommandParseError::TextRequired("insert".into())
        );
        assert_eq!(
            parse_command("replace text").unwrap(),
            AuditionCommand::Replace {
                range: None,
                replacement: ReplacementText {
                    text: "text".into(),
                    exact_boundaries: false,
                },
            }
        );
        assert_eq!(
            parse_command("delete").unwrap(),
            AuditionCommand::Delete { range: None }
        );
        assert!(matches!(
            parse_command("1.2replace text"),
            Err(CommandParseError::InvalidAddress { .. })
        ));
        assert!(matches!(
            parse_command("1.2delete"),
            Err(CommandParseError::InvalidAddress { .. })
        ));
    }

    #[test]
    fn quoted_replacement_controls_boundaries_and_escapes_quotes_and_backslashes() {
        assert_eq!(
            parse_command(r#"replace " exact \"text\"\\ ""#).unwrap(),
            AuditionCommand::Replace {
                range: None,
                replacement: ReplacementText {
                    text: " exact \"text\"\\ ".into(),
                    exact_boundaries: true,
                },
            }
        );
        assert!(matches!(
            parse_command(r#"replace "unfinished"#),
            Err(CommandParseError::InvalidQuotedReplacement(_))
        ));
        assert!(matches!(
            parse_command(r#"replace """#),
            Err(CommandParseError::InvalidQuotedReplacement(_))
        ));
        assert!(matches!(
            parse_command(r#"replace "bad\n""#),
            Err(CommandParseError::InvalidQuotedReplacement(_))
        ));
    }

    #[test]
    fn help_explains_each_session_command_with_examples() {
        let mut output = Vec::new();

        render_help(&mut output).unwrap();

        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("print, p"));
        assert!(output.contains("Mprint, Mp"));
        assert!(output.contains("M.N, M@N        move the caret"));
        assert!(output.contains("Aselect, Asel, As"));
        assert!(output.contains("Mtokens"));
        assert!(output.contains("[M.N,M.U]replace TEXT"));
        assert!(output.contains("unquoted keeps selected boundary whitespace"));
        assert!(output.contains("quoted \"TEXT\" controls boundaries exactly"));
        assert!(output.contains("split/isplit split chunk before"));
        assert!(output.contains("parasplit split paragraph"));
        assert!(output.contains("M@Nmerge"));
        assert!(output.contains("[A]play        play current/addressed"));
        assert!(output.contains("M@N,M@Uplay   play marker interval"));
        assert!(output.contains("[A]slowplay"));
        assert!(output.contains("replay"));
        assert!(output.contains("stop"));
        assert!(output.contains("M@Ninfo, M@Ni"));
        assert!(output.contains("list, l"));
        assert!(output.contains("save [PATH]    save the visible document"));
        assert!(output.contains("save draft.rde.json"));
        assert!(output.contains("load PATH"));
        assert!(output.contains("edit PATH"));
        assert!(output.contains("help, h"));
        assert!(output.contains("quit, q"));
    }

    #[test]
    fn ffplay_seconds_preserve_sample_precision() {
        assert_eq!(samples_as_seconds(1, 16_000), "0.000062500");
        assert_eq!(samples_as_seconds(480_001, 16_000), "30.000062500");
    }
}
