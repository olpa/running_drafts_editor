//! Address-first command syntax for the line-oriented editor.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenAddress {
    pub paragraph: usize,
    pub token: usize,
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
    #[error("token range '{0}' must stay within one paragraph")]
    CrossParagraphRange(String),
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
        if start.paragraph != end.paragraph {
            return Err(SyntaxError::CrossParagraphRange(input.into()));
        }
        if start.token > end.token {
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn rejects_invalid_addresses_and_ranges() {
        assert_eq!(
            parse_address("0.1").unwrap_err(),
            SyntaxError::ZeroAddress("0.1".into())
        );
        assert_eq!(
            parse_address("1.2,2.3").unwrap_err(),
            SyntaxError::CrossParagraphRange("1.2,2.3".into())
        );
        assert_eq!(
            parse_address("1.3,1.2").unwrap_err(),
            SyntaxError::ReversedRange("1.3,1.2".into())
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
