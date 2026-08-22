use std::{
    cmp::Ordering,
    collections::HashSet,
    io::{self, Write},
};

use crate::{
    document::{Document, VisibleTokenId},
    navigation::{Address, Caret, NavigationState, Selection, TokenAddress},
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
    pub token_ids: Vec<VisibleTokenId>,
    pub resolved_index: Option<usize>,
}

impl IssueEntry {
    pub fn is_open(&self) -> bool {
        self.resolved_index.is_none()
    }
}

pub(crate) fn confidence(
    document: &Document,
    id: &VisibleTokenId,
    settings: IssueThresholds,
) -> Option<Confidence> {
    if document
        .resolved_issues()
        .iter()
        .any(|issue| issue.token_ids().contains(id))
    {
        return None;
    }
    let probability = document
        .recognition_token_evidence()
        .iter()
        .find(|e| e.token_id() == id)?
        .probability();
    if probability < settings.red {
        Some(Confidence::Red)
    } else if probability < settings.orange {
        Some(Confidence::Orange)
    } else {
        None
    }
}

pub(crate) fn entries(document: &Document, settings: IssueThresholds) -> Vec<IssueEntry> {
    let resolved_ids = document
        .resolved_issues()
        .iter()
        .flat_map(|r| r.token_ids())
        .collect::<HashSet<_>>();
    let mut result = Vec::new();
    for (pi, paragraph) in document.paragraphs().iter().enumerate() {
        let mut chunk_start = 0;
        for marker in paragraph.chunk_boundaries() {
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
                        push_open(&mut result, paragraph, pi, start, ti - 1);
                    }
                }
            }
            if let Some(start) = open_start {
                push_open(&mut result, paragraph, pi, start, marker.after_tokens() - 1);
            }
            chunk_start = marker.after_tokens();
        }
    }
    for (ri, resolved) in document.resolved_issues().iter().enumerate() {
        let positions = resolved
            .token_ids()
            .iter()
            .filter_map(|id| find_token(document, id))
            .collect::<Vec<_>>();
        if let (Some(start), Some(end)) = (positions.first(), positions.last()) {
            result.push(IssueEntry {
                start: *start,
                end: *end,
                token_ids: resolved.token_ids().to_vec(),
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
    start: usize,
    end: usize,
) {
    out.push(IssueEntry {
        start: TokenAddress {
            paragraph: pi + 1,
            token: start + 1,
        },
        end: TokenAddress {
            paragraph: pi + 1,
            token: end + 1,
        },
        token_ids: paragraph.tokens()[start..=end]
            .iter()
            .map(|t| t.id().clone())
            .collect(),
        resolved_index: None,
    });
}
fn find_token(document: &Document, id: &VisibleTokenId) -> Option<TokenAddress> {
    document
        .paragraphs()
        .iter()
        .enumerate()
        .find_map(|(pi, p)| {
            p.tokens()
                .iter()
                .position(|t| t.id() == id)
                .map(|ti| TokenAddress {
                    paragraph: pi + 1,
                    token: ti + 1,
                })
        })
}
fn position_cmp(a: TokenAddress, b: TokenAddress) -> Ordering {
    (a.paragraph, a.token).cmp(&(b.paragraph, b.token))
}

pub(crate) fn list(
    document: &Document,
    settings: IssueThresholds,
    output: &mut impl Write,
) -> io::Result<()> {
    let values = entries(document, settings);
    if values.is_empty() {
        return writeln!(output, "no issues");
    }
    for (i, issue) in values.iter().enumerate() {
        let text = issue
            .token_ids
            .iter()
            .filter_map(|id| find_token(document, id))
            .filter_map(|a| document.token(a.paragraph, a.token))
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
    document: &Document,
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
            token: 0,
        },
        TokenAddress {
            paragraph: 0,
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
            &Address::TokenRange {
                start: issue.start,
                end: issue.end,
            },
        )
        .expect("current issue addresses resolve");
    writeln!(
        output,
        "selected {}.{},{}.{}{}",
        issue.start.paragraph,
        issue.start.token,
        issue.end.paragraph,
        issue.end.token,
        if wrapped { " (wrapped)" } else { "" }
    )
}

fn navigation_bounds(
    document: &Document,
    navigation: &NavigationState,
) -> Option<(TokenAddress, TokenAddress)> {
    match navigation.selection() {
        Some(Selection::Tokens {
            start,
            end_inclusive,
            ..
        }) => Some((
            find_token(document, &start.token_id)?,
            find_token(document, &end_inclusive.token_id)?,
        )),
        Some(Selection::Paragraph { paragraph_id, .. }) => {
            let p = document
                .paragraphs()
                .iter()
                .position(|p| p.id() == paragraph_id)?
                + 1;
            Some((
                TokenAddress {
                    paragraph: p,
                    token: 0,
                },
                TokenAddress {
                    paragraph: p,
                    token: usize::MAX,
                },
            ))
        }
        Some(Selection::Marker(m)) => marker_bounds(document, &m.paragraph_id, &m.chunk_id),
        Some(Selection::MarkerRange {
            start,
            end_exclusive,
            ..
        }) => Some((
            marker_bounds(document, &start.paragraph_id, &start.chunk_id)?.0,
            marker_bounds(
                document,
                &end_exclusive.paragraph_id,
                &end_exclusive.chunk_id,
            )?
            .1,
        )),
        None => match navigation.caret()? {
            Caret::Token(t) => find_token(document, &t.token_id).map(|a| (a, a)),
            Caret::Marker(m) => marker_bounds(document, &m.paragraph_id, &m.chunk_id),
        },
    }
}
fn marker_bounds(
    document: &Document,
    pid: &str,
    cid: &str,
) -> Option<(TokenAddress, TokenAddress)> {
    let (pi, p) = document
        .paragraphs()
        .iter()
        .enumerate()
        .find(|(_, p)| p.id() == pid)?;
    let n = p
        .chunk_boundaries()
        .iter()
        .find(|m| m.chunk_id() == cid)?
        .after_tokens();
    let a = TokenAddress {
        paragraph: pi + 1,
        token: n,
    };
    Some((a, a))
}
