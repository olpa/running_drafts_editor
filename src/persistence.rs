//! Versioned persistence for a project and its authoritative document.

use std::{
    collections::HashSet,
    fs::{self, OpenOptions},
    io::{self, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
};

use crate::project::{Project, PROJECT_SCHEMA};
use crate::transcription::{ChunkBoundaryReason, DecodeSpanItem};

#[derive(Debug, thiserror::Error)]
pub enum ProjectIoError {
    #[error("could not open document '{}': {source}", path.display())]
    Open { path: PathBuf, source: io::Error },
    #[error("could not read document '{}': {source}", path.display())]
    Read {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("unsupported project schema '{found}'; expected '{PROJECT_SCHEMA}'")]
    UnsupportedSchema { found: String },
    #[error("invalid document: {0}")]
    Invalid(String),
    #[error("could not save document '{}': {source}", path.display())]
    Save { path: PathBuf, source: io::Error },
    #[error("could not encode document '{}': {source}", path.display())]
    Encode {
        path: PathBuf,
        source: serde_json::Error,
    },
}

pub fn load_project(path: &Path) -> Result<Project, ProjectIoError> {
    let file = fs::File::open(path).map_err(|source| ProjectIoError::Open {
        path: path.into(),
        source,
    })?;
    let project =
        serde_json::from_reader(BufReader::new(file)).map_err(|source| ProjectIoError::Read {
            path: path.into(),
            source,
        })?;
    validate(&project)?;
    Ok(project)
}

pub fn save_project(path: &Path, project: &Project) -> Result<(), ProjectIoError> {
    validate(project)?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("document");
    let mut temporary = None;
    for attempt in 0..100 {
        let candidate = parent.join(format!(".{name}.{}.{}.tmp", std::process::id(), attempt));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                temporary = Some((candidate, file));
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(source) => {
                return Err(ProjectIoError::Save {
                    path: path.into(),
                    source,
                })
            }
        }
    }
    let Some((temporary_path, file)) = temporary else {
        return Err(ProjectIoError::Save {
            path: path.into(),
            source: io::Error::new(
                io::ErrorKind::AlreadyExists,
                "no temporary filename available",
            ),
        });
    };
    let result = (|| {
        let mut writer = BufWriter::new(file);
        serde_json::to_writer_pretty(&mut writer, project).map_err(|source| {
            ProjectIoError::Encode {
                path: path.into(),
                source,
            }
        })?;
        writer
            .write_all(b"\n")
            .map_err(|source| ProjectIoError::Save {
                path: path.into(),
                source,
            })?;
        writer.flush().map_err(|source| ProjectIoError::Save {
            path: path.into(),
            source,
        })?;
        writer
            .get_ref()
            .sync_all()
            .map_err(|source| ProjectIoError::Save {
                path: path.into(),
                source,
            })?;
        fs::rename(&temporary_path, path).map_err(|source| ProjectIoError::Save {
            path: path.into(),
            source,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result
}

/// Write disposable plain text, including intentional attention flags but no
/// transcription or replay metadata.
pub fn export_text(path: &Path, project: &Project) -> Result<(), ProjectIoError> {
    validate(project)?;
    let mut bytes = Vec::new();
    for (paragraph_index, paragraph) in project.paragraphs().iter().enumerate() {
        if paragraph_index > 0 {
            bytes.extend_from_slice(b"\n\n");
        }
        let mut start = 0;
        for chunk in paragraph.chunk_boundaries() {
            if chunk.after_tokens() == start {
                bytes.extend_from_slice(chunk.text().as_bytes());
            } else {
                for token in &paragraph.tokens()[start..chunk.after_tokens()] {
                    if project.is_attention_marked(token.id()) {
                        bytes.extend_from_slice("⚑".as_bytes());
                    }
                    bytes.extend_from_slice(token.text().as_bytes());
                }
            }
            start = chunk.after_tokens();
        }
    }
    fs::write(path, bytes).map_err(|source| ProjectIoError::Save {
        path: path.into(),
        source,
    })
}

pub(crate) fn validate(project: &Project) -> Result<(), ProjectIoError> {
    if project.schema() != PROJECT_SCHEMA {
        return Err(ProjectIoError::UnsupportedSchema {
            found: project.schema().into(),
        });
    }
    let invalid = |message: &str| ProjectIoError::Invalid(message.into());
    if project.id().is_empty() {
        return Err(invalid("document ID is empty"));
    }
    let (stored_chunk_ids, stored_breaks) = validate_decode_spans(project)?;
    let mut ids = HashSet::new();
    for t in project.transcriptions() {
        if t.id.is_empty() || t.chunk_id.is_empty() || !ids.insert(&t.id) {
            return Err(invalid(
                "transcription identities must be nonempty and unique",
            ));
        }
        if t.source.sample_rate_hz != 16_000
            || t.source.channels != 1
            || t.audio_range.is_empty()
            || t.audio_range.end_sample > t.source.decoded_sample_count
        {
            return Err(invalid("invalid transcription audio coordinates"));
        }
        let mut segments = HashSet::new();
        for s in &t.segments {
            if s.id.is_empty() || !segments.insert(&s.id) {
                return Err(invalid("duplicate or empty segment identity"));
            }
        }
    }
    for (index, t) in project.transcriptions().iter().enumerate() {
        if let Some(previous) = &t.previous_id {
            if !project.transcriptions()[..index].iter().any(|p| {
                p.id == *previous && p.chunk_id == t.chunk_id && p.audio_range == t.audio_range
            }) {
                return Err(invalid("transcription predecessor must identify an earlier proposal for the same chunk"));
            }
        }
    }
    let sources = project
        .audio_sources()
        .iter()
        .map(|s| s.id())
        .collect::<HashSet<_>>();
    if sources.len() != project.audio_sources().len() {
        return Err(invalid("duplicate audio source identity"));
    }
    validate_state(
        project,
        &project.document.paragraphs,
        &project.chunk_audio_mappings,
        &project.token_audio_mappings,
        &project.attention_marks,
        &project.resolved_issues,
        &sources,
    )?;
    let document_chunk_ids = project
        .paragraphs()
        .iter()
        .flat_map(|paragraph| paragraph.chunk_boundaries())
        .map(|marker| marker.chunk_id().to_owned())
        .collect::<Vec<_>>();
    if document_chunk_ids != stored_chunk_ids {
        return Err(invalid(
            "document chunk order differs from decode-span content",
        ));
    }
    for marker in project
        .paragraphs()
        .iter()
        .flat_map(|paragraph| paragraph.chunk_boundaries())
    {
        let selected = project
            .chunks()
            .find(|chunk| chunk.id == marker.chunk_id())
            .map(|chunk| chunk.current_transcription_id.as_str());
        if selected != Some(marker.transcription_id()) {
            return Err(invalid(
                "document projection differs from selected chunk transcription",
            ));
        }
    }
    if project.chunk_audio_mappings().len() != stored_chunk_ids.len() {
        return Err(invalid("not every finalized chunk has one audio mapping"));
    }
    let document_breaks = project
        .paragraphs()
        .iter()
        .take(project.paragraphs().len().saturating_sub(1))
        .filter_map(|paragraph| paragraph.chunk_boundaries().last())
        .map(|marker| marker.chunk_id().to_owned())
        .collect::<Vec<_>>();
    if document_breaks != stored_breaks {
        return Err(invalid(
            "document paragraphs differ from decode-span content",
        ));
    }
    for state in project
        .edit_history
        .iter()
        .map(|e| &e.before)
        .chain(project.redo_history.iter())
    {
        validate_selected_transcriptions(
            project,
            &state.paragraphs,
            &state.current_transcriptions,
            stored_chunk_ids.len(),
        )?;
        validate_state(
            project,
            &state.paragraphs,
            &state.chunk_audio_mappings,
            &state.token_audio_mappings,
            &state.attention_marks,
            &state.resolved_issues,
            &sources,
        )?;
    }
    Ok(())
}

fn validate_selected_transcriptions(
    project: &Project,
    paragraphs: &[crate::document::Paragraph],
    selected: &[(String, String)],
    chunk_count: usize,
) -> Result<(), ProjectIoError> {
    let invalid = |message: &str| ProjectIoError::Invalid(message.into());
    let unique = selected
        .iter()
        .map(|(chunk_id, _)| chunk_id.as_str())
        .collect::<HashSet<_>>();
    if selected.len() != chunk_count || unique.len() != selected.len() {
        return Err(invalid(
            "history has incomplete chunk transcription selection",
        ));
    }
    for (chunk_id, transcription_id) in selected {
        let valid = project.chunks().any(|chunk| {
            chunk.id == *chunk_id
                && chunk
                    .transcriptions
                    .iter()
                    .any(|transcription| transcription.id == *transcription_id)
        });
        if !valid {
            return Err(invalid("history selects an unknown chunk transcription"));
        }
    }
    for marker in paragraphs
        .iter()
        .flat_map(|paragraph| paragraph.chunk_boundaries())
    {
        let transcription_id = selected
            .iter()
            .find(|(chunk_id, _)| chunk_id == marker.chunk_id())
            .map(|(_, transcription_id)| transcription_id.as_str());
        if transcription_id != Some(marker.transcription_id()) {
            return Err(invalid(
                "historical document differs from its selected chunk transcription",
            ));
        }
    }
    Ok(())
}

fn validate_decode_spans(project: &Project) -> Result<(Vec<String>, Vec<String>), ProjectIoError> {
    let invalid = |message: &str| ProjectIoError::Invalid(message.into());
    let Some(evidence) = &project.initial_evidence else {
        return Err(invalid("initial decode-span evidence is missing"));
    };
    let mut chunk_ids = Vec::new();
    let mut breaks = Vec::new();
    let mut seen_chunk_ids = HashSet::new();
    let mut previous_chunk_end = None;
    let mut expected_start = 0_u64;

    for (index, span) in evidence.decode_spans.iter().enumerate() {
        if span.submitted.start_sample != expected_start
            || span.ordinal != u32::try_from(index + 1).unwrap_or(u32::MAX)
            || span.submitted.is_empty()
            || span.submitted.end_sample > evidence.source.decoded_sample_count
            || span.continuation_boundary <= span.submitted.start_sample
            || span.continuation_boundary > span.submitted.end_sample
        {
            return Err(invalid("invalid decode-span coordinates"));
        }
        let hypothesis_ids = span
            .hypotheses
            .iter()
            .map(|segment| segment.id.as_str())
            .collect::<HashSet<_>>();
        if hypothesis_ids.len() != span.hypotheses.len()
            || span.hypotheses.iter().any(|segment| {
                segment.audio_range.is_empty()
                    || segment.audio_range.start_sample < span.submitted.start_sample
                    || segment.audio_range.end_sample > span.submitted.end_sample
            })
            || span
                .accepted_segment_ids
                .iter()
                .any(|id| !hypothesis_ids.contains(id.as_str()))
        {
            return Err(invalid("decode span accepts an unknown segment"));
        }
        let accepted_ids = span
            .accepted_segment_ids
            .iter()
            .map(String::as_str)
            .collect::<HashSet<_>>();
        if accepted_ids.len() != span.accepted_segment_ids.len()
            || span.hypotheses.iter().any(|segment| {
                accepted_ids.contains(segment.id.as_str())
                    && (segment.audio_range.start_sample < span.submitted.start_sample
                        || segment.audio_range.end_sample > span.continuation_boundary)
            })
        {
            return Err(invalid("invalid accepted segment in decode span"));
        }
        let mut used_segment_ids = HashSet::new();
        let mut last_span_chunk = None;
        for item in &span.content {
            match item {
                DecodeSpanItem::Chunk(chunk) => {
                    if chunk.id.is_empty()
                        || !seen_chunk_ids.insert(chunk.id.as_str())
                        || chunk.audio_range.is_empty()
                        || chunk.audio_range.start_sample < span.submitted.start_sample
                        || chunk.audio_range.end_sample > span.continuation_boundary
                        || previous_chunk_end
                            .is_some_and(|end| end > chunk.audio_range.start_sample)
                        || chunk.segment_ids.iter().any(|id| {
                            !accepted_ids.contains(id.as_str())
                                || !used_segment_ids.insert(id.as_str())
                        })
                        || chunk.segment_ids.is_empty()
                        || chunk.transcriptions.is_empty()
                        || chunk.current_transcription().is_none()
                        || chunk.transcriptions.iter().any(|transcription| {
                            transcription.chunk_id != chunk.id
                                || transcription.audio_range != chunk.audio_range
                                || transcription.boundary != chunk.boundary
                        })
                    {
                        return Err(invalid("invalid finalized chunk in decode-span content"));
                    }
                    previous_chunk_end = Some(chunk.audio_range.end_sample);
                    last_span_chunk = Some(chunk);
                    chunk_ids.push(chunk.id.clone());
                }
                DecodeSpanItem::ParagraphBreak(_) => {
                    if let Some(chunk_id) = chunk_ids.last() {
                        if breaks.last() != Some(chunk_id) {
                            breaks.push(chunk_id.clone());
                        }
                    }
                }
            }
        }
        if used_segment_ids != accepted_ids {
            return Err(invalid(
                "accepted decode-span text is lost or duplicated across chunks",
            ));
        }
        if let Some(chunk) = last_span_chunk {
            let expected_reason =
                if span.continuation_boundary == evidence.source.decoded_sample_count {
                    ChunkBoundaryReason::SourceEnd
                } else {
                    ChunkBoundaryReason::Continuation
                };
            if chunk.boundary.reason != expected_reason {
                return Err(invalid(
                    "last decode-span chunk has the wrong boundary reason",
                ));
            }
        }
        expected_start = span.continuation_boundary;
    }
    if evidence.source.decoded_sample_count != 0
        && evidence
            .decode_spans
            .last()
            .is_none_or(|span| span.continuation_boundary != evidence.source.decoded_sample_count)
    {
        return Err(invalid("decode spans do not reach source end"));
    }
    if let Some(final_chunk_id) = chunk_ids.last() {
        breaks.retain(|chunk_id| chunk_id != final_chunk_id);
    }
    Ok((chunk_ids, breaks))
}

fn validate_state(
    project: &Project,
    paragraphs: &[crate::document::Paragraph],
    chunks: &[crate::document::ChunkAudioMapping],
    mappings: &[crate::document::TokenAudioMapping],
    marks: &[crate::document::AttentionMark],
    issues: &[crate::document::ResolvedIssue],
    sources: &HashSet<&str>,
) -> Result<(), ProjectIoError> {
    let invalid = |message: &str| ProjectIoError::Invalid(message.into());
    let mut paragraph_ids = HashSet::new();
    let mut chunk_ids = HashSet::new();
    let mut token_ids = HashSet::new();
    let transcriptions = project.transcriptions();
    for p in paragraphs {
        if p.id().is_empty()
            || p.revision() == 0
            || !paragraph_ids.insert(p.id())
            || p.chunk_boundaries().is_empty()
        {
            return Err(invalid(
                "invalid paragraph identity, revision, or composition",
            ));
        }
        let mut start = 0;
        for c in p.chunk_boundaries() {
            if !chunk_ids.insert(c.chunk_id()) {
                return Err(invalid("duplicate chunk identity"));
            }
            let t = transcriptions
                .iter()
                .find(|t| t.id == c.transcription_id())
                .ok_or_else(|| invalid("composition refers to an unknown transcription"))?;
            if t.chunk_id != c.chunk_id() || t.text != c.text() {
                return Err(invalid(
                    "current text or chunk identity differs from its transcription",
                ));
            }
            let real = t
                .segments
                .iter()
                .flat_map(|s| {
                    s.tokens
                        .iter()
                        .enumerate()
                        .map(move |(i, token)| (s, i, token))
                })
                .filter(|(_, _, token)| !token.is_special)
                .collect::<Vec<_>>();
            let text = real
                .iter()
                .map(|(_, _, token)| token.text.as_str())
                .collect::<String>();
            let expected = if text == t.text { real.as_slice() } else { &[] };
            let current = p
                .tokens()
                .get(start..c.after_tokens())
                .ok_or_else(|| invalid("invalid chunk token bounds"))?;
            if current.len() != expected.len() {
                return Err(invalid("chunk exposes invented or missing tokens"));
            }
            for (token, (segment, index, real)) in current.iter().zip(expected) {
                if token.id().transcription_id != t.id
                    || token.id().segment_id != segment.id
                    || token.id().token_index != *index
                    || token.text() != real.text
                    || token.vocabulary_id() != real.token_id
                    || !token_ids.insert(token.id())
                {
                    return Err(invalid("text token differs from its Whisper evidence"));
                }
            }
            start = c.after_tokens();
        }
        if start != p.tokens().len() {
            return Err(invalid("tokens lie outside chunks"));
        }
    }
    let mut mapped_chunks = HashSet::new();
    for c in chunks {
        if !chunk_ids.contains(c.chunk_id())
            || !sources.contains(c.source_id())
            || !mapped_chunks.insert(c.chunk_id())
        {
            return Err(invalid(
                "chunk audio mapping has an unknown or duplicate target",
            ));
        }
        validate_audio_range(project, c.source_id(), c.range())?;
        if transcriptions
            .iter()
            .filter(|t| t.chunk_id == c.chunk_id())
            .any(|t| t.audio_range != c.range())
        {
            return Err(invalid("finalized chunk audio boundaries changed"));
        }
    }
    let mut mapped_tokens = HashSet::new();
    for m in mappings {
        let p = paragraphs
            .iter()
            .find(|p| p.id() == m.paragraph_id())
            .ok_or_else(|| invalid("unknown mapped paragraph"))?;
        if p.revision() != m.paragraph_revision()
            || !p.tokens().iter().any(|t| t.id() == m.token_identity())
            || !sources.contains(m.source_id())
            || !mapped_tokens.insert(m.token_identity())
        {
            return Err(invalid("unknown, stale, or duplicate token audio mapping"));
        }
        validate_audio_range(project, m.source_id(), m.range())?;
    }
    let mut marked = HashSet::new();
    for mark in marks {
        let valid = paragraphs.iter().any(|p| {
            let mut start = 0;
            p.chunk_boundaries().iter().any(|c| {
                let contains = c.chunk_id() == mark.chunk_id()
                    && p.tokens()[start..c.after_tokens()]
                        .iter()
                        .any(|t| t.id() == mark.token_identity());
                start = c.after_tokens();
                contains
            })
        });
        if !valid || !marked.insert(mark.token_identity()) {
            return Err(invalid("unknown or duplicate attention-mark target"));
        }
    }
    for issue in issues {
        if issue.token_identities().is_empty()
            || issue
                .token_identities()
                .iter()
                .any(|id| !token_ids.contains(id))
        {
            return Err(invalid("resolved issue refers to an unknown token"));
        }
    }
    Ok(())
}

fn validate_audio_range(
    project: &Project,
    source: &str,
    range: crate::chunking::SampleRange,
) -> Result<(), ProjectIoError> {
    if range.is_empty()
        || project
            .audio_source(source)
            .and_then(|s| s.canonical_sample_count())
            .is_some_and(|n| range.end_sample > n)
    {
        return Err(ProjectIoError::Invalid(
            "audio mapping exceeds source bounds or has an empty range".into(),
        ));
    }
    Ok(())
}
