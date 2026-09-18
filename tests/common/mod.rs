#![allow(dead_code)]

use running_drafts_editor::{
    chunking::{SampleRange, SourceFacts},
    project::Project,
    transcription::{
        AdvanceReason, ChunkBoundary, ChunkBoundaryReason, DecodedSegment,
        InitialTranscriptionResult, ProvisionalChunkEvidence, TranscriberIdentity,
        TranscriptionChunk, TranscriptionConfig, TranscriptionStatus, WhisperToken,
    },
};

pub fn batch(id: &str, texts: &[&str]) -> InitialTranscriptionResult {
    let segments = texts
        .iter()
        .enumerate()
        .map(|(index, text)| DecodedSegment {
            id: format!("s{index}"),
            audio_range: SampleRange {
                start_sample: index as u64 * 100,
                end_sample: (index as u64 + 1) * 100,
            },
            text: (*text).into(),
            no_speech_probability: 0.1,
            tokens: vec![WhisperToken {
                token_id: index as i32 + 1,
                text: (*text).into(),
                probability: 0.1,
                is_special: false,
                audio_range: Some(SampleRange {
                    start_sample: index as u64 * 100,
                    end_sample: (index as u64 + 1) * 100,
                }),
                alternatives: Vec::new(),
            }],
        })
        .collect::<Vec<_>>();
    InitialTranscriptionResult {
        id: id.into(),
        revision: 1,
        source: SourceFacts {
            sha256: "11".repeat(32),
            sample_rate_hz: 16_000,
            channels: 1,
            decoded_sample_count: texts.len() as u64 * 100,
        },
        transcriber: TranscriberIdentity {
            name: "synthetic Whisper decoder".into(),
            implementation: "test".into(),
            model_sha256: "22".repeat(32),
        },
        config: TranscriptionConfig::default(),
        status: TranscriptionStatus::Succeeded,
        chunks: segments
            .iter()
            .enumerate()
            .map(|(index, s)| TranscriptionChunk {
                id: format!("c{index}"),
                ordinal: index as u32 + 1,
                segment_ids: vec![s.id.clone()],
                audio_range: s.audio_range,
                text: s.text.clone(),
                token_count: 1,
                boundary: ChunkBoundary {
                    reason: if index + 1 == texts.len() {
                        ChunkBoundaryReason::SourceEnd
                    } else {
                        ChunkBoundaryReason::ScoredPause
                    },
                    pause_samples: None,
                },
            })
            .collect(),
        windows: vec![ProvisionalChunkEvidence {
            ordinal: 1,
            submitted: SampleRange {
                start_sample: 0,
                end_sample: texts.len() as u64 * 100,
            },
            core: SampleRange {
                start_sample: 0,
                end_sample: texts.len() as u64 * 100,
            },
            prompt_token_ids: Vec::new(),
            advance_reason: AdvanceReason::SourceEnd,
            hypotheses: segments.clone(),
            accepted_segment_ids: segments.iter().map(|s| s.id.clone()).collect(),
            error: None,
        }],
        segments,
    }
}

pub fn project(texts: &[&str]) -> Project {
    Project::from_initial_transcription(&batch("initial", texts))
}

pub fn proposal(
    project: &Project,
    result: InitialTranscriptionResult,
) -> running_drafts_editor::transcription::Transcription {
    let current = project.current_transcription(1, 1).unwrap();
    result.transcription_for(
        &result.chunks[0],
        &current.chunk_id,
        Some(current.id.clone()),
    )
}
