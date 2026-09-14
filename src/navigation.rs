//! Hierarchical, address-first navigation for the line-oriented editor.

use std::{cmp::Ordering, fmt};

use crate::document::{Document, VisibleTokenId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkAddress {
    pub paragraph: usize,
    pub chunk: usize,
}

impl fmt::Display for ChunkAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.paragraph, self.chunk)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TokenAddress {
    pub paragraph: usize,
    pub chunk: usize,
    pub token: usize,
}

impl fmt::Display for TokenAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.paragraph, self.chunk, self.token)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PositionAddress {
    Paragraph(usize),
    Chunk(ChunkAddress),
    Token(TokenAddress),
}

impl fmt::Display for PositionAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Paragraph(paragraph) => write!(f, "{paragraph}"),
            Self::Chunk(address) => write!(f, "{address}"),
            Self::Token(address) => write!(f, "{address}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Address {
    Current,
    Position(PositionAddress),
    Range {
        start: PositionAddress,
        end: PositionAddress,
    },
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Current => f.write_str("."),
            Self::Position(position) => write!(f, "{position}"),
            Self::Range { start, end } => write!(f, "{start},{end}"),
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
    #[error("invalid address '{0}'; expected N, N.M, N.M.K, A,B, or .")]
    InvalidAddress(String),
    #[error("address numbers must be positive in '{0}'")]
    ZeroAddress(String),
    #[error("invalid command syntax '{0}'")]
    InvalidCommand(String),
}

pub fn parse_line(input: &str) -> Result<CommandLine, SyntaxError> {
    let input = input.trim_end_matches(['\r', '\n']).trim_start();
    if input.trim().is_empty() {
        return Ok(CommandLine::Empty);
    }
    let (first, raw_tail) = split_head(input);
    let parsed_address = parse_address(first);
    if let Ok(address) = parsed_address {
        let tail = raw_tail.trim_start();
        if tail.is_empty() {
            return Ok(CommandLine::Address(address));
        }
        let (name, arguments) = split_head(tail);
        let name = parse_command_name(name)?;
        return Ok(CommandLine::Command {
            address: Some(address),
            arguments: command_arguments(&name, arguments),
            name,
        });
    }
    let split = first.find(char::is_alphabetic).unwrap_or(0);
    if split > 0 {
        let (address, name) = first.split_at(split);
        let name = parse_command_name(name)?;
        return Ok(CommandLine::Command {
            address: Some(parse_address(address)?),
            arguments: command_arguments(&name, raw_tail),
            name,
        });
    }
    if first
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_digit() || matches!(c, '.' | '@' | ','))
    {
        return Err(parsed_address.expect_err("address-shaped input did not parse"));
    }
    let name = parse_command_name(first)?;
    Ok(CommandLine::Command {
        address: None,
        arguments: command_arguments(&name, raw_tail),
        name,
    })
}

fn split_head(input: &str) -> (&str, &str) {
    input
        .find(char::is_whitespace)
        .map_or((input, ""), |split| {
            let len = input[split..].chars().next().unwrap().len_utf8();
            (&input[..split], &input[split + len..])
        })
}

fn command_arguments(name: &str, raw: &str) -> String {
    if matches!(name, "insert" | "append" | "replace") {
        raw.into()
    } else {
        raw.trim_start().into()
    }
}

pub fn parse_address(input: &str) -> Result<Address, SyntaxError> {
    if input == "." {
        return Ok(Address::Current);
    }
    if input.contains('@') {
        return Err(SyntaxError::InvalidAddress(input.into()));
    }
    if let Some((start, end)) = split_once(input, ',')? {
        return Ok(Address::Range {
            start: parse_position(start, input)?,
            end: parse_position(end, input)?,
        });
    }
    parse_position(input, input).map(Address::Position)
}

