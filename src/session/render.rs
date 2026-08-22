use std::{
    fmt,
    io::{self, Write},
    path::Path,
};

use crate::{
    document::Document,
    navigation::{Caret, NavigationState, Selection},
    recognition::{ChunkBoundaryReason, RecognitionRun},
};

pub fn render_recognition_chunks(
    run: &RecognitionRun,
    source: &Path,
    output: &mut impl Write,
) -> io::Result<()> {
    let document = Document::from_run(run);
    render_recognition_document(run, &document, source, output)
}

pub(crate) fn render_recognition_document(
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
    render_paragraph_inner(paragraph, paragraph_number, navigation, None, None, output)
}

pub(crate) fn render_issue_paragraph(
    document: &Document,
    paragraph: &crate::document::Paragraph,
    paragraph_number: usize,
    navigation: Option<&NavigationState>,
    settings: super::issues::IssueThresholds,
    color: bool,
    output: &mut impl Write,
) -> io::Result<()> {
    render_paragraph_inner(
        paragraph,
        paragraph_number,
        navigation,
        color.then_some((document, settings)),
        Some((document, color)),
        output,
    )
}

fn render_paragraph_inner(
    paragraph: &crate::document::Paragraph,
    paragraph_number: usize,
    navigation: Option<&NavigationState>,
    issues: Option<(&Document, super::issues::IssueThresholds)>,
    attention: Option<(&Document, bool)>,
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
            if let Some((_, color)) =
                attention.filter(|(document, _)| document.is_attention_marked(token.id()))
            {
                if color {
                    write!(output, "\x1b[31m⚑\x1b[0m")?;
                } else {
                    write!(output, "⚑")?;
                }
            }
            let confidence = issues.and_then(|(document, settings)| {
                super::issues::confidence(document, token.id(), settings)
            });
            if let Some(confidence) = confidence {
                write!(
                    output,
                    "{}",
                    match confidence {
                        super::issues::Confidence::Red => "\x1b[31m",
                        super::issues::Confidence::Orange => "\x1b[38;5;208m",
                    }
                )?;
            }
            write!(output, "{}", token.text())?;
            if confidence.is_some() {
                write!(output, "\x1b[0m")?;
            }
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
    document: &Document,
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
    document: &Document,
    paragraph: &crate::document::Paragraph,
    paragraph_number: usize,
    start: usize,
    end_exclusive: usize,
    settings: super::issues::IssueThresholds,
    color: bool,
    output: &mut impl Write,
) -> io::Result<()> {
    let mut marker_index = 0;
    for token_index in 0..=paragraph.tokens().len() {
        while paragraph
            .chunk_boundaries()
            .get(marker_index)
            .is_some_and(|marker| marker.after_tokens() == token_index)
        {
            if (start..=end_exclusive).contains(&token_index) {
                writeln!(
                    output,
                    "{}@{}  marker  chunk boundary",
                    paragraph_number,
                    marker_index + 1
                )?;
            }
            marker_index += 1;
        }
        if (start..end_exclusive).contains(&token_index) {
            if let Some(token) = paragraph.tokens().get(token_index) {
                let probability = document
                    .recognition_token_evidence()
                    .iter()
                    .find(|evidence| evidence.token_id() == token.id())
                    .map(|evidence| format!("{:.3}", evidence.probability()))
                    .unwrap_or_else(|| "-".into());
                write!(
                    output,
                    "{}.{}  {:>5}  ",
                    paragraph_number,
                    token_index + 1,
                    probability
                )?;
                if document.is_attention_marked(token.id()) {
                    if color {
                        write!(output, "\x1b[31m⚑\x1b[0m")?;
                    } else {
                        write!(output, "⚑")?;
                    }
                }
                let confidence = color
                    .then(|| super::issues::confidence(document, token.id(), settings))
                    .flatten();
                if let Some(confidence) = confidence {
                    write!(
                        output,
                        "{}",
                        match confidence {
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
        }
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
    use serde_json::json;

    #[test]
    fn token_listing_uses_live_confidence_colors_on_token_text() {
        let id =
            |index| json!({"kind":"recognition","run_id":"r","segment_id":"s","token_index":index});
        let document: Document = serde_json::from_value(json!({
            "schema":"rde-document/v1-experimental","id":"d","paragraphs":[{
                "id":"p","revision":1,"tokens":[
                    {"id":id(0),"text":"red","origin":{"kind":"recognition"}},
                    {"id":id(1),"text":"orange","origin":{"kind":"recognition"}}
                ],"chunk_boundaries":[{"chunk_id":"c","after_tokens":2}]
            }],"recognition_token_evidence":[
                {"token_id":id(0),"recognition_token_id":1,"probability":0.1,"alternatives":[]},
                {"token_id":id(1),"recognition_token_id":2,"probability":0.2,"alternatives":[]}
            ]
        }))
        .unwrap();
        let mut output = Vec::new();
        render_tokens(
            &document,
            document.paragraph(1).unwrap(),
            1,
            super::super::issues::IssueThresholds::default(),
            true,
            &mut output,
        )
        .unwrap();
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("1.1  0.100  \x1b[31m\"red\"\x1b[0m"));
        assert!(output.contains("1.2  0.200  \x1b[38;5;208m\"orange\"\x1b[0m"));
    }

    #[test]
    fn attention_flag_is_literal_without_color_and_red_with_its_own_reset() {
        let document: Document = serde_json::from_value(json!({
            "schema":"rde-document/v1-experimental","id":"d","paragraphs":[{
                "id":"p","revision":1,"tokens":[
                    {"id":{"kind":"pseudo","id":"t"},"text":" text","origin":{"kind":"pseudo","reason":"test"}}
                ],"chunk_boundaries":[{"chunk_id":"c","after_tokens":1}]
            }],"attention_marks":[{"token_id":{"kind":"pseudo","id":"t"}}]
        })).unwrap();
        for (color, expected) in [(false, "⚑ text"), (true, "\x1b[31m⚑\x1b[0m text")] {
            let mut output = Vec::new();
            render_issue_paragraph(
                &document,
                document.paragraph(1).unwrap(),
                1,
                None,
                super::super::issues::IssueThresholds::default(),
                color,
                &mut output,
            )
            .unwrap();
            assert!(String::from_utf8(output).unwrap().contains(expected));
        }
    }
}
