use std::path::PathBuf;

use crate::navigation::{parse_line, Address, CommandLine, SyntaxError, TokenAddress};

use super::playback::PlaybackSpeed;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionCommand {
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
    Undo(usize),
    Redo(usize),
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
    #[error("{0} requires a positive count")]
    HistoryCountRequired(String),
}

pub fn parse_command(input: &str) -> Result<SessionCommand, CommandParseError> {
    let compact = input.trim();
    for (suffix, command) in [
        ("undo", SessionCommand::Undo as fn(usize) -> SessionCommand),
        ("redo", SessionCommand::Redo as fn(usize) -> SessionCommand),
    ] {
        if let Some(count) = compact
            .strip_suffix(suffix)
            .filter(|value| !value.is_empty())
        {
            if count.chars().all(|character| character.is_ascii_digit()) {
                let count = count
                    .parse::<usize>()
                    .ok()
                    .filter(|value| *value > 0)
                    .ok_or_else(|| CommandParseError::HistoryCountRequired(suffix.into()))?;
                return Ok(command(count));
            }
        }
    }
    let (address, name, arguments) = match parse_line(input)? {
        CommandLine::Empty => return Ok(SessionCommand::Empty),
        CommandLine::Address(address) => return Ok(SessionCommand::Move(address)),
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
            SessionCommand::Model((!arguments.is_empty()).then(|| PathBuf::from(arguments))),
        ),
        "language" => no_address(
            address,
            name,
            SessionCommand::Language((!arguments.is_empty()).then_some(arguments)),
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
            Ok(SessionCommand::Refresh { marker })
        }
        "save" => no_address(
            address,
            name,
            SessionCommand::Save((!arguments.is_empty()).then(|| PathBuf::from(arguments))),
        ),
        "load" | "edit" => {
            if arguments.is_empty() {
                return Err(CommandParseError::PathRequired(name));
            }
            no_address(
                address,
                name,
                SessionCommand::Load(PathBuf::from(arguments)),
            )
        }
        "print" | "p" | "list" | "l" => {
            reject_arguments(&name, &arguments)?;
            match address {
                None => Ok(SessionCommand::Print(None)),
                Some(Address::Paragraph(paragraph)) => Ok(SessionCommand::Print(Some(paragraph))),
                Some(address) => Err(CommandParseError::InvalidAddress {
                    command: name,
                    address,
                    expected: "a paragraph address M",
                }),
            }
        }
        "play" | "slowplay" => {
            reject_arguments(&name, &arguments)?;
            Ok(SessionCommand::Play {
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
                SessionCommand::Replay {
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
            no_address(address, name, SessionCommand::Stop)
        }
        "select" | "sel" | "s" => {
            reject_arguments(&name, &arguments)?;
            address
                .map(SessionCommand::Select)
                .ok_or(CommandParseError::AddressRequired {
                    command: name,
                    expected: "an address M, M.N, M.N,M.U, M@N, M@N,M@U, or .",
                })
        }
        "tokens" => {
            reject_arguments(&name, &arguments)?;
            match address {
                Some(Address::Paragraph(paragraph)) => Ok(SessionCommand::Tokens(paragraph)),
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
            optional_token(address, name).map(|address| SessionCommand::Alternatives { address })
        }
        "choose" => {
            let candidate = arguments
                .parse::<usize>()
                .ok()
                .filter(|value| *value > 0)
                .ok_or_else(|| CommandParseError::AlternativeNumberRequired(name.clone()))?;
            optional_token(address, name)
                .map(|address| SessionCommand::ChooseAlternative { address, candidate })
        }
        "insert" | "append" => {
            if arguments.is_empty() {
                return Err(CommandParseError::TextRequired(name));
            }
            match address {
                Some(Address::Token(address)) if name == "insert" => Ok(SessionCommand::Insert {
                    address,
                    text: arguments,
                }),
                Some(Address::Token(address)) => Ok(SessionCommand::Append {
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
                .map(|range| SessionCommand::Replace { range, replacement })
        }
        "delete" => {
            reject_arguments(&name, &arguments)?;
            optional_token_range(address, name).map(|range| SessionCommand::Delete { range })
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
            Ok(SessionCommand::SplitChunk {
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
            Ok(SessionCommand::SplitParagraph { marker })
        }
        "merge" => {
            reject_arguments(&name, &arguments)?;
            match address {
                Some(Address::Paragraph(paragraph)) => {
                    Ok(SessionCommand::MergeParagraph(paragraph))
                }
                Some(Address::Marker { paragraph, marker }) => {
                    Ok(SessionCommand::MergeChunks { paragraph, marker })
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
        "undo" | "redo" => {
            reject_arguments(&name, &arguments)?;
            no_address(
                address,
                name.clone(),
                if name == "undo" {
                    SessionCommand::Undo(1)
                } else {
                    SessionCommand::Redo(1)
                },
            )
        }
        "info" | "i" => {
            reject_arguments(&name, &arguments)?;
            marker_command(address, name, |paragraph, chunk| SessionCommand::Info {
                paragraph,
                chunk,
            })
        }
        "help" | "h" => {
            reject_arguments(&name, &arguments)?;
            no_address(address, name, SessionCommand::Help)
        }
        "quit" | "q" => {
            reject_arguments(&name, &arguments)?;
            no_address(address, name, SessionCommand::Quit)
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
    build: impl FnOnce(usize, usize) -> SessionCommand,
) -> Result<SessionCommand, CommandParseError> {
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
    result: SessionCommand,
) -> Result<SessionCommand, CommandParseError> {
    if address.is_some() {
        Err(CommandParseError::UnexpectedAddress(command))
    } else {
        Ok(result)
    }
}