fn parse_position(input: &str, whole: &str) -> Result<PositionAddress, SyntaxError> {
    let parts = input.split('.').collect::<Vec<_>>();
    if parts.is_empty() || parts.len() > 3 || parts.iter().any(|part| part.is_empty()) {
        return Err(SyntaxError::InvalidAddress(whole.into()));
    }
    let values = parts
        .iter()
        .map(|part| parse_number(part, whole))
        .collect::<Result<Vec<_>, _>>()?;
    match values.as_slice() {
        [p] => Ok(PositionAddress::Paragraph(*p)),
        [p, c] => Ok(PositionAddress::Chunk(ChunkAddress {
            paragraph: *p,
            chunk: *c,
        })),
        [p, c, t] => Ok(PositionAddress::Token(TokenAddress {
            paragraph: *p,
            chunk: *c,
            token: *t,
        })),
        _ => Err(SyntaxError::InvalidAddress(whole.into())),
    }
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
        .and_then(|n| {
            (n > 0)
                .then_some(n)
                .ok_or_else(|| SyntaxError::ZeroAddress(whole.into()))
        })
}

fn parse_command_name(input: &str) -> Result<String, SyntaxError> {
    if input.is_empty() || !input.chars().all(|c| c.is_ascii_alphabetic()) {
        Err(SyntaxError::InvalidCommand(input.into()))
    } else {
        Ok(input.to_ascii_lowercase())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StablePosition {
    address: PositionAddress,
    paragraph_id: Option<String>,
    paragraph_revision: Option<u64>,
    chunk_id: Option<String>,
    token_id: Option<VisibleTokenId>,
    document_end: Option<Vec<(String, u64)>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    start: StablePosition,
    end: StablePosition,
    document_revisions: Vec<(String, u64)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NavigationState {
    selection: Option<Selection>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NavigationError {
    #[error("unknown paragraph position {0}")]
    UnknownParagraph(usize),
    #[error("unknown chunk position {paragraph}.{chunk}")]
    UnknownChunk { paragraph: usize, chunk: usize },
    #[error("unknown token position {paragraph}.{chunk}.{token}")]
    UnknownToken {
        paragraph: usize,
        chunk: usize,
        token: usize,
    },
    #[error("chunk {paragraph}.{chunk} has no token positions")]
    ChunkHasNoTokens { paragraph: usize, chunk: usize },
    #[error("address '{0}' cannot name one position")]
    InvalidPositionAddress(Address),
    #[error("range '{start},{end}' ends before it starts")]
    ReversedRange {
        start: PositionAddress,
        end: PositionAddress,
    },
    #[error("there is no current position or selection")]
    NoCurrentPosition,
    #[error("there is no current token selection")]
    NoTokenSelection,
    #[error("the current selection is stale")]
    StaleSelection,
    #[error("text-edit ranges cannot cross paragraph boundaries")]
    CrossParagraphSelection,
    #[error("text-edit ranges cannot cross chunk boundaries")]
    CrossChunkSelection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Ordinal {
    chunk: usize,
    token: usize,
}
impl Ord for Ordinal {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.chunk, self.token).cmp(&(other.chunk, other.token))
    }
}
impl PartialOrd for Ordinal {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl NavigationState {
    pub fn new(document: &Document) -> Self {
        let first = document
            .paragraphs()
            .iter()
            .enumerate()
            .find_map(|(pi, paragraph)| {
                paragraph.chunk_boundaries().first().map(|marker| {
                    if marker.after_tokens() > 0 {
                        PositionAddress::Token(TokenAddress {
                            paragraph: pi + 1,
                            chunk: 1,
                            token: 1,
                        })
                    } else {
                        PositionAddress::Chunk(ChunkAddress {
                            paragraph: pi + 1,
                            chunk: 1,
                        })
                    }
                })
            });
        let selection = first
            .and_then(|position| stable_position(document, position).ok())
            .map(|p| Selection {
                start: p.clone(),
                end: p,
                document_revisions: document_revisions(document),
            });
        Self { selection }
    }

    pub fn selection(&self) -> Option<&Selection> {
        self.selection.as_ref()
    }

    pub fn move_to(
        &mut self,
        document: &Document,
        address: &Address,
    ) -> Result<(), NavigationError> {
        let point = match address {
            Address::Current => return self.current_range(document).map(|_| ()),
            Address::Position(point) => *point,
            Address::Range { .. } => {
                return Err(NavigationError::InvalidPositionAddress(address.clone()))
            }
        };
        let stable = stable_position(document, point)?;
        self.selection = Some(Selection {
            start: stable.clone(),
            end: stable,
            document_revisions: document_revisions(document),
        });
        Ok(())
    }

    pub fn select(
        &mut self,
        document: &Document,
        address: &Address,
    ) -> Result<(), NavigationError> {
        match address {
            Address::Current => self.current_range(document).map(|_| ()),
            Address::Position(_) => self.move_to(document, address),
            Address::Range { start, end } => {
                if resolve_position(document, *start)? > resolve_position(document, *end)? {
                    return Err(NavigationError::ReversedRange {
                        start: *start,
                        end: *end,
                    });
                }
                self.selection = Some(Selection {
                    start: stable_position(document, *start)?,
                    end: stable_position(document, *end)?,
                    document_revisions: document_revisions(document),
                });
                Ok(())
            }
        }
    }

    pub fn current_range(
        &self,
        document: &Document,
    ) -> Result<(PositionAddress, PositionAddress), NavigationError> {
        let s = self
            .selection
            .as_ref()
            .ok_or(NavigationError::NoCurrentPosition)?;
        if s.document_revisions != document_revisions(document) {
            return Err(NavigationError::StaleSelection);
        }
        Ok((
            resolve_stable(document, &s.start)?,
            resolve_stable(document, &s.end)?,
        ))
    }

    pub fn selection_is_empty(&self, document: &Document) -> Result<bool, NavigationError> {
        let (start, end) = self.current_range(document)?;
        Ok(resolve_position(document, start)? == resolve_position(document, end)?)
    }

    pub fn selected_token_endpoints(
        &self,
        document: &Document,
    ) -> Result<(TokenAddress, TokenAddress), NavigationError> {
        let (start, end) = self.current_range(document)?;
        let tokens = tokens_in_range(document, start, end)?;
        match (tokens.first(), tokens.last()) {
            (Some(a), Some(b)) => Ok((*a, *b)),
            _ => Err(NavigationError::NoTokenSelection),
        }
    }

    pub fn selected_token_range(
        &self,
        document: &Document,
    ) -> Result<(TokenAddress, TokenAddress), NavigationError> {
        let (start, end) = self.selected_token_endpoints(document)?;
        if start.paragraph != end.paragraph {
            return Err(NavigationError::CrossParagraphSelection);
        }
        if start.chunk != end.chunk {
            return Err(NavigationError::CrossChunkSelection);
        }
        Ok((start, end))
    }

    pub fn current_token_address(
        &self,
        document: &Document,
    ) -> Result<TokenAddress, NavigationError> {
        let (start, end) = self.current_range(document)?;
        if resolve_position(document, start)? == resolve_position(document, end)? {
            if let PositionAddress::Token(address) = start {
                return item_token(document, address);
            }
            return Err(NavigationError::NoTokenSelection);
        }
        let tokens = tokens_in_range(document, start, end)?;
        (tokens.len() == 1)
            .then_some(tokens[0])
            .ok_or(NavigationError::NoTokenSelection)
    }

    pub fn current_chunk_address(
        &self,
        document: &Document,
    ) -> Result<ChunkAddress, NavigationError> {
        let (start, end) = self.current_range(document)?;
        if resolve_position(document, start)? == resolve_position(document, end)? {
            return match start {
                PositionAddress::Chunk(address) => item_chunk(document, address),
                PositionAddress::Token(address) => {
                    resolve_position(document, start)?;
                    Ok(ChunkAddress {
                        paragraph: address.paragraph,
                        chunk: address.chunk,
                    })
                }
                PositionAddress::Paragraph(_) => Err(NavigationError::NoCurrentPosition),
            };
        }
        let chunks = chunks_in_range(document, start, end)?;
        if chunks.len() == 1 {
            return Ok(chunks[0]);
        }
        if chunks.is_empty() {
            let tokens = tokens_in_range(document, start, end)?;
            if let Some(first) = tokens.first() {
                if tokens
                    .iter()
                    .all(|token| token.paragraph == first.paragraph && token.chunk == first.chunk)
                {
                    return Ok(ChunkAddress {
                        paragraph: first.paragraph,
                        chunk: first.chunk,
                    });
                }
            }
        }
        Err(NavigationError::NoCurrentPosition)
    }
}

fn document_revisions(document: &Document) -> Vec<(String, u64)> {
    document
        .paragraphs()
        .iter()
        .map(|paragraph| (paragraph.id().to_owned(), paragraph.revision()))
        .collect()
}

pub fn item_paragraph(document: &Document, paragraph: usize) -> Result<usize, NavigationError> {
    document
        .paragraph(paragraph)
        .map(|_| paragraph)
        .ok_or(NavigationError::UnknownParagraph(paragraph))
}

pub fn item_chunk(
    document: &Document,
    address: ChunkAddress,
) -> Result<ChunkAddress, NavigationError> {
    document
        .chunk_token_count(address.paragraph, address.chunk)
        .map(|_| address)
        .ok_or(NavigationError::UnknownChunk {
            paragraph: address.paragraph,
            chunk: address.chunk,
        })
}

pub fn item_token(
    document: &Document,
    address: TokenAddress,
) -> Result<TokenAddress, NavigationError> {
    document
        .chunk_token(address.paragraph, address.chunk, address.token)
        .map(|_| address)
        .ok_or(NavigationError::UnknownToken {
            paragraph: address.paragraph,
            chunk: address.chunk,
            token: address.token,
        })
}

pub fn tokens_in_range(
    document: &Document,
    start: PositionAddress,
    end: PositionAddress,
) -> Result<Vec<TokenAddress>, NavigationError> {
    let left = resolve_position(document, start)?;
    let right = resolve_position(document, end)?;
    if left > right {
        return Err(NavigationError::ReversedRange { start, end });
    }
    let mut result = Vec::new();
    let mut global_chunk = 0;
    for (pi, paragraph) in document.paragraphs().iter().enumerate() {
        let mut previous = 0;
        for (ci, marker) in paragraph.chunk_boundaries().iter().enumerate() {
            let count = marker.after_tokens() - previous;
            for local in 0..count {
                let token_start = Ordinal {
                    chunk: global_chunk,
                    token: local,
                };
                let token_end = if local + 1 == count {
                    Ordinal {
                        chunk: global_chunk + 1,
                        token: 0,
                    }
                } else {
                    Ordinal {
                        chunk: global_chunk,
                        token: local + 1,
                    }
                };
                if token_start >= left && token_end <= right {
                    result.push(TokenAddress {
                        paragraph: pi + 1,
                        chunk: ci + 1,
                        token: local + 1,
                    });
                }
            }
            previous = marker.after_tokens();
            global_chunk += 1;
        }
    }
    Ok(result)
}

pub fn chunks_in_range(
    document: &Document,
    start: PositionAddress,
    end: PositionAddress,
) -> Result<Vec<ChunkAddress>, NavigationError> {
    let left = resolve_position(document, start)?;
    let right = resolve_position(document, end)?;
    if left > right {
        return Err(NavigationError::ReversedRange { start, end });
    }
    let mut result = Vec::new();
    let mut global = 0;
    for (pi, paragraph) in document.paragraphs().iter().enumerate() {
        for ci in 0..paragraph.chunk_boundaries().len() {
            if (Ordinal {
                chunk: global,
                token: 0,
            }) >= left
                && (Ordinal {
                    chunk: global + 1,
                    token: 0,
                }) <= right
            {
                result.push(ChunkAddress {
                    paragraph: pi + 1,
                    chunk: ci + 1,
                });
            }
            global += 1;
        }
    }
    Ok(result)
}

fn chunks_before(document: &Document, paragraph_index: usize) -> usize {
    document.paragraphs()[..paragraph_index]
        .iter()
        .map(|p| p.chunk_boundaries().len())
        .sum()
}

fn resolve_position(
    document: &Document,
    address: PositionAddress,
) -> Result<Ordinal, NavigationError> {
    match address {
        PositionAddress::Paragraph(paragraph) => {
            if paragraph == document.paragraphs().len() + 1 && !document.paragraphs().is_empty() {
                return Ok(Ordinal {
                    chunk: chunks_before(document, document.paragraphs().len()),
                    token: 0,
                });
            }
            item_paragraph(document, paragraph)?;
            Ok(Ordinal {
                chunk: chunks_before(document, paragraph - 1),
                token: 0,
            })
        }
        PositionAddress::Chunk(address) => {
            let p = document
                .paragraph(address.paragraph)
                .ok_or(NavigationError::UnknownParagraph(address.paragraph))?;
            if address.chunk == p.chunk_boundaries().len() + 1 && !p.chunk_boundaries().is_empty() {
                return Ok(Ordinal {
                    chunk: chunks_before(document, address.paragraph - 1)
                        + p.chunk_boundaries().len(),
                    token: 0,
                });
            }
            item_chunk(document, address)?;
            Ok(Ordinal {
                chunk: chunks_before(document, address.paragraph - 1) + address.chunk - 1,
                token: 0,
            })
        }
        PositionAddress::Token(address) => {
            let count = document
                .chunk_token_count(address.paragraph, address.chunk)
                .ok_or(NavigationError::UnknownChunk {
                    paragraph: address.paragraph,
                    chunk: address.chunk,
                })?;
            if count == 0 {
                return Err(NavigationError::ChunkHasNoTokens {
                    paragraph: address.paragraph,
                    chunk: address.chunk,
                });
            }
            if address.token == 0 || address.token > count + 1 {
                return Err(NavigationError::UnknownToken {
                    paragraph: address.paragraph,
                    chunk: address.chunk,
                    token: address.token,
                });
            }
            let global = chunks_before(document, address.paragraph - 1) + address.chunk - 1;
            Ok(if address.token == count + 1 {
                Ordinal {
                    chunk: global + 1,
                    token: 0,
                }
            } else {
                Ordinal {
                    chunk: global,
                    token: address.token - 1,
                }
            })
        }
    }
}

fn stable_position(
    document: &Document,
    address: PositionAddress,
) -> Result<StablePosition, NavigationError> {
    resolve_position(document, address)?;
    let (paragraph_number, chunk_number, token_number) = match address {
        PositionAddress::Paragraph(p) => (p, None, None),
        PositionAddress::Chunk(a) => (a.paragraph, Some(a.chunk), None),
        PositionAddress::Token(a) => (a.paragraph, Some(a.chunk), Some(a.token)),
    };
    if paragraph_number == document.paragraphs().len() + 1 {
        return Ok(StablePosition {
            address,
            paragraph_id: None,
            paragraph_revision: None,
            chunk_id: None,
            token_id: None,
            document_end: Some(
                document
                    .paragraphs()
                    .iter()
                    .map(|p| (p.id().into(), p.revision()))
                    .collect(),
            ),
        });
    }
    let paragraph = document.paragraph(paragraph_number).unwrap();
    let chunk_id = chunk_number
        .and_then(|c| paragraph.chunk_boundaries().get(c - 1))
        .map(|m| m.chunk_id().to_owned());
    let token_id = match (chunk_number, token_number) {
        (Some(c), Some(t)) => document
            .chunk_token(paragraph_number, c, t)
            .map(|v| v.id().clone()),
        _ => None,
    };
    Ok(StablePosition {
        address,
        paragraph_id: Some(paragraph.id().into()),
        paragraph_revision: Some(paragraph.revision()),
        chunk_id,
        token_id,
        document_end: None,
    })
}

fn resolve_stable(
    document: &Document,
    stable: &StablePosition,
) -> Result<PositionAddress, NavigationError> {
    if let Some(shape) = &stable.document_end {
        let current = document
            .paragraphs()
            .iter()
            .map(|p| (p.id().to_owned(), p.revision()))
            .collect::<Vec<_>>();
        return (shape == &current)
            .then_some(stable.address)
            .ok_or(NavigationError::StaleSelection);
    }
    let paragraph_number = document
        .paragraphs()
        .iter()
        .position(|p| {
            Some(p.id()) == stable.paragraph_id.as_deref()
                && Some(p.revision()) == stable.paragraph_revision
        })
        .ok_or(NavigationError::StaleSelection)?
        + 1;
    let current = match stable.address {
        PositionAddress::Paragraph(_) => PositionAddress::Paragraph(paragraph_number),
        PositionAddress::Chunk(original) => {
            let p = document.paragraph(paragraph_number).unwrap();
            let chunk = if let Some(id) = &stable.chunk_id {
                p.chunk_boundaries()
                    .iter()
                    .position(|m| m.chunk_id() == id)
                    .ok_or(NavigationError::StaleSelection)?
                    + 1
            } else if original.chunk == p.chunk_boundaries().len() + 1 {
                original.chunk
            } else {
                return Err(NavigationError::StaleSelection);
            };
            PositionAddress::Chunk(ChunkAddress {
                paragraph: paragraph_number,
                chunk,
            })
        }
        PositionAddress::Token(original) => {
            let p = document.paragraph(paragraph_number).unwrap();
            let chunk = p
                .chunk_boundaries()
                .iter()
                .position(|m| Some(m.chunk_id()) == stable.chunk_id.as_deref())
                .ok_or(NavigationError::StaleSelection)?
                + 1;
            let count = document.chunk_token_count(paragraph_number, chunk).unwrap();
            let token = if let Some(id) = &stable.token_id {
                let (first, last) = document
                    .chunk_token_bounds(paragraph_number, chunk)
                    .unwrap();
                p.tokens()[first..last]
                    .iter()
                    .position(|t| t.id() == id)
                    .ok_or(NavigationError::StaleSelection)?
                    + 1
            } else if original.token == count + 1 {
                original.token
            } else {
                return Err(NavigationError::StaleSelection);
            };
            PositionAddress::Token(TokenAddress {
                paragraph: paragraph_number,
                chunk,
                token,
            })
        }
    };
    resolve_position(document, current)?;
    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::Project;
    use serde_json::json;

    fn structured_document() -> Project {
        let token = |id: &str| json!({"id":{"kind":"pseudo","id":id},"text":id,"origin":{"kind":"pseudo","reason":"test"}});
        serde_json::from_value(json!({
            "schema":"rde-document/v1-experimental", "id":"document:positions",
            "paragraphs":[
                {"id":"p1","revision":1,"tokens":[token("a"),token("b"),token("c")],"chunk_boundaries":[
                    {"chunk_id":"c1","after_tokens":2},
                    {"chunk_id":"c2","after_tokens":2},
                    {"chunk_id":"c3","after_tokens":3}
                ]},
                {"id":"p2","revision":1,"tokens":[],"chunk_boundaries":[{"chunk_id":"c4","after_tokens":0}]}
            ]
        })).unwrap()
    }

    #[test]
    fn parses_hierarchical_positions_ranges_and_rejects_markers() {
        assert_eq!(
            parse_address("2").unwrap(),
            Address::Position(PositionAddress::Paragraph(2))
        );
        assert_eq!(
            parse_address("2.4").unwrap(),
            Address::Position(PositionAddress::Chunk(ChunkAddress {
                paragraph: 2,
                chunk: 4
            }))
        );
        assert_eq!(
            parse_address("2.4.9").unwrap(),
            Address::Position(PositionAddress::Token(TokenAddress {
                paragraph: 2,
                chunk: 4,
                token: 9
            }))
        );
        assert_eq!(
            parse_address("2,3.1.4").unwrap(),
            Address::Range {
                start: PositionAddress::Paragraph(2),
                end: PositionAddress::Token(TokenAddress {
                    paragraph: 3,
                    chunk: 1,
                    token: 4
                })
            }
        );
        assert!(matches!(
            parse_address("2@1"),
            Err(SyntaxError::InvalidAddress(_))
        ));
        assert!(matches!(
            parse_address("0.1"),
            Err(SyntaxError::ZeroAddress(_))
        ));
    }

    #[test]
    fn validates_each_depth_end_positions_and_per_chunk_token_numbers() {
        let document = structured_document();
        assert!(item_paragraph(&document, 2).is_ok());
        assert!(resolve_position(&document, PositionAddress::Paragraph(3)).is_ok());
        assert!(resolve_position(
            &document,
            PositionAddress::Chunk(ChunkAddress {
                paragraph: 1,
                chunk: 4
            })
        )
        .is_ok());
        assert!(resolve_position(
            &document,
            PositionAddress::Token(TokenAddress {
                paragraph: 1,
                chunk: 1,
                token: 3
            })
        )
        .is_ok());
        assert!(item_token(
            &document,
            TokenAddress {
                paragraph: 1,
                chunk: 3,
                token: 1
            }
        )
        .is_ok());
        assert!(matches!(
            resolve_position(
                &document,
                PositionAddress::Token(TokenAddress {
                    paragraph: 1,
                    chunk: 2,
                    token: 1
                })
            ),
            Err(NavigationError::ChunkHasNoTokens { .. })
        ));
        assert!(matches!(
            resolve_position(
                &document,
                PositionAddress::Token(TokenAddress {
                    paragraph: 1,
                    chunk: 3,
                    token: 3
                })
            ),
            Err(NavigationError::UnknownToken { .. })
        ));
    }

    #[test]
    fn half_open_mixed_depth_ranges_cover_empty_chunks_and_shared_positions() {
        let document = structured_document();
        let empty_start = PositionAddress::Chunk(ChunkAddress {
            paragraph: 1,
            chunk: 2,
        });
        let empty_end = PositionAddress::Chunk(ChunkAddress {
            paragraph: 1,
            chunk: 3,
        });
        assert!(tokens_in_range(&document, empty_start, empty_end)
            .unwrap()
            .is_empty());
        assert_eq!(
            chunks_in_range(&document, empty_start, empty_end).unwrap(),
            vec![ChunkAddress {
                paragraph: 1,
                chunk: 2
            }]
        );

        let paragraph_start = PositionAddress::Paragraph(1);
        let token_start = PositionAddress::Token(TokenAddress {
            paragraph: 1,
            chunk: 1,
            token: 1,
        });
        assert_eq!(
            resolve_position(&document, paragraph_start).unwrap(),
            resolve_position(&document, token_start).unwrap()
        );

        let mut navigation = NavigationState::new(&document);
        navigation
            .select(
                &document,
                &Address::Range {
                    start: PositionAddress::Token(TokenAddress {
                        paragraph: 1,
                        chunk: 1,
                        token: 3,
                    }),
                    end: empty_start,
                },
            )
            .unwrap();
        assert!(navigation.selection_is_empty(&document).unwrap());
        navigation
            .select(
                &document,
                &Address::Range {
                    start: empty_start,
                    end: empty_end,
                },
            )
            .unwrap();
        assert!(!navigation.selection_is_empty(&document).unwrap());
        assert!(matches!(
            navigation.select(
                &document,
                &Address::Range {
                    start: empty_end,
                    end: empty_start
                }
            ),
            Err(NavigationError::ReversedRange { .. })
        ));
    }

    #[test]
    fn stable_selections_do_not_retarget_after_an_edit() {
        let mut document = structured_document();
        let mut navigation = NavigationState::new(&document);
        navigation
            .move_to(
                &document,
                &Address::Position(PositionAddress::Token(TokenAddress {
                    paragraph: 1,
                    chunk: 1,
                    token: 2,
                })),
            )
            .unwrap();
        document.insert_text(1, 1, false, "new".into()).unwrap();
        assert_eq!(
            navigation.current_range(&document),
            Err(NavigationError::StaleSelection)
        );
    }
}
