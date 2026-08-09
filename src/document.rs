//! Visible document structure derived from replay chunks.

use crate::recognition::{ChunkBoundaryReason, RecognitionChunk};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    paragraphs: Vec<Paragraph>,
}

impl Document {
    pub fn from_chunks(chunks: &[RecognitionChunk]) -> Self {
        let mut paragraphs = Vec::new();
        let mut paragraph_chunks = Vec::new();

        for chunk in chunks {
            paragraph_chunks.push(chunk);
            if matches!(
                chunk.boundary.reason,
                ChunkBoundaryReason::LongPause | ChunkBoundaryReason::SourceEnd
            ) {
                paragraphs.push(Paragraph::from_chunks(&paragraph_chunks));
                paragraph_chunks.clear();
            }
        }

        if !paragraph_chunks.is_empty() {
            paragraphs.push(Paragraph::from_chunks(&paragraph_chunks));
        }

        Self { paragraphs }
    }

    pub fn paragraphs(&self) -> &[Paragraph] {
        &self.paragraphs
    }

    pub fn chunk_marker(&self, paragraph: usize, chunk: usize) -> Option<&ChunkBoundaryMarker> {
        self.paragraphs
            .get(paragraph.checked_sub(1)?)?
            .chunk_boundaries
            .get(chunk.checked_sub(1)?)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paragraph {
    text: String,
    chunk_boundaries: Vec<ChunkBoundaryMarker>,
}

impl Paragraph {
    fn from_chunks(chunks: &[&RecognitionChunk]) -> Self {
        let mut text = String::new();
        let mut chunk_boundaries = Vec::with_capacity(chunks.len());
        for chunk in chunks {
            text.push_str(&chunk.text);
            chunk_boundaries.push(ChunkBoundaryMarker {
                chunk_id: chunk.id.clone(),
                end_offset: text.len(),
            });
        }
        Self {
            text,
            chunk_boundaries,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn chunk_boundaries(&self) -> &[ChunkBoundaryMarker] {
        &self.chunk_boundaries
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkBoundaryMarker {
    chunk_id: String,
    end_offset: usize,
}

impl ChunkBoundaryMarker {
    pub fn chunk_id(&self) -> &str {
        &self.chunk_id
    }

    pub fn end_offset(&self) -> usize {
        self.end_offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{chunking::SampleRange, recognition::ChunkBoundary};

    fn chunk(id: &str, text: &str, reason: ChunkBoundaryReason) -> RecognitionChunk {
        RecognitionChunk {
            id: id.into(),
            ordinal: 1,
            segment_ids: Vec::new(),
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
        }
    }

    #[test]
    fn long_pauses_end_paragraphs_without_splitting_chunks() {
        let chunks = vec![
            chunk("a", "one", ChunkBoundaryReason::StrongPause),
            chunk("b", " two", ChunkBoundaryReason::LongPause),
            chunk("c", " three", ChunkBoundaryReason::SourceEnd),
        ];

        let document = Document::from_chunks(&chunks);

        assert_eq!(document.paragraphs().len(), 2);
        assert_eq!(document.paragraphs()[0].text(), "one two");
        assert_eq!(document.paragraphs()[1].text(), " three");
        assert_eq!(
            document.paragraphs()[0]
                .chunk_boundaries()
                .iter()
                .map(ChunkBoundaryMarker::chunk_id)
                .collect::<Vec<_>>(),
            vec!["a", "b"]
        );
        assert_eq!(document.chunk_marker(1, 2).unwrap().end_offset(), 7);
        assert!(document.chunk_marker(0, 1).is_none());
        assert!(document.chunk_marker(1, 0).is_none());
        assert!(document.chunk_marker(3, 1).is_none());
    }

    #[test]
    fn trailing_chunks_still_form_a_paragraph_when_source_end_is_missing() {
        let chunks = vec![chunk("a", "one", ChunkBoundaryReason::MaximumTokens)];

        let document = Document::from_chunks(&chunks);

        assert_eq!(document.paragraphs().len(), 1);
        assert_eq!(document.paragraphs()[0].text(), "one");
    }
}
