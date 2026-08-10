//! Address-first command syntax for the line-oriented editor.

use std::fmt;

use crate::document::{Document, VisibleTokenId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenAddress {
    pub paragraph: usize,
    pub token: usize,
}

impl fmt::Display for TokenAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.paragraph, self.token)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Address {
    Current,
    Paragraph(usize),
    Token(TokenAddress),
    TokenRange {
        start: TokenAddress,
        end: TokenAddress,
    },
    Marker {
        paragraph: usize,
        marker: usize,
    },
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Current => f.write_str("."),
            Self::Paragraph(p) => write!(f, "{p}"),
            Self::Token(a) => write!(f, "{}.{}", a.paragraph, a.token),
            Self::TokenRange { start, end } => write!(
                f,
                "{}.{},{}.{}",
                start.paragraph, start.token, end.paragraph, end.token
            ),
            Self::Marker { paragraph, marker } => write!(f, "{paragraph}@{marker}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandLine {
    Empty,
    Address(Address),
    Command {
        address: Option<Address>,
        name: String,
        arguments: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SyntaxError {
    #[error("invalid address '{0}'; expected M, M.N, M.N,M.U, M@N, or .")]
    InvalidAddress(String),
    #[error("address numbers must be positive in '{0}'")]
    ZeroAddress(String),
    #[error("token range '{0}' ends before it starts")]
    ReversedRange(String),
    #[error("invalid command syntax '{0}'")]
    InvalidCommand(String),
}

pub fn parse_line(input: &str) -> Result<CommandLine, SyntaxError> {
    let input = input.trim_end_matches(['\r', '\n']).trim_start();
    if input.trim().is_empty() {
        return Ok(CommandLine::Empty);
    }
    let (first, tail) = split_head(input);
    let parsed_address = parse_address(first);

    if let Ok(address) = parsed_address {
        if tail.is_empty() {
            return Ok(CommandLine::Address(address));
        }
        let (name, arguments) = split_head(tail);
        return Ok(CommandLine::Command {
            address: Some(address),
            name: parse_command_name(name)?,
            arguments: arguments.into(),
        });
    }

    let split = first.find(char::is_alphabetic).unwrap_or(0);
    if split > 0 {
        let (address, name) = first.split_at(split);
        return Ok(CommandLine::Command {
            address: Some(parse_address(address)?),
            name: parse_command_name(name)?,
            arguments: tail.into(),
        });
    }

    if first
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_digit() || matches!(character, '.' | '@' | ','))
    {
        return Err(parsed_address.expect_err("address-shaped input did not parse"));
    }

    Ok(CommandLine::Command {
        address: None,
        name: parse_command_name(first)?,
        arguments: tail.into(),
    })
}

fn split_head(input: &str) -> (&str, &str) {
    input
        .find(char::is_whitespace)
        .map_or((input, ""), |split| {
            (&input[..split], input[split..].trim_start())
        })
}

pub fn parse_address(input: &str) -> Result<Address, SyntaxError> {
    if input == "." {
        return Ok(Address::Current);
    }
    if let Some((start, end)) = split_once(input, ',')? {
        let start = parse_token(start, input)?;
        let end = parse_token(end, input)?;
        if (start.paragraph, start.token) > (end.paragraph, end.token) {
            return Err(SyntaxError::ReversedRange(input.into()));
        }
        return Ok(Address::TokenRange { start, end });
    }
    if let Some((paragraph, marker)) = split_once(input, '@')? {
        return Ok(Address::Marker {
            paragraph: parse_number(paragraph, input)?,
            marker: parse_number(marker, input)?,
        });
    }
    if input.contains('.') {
        return parse_token(input, input).map(Address::Token);
    }
    parse_number(input, input).map(Address::Paragraph)
}

fn parse_token(input: &str, whole: &str) -> Result<TokenAddress, SyntaxError> {
    let Some((paragraph, token)) = split_once(input, '.')? else {
        return Err(SyntaxError::InvalidAddress(whole.into()));
    };
    Ok(TokenAddress {
        paragraph: parse_number(paragraph, whole)?,
        token: parse_number(token, whole)?,
    })
}

fn split_once(input: &str, separator: char) -> Result<Option<(&str, &str)>, SyntaxError> {
    let Some((left, right)) = input.split_once(separator) else {
        return Ok(None);
    };
    if left.is_empty() || right.is_empty() || right.contains(separator) {
        return Err(SyntaxError::InvalidAddress(input.into()));
    }
    Ok(Some((left, right)))
}

fn parse_number(input: &str, whole: &str) -> Result<usize, SyntaxError> {
    input
        .parse::<usize>()
        .map_err(|_| SyntaxError::InvalidAddress(whole.into()))
        .and_then(|number| {
            (number > 0)
                .then_some(number)
                .ok_or_else(|| SyntaxError::ZeroAddress(whole.into()))
        })
}

fn parse_command_name(input: &str) -> Result<String, SyntaxError> {
    if input.is_empty()
        || !input
            .chars()
            .all(|character| character.is_ascii_alphabetic())
    {
        Err(SyntaxError::InvalidCommand(input.into()))
    } else {
        Ok(input.to_ascii_lowercase())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StableTokenPosition {
    pub paragraph_id: String,
    pub paragraph_revision: u64,
    pub token_id: VisibleTokenId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StableMarkerPosition {
    pub paragraph_id: String,
    pub paragraph_revision: u64,
    pub chunk_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StableParagraphRevision {
    pub paragraph_id: String,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Caret {
    Token(StableTokenPosition),
    Marker(StableMarkerPosition),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    Tokens {
        start: StableTokenPosition,
        end_inclusive: StableTokenPosition,
        paragraph_revisions: Vec<StableParagraphRevision>,
    },
    Paragraph {
        paragraph_id: String,
        paragraph_revision: u64,
    },
    Marker(StableMarkerPosition),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NavigationState {
    caret: Option<Caret>,
    selection: Option<Selection>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NavigationError {
    #[error("unknown paragraph {0}")]
    UnknownParagraph(usize),
    #[error("unknown token {paragraph}.{token}")]
    UnknownToken { paragraph: usize, token: usize },
    #[error("unknown chunk marker {paragraph}@{marker}")]
    UnknownMarker { paragraph: usize, marker: usize },
    #[error("address '{0}' cannot hold a caret; expected M.N or M@N")]
    InvalidCaretAddress(Address),
    #[error("token range '{start},{end}' ends before it starts")]
    ReversedTokenRange {
        start: TokenAddress,
        end: TokenAddress,
    },
    #[error("there is no current caret or selection")]
    NoCurrentPosition,
}

impl NavigationState {
    pub fn new(document: &Document) -> Self {
        let mut state = Self::default();
        for (paragraph_index, paragraph) in document.paragraphs().iter().enumerate() {
            if !paragraph.tokens().is_empty() {
                state.caret = resolve_token(document, paragraph_index + 1, 1)
                    .ok()
                    .map(Caret::Token);
                break;
            }
            if let Some(marker) = paragraph.chunk_boundaries().first() {
                state.caret = Some(Caret::Marker(StableMarkerPosition {
                    paragraph_id: paragraph.id().into(),
                    paragraph_revision: paragraph.revision(),
                    chunk_id: marker.chunk_id().into(),
                }));
                break;
            }
        }
        state
    }

    pub fn caret(&self) -> Option<&Caret> {
        self.caret.as_ref()
    }

    pub fn selection(&self) -> Option<&Selection> {
        self.selection.as_ref()
    }

    pub fn move_to(
        &mut self,
        document: &Document,
        address: &Address,
    ) -> Result<(), NavigationError> {
        let caret = match address {
            Address::Token(address) => {
                Caret::Token(resolve_token(document, address.paragraph, address.token)?)
            }
            Address::Marker { paragraph, marker } => {
                Caret::Marker(resolve_marker(document, *paragraph, *marker)?)
            }
            Address::Current => {
                return self
                    .caret
                    .as_ref()
                    .map(|_| ())
                    .ok_or(NavigationError::NoCurrentPosition)
            }
            address => return Err(NavigationError::InvalidCaretAddress(address.clone())),
        };
        self.caret = Some(caret);
        self.selection = None;
        Ok(())
    }

    pub fn select(
        &mut self,
        document: &Document,
        address: &Address,
    ) -> Result<(), NavigationError> {
        let selection = match address {
            Address::Token(address) => {
                let token = resolve_token(document, address.paragraph, address.token)?;
                Selection::Tokens {
                    start: token.clone(),
                    end_inclusive: token,
                    paragraph_revisions: stable_paragraph_revisions(
                        document,
                        address.paragraph,
                        address.paragraph,
                    )?,
                }
            }
            Address::TokenRange { start, end }
                if (start.paragraph, start.token) > (end.paragraph, end.token) =>
            {
                return Err(NavigationError::ReversedTokenRange {
                    start: *start,
                    end: *end,
                });
            }
            Address::TokenRange { start, end } => Selection::Tokens {
                start: resolve_token(document, start.paragraph, start.token)?,
                end_inclusive: resolve_token(document, end.paragraph, end.token)?,
                paragraph_revisions: stable_paragraph_revisions(
                    document,
                    start.paragraph,
                    end.paragraph,
                )?,
            },
            Address::Paragraph(paragraph) => {
                let value = document
                    .paragraph(*paragraph)
                    .ok_or(NavigationError::UnknownParagraph(*paragraph))?;
                Selection::Paragraph {
                    paragraph_id: value.id().into(),
                    paragraph_revision: value.revision(),
                }
            }
            Address::Marker { paragraph, marker } => {
                Selection::Marker(resolve_marker(document, *paragraph, *marker)?)
            }
            Address::Current => return self.select_current(),
        };
        self.selection = Some(selection);
        Ok(())
    }

    fn select_current(&mut self) -> Result<(), NavigationError> {
        if self.selection.is_some() {
            return Ok(());
        }
        let selection = match self.caret.clone() {
            Some(Caret::Token(token)) => Selection::Tokens {
                paragraph_revisions: vec![StableParagraphRevision {
                    paragraph_id: token.paragraph_id.clone(),
                    revision: token.paragraph_revision,
                }],
                start: token.clone(),
                end_inclusive: token,
            },
            Some(Caret::Marker(marker)) => Selection::Marker(marker),
            None => return Err(NavigationError::NoCurrentPosition),
        };
        self.selection = Some(selection);
        Ok(())
    }
}

fn stable_paragraph_revisions(
    document: &Document,
    start: usize,
    end: usize,
) -> Result<Vec<StableParagraphRevision>, NavigationError> {
    (start..=end)
        .map(|paragraph| {
            let value = document
                .paragraph(paragraph)
                .ok_or(NavigationError::UnknownParagraph(paragraph))?;
            Ok(StableParagraphRevision {
                paragraph_id: value.id().into(),
                revision: value.revision(),
            })
        })
        .collect()
}

fn resolve_token(
    document: &Document,
    paragraph: usize,
    token: usize,
) -> Result<StableTokenPosition, NavigationError> {
    let value = document
        .paragraph(paragraph)
        .ok_or(NavigationError::UnknownParagraph(paragraph))?;
    let visible_token = value
        .tokens()
        .get(token.checked_sub(1).unwrap_or(usize::MAX))
        .ok_or(NavigationError::UnknownToken { paragraph, token })?;
    Ok(StableTokenPosition {
        paragraph_id: value.id().into(),
        paragraph_revision: value.revision(),
        token_id: visible_token.id().clone(),
    })
}

fn resolve_marker(
    document: &Document,
    paragraph: usize,
    marker: usize,
) -> Result<StableMarkerPosition, NavigationError> {
    let value = document
        .paragraph(paragraph)
        .ok_or(NavigationError::UnknownParagraph(paragraph))?;
    let chunk_marker = value
        .chunk_boundaries()
        .get(marker.checked_sub(1).unwrap_or(usize::MAX))
        .ok_or(NavigationError::UnknownMarker { paragraph, marker })?;
    Ok(StableMarkerPosition {
        paragraph_id: value.id().into(),
        paragraph_revision: value.revision(),
        chunk_id: chunk_marker.chunk_id().into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        chunking::SampleRange,
        document::Document,
        recognition::{
            ChunkBoundary, ChunkBoundaryReason, DecodedSegment, RecognitionChunk, RecognitionToken,
        },
    };

    #[test]
    fn parses_each_address_kind() {
        assert_eq!(parse_address("2").unwrap(), Address::Paragraph(2));
        assert_eq!(
            parse_address("2.4").unwrap(),
            Address::Token(TokenAddress {
                paragraph: 2,
                token: 4
            })
        );
        assert_eq!(
            parse_address("2@3").unwrap(),
            Address::Marker {
                paragraph: 2,
                marker: 3
            }
        );
        assert_eq!(
            parse_address("2.4,2.9").unwrap(),
            Address::TokenRange {
                start: TokenAddress {
                    paragraph: 2,
                    token: 4
                },
                end: TokenAddress {
                    paragraph: 2,
                    token: 9
                },
            }
        );
        assert_eq!(parse_address(".").unwrap(), Address::Current);
    }

    #[test]
    fn parses_attached_and_separated_commands() {
        let expected = CommandLine::Command {
            address: Some(Address::Marker {
                paragraph: 2,
                marker: 3,
            }),
            name: "info".into(),
            arguments: String::new(),
        };
        assert_eq!(parse_line(" 2@3info ").unwrap(), expected);
        assert_eq!(parse_line("2@3 info").unwrap(), expected);
        assert_eq!(
            parse_line("2.4,2.9 select").unwrap(),
            CommandLine::Command {
                address: Some(Address::TokenRange {
                    start: TokenAddress {
                        paragraph: 2,
                        token: 4
                    },
                    end: TokenAddress {
                        paragraph: 2,
                        token: 9
                    },
                }),
                name: "select".into(),
                arguments: String::new(),
            }
        );
    }

    #[test]
    fn retains_arguments_for_command_specific_parsing() {
        assert_eq!(
            parse_line("2.4replace some  new text  \n").unwrap(),
            CommandLine::Command {
                address: Some(Address::Token(TokenAddress {
                    paragraph: 2,
                    token: 4
                })),
                name: "replace".into(),
                arguments: "some  new text  ".into(),
            }
        );
    }

    #[test]
    fn selection_uses_stable_ids_across_paragraphs() {
        let recognition_token = |text: &str| RecognitionToken {
            token_id: 1,
            text: text.into(),
            probability: 1.0,
            is_special: false,
            audio_range: None,
            alternatives: Vec::new(),
        };
        let segments = vec![
            DecodedSegment {
                id: "s1".into(),
                audio_range: SampleRange {
                    start_sample: 0,
                    end_sample: 1,
                },
                text: "one".into(),
                no_speech_probability: 0.0,
                tokens: vec![recognition_token("one")],
            },
            DecodedSegment {
                id: "s2".into(),
                audio_range: SampleRange {
                    start_sample: 1,
                    end_sample: 2,
                },
                text: " two".into(),
                no_speech_probability: 0.0,
                tokens: vec![recognition_token(" two")],
            },
        ];
        let chunk = |id: &str, segment: &str, text: &str, reason| RecognitionChunk {
            id: id.into(),
            ordinal: 1,
            segment_ids: vec![segment.into()],
            audio_range: SampleRange {
                start_sample: 0,
                end_sample: 1,
            },
            text: text.into(),
            token_count: 1,
            boundary: ChunkBoundary {
                reason,
                pause_samples: None,
            },
        };
        let chunks = vec![
            chunk("c1", "s1", "one", ChunkBoundaryReason::LongPause),
            chunk("c2", "s2", " two", ChunkBoundaryReason::SourceEnd),
        ];
        let document = Document::from_evidence("run", &segments, &chunks);
        let mut state = NavigationState::new(&document);
        let range = Address::TokenRange {
            start: TokenAddress {
                paragraph: 1,
                token: 1,
            },
            end: TokenAddress {
                paragraph: 2,
                token: 1,
            },
        };

        state.select(&document, &range).unwrap();

        let Selection::Tokens {
            start,
            end_inclusive,
            paragraph_revisions,
        } = state.selection().unwrap()
        else {
            panic!("expected token selection");
        };
        assert_ne!(start.paragraph_id, end_inclusive.paragraph_id);
        assert_eq!(paragraph_revisions.len(), 2);
        assert_eq!(start.paragraph_revision, 1);
        assert!(matches!(start.token_id, VisibleTokenId::Recognition { .. }));
    }

    #[test]
    fn rejects_invalid_addresses_and_ranges() {
        assert_eq!(
            parse_address("0.1").unwrap_err(),
            SyntaxError::ZeroAddress("0.1".into())
        );
        assert_eq!(
            parse_address("1.2,2.3").unwrap(),
            Address::TokenRange {
                start: TokenAddress {
                    paragraph: 1,
                    token: 2
                },
                end: TokenAddress {
                    paragraph: 2,
                    token: 3
                },
            }
        );
        assert_eq!(
            parse_address("2.1,1.3").unwrap_err(),
            SyntaxError::ReversedRange("2.1,1.3".into())
        );
        assert!(matches!(
            parse_address("1@2@3"),
            Err(SyntaxError::InvalidAddress(_))
        ));
        assert_eq!(
            parse_line("0").unwrap_err(),
            SyntaxError::ZeroAddress("0".into())
        );
        assert!(matches!(
            parse_line("1@2@3 info"),
            Err(SyntaxError::InvalidAddress(_))
        ));
    }
}
