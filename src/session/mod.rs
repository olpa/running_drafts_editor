//! Shared line-oriented session commands, rendering, and audio playback.

mod command;
mod editing;
mod playback;
mod render;
mod shell;

pub use playback::{AudioPlayer, Ffplay, PlaybackError, PlaybackSpeed};
pub use render::render_recognition_chunks;
pub use shell::{run_session, SessionContext};

pub(crate) use render::render_paragraph;

#[cfg(test)]
pub(crate) use command::{parse_command, CommandParseError, ReplacementText, SessionCommand};

#[cfg(test)]
pub(crate) use playback::samples_as_seconds;
#[cfg(test)]
pub(crate) use shell::render_help;

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
        assert_eq!(parse_command("2tokens").unwrap(), SessionCommand::Tokens(2));
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

    #[test]
    fn help_explains_each_session_command_with_examples() {
        let mut output = Vec::new();

        render_help(&mut output).unwrap();

        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("p | print"));
        assert!(output.contains("Mp"));
        assert!(output.contains("M.N"));
        assert!(output.contains("Aselect | Asel | As"));
        assert!(output.contains("Mtokens"));
        assert!(output.contains("[M.N,M.U]replace TEXT"));
        assert!(output.contains("unquoted keeps selected boundary whitespace"));
        assert!(output.contains("quoted \"TEXT\" controls boundaries exactly"));
        assert!(output.contains("split | [M.N]isplit"));
        assert!(output.contains("parasplit"));
        assert!(output.contains("M@Nmerge"));
        assert!(output.contains("[A]play | [A]slowplay"));
        assert!(output.contains("M@N,M@Uplay"));
        assert!(output.contains("[A]slowplay"));
        assert!(output.contains("replay"));
        assert!(output.contains("stop"));
        assert!(output.contains("M@Ninfo"));
        assert!(output.contains("save [PATH]"));
        assert!(output.contains("load PATH"));
        assert!(output.contains("edit PATH"));
        assert!(output.contains("h | help"));
        assert!(output.contains("q | quit"));
    }

    #[test]
    fn ffplay_seconds_preserve_sample_precision() {
        assert_eq!(samples_as_seconds(1, 16_000), "0.000062500");
        assert_eq!(samples_as_seconds(480_001, 16_000), "30.000062500");
    }
}
