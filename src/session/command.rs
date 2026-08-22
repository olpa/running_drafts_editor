use std::path::PathBuf;

use crate::navigation::{parse_line, Address, CommandLine, SyntaxError, TokenAddress};

use super::playback::PlaybackSpeed;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SessionCommand {
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
    Tokens(Option<usize>),
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
    NextIssue,
    PreviousIssue,
    Issues,
    Ignore(Option<usize>),
    Unignore(usize),
    IssueProbability {
        level: Option<String>,
        value: Option<String>,
    },
    Save(Option<PathBuf>),
    Load(PathBuf),
    Help,
    Quit,
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReplacementText {
    pub(crate) text: String,
    pub(crate) exact_boundaries: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum CommandParseError {
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

pub(crate) fn parse_command(input: &str) -> Result<SessionCommand, CommandParseError> {
    let compact = input.trim();
    if compact == "issue-prob" {
        return Ok(SessionCommand::IssueProbability {
            level: None,
            value: None,
        });
    }
    if let Some(arguments) = compact.strip_prefix("issue-prob ") {
        let mut parts = arguments.split_whitespace();
        let level = parts.next().map(str::to_owned);
        let value = parts.next().map(str::to_owned);
        if level.is_none() || value.is_none() || parts.next().is_some() {
            return Err(CommandParseError::ExtraArguments(
                "issue-prob requires red VALUE or orange VALUE".into(),
            ));
        }
        return Ok(SessionCommand::IssueProbability { level, value });
    }
    for (suffix, unignore) in [("unignore", true), ("ignore", false), ("resolve", false)] {
        if let Some(number) = compact.strip_suffix(suffix).filter(|v| !v.is_empty()) {
            if number.chars().all(|c| c.is_ascii_digit()) {
                let number = number
                    .parse::<usize>()
                    .ok()
                    .filter(|n| *n > 0)
                    .ok_or_else(|| CommandParseError::HistoryCountRequired(suffix.into()))?;
                return Ok(if unignore {
                    SessionCommand::Unignore(number)
                } else {
                    SessionCommand::Ignore(Some(number))
                });
            }
        }
    }
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
        "next" => {
            reject_arguments(&name, &arguments)?;
            no_address(address, name, SessionCommand::NextIssue)
        }
        "prev" => {
            reject_arguments(&name, &arguments)?;
            no_address(address, name, SessionCommand::PreviousIssue)
        }
        "issues" => {
            reject_arguments(&name, &arguments)?;
            no_address(address, name, SessionCommand::Issues)
        }
        "ignore" | "resolve" => {
            reject_arguments(&name, &arguments)?;
            no_address(address, name, SessionCommand::Ignore(None))
        }
        "unignore" => Err(CommandParseError::AlternativeNumberRequired(name)),
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
        "print" | "show" | "p" | "list" | "l" => {
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
                Some(Address::Paragraph(paragraph)) => Ok(SessionCommand::Tokens(Some(paragraph))),
                Some(address) => Err(CommandParseError::InvalidAddress {
                    command: name,
                    address,
                    expected: "a paragraph address M",
                }),
                None => Ok(SessionCommand::Tokens(None)),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::navigation::{Address, SyntaxError, TokenAddress};
    use std::path::PathBuf;

    #[test]
    fn parser_accepts_addressed_commands_and_aliases() {
        assert_eq!(parse_command(" p ").unwrap(), SessionCommand::Print(None));
        assert_eq!(
            parse_command("2print").unwrap(),
            SessionCommand::Print(Some(2))
        );
        assert_eq!(
            parse_command(" 2@3play ").unwrap(),
            SessionCommand::Play {
                address: Some(Address::Marker {
                    paragraph: 2,
                    marker: 3
                }),
                speed: PlaybackSpeed::Normal,
            }
        );
        assert_eq!(
            parse_command("play").unwrap(),
            SessionCommand::Play {
                address: None,
                speed: PlaybackSpeed::Normal
            }
        );
        assert_eq!(
            parse_command("2slowplay").unwrap(),
            SessionCommand::Play {
                address: Some(Address::Paragraph(2)),
                speed: PlaybackSpeed::Slow
            }
        );
        assert_eq!(
            parse_command("replay").unwrap(),
            SessionCommand::Replay {
                speed: PlaybackSpeed::Normal
            }
        );
        assert_eq!(parse_command("stop").unwrap(), SessionCommand::Stop);
        assert_eq!(parse_command("undo").unwrap(), SessionCommand::Undo(1));
        assert_eq!(parse_command(" 12undo ").unwrap(), SessionCommand::Undo(12));
        assert_eq!(parse_command("redo").unwrap(), SessionCommand::Redo(1));
        assert_eq!(parse_command("3redo").unwrap(), SessionCommand::Redo(3));
        assert_eq!(
            parse_command("1.2split").unwrap(),
            SessionCommand::SplitChunk {
                address: Some(TokenAddress {
                    paragraph: 1,
                    token: 2,
                }),
                after: false,
            }
        );
        assert_eq!(
            parse_command("asplit").unwrap(),
            SessionCommand::SplitChunk {
                address: None,
                after: true,
            }
        );
        assert_eq!(
            parse_command("1@2parasplit").unwrap(),
            SessionCommand::SplitParagraph {
                marker: Some((1, 2)),
            }
        );
        assert_eq!(
            parse_command("1merge").unwrap(),
            SessionCommand::MergeParagraph(1)
        );
        assert_eq!(
            parse_command("1@2merge").unwrap(),
            SessionCommand::MergeChunks {
                paragraph: 1,
                marker: 2,
            }
        );
        assert_eq!(
            parse_command("1.2insert  typed text  ").unwrap(),
            SessionCommand::Insert {
                address: TokenAddress {
                    paragraph: 1,
                    token: 2,
                },
                text: " typed text  ".into(),
            }
        );
        assert_eq!(
            parse_command("1.2 append text").unwrap(),
            SessionCommand::Append {
                address: TokenAddress {
                    paragraph: 1,
                    token: 2,
                },
                text: "text".into(),
            }
        );
        assert_eq!(
            parse_command("1.2,1.4replace new text").unwrap(),
            SessionCommand::Replace {
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
            SessionCommand::Delete {
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
            SessionCommand::Select(Address::MarkerRange {
                start_paragraph: 1,
                start_marker: 1,
                end_paragraph: 1,
                end_marker_exclusive: 2,
            })
        );
        assert_eq!(
            parse_command("2@3 i").unwrap(),
            SessionCommand::Info {
                paragraph: 2,
                chunk: 3
            }
        );
        assert_eq!(parse_command("list").unwrap(), SessionCommand::Print(None));
        assert_eq!(parse_command("show").unwrap(), SessionCommand::Print(None));
        assert_eq!(parse_command("print").unwrap(), SessionCommand::Print(None));
        assert_eq!(
            parse_command("2show").unwrap(),
            SessionCommand::Print(Some(2))
        );
        assert_eq!(
            parse_command("resolve").unwrap(),
            SessionCommand::Ignore(None)
        );
        assert_eq!(
            parse_command("3resolve").unwrap(),
            SessionCommand::Ignore(Some(3))
        );
        assert_eq!(parse_command("h").unwrap(), SessionCommand::Help);
        assert_eq!(
            parse_command("save document.rde.json").unwrap(),
            SessionCommand::Save(Some(PathBuf::from("document.rde.json")))
        );
        assert_eq!(parse_command("save").unwrap(), SessionCommand::Save(None));
        assert_eq!(
            parse_command("load document.rde.json").unwrap(),
            SessionCommand::Load(PathBuf::from("document.rde.json"))
        );
        assert_eq!(
            parse_command("edit other document.json").unwrap(),
            SessionCommand::Load(PathBuf::from("other document.json"))
        );
        assert_eq!(parse_command(" q ").unwrap(), SessionCommand::Quit);

        assert_eq!(parse_command("  ").unwrap(), SessionCommand::Empty);
    }

    #[test]
    fn parser_reports_command_specific_address_errors() {
        assert_eq!(
            parse_command("1play").unwrap(),
            SessionCommand::Play {
                address: Some(Address::Paragraph(1)),
                speed: PlaybackSpeed::Normal
            }
        );
        assert_eq!(
            parse_command("7").unwrap(),
            SessionCommand::Move(Address::Paragraph(7))
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
            SessionCommand::Select(Address::TokenRange {
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
            SessionCommand::Tokens(Some(2))
        );
        assert_eq!(
            parse_command("tokens").unwrap(),
            SessionCommand::Tokens(None)
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
            SessionCommand::Replace {
                range: None,
                replacement: ReplacementText {
                    text: "text".into(),
                    exact_boundaries: false,
                },
            }
        );
        assert_eq!(
            parse_command("delete").unwrap(),
            SessionCommand::Delete { range: None }
        );
        assert!(matches!(
            parse_command("1.2replace text"),
            Err(CommandParseError::InvalidAddress { .. })
        ));
        assert!(matches!(
            parse_command("1.2delete"),
            Err(CommandParseError::InvalidAddress { .. })
        ));
        assert_eq!(
            parse_command("0undo"),
            Err(CommandParseError::HistoryCountRequired("undo".into()))
        );
    }

    #[test]
    fn quoted_replacement_controls_boundaries_and_escapes_quotes_and_backslashes() {
        assert_eq!(
            parse_command(r#"replace " exact \"text\"\\ ""#).unwrap(),
            SessionCommand::Replace {
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
}
