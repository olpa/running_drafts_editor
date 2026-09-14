use crate::{
    navigation::{
        chunks_in_range, tokens_in_range, ChunkAddress, NavigationState, PositionAddress,
        TokenAddress,
    },
    project::Project,
    recognition::{ChunkBoundaryReason, RecognitionRun},
};
use std::{
    fmt,
    io::{self, Write},
    path::Path,
};

pub fn render_recognition_chunks(
    run: &RecognitionRun,
    source: &Path,
    output: &mut impl Write,
) -> io::Result<()> {
    let document = Project::from_run(run);
    render_recognition_document(run, &document, source, output)
}

pub(crate) fn render_recognition_document(
    run: &RecognitionRun,
    document: &Project,
    source: &Path,
    output: &mut impl Write,
) -> io::Result<()> {
    writeln!(
        output,
        "Built {} chunks from {}",
        run.chunks.len(),
        source.display()
    )?;
    if !run.chunks.is_empty() {
        writeln!(output)?;
    }
    for (index, paragraph) in document.paragraphs().iter().enumerate() {
        render_paragraph(paragraph, index + 1, None, output)?;
        if index + 1 < document.paragraphs().len() {
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
    render_paragraph_inner(
        None,
        paragraph,
        paragraph_number,
        navigation,
        None,
        false,
        output,
    )
}

pub(crate) fn render_issue_paragraph(
    document: &Project,
    paragraph: &crate::document::Paragraph,
    paragraph_number: usize,
    navigation: Option<&NavigationState>,
    settings: super::issues::IssueThresholds,
    color: bool,
    output: &mut impl Write,
) -> io::Result<()> {
    render_paragraph_inner(
        Some(document),
        paragraph,
        paragraph_number,
        navigation,
        Some(settings),
        color,
        output,
    )
}

fn render_paragraph_inner(
    document: Option<&Project>,
    paragraph: &crate::document::Paragraph,
    paragraph_number: usize,
    navigation: Option<&NavigationState>,
    settings: Option<super::issues::IssueThresholds>,
    color: bool,
    output: &mut impl Write,
) -> io::Result<()> {
    let range = document.and_then(|doc| navigation.and_then(|n| n.current_range(doc).ok()));
    let selected_tokens = document
        .and_then(|doc| range.and_then(|(a, b)| tokens_in_range(doc, a, b).ok()))
        .unwrap_or_default();
    let selected_chunks = document
        .and_then(|doc| range.and_then(|(a, b)| chunks_in_range(doc, a, b).ok()))
        .unwrap_or_default();
    let current = range
        .filter(|_| {
            navigation
                .is_some_and(|n| document.is_some_and(|d| n.selection_is_empty(d).unwrap_or(false)))
        })
        .map(|(a, _)| a);
    let mut paragraph_token = 0;
    for (chunk_index, marker) in paragraph.chunk_boundaries().iter().enumerate() {
        if chunk_index > 0 {
            write!(output, " ")?;
        }
        let chunk_address = ChunkAddress {
            paragraph: paragraph_number,
            chunk: chunk_index + 1,
        };
        let selected_empty_chunk = selected_chunks.contains(&chunk_address)
            && paragraph
                .chunk_boundaries()
                .get(chunk_index.wrapping_sub(1))
                .map_or(0, |m| m.after_tokens())
                == marker.after_tokens();
        let current_chunk = match current {
            Some(PositionAddress::Paragraph(number)) => {
                number == paragraph_number && chunk_index == 0
            }
            Some(PositionAddress::Chunk(address)) => address == chunk_address,
            Some(PositionAddress::Token(address))
                if chunk_index > 0
                    && address.paragraph == paragraph_number
                    && address.chunk == chunk_index =>
            {
                document
                    .and_then(|doc| doc.chunk_token_count(address.paragraph, address.chunk))
                    .is_some_and(|count| address.token == count + 1)
            }
            _ => false,
        };
        if selected_empty_chunk {
            write!(output, "⟪")?;
        } else if current_chunk {
            write!(output, "‹")?;
        }
        write!(output, "⟦{chunk_address}⟧")?;
        if selected_empty_chunk {
            write!(output, "⟫")?;
        } else if current_chunk {
            write!(output, "›")?;
        }
        let chunk_start = paragraph_token;
        let chunk_count = marker.after_tokens() - chunk_start;
        for local in 0..chunk_count {
            let token = &paragraph.tokens()[paragraph_token];
            let address = TokenAddress {
                paragraph: paragraph_number,
                chunk: chunk_index + 1,
                token: local + 1,
            };
            let selected = selected_tokens.contains(&address);
            let first = selected && selected_tokens.first() == Some(&address);
            let last = selected && selected_tokens.last() == Some(&address);
            let current_token = current == Some(PositionAddress::Token(address));
            if first {
                write!(output, "⟪")?;
            } else if current_token {
                write!(output, "‹")?;
            }
            if let Some(doc) = document {
                if doc.is_attention_marked(token.id()) {
                    write!(
                        output,
                        "{}⚑{}",
                        if color { "\x1b[31m" } else { "" },
                        if color { "\x1b[0m" } else { "" }
                    )?;
                }
                let confidence =
                    settings.and_then(|s| super::issues::confidence(doc, token.id(), s));
                if color {
                    if let Some(level) = confidence {
                        write!(
                            output,
                            "{}",
                            match level {
                                super::issues::Confidence::Red => "\x1b[31m",
                                super::issues::Confidence::Orange => "\x1b[38;5;208m",
                            }
                        )?;
                    }
                }
                write!(output, "{}", token.text())?;
                if color && confidence.is_some() {
                    write!(output, "\x1b[0m")?;
                }
            } else {
                write!(output, "{}", token.text())?;
            }
            if last {
                write!(output, "⟫")?;
            } else if current_token {
                write!(output, "›")?;
            }
            paragraph_token += 1;
        }
    }
    writeln!(output)
}

pub(crate) fn render_tokens(
    document: &Project,
    paragraph: &crate::document::Paragraph,
    paragraph_number: usize,
    settings: super::issues::IssueThresholds,
    color: bool,
    output: &mut impl Write,
) -> io::Result<()> {
    render_token_range(
        document,
        paragraph,
        paragraph_number,
        0,
        paragraph.tokens().len(),
        settings,
        color,
        output,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_token_range(
    document: &Project,
    paragraph: &crate::document::Paragraph,
    paragraph_number: usize,
    start: usize,
    end_exclusive: usize,
    settings: super::issues::IssueThresholds,
    color: bool,
    output: &mut impl Write,
) -> io::Result<()> {
    let mut previous = 0;
    for (chunk_index, marker) in paragraph.chunk_boundaries().iter().enumerate() {
        if (start..=end_exclusive).contains(&previous) {
            writeln!(
                output,
                "{}.{}  chunk  {}",
                paragraph_number,
                chunk_index + 1,
                if marker.after_tokens() > previous {
                    "has_tokens"
                } else {
                    "no tokens"
                }
            )?;
        }
        for global in previous..marker.after_tokens() {
            if !(start..end_exclusive).contains(&global) {
                continue;
            }
            let token = &paragraph.tokens()[global];
            let probability = document
                .recognition_token_evidence()
                .iter()
                .find(|e| e.token_id() == token.id())
                .map(|e| format!("{:.3}", e.probability()))
                .unwrap_or_else(|| "-".into());
            write!(
                output,
                "{}.{}.{}  {:>5}  ",
                paragraph_number,
                chunk_index + 1,
                global - previous + 1,
                probability
            )?;
            if document.is_attention_marked(token.id()) {
                write!(
                    output,
                    "{}⚑{}",
                    if color { "\x1b[31m" } else { "" },
                    if color { "\x1b[0m" } else { "" }
                )?;
            }
            let confidence = color
                .then(|| super::issues::confidence(document, token.id(), settings))
                .flatten();
            if let Some(level) = confidence {
                write!(
                    output,
                    "{}",
                    match level {
                        super::issues::Confidence::Red => "\x1b[31m",
                        super::issues::Confidence::Orange => "\x1b[38;5;208m",
                    }
                )?;
            }
            write!(output, "{:?}", token.text())?;
            if confidence.is_some() {
                write!(output, "\x1b[0m")?;
            }
            writeln!(output)?;
        }
        previous = marker.after_tokens();
    }
    Ok(())
}

pub(crate) fn render_chunk_info(
    run: &RecognitionRun,
    chunk: &crate::recognition::RecognitionChunk,
    paragraph: usize,
    chunk_number: usize,
    output: &mut impl Write,
) -> io::Result<()> {
    writeln!(
        output,
        "{}.{}  {} – {}  {:>9}  {:>3} tokens  {}",
        paragraph,
        chunk_number,
        Timestamp::new(chunk.audio_range.start_sample, run.source.sample_rate_hz),
        Timestamp::new(chunk.audio_range.end_sample, run.source.sample_rate_hz),
        Duration::new(chunk.audio_range.len(), run.source.sample_rate_hz),
        chunk.token_count,
        chunk_boundary_label(chunk, run.source.sample_rate_hz)
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
        ChunkBoundaryReason::MaximumTokens => return "token limit".into(),
        ChunkBoundaryReason::SourceEnd => "source end",
    };
    chunk.boundary.pause_samples.map_or_else(
        || reason.into(),
        |samples| format!("{reason} ({})", Duration::new(samples, sample_rate_hz)),
    )
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ms = u128::from(self.samples) * 1000 / u128::from(self.sample_rate_hz);
        write!(
            f,
            "{:02}:{:02}:{:02}.{:03}",
            ms / 3_600_000,
            ms / 60_000 % 60,
            ms / 1000 % 60,
            ms % 1000
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ms = u128::from(self.samples) * 1000 / u128::from(self.sample_rate_hz);
        write!(f, "{}.{:03} s", ms / 1000, ms % 1000)
    }
}
