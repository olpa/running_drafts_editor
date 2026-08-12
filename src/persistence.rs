//! Versioned persistence for the authoritative visible document.

use std::{
    collections::HashSet,
    fs::{self, OpenOptions},
    io::{self, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
};

use crate::document::{Document, VisibleTokenId, DOCUMENT_SCHEMA};

#[derive(Debug, thiserror::Error)]
pub enum DocumentIoError {
    #[error("could not open document '{}': {source}", path.display())]
    Open { path: PathBuf, source: io::Error },
    #[error("could not read document '{}': {source}", path.display())]
    Read {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("unsupported document schema '{found}'; expected '{DOCUMENT_SCHEMA}'")]
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

pub fn load_document(path: &Path) -> Result<Document, DocumentIoError> {
    let file = fs::File::open(path).map_err(|source| DocumentIoError::Open {
        path: path.into(),
        source,
    })?;
    let document =
        serde_json::from_reader(BufReader::new(file)).map_err(|source| DocumentIoError::Read {
            path: path.into(),
            source,
        })?;
    validate(&document)?;
    Ok(document)
}

pub fn save_document(path: &Path, document: &Document) -> Result<(), DocumentIoError> {
    validate(document)?;
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
                return Err(DocumentIoError::Save {
                    path: path.into(),
                    source,
                })
            }
        }
    }
    let Some((temporary_path, file)) = temporary else {
        return Err(DocumentIoError::Save {
            path: path.into(),
            source: io::Error::new(
                io::ErrorKind::AlreadyExists,
                "no temporary filename available",
            ),
        });
    };
    let result = (|| {
        let mut writer = BufWriter::new(file);
        serde_json::to_writer_pretty(&mut writer, document).map_err(|source| {
            DocumentIoError::Encode {
                path: path.into(),
                source,
            }
        })?;
        writer
            .write_all(b"\n")
            .map_err(|source| DocumentIoError::Save {
                path: path.into(),
                source,
            })?;
        writer.flush().map_err(|source| DocumentIoError::Save {
            path: path.into(),
            source,
        })?;
        writer
            .get_ref()
            .sync_all()
            .map_err(|source| DocumentIoError::Save {
                path: path.into(),
                source,
            })?;
        fs::rename(&temporary_path, path).map_err(|source| DocumentIoError::Save {
            path: path.into(),
            source,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result
}

fn validate(document: &Document) -> Result<(), DocumentIoError> {
    if document.schema() != DOCUMENT_SCHEMA {
        return Err(DocumentIoError::UnsupportedSchema {
            found: document.schema().into(),
        });
    }
    if document.id().is_empty() {
        return Err(DocumentIoError::Invalid("document ID is empty".into()));
    }
    let mut paragraph_ids = HashSet::new();
    let mut token_ids = HashSet::new();
    let mut chunk_ids = HashSet::new();
    let mut evidence_ids = HashSet::new();
    for evidence in document.recognition_token_evidence() {
        if !matches!(evidence.token_id(), VisibleTokenId::Recognition { .. }) {
            return Err(DocumentIoError::Invalid(
                "recognition evidence refers to a pseudo-token".into(),
            ));
        }
        if !evidence_ids.insert(token_id_key(evidence.token_id())) {
            return Err(DocumentIoError::Invalid(
                "duplicate recognition token evidence".into(),
            ));
        }
    }
    for paragraph in document.paragraphs() {
        if !paragraph_ids.insert(paragraph.id()) {
            return Err(DocumentIoError::Invalid(format!(
                "duplicate paragraph ID '{}'",
                paragraph.id()
            )));
        }
        if paragraph.revision() == 0 {
            return Err(DocumentIoError::Invalid(format!(
                "paragraph '{}' has revision zero",
                paragraph.id()
            )));
        }
        for token in paragraph.tokens() {
            if !token_ids.insert(token_id_key(token.id())) {
                return Err(DocumentIoError::Invalid(
                    "duplicate visible token ID".into(),
                ));
            }
        }
        let mut previous = 0;
        for marker in paragraph.chunk_boundaries() {
            if marker.after_tokens() < previous || marker.after_tokens() > paragraph.tokens().len()
            {
                return Err(DocumentIoError::Invalid(format!(
                    "chunk marker '{}' has an invalid token position",
                    marker.chunk_id()
                )));
            }
            if !chunk_ids.insert(marker.chunk_id()) {
                return Err(DocumentIoError::Invalid(format!(
                    "duplicate chunk marker ID '{}'",
                    marker.chunk_id()
                )));
            }
            previous = marker.after_tokens();
        }
        if paragraph
            .chunk_boundaries()
            .last()
            .is_some_and(|m| m.after_tokens() != paragraph.tokens().len())
        {
            return Err(DocumentIoError::Invalid(format!(
                "paragraph '{}' does not end at a chunk boundary",
                paragraph.id()
            )));
        }
    }
    if !document.replay_chunks().is_empty() {
        let mut replay_chunk_ids = HashSet::new();
        for chunk in document.replay_chunks() {
            if chunk.id().is_empty() || !replay_chunk_ids.insert(chunk.id()) {
                return Err(DocumentIoError::Invalid(
                    "derived replay chunk IDs must be nonempty and unique".into(),
                ));
            }
        }
        if let Some(missing) = chunk_ids.iter().find(|id| !replay_chunk_ids.contains(*id)) {
            return Err(DocumentIoError::Invalid(format!(
                "chunk marker '{missing}' has no replay chunk record"
            )));
        }
        for paragraph in document.paragraphs() {
            let mut start = 0;
            for marker in paragraph.chunk_boundaries() {
                let chunk = document
                    .replay_chunks()
                    .iter()
                    .find(|chunk| chunk.id() == marker.chunk_id())
                    .expect("current marker record was checked above");
                let expected = paragraph.tokens()[start..marker.after_tokens()]
                    .iter()
                    .map(|token| token.id())
                    .collect::<Vec<_>>();
                if chunk.token_ids().iter().collect::<Vec<_>>() != expected {
                    return Err(DocumentIoError::Invalid(format!(
                        "replay chunk '{}' has stale token membership",
                        chunk.id()
                    )));
                }
                start = marker.after_tokens();
            }
        }
    }
    let source_ids = document
        .audio_sources()
        .iter()
        .map(|s| s.id())
        .collect::<HashSet<_>>();
    let mut mapped_chunks = HashSet::new();
    for mapping in document.chunk_audio_mappings() {
        if !chunk_ids.contains(mapping.chunk_id()) {
            return Err(DocumentIoError::Invalid(format!(
                "audio mapping refers to unknown chunk '{}'",
                mapping.chunk_id()
            )));
        }
        if !source_ids.contains(mapping.source_id()) {
            return Err(DocumentIoError::Invalid(format!(
                "audio mapping refers to unknown source '{}'",
                mapping.source_id()
            )));
        }
        if mapping.range().start_sample >= mapping.range().end_sample {
            return Err(DocumentIoError::Invalid(format!(
                "audio mapping for '{}' has an empty or reversed range",
                mapping.chunk_id()
            )));
        }
        if !mapped_chunks.insert(mapping.chunk_id()) {
            return Err(DocumentIoError::Invalid(format!(
                "chunk '{}' has more than one audio mapping",
                mapping.chunk_id()
            )));
        }
    }
    let mut mapped_tokens = HashSet::new();
    for mapping in document.token_audio_mappings() {
        let Some(paragraph) = document
            .paragraphs()
            .iter()
            .find(|paragraph| paragraph.id() == mapping.paragraph_id())
        else {
            return Err(DocumentIoError::Invalid(format!(
                "token audio mapping refers to unknown paragraph '{}'",
                mapping.paragraph_id()
            )));
        };
        if paragraph.revision() != mapping.paragraph_revision() {
            return Err(DocumentIoError::Invalid(format!(
                "token audio mapping for paragraph '{}' has a stale revision",
                mapping.paragraph_id()
            )));
        }
        let key = token_id_key(mapping.token_id());
        if !paragraph
            .tokens()
            .iter()
            .any(|token| token_id_key(token.id()) == key)
        {
            return Err(DocumentIoError::Invalid(
                "token audio mapping refers to an unknown visible token".into(),
            ));
        }
        if !source_ids.contains(mapping.source_id()) {
            return Err(DocumentIoError::Invalid(format!(
                "token audio mapping refers to unknown source '{}'",
                mapping.source_id()
            )));
        }
        if mapping.range().start_sample >= mapping.range().end_sample {
            return Err(DocumentIoError::Invalid(
                "token audio mapping has an empty or reversed range".into(),
            ));
        }
        if !mapped_tokens.insert(key) {
            return Err(DocumentIoError::Invalid(
                "visible token has more than one audio mapping".into(),
            ));
        }
    }
    Ok(())
}

fn token_id_key(id: &VisibleTokenId) -> String {
    match id {
        VisibleTokenId::Recognition {
            run_id,
            segment_id,
            token_index,
        } => format!("recognition\0{run_id}\0{segment_id}\0{token_index}"),
        VisibleTokenId::Pseudo { id } => format!("pseudo\0{id}"),
    }
}
