//! Short-lived projections of issues in the current document structure.
//!
//! Durable dismissed-issue state stores stable token identities. Build these
//! entries when a command needs them, and do not retain an entry across a
//! project mutation. See ADR-0011.

use std::{
    cmp::Ordering,
    collections::HashSet,
    io::{self, Write},
};

use crate::{
    document::TokenIdentity,
    navigation::{Address, NavigationState, PositionAddress, TokenAddress},
    project::Project,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct IssueThresholds {
    pub red: f32,
    pub orange: f32,
}

impl Default for IssueThresholds {
    fn default() -> Self {
        Self {
            red: 0.15,
            orange: 0.50,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Confidence {
    Red,
    Orange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IssueEntry {
    pub start: TokenAddress,
    pub end: TokenAddress,
    pub token_identities: Vec<TokenIdentity>,
    pub resolved_index: Option<usize>,
}

impl IssueEntry {
    pub fn is_open(&self) -> bool {
        self.resolved_index.is_none()
    }
}

pub(crate) fn confidence(
    document: &Project,
    id: &TokenIdentity,
    settings: IssueThresholds,
) -> Option<Confidence> {
    if document
        .resolved_issues()
        .iter()
        .any(|issue| issue.token_identities().contains(id))
    {
        return None;
    }
    let probability = document.token_evidence(id)?.probability();
    if probability < settings.red {
        Some(Confidence::Red)
    } else if probability < settings.orange {
        Some(Confidence::Orange)
    } else {
        None
    }
}

pub(crate) fn entries(document: &Project, settings: IssueThresholds) -> Vec<IssueEntry> {
    let resolved_ids = document
        .resolved_issues()
        .iter()
        .flat_map(|r| r.token_identities())
        .collect::<HashSet<_>>();
    let mut result = Vec::new();
    for (pi, paragraph) in document.paragraphs().iter().enumerate() {
        let mut chunk_start = 0;
        for (chunk_index, marker) in paragraph.chunk_boundaries().iter().enumerate() {
            let mut open_start = None;
            for ti in chunk_start..marker.after_tokens() {
                let token = &paragraph.tokens()[ti];
                let red = !resolved_ids.contains(token.id())
                    && confidence(document, token.id(), settings) == Some(Confidence::Red);
                if red && open_start.is_none() {
                    open_start = Some(ti);
                }
                if !red {
                    if let Some(start) = open_start.take() {
                        push_open(
                            &mut result,
                            paragraph,
                            pi,
                            chunk_index,
                            chunk_start,
                            start,
                            ti - 1,
                        );
                    }
                }
            }
            if let Some(start) = open_start {
                push_open(
                    &mut result,
                    paragraph,
                    pi,
                    chunk_index,
                    chunk_start,
                    start,
                    marker.after_tokens() - 1,
                );
            }
            chunk_start = marker.after_tokens();
        }
    }
    for (ri, resolved) in document.resolved_issues().iter().enumerate() {
        let positions = resolved
            .token_identities()
            .iter()
            .filter_map(|id| find_token(document, id))
            .collect::<Vec<_>>();
        if let (Some(start), Some(end)) = (positions.first(), positions.last()) {
            result.push(IssueEntry {
                start: *start,
                end: *end,
                token_identities: resolved.token_identities().to_vec(),
                resolved_index: Some(ri),
            });
        }
    }
    result.sort_by(|a, b| position_cmp(a.start, b.start));
    result
}

fn push_open(
    out: &mut Vec<IssueEntry>,
    paragraph: &crate::document::Paragraph,
    pi: usize,
    chunk_index: usize,
    chunk_start: usize,
    start: usize,
    end: usize,
) {
    out.push(IssueEntry {
        start: TokenAddress {
            paragraph: pi + 1,
            chunk: chunk_index + 1,
            token: start - chunk_start + 1,
        },
        end: TokenAddress {
            paragraph: pi + 1,
            chunk: chunk_index + 1,
            token: end - chunk_start + 1,
        },
        token_identities: paragraph.tokens()[start..=end]
            .iter()
            .map(|t| t.id().clone())
            .collect(),
        resolved_index: None,
    });
}
fn find_token(document: &Project, id: &TokenIdentity) -> Option<TokenAddress> {
    document
        .paragraphs()
        .iter()
        .enumerate()
        .find_map(|(pi, p)| {
            p.tokens().iter().position(|t| t.id() == id).and_then(|ti| {
                document
                    .chunk_token_address(pi + 1, ti + 1)
                    .map(|(chunk, token)| TokenAddress {
                        paragraph: pi + 1,
                        chunk,
                        token,
                    })
            })
        })
}
fn position_cmp(a: TokenAddress, b: TokenAddress) -> Ordering {
    (a.paragraph, a.chunk, a.token).cmp(&(b.paragraph, b.chunk, b.token))
}

pub(crate) fn list(
    document: &Project,
    settings: IssueThresholds,
    output: &mut impl Write,
) -> io::Result<()> {
    let values = entries(document, settings);
    if values.is_empty() {
        return writeln!(output, "no issues");
    }
    for (i, issue) in values.iter().enumerate() {
        let text = issue
            .token_identities
            .iter()
            .filter_map(|id| find_token(document, id))
            .filter_map(|a| document.chunk_token(a.paragraph, a.chunk, a.token))
            .map(|t| t.text())
            .collect::<String>();
        writeln!(
            output,
            "{}  {}  {:?}",
            i + 1,
            if issue.is_open() { "open" } else { "resolved" },
            text
        )?;
    }
    Ok(())
}

pub(crate) fn navigate(
    document: &Project,
    navigation: &mut NavigationState,
    settings: IssueThresholds,
    forward: bool,
    output: &mut impl Write,
) -> io::Result<()> {
    let open = entries(document, settings)
        .into_iter()
        .filter(IssueEntry::is_open)
        .collect::<Vec<_>>();
    if open.is_empty() {
        return writeln!(output, "no open issues");
    }
    let (low, high) = navigation_bounds(document, navigation).unwrap_or((
        TokenAddress {
            paragraph: 0,
            chunk: 0,
            token: 0,
        },
        TokenAddress {
            paragraph: 0,
            chunk: 0,
            token: 0,
        },
    ));
    let found = if forward {
        open.iter()
            .position(|i| position_cmp(i.start, high).is_gt())
    } else {
        open.iter().rposition(|i| position_cmp(i.end, low).is_lt())
    };
    let wrapped = found.is_none();
    let issue = if forward {
        &open[found.unwrap_or(0)]
    } else {
        &open[found.unwrap_or(open.len() - 1)]
    };
    navigation
        .select(
            document,
            &Address::Range {
                start: PositionAddress::Token(issue.start),
                end: PositionAddress::Token(TokenAddress {
                    token: issue.end.token + 1,
                    ..issue.end
                }),
            },
        )
        .expect("current issue addresses resolve");
    writeln!(
        output,
        "selected {},{}.{}.{}{}",
        issue.start,
        issue.end.paragraph,
        issue.end.chunk,
        issue.end.token + 1,
        if wrapped { " (wrapped)" } else { "" }
    )
}

fn navigation_bounds(
    document: &Project,
    navigation: &NavigationState,
) -> Option<(TokenAddress, TokenAddress)> {
    navigation
        .selected_token_endpoints(document)
        .ok()
        .or_else(|| {
            navigation
                .current_token_address(document)
                .ok()
                .map(|a| (a, a))
        })
}
