#![allow(dead_code)]

use running_drafts_editor::{
    chunking::{SampleRange, SourceFacts},
    project::Project,
    transcription::{
        AdvanceReason, Chunk, ChunkBoundary, ChunkBoundaryReason, DecodeSpan, DecodeSpanItem,
        DecodedSegment, DecoderTimestamps, EmptyPromptReason, InitialTranscriptionResult,
        TranscriberIdentity, Transcription, TranscriptionConfig, TranscriptionStatus, WhisperToken,
    },
};

pub fn batch(id: &str, texts: &[&str]) -> InitialTranscriptionResult {
    let segments = texts
        .iter()
        .enumerate()
        .map(|(index, text)| DecodedSegment {
            id: format!("s{index}"),
            raw_timestamps: Some(DecoderTimestamps {
                start: index as i64 * 100,
                end: (index as i64 + 1) * 100,
                samples_per_unit: 1,
            }),
            audio_range: Some(SampleRange {
                start_sample: index as u64 * 100,
                end_sample: (index as u64 + 1) * 100,
            }),
            text: (*text).into(),
            no_speech_probability: 0.1,
            tokens: vec![WhisperToken {
                token_id: index as i32 + 1,
                text: (*text).into(),
                probability: 0.1,
                is_special: false,
                raw_timestamps: Some(DecoderTimestamps {
                    start: index as i64 * 100,
                    end: (index as i64 + 1) * 100,
                    samples_per_unit: 1,
                }),
                audio_range: Some(SampleRange {
                    start_sample: index as u64 * 100,
                    end_sample: (index as u64 + 1) * 100,
                }),
                alternatives: Vec::new(),
            }],
        })
        .collect::<Vec<_>>();
    let source = SourceFacts {
        sha256: "11".repeat(32),
        sample_rate_hz: 16_000,
        channels: 1,
        decoded_sample_count: texts.len() as u64 * 100,
    };
    let transcriber = TranscriberIdentity {
        name: "synthetic Whisper decoder".into(),
        implementation: "test".into(),
        model_sha256: "22".repeat(32),
    };
    let config = TranscriptionConfig::default();
    let chunks = segments
        .iter()
        .enumerate()
        .map(|(index, segment)| {
            let chunk_id = format!("c{index}");
            let transcription_id = format!("{id}:{chunk_id}");
            let boundary = ChunkBoundary {
                reason: if index + 1 == texts.len() {
                    ChunkBoundaryReason::SourceEnd
                } else {
                    ChunkBoundaryReason::ScoredPause
                },
                pause_samples: None,
            };
            Chunk {
                id: chunk_id.clone(),
                ordinal: index as u32 + 1,
                segment_ids: vec![segment.id.clone()],
                audio_range: segment.audio_range.unwrap(),
                text: segment.text.clone(),
                token_count: 1,
                boundary: boundary.clone(),
                transcriptions: vec![Transcription {
                    id: transcription_id.clone(),
                    chunk_id,
                    previous_id: None,
                    text: segment.text.clone(),
                    source: source.clone(),
                    transcriber: transcriber.clone(),
                    config: config.clone(),
                    audio_range: segment.audio_range.unwrap(),
                    boundary,
                    segments: vec![segment.clone()],
                    prompt_token_ids: Vec::new(),
                    forced_token_ids: Vec::new(),
                }],
                current_transcription_id: transcription_id,
            }
        })
        .collect::<Vec<_>>();
    InitialTranscriptionResult {
        id: id.into(),
        revision: 1,
        source,
        transcriber,
        config,
        status: TranscriptionStatus::Succeeded,
        decode_spans: vec![DecodeSpan {
            ordinal: 1,
            submitted: SampleRange {
                start_sample: 0,
                end_sample: texts.len() as u64 * 100,
            },
            continuation_boundary: texts.len() as u64 * 100,
            prompt_token_ids: Vec::new(),
            prompt_limit: 223,
            prompt_omitted_token_count: 0,
            empty_prompt_reason: Some(EmptyPromptReason::FirstDecode),
            advance_reason: AdvanceReason::SourceEnd,
            continuation_pause_samples: None,
            hypotheses: segments.clone(),
            accepted_segment_ids: segments.iter().map(|s| s.id.clone()).collect(),
            content: chunks.into_iter().map(DecodeSpanItem::Chunk).collect(),
            error: None,
        }],
    }
}

pub fn project(texts: &[&str]) -> Project {
    Project::from_initial_transcription(&batch("initial", texts))
}

pub fn synchronize_initial_transcriptions(result: &mut InitialTranscriptionResult) {
    let source = result.source.clone();
    let transcriber = result.transcriber.clone();
    let config = result.config.clone();
    for chunk in result.chunks_mut() {
        for transcription in &mut chunk.transcriptions {
            transcription.source = source.clone();
            transcription.transcriber = transcriber.clone();
            transcription.config = config.clone();
            transcription.audio_range = chunk.audio_range;
            transcription.boundary = chunk.boundary.clone();
        }
    }
}

pub fn proposal(
    project: &Project,
    result: InitialTranscriptionResult,
) -> running_drafts_editor::transcription::Transcription {
    let current = project.current_transcription(1, 1).unwrap();
    result.transcription_for(
        result.chunks().next().unwrap(),
        &current.chunk_id,
        Some(current.id.clone()),
    )
}
