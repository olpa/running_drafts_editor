//! Immutable Whisper transcription evidence and bounded decode-span orchestration.

use std::{fs::File, io::Read, path::Path, sync::Arc};

use hfvc_lib::{InteractiveSession, SessionConfig, Transcription as DecoderTranscription};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState,
};

use crate::chunking::{SampleRange, SourceFacts};

const WHISPER_SAMPLE_RATE_HZ: u32 = 16_000;
const WHISPER_MAX_SAMPLES: u64 = 480_000;
const SAMPLES_PER_CENTISECOND: u64 = 160;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptionConfig {
    pub decode_span_samples: u64,
    pub continuation_search_samples: u64,
    pub continuation_strong_pause_ms: u64,
    pub language: String,
    pub threads: usize,
    pub top_candidates: usize,
    pub chunking: ChunkConstructionConfig,
}

impl Default for TranscriptionConfig {
    fn default() -> Self {
        Self {
            decode_span_samples: 480_000,
            continuation_search_samples: 96_000,
            continuation_strong_pause_ms: 800,
            language: "auto".into(),
            threads: 4,
            top_candidates: 20,
            chunking: ChunkConstructionConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkConstructionConfig {
    pub minimum_tokens: usize,
    pub target_tokens: usize,
    pub maximum_tokens: usize,
    pub usable_pause_ms: u64,
    pub strong_pause_ms: u64,
    pub long_pause_ms: u64,
    pub distance_penalty_ms: u64,
}

impl Default for ChunkConstructionConfig {
    fn default() -> Self {
        Self {
            minimum_tokens: 8,
            target_tokens: 32,
            maximum_tokens: 64,
            usable_pause_ms: 300,
            strong_pause_ms: 800,
            long_pause_ms: 2_000,
            distance_penalty_ms: 20,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriberIdentity {
    pub name: String,
    pub implementation: String,
    pub model_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptionStatus {
    Succeeded,
    Partial,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdvanceReason {
    StrongPause,
    LatestTimestamp,
    EarlyTimestampFallback,
    SourceEnd,
    NoUsableTimestamp,
    DecodeFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecoderTimestamps {
    pub start: i64,
    pub end: i64,
    pub samples_per_unit: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TokenAlternative {
    pub token_id: i32,
    pub text: String,
    pub probability: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WhisperToken {
    pub token_id: i32,
    pub text: String,
    pub probability: f32,
    pub is_special: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_timestamps: Option<DecoderTimestamps>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_range: Option<SampleRange>,
    pub alternatives: Vec<TokenAlternative>,
}

impl WhisperToken {
    pub fn probability(&self) -> f32 {
        self.probability
    }
}
impl TokenAlternative {
    pub fn token_id(&self) -> i32 {
        self.token_id
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn probability(&self) -> f32 {
        self.probability
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecodedSegment {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_timestamps: Option<DecoderTimestamps>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_range: Option<SampleRange>,
    pub text: String,
    pub no_speech_probability: f32,
    pub tokens: Vec<WhisperToken>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChunkBoundaryReason {
    LongPause,
    StrongPause,
    ScoredPause,
    MaximumTokens,
    Continuation,
    SourceEnd,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkBoundary {
    pub reason: ChunkBoundaryReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pause_samples: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chunk {
    pub id: String,
    pub ordinal: u32,
    pub segment_ids: Vec<String>,
    pub audio_range: SampleRange,
    pub text: String,
    pub token_count: usize,
    pub boundary: ChunkBoundary,
    pub transcriptions: Vec<Transcription>,
    pub current_transcription_id: String,
}

impl Chunk {
    pub fn current_transcription(&self) -> Option<&Transcription> {
        self.transcriptions
            .iter()
            .find(|transcription| transcription.id == self.current_transcription_id)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParagraphBreak;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum DecodeSpanItem {
    Chunk(Chunk),
    ParagraphBreak(ParagraphBreak),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecodeSpan {
    pub ordinal: u32,
    pub submitted: SampleRange,
    pub continuation_boundary: u64,
    pub prompt_token_ids: Vec<i32>,
    pub prompt_limit: usize,
    pub prompt_omitted_token_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub empty_prompt_reason: Option<EmptyPromptReason>,
    pub advance_reason: AdvanceReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub continuation_pause_samples: Option<u64>,
    pub hypotheses: Vec<DecodedSegment>,
    pub accepted_segment_ids: Vec<String>,
    pub content: Vec<DecodeSpanItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmptyPromptReason {
    FirstDecode,
    NoAcceptedText,
    ResetAfterDecodeFailure,
    ResetAfterUnknownGap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InitialTranscriptionResult {
    pub id: String,
    pub revision: u64,
    pub source: SourceFacts,
    pub transcriber: TranscriberIdentity,
    pub config: TranscriptionConfig,
    pub status: TranscriptionStatus,
    pub decode_spans: Vec<DecodeSpan>,
}

/// Project-owned evidence from the initial process, including failed decoding.
/// This is not a transcription: it may support several finalized chunks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InitialTranscriptionEvidence {
    pub id: String,
    pub source: SourceFacts,
    pub transcriber: TranscriberIdentity,
    pub config: TranscriptionConfig,
    pub status: TranscriptionStatus,
    pub decode_spans: Vec<DecodeSpan>,
}

/// One immutable proposal for one finalized chunk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transcription {
    pub id: String,
    pub chunk_id: String,
    pub previous_id: Option<String>,
    pub text: String,
    pub source: SourceFacts,
    pub transcriber: TranscriberIdentity,
    pub config: TranscriptionConfig,
    pub audio_range: SampleRange,
    pub boundary: ChunkBoundary,
    pub segments: Vec<DecodedSegment>,
    pub prompt_token_ids: Vec<i32>,
    pub forced_token_ids: Vec<i32>,
}

impl InitialTranscriptionResult {
    pub fn evidence(&self) -> InitialTranscriptionEvidence {
        InitialTranscriptionEvidence {
            id: self.id.clone(),
            source: self.source.clone(),
            transcriber: self.transcriber.clone(),
            config: self.config.clone(),
            status: self.status,
            decode_spans: self.decode_spans.clone(),
        }
    }

    pub fn chunks(&self) -> impl Iterator<Item = &Chunk> {
        self.decode_spans.iter().flat_map(|span| {
            span.content.iter().filter_map(|item| match item {
                DecodeSpanItem::Chunk(chunk) => Some(chunk),
                DecodeSpanItem::ParagraphBreak(_) => None,
            })
        })
    }

    pub fn chunks_mut(&mut self) -> impl Iterator<Item = &mut Chunk> {
        self.decode_spans.iter_mut().flat_map(|span| {
            span.content.iter_mut().filter_map(|item| match item {
                DecodeSpanItem::Chunk(chunk) => Some(chunk),
                DecodeSpanItem::ParagraphBreak(_) => None,
            })
        })
    }

    pub fn accepted_segments(&self) -> impl Iterator<Item = &DecodedSegment> {
        self.decode_spans.iter().flat_map(|span| {
            span.hypotheses
                .iter()
                .filter(|segment| span.accepted_segment_ids.iter().any(|id| id == &segment.id))
        })
    }

    pub fn chunk_count(&self) -> usize {
        self.chunks().count()
    }

    pub fn namespace_chunks(&mut self, recording_id: &str) {
        let run_id = self.id.clone();
        for chunk in self.chunks_mut() {
            let chunk_id = format!("chunk:{recording_id}:{run_id}:{}", chunk.ordinal);
            chunk.id.clone_from(&chunk_id);
            for transcription in &mut chunk.transcriptions {
                transcription.chunk_id.clone_from(&chunk_id);
                if transcription.previous_id.is_none() {
                    transcription.id = format!("{run_id}:{chunk_id}");
                }
            }
            chunk.current_transcription_id = chunk
                .transcriptions
                .first()
                .map(|transcription| transcription.id.clone())
                .unwrap_or_default();
        }
    }

    fn initialize_chunk_transcriptions(&mut self) {
        let run_id = self.id.clone();
        let source = self.source.clone();
        let transcriber = self.transcriber.clone();
        let config = self.config.clone();
        for span in &mut self.decode_spans {
            let prompt_token_ids = span.prompt_token_ids.clone();
            let hypotheses = span.hypotheses.clone();
            for item in &mut span.content {
                let DecodeSpanItem::Chunk(chunk) = item else {
                    continue;
                };
                let transcription = Transcription {
                    id: format!("{run_id}:{}", chunk.id),
                    chunk_id: chunk.id.clone(),
                    previous_id: None,
                    text: chunk.text.clone(),
                    source: source.clone(),
                    transcriber: transcriber.clone(),
                    config: config.clone(),
                    audio_range: chunk.audio_range,
                    boundary: chunk.boundary.clone(),
                    segments: hypotheses
                        .iter()
                        .filter(|segment| chunk.segment_ids.contains(&segment.id))
                        .cloned()
                        .collect(),
                    prompt_token_ids: prompt_token_ids.clone(),
                    forced_token_ids: Vec::new(),
                };
                chunk.current_transcription_id = transcription.id.clone();
                chunk.transcriptions.push(transcription);
            }
        }
    }

    pub fn transcription_for(
        &self,
        chunk: &Chunk,
        chunk_id: &str,
        previous_id: Option<String>,
    ) -> Transcription {
        Transcription {
            id: format!("{}:{}", self.id, chunk.id),
            chunk_id: chunk_id.into(),
            previous_id,
            text: chunk.text.clone(),
            source: self.source.clone(),
            transcriber: self.transcriber.clone(),
            config: self.config.clone(),
            audio_range: chunk.audio_range,
            boundary: chunk.boundary.clone(),
            segments: self
                .decode_spans
                .iter()
                .flat_map(|span| &span.hypotheses)
                .filter(|s| chunk.segment_ids.contains(&s.id))
                .cloned()
                .collect(),
            prompt_token_ids: self
                .decode_spans
                .iter()
                .find(|span| {
                    span.content.iter().any(|item| {
                        matches!(item, DecodeSpanItem::Chunk(candidate) if candidate.id == chunk.id)
                    })
                })
                .map(|span| span.prompt_token_ids.clone())
                .unwrap_or_default(),
            forced_token_ids: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChunkTranscriptionRequest {
    pub chunk_id: String,
    pub previous_id: String,
    pub source: SourceFacts,
    pub chunk_range: SampleRange,
    pub language: String,
    pub forced_tokens: Vec<i32>,
    pub revision: u64,
}

/// Decoder seam for correction and transaction tests. Tokens always use the
/// selected Whisper vocabulary, including tokens forced by an edit.
pub trait ChunkTranscriber {
    fn tokenize(&self, text: &str) -> Result<Vec<i32>, TranscriptionError>;
    fn render_tokens(&self, tokens: &[i32]) -> Result<String, TranscriptionError>;
    fn beginning_timestamp_token(&self) -> i32;
    fn transcribe_chunk(
        &mut self,
        request: ChunkTranscriptionRequest,
        samples: &[f32],
    ) -> Result<Transcription, TranscriptionError>;
}

impl ChunkTranscriber for TranscriberSession {
    fn tokenize(&self, text: &str) -> Result<Vec<i32>, TranscriptionError> {
        self.tokenize(text)
    }
    fn render_tokens(&self, tokens: &[i32]) -> Result<String, TranscriptionError> {
        self.render_tokens(tokens)
    }
    fn beginning_timestamp_token(&self) -> i32 {
        self.beginning_timestamp_token()
    }
    fn transcribe_chunk(
        &mut self,
        request: ChunkTranscriptionRequest,
        samples: &[f32],
    ) -> Result<Transcription, TranscriptionError> {
        self.transcribe_chunk(request, samples)
    }
}

/// Owns one model context and at most one exact-range decode cache.
/// `InteractiveSession` contains audio- and history-specific Whisper state, so
/// a cache must never be reused for a different source, range, or language.
pub struct TranscriberSession {
    chunk_decode_cache: Option<ChunkDecodeCache>,
    decoder: WhisperDecoder,
    model_path: std::path::PathBuf,
}

struct ChunkDecodeCache {
    source_sha256: String,
    range: SampleRange,
    language: String,
    session: InteractiveSession,
}

impl TranscriberSession {
    pub fn load(model: &Path, config: &TranscriptionConfig) -> Result<Self, TranscriptionError> {
        Ok(Self {
            chunk_decode_cache: None,
            decoder: WhisperDecoder::load(model, config)?,
            model_path: model.into(),
        })
    }

    pub fn from_decoder(decoder: WhisperDecoder, model_path: &Path) -> Self {
        Self {
            chunk_decode_cache: None,
            decoder,
            model_path: model_path.into(),
        }
    }

    pub fn model_path(&self) -> &Path {
        &self.model_path
    }
    pub fn language(&self) -> &str {
        &self.decoder.language
    }
    pub fn set_language(&mut self, language: String) {
        self.decoder.language = language;
    }

    pub fn tokenize(&self, text: &str) -> Result<Vec<i32>, TranscriptionError> {
        let maximum = text.len().saturating_add(256).max(256);
        self.decoder
            .context
            .tokenize(text, maximum)
            .map_err(|error| TranscriptionError::Model(error.to_string()))
    }

    pub fn render_tokens(&self, tokens: &[i32]) -> Result<String, TranscriptionError> {
        tokens
            .iter()
            .map(|id| {
                self.decoder
                    .context
                    .token_to_string(*id)
                    .map_err(|error| TranscriptionError::Model(error.to_string()))
            })
            .collect()
    }

    pub fn beginning_timestamp_token(&self) -> i32 {
        self.decoder.context.token_beg()
    }

    pub fn transcribe_chunk(
        &mut self,
        request: ChunkTranscriptionRequest,
        samples: &[f32],
    ) -> Result<Transcription, TranscriptionError> {
        if request.source.sample_rate_hz != WHISPER_SAMPLE_RATE_HZ
            || request.source.channels != 1
            || request.source.decoded_sample_count != samples.len() as u64
            || request.chunk_range.is_empty()
            || request.chunk_range.end_sample > request.source.decoded_sample_count
            || request.chunk_range.len() > 480_000
        {
            return Err(TranscriptionError::InvalidConfiguration(
                "invalid existing chunk audio range".into(),
            ));
        }
        let start = usize::try_from(request.chunk_range.start_sample)
            .map_err(|_| TranscriptionError::AudioTooLong)?;
        let end = usize::try_from(request.chunk_range.end_sample)
            .map_err(|_| TranscriptionError::AudioTooLong)?;
        self.decoder.language = request.language.clone();
        let cache_matches = self.chunk_decode_cache.as_ref().is_some_and(|cached| {
            cached.source_sha256 == request.source.sha256
                && cached.range == request.chunk_range
                && cached.language == request.language
        });
        if !cache_matches {
            let mut config = SessionConfig::default()
                .with_threads(self.decoder.threads)
                .with_top_candidates(self.decoder.top_candidates)
                .with_greedy(1)
                .with_token_timestamps(true);
            if request.language != "auto" {
                config = config.with_language(request.language.clone());
            }
            let mut session =
                InteractiveSession::new_with_context(Arc::clone(&self.decoder.context), config)
                    .map_err(|error| TranscriptionError::Model(error.to_string()))?;
            session
                .load_audio(&samples[start..end])
                .map_err(|error| TranscriptionError::Model(error.to_string()))?;
            self.chunk_decode_cache = Some(ChunkDecodeCache {
                source_sha256: request.source.sha256.clone(),
                range: request.chunk_range,
                language: request.language.clone(),
                session,
            });
        }
        let cached = self
            .chunk_decode_cache
            .as_mut()
            .expect("chunk cache was initialized");
        let transcription = if request.forced_tokens.is_empty() {
            if cache_matches {
                cached
                    .session
                    .reset()
                    .map_err(|error| TranscriptionError::Model(error.to_string()))?
                    .clone()
            } else {
                cached
                    .session
                    .transcription()
                    .expect("new interactive session has a transcription")
                    .clone()
            }
        } else {
            cached
                .session
                .force_prefix_tokens(&request.forced_tokens)
                .map_err(|error| TranscriptionError::Model(error.to_string()))?
                .clone()
        };
        let relative = self
            .decoder
            .interactive_segments(&cached.session, &transcription)?;
        let segments = normalize_segments(relative, request.chunk_range, 1);
        let chunk_text = segments
            .iter()
            .flat_map(|s| &s.tokens)
            .filter(|t| !t.is_special)
            .map(|t| t.text.as_str())
            .collect::<String>();
        let segment_ids = segments.iter().map(|s| s.id.clone()).collect::<Vec<_>>();
        let chunk = Chunk {
            id: "decoded-chunk".into(),
            ordinal: 1,
            segment_ids,
            audio_range: request.chunk_range,
            text: chunk_text,
            token_count: segments
                .iter()
                .flat_map(|s| &s.tokens)
                .filter(|t| !t.is_special)
                .count(),
            boundary: ChunkBoundary {
                reason: ChunkBoundaryReason::SourceEnd,
                pause_samples: None,
            },
            transcriptions: Vec::new(),
            current_transcription_id: String::new(),
        };
        let config = TranscriptionConfig {
            language: request.language,
            threads: self.decoder.threads,
            top_candidates: self.decoder.top_candidates,
            ..TranscriptionConfig::default()
        };
        let span = DecodeSpan {
            ordinal: 1,
            submitted: request.chunk_range,
            continuation_boundary: request.chunk_range.end_sample,
            prompt_token_ids: Vec::new(),
            prompt_limit: 0,
            prompt_omitted_token_count: 0,
            empty_prompt_reason: Some(EmptyPromptReason::NoAcceptedText),
            advance_reason: AdvanceReason::SourceEnd,
            continuation_pause_samples: None,
            hypotheses: segments.clone(),
            accepted_segment_ids: segments.iter().map(|s| s.id.clone()).collect(),
            content: vec![DecodeSpanItem::Chunk(chunk)],
            error: None,
        };
        let transcriber = self.decoder.identity.clone();
        let base_id = run_id(
            &request.source,
            &transcriber,
            &config,
            std::slice::from_ref(&span),
        );
        let id = format!("{base_id}-r{}", request.revision);
        let mut result = InitialTranscriptionResult {
            id,
            revision: request.revision,
            source: request.source,
            transcriber,
            config,
            status: TranscriptionStatus::Succeeded,
            decode_spans: vec![span],
        };
        result.initialize_chunk_transcriptions();
        let chunk = result.chunks().next().expect("one correction chunk");
        let mut transcription =
            result.transcription_for(chunk, &request.chunk_id, Some(request.previous_id));
        transcription.forced_token_ids = request.forced_tokens;
        Ok(transcription)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecodeSpanSegment {
    pub raw_timestamps: Option<DecoderTimestamps>,
    pub text: String,
    pub no_speech_probability: f32,
    pub tokens: Vec<WhisperToken>,
}

pub trait DecodeSpanDecoder {
    fn identity(&self) -> TranscriberIdentity;
    fn prompt_capacity(&self) -> usize;
    fn decode(
        &mut self,
        audio: &[f32],
        prompt_token_ids: &[i32],
    ) -> Result<Vec<DecodeSpanSegment>, String>;
}

#[derive(Debug, thiserror::Error)]
pub enum TranscriptionError {
    #[error("invalid transcription configuration: {0}")]
    InvalidConfiguration(String),
    #[error("audio sample count does not fit this platform")]
    AudioTooLong,
    #[error("could not load Whisper model: {0}")]
    Model(String),
    #[error("could not read Whisper model: {0}")]
    ModelRead(#[from] std::io::Error),
}

pub fn transcribe_initial<D: DecodeSpanDecoder>(
    source: SourceFacts,
    samples: &[f32],
    config: TranscriptionConfig,
    decoder: &mut D,
) -> Result<InitialTranscriptionResult, TranscriptionError> {
    validate_config(&source, samples, &config)?;
    let total = source.decoded_sample_count;
    let mut cursor = 0_u64;
    let mut decode_spans = Vec::new();
    let mut prompt_history = Vec::new();
    let mut next_empty_prompt_reason = Some(EmptyPromptReason::FirstDecode);
    let mut failures = 0_usize;
    let mut unplaceable_text = false;
    let mut next_chunk_ordinal = 1_u32;
    let mut previous_chunk_end = None;
    let mut previous_accepted_end = None;

    while cursor < total {
        let submitted_start = cursor;
        let submitted_end = cursor.saturating_add(config.decode_span_samples).min(total);
        let submitted = SampleRange {
            start_sample: submitted_start,
            end_sample: submitted_end,
        };
        let start =
            usize::try_from(submitted_start).map_err(|_| TranscriptionError::AudioTooLong)?;
        let end = usize::try_from(submitted_end).map_err(|_| TranscriptionError::AudioTooLong)?;
        let ordinal = u32::try_from(decode_spans.len() + 1).unwrap_or(u32::MAX);
        let prompt_limit = decoder.prompt_capacity();
        let prompt_omitted_token_count = prompt_history.len().saturating_sub(prompt_limit);
        let span_prompt_token_ids = prompt_history[prompt_omitted_token_count..].to_vec();
        let empty_prompt_reason = span_prompt_token_ids.is_empty().then(|| {
            next_empty_prompt_reason
                .take()
                .unwrap_or(EmptyPromptReason::NoAcceptedText)
        });

        let decoded = decoder.decode(&samples[start..end], &span_prompt_token_ids);
        let (hypotheses, decision, error) = match decoded {
            Ok(relative) => {
                let hypotheses = normalize_segments(relative, submitted, ordinal);
                let decision = choose_continuation_boundary(submitted, total, &hypotheses, &config);
                unplaceable_text |= hypotheses
                    .iter()
                    .skip(decision.valid_prefix_len)
                    .any(segment_has_text_evidence);
                (hypotheses, decision, None)
            }
            Err(error) => {
                failures += 1;
                prompt_history.clear();
                next_empty_prompt_reason = Some(EmptyPromptReason::ResetAfterDecodeFailure);
                (
                    Vec::new(),
                    BoundaryDecision {
                        boundary: submitted_end,
                        reason: AdvanceReason::DecodeFailure,
                        pause_samples: None,
                        accepted_len: 0,
                        valid_prefix_len: 0,
                    },
                    Some(error),
                )
            }
        };

        let mut accepted_ids = Vec::new();
        let accepted = hypotheses
            .iter()
            .take(decision.accepted_len)
            .cloned()
            .collect::<Vec<_>>();
        if let (Some(previous_end), Some(first)) = (previous_accepted_end, accepted.first()) {
            let first_range = first
                .audio_range
                .expect("accepted segments always have a canonical range");
            let pause_samples = first_range.start_sample.saturating_sub(previous_end);
            let pause_ms = pause_samples.saturating_mul(1_000) / u64::from(source.sample_rate_hz);
            if pause_ms >= config.chunking.long_pause_ms {
                append_paragraph_break_after_last_chunk(&mut decode_spans);
            }
        }
        for segment in &accepted {
            accepted_ids.push(segment.id.clone());
            // Preserve the exact accepted token sequence. A text round trip
            // could retokenize it, while special tokens belong to this
            // decode span's control and timestamp context.
            prompt_history.extend(
                segment
                    .tokens
                    .iter()
                    .filter(|token| !token.is_special)
                    .map(|token| token.token_id),
            );
        }
        if let Some(last) = accepted.last() {
            previous_accepted_end = last.audio_range.map(|range| range.end_sample);
        }
        if error.is_none()
            && decision.reason == AdvanceReason::NoUsableTimestamp
            && accepted.is_empty()
        {
            prompt_history.clear();
            next_empty_prompt_reason = Some(EmptyPromptReason::ResetAfterUnknownGap);
        }

        let terminal_reason = if decision.boundary == total {
            ChunkBoundaryReason::SourceEnd
        } else {
            ChunkBoundaryReason::Continuation
        };
        let chunks = build_chunks(
            &accepted,
            source.sample_rate_hz,
            &config.chunking,
            terminal_reason,
            &mut next_chunk_ordinal,
            &mut previous_chunk_end,
        );
        let mut content = Vec::new();
        for chunk in chunks {
            let paragraph_break = chunk.boundary.reason == ChunkBoundaryReason::LongPause;
            content.push(DecodeSpanItem::Chunk(chunk));
            if paragraph_break {
                content.push(DecodeSpanItem::ParagraphBreak(ParagraphBreak));
            }
        }

        decode_spans.push(DecodeSpan {
            ordinal,
            submitted,
            continuation_boundary: decision.boundary,
            prompt_token_ids: span_prompt_token_ids,
            prompt_limit,
            prompt_omitted_token_count,
            empty_prompt_reason,
            advance_reason: decision.reason,
            continuation_pause_samples: decision.pause_samples,
            hypotheses,
            accepted_segment_ids: accepted_ids,
            content,
            error,
        });
        cursor = decision.boundary;
    }

    let status = if failures == decode_spans.len() {
        TranscriptionStatus::Failed
    } else if failures == 0 && !unplaceable_text {
        TranscriptionStatus::Succeeded
    } else {
        TranscriptionStatus::Partial
    };
    let transcriber = decoder.identity();
    let id = run_id(&source, &transcriber, &config, &decode_spans);
    let mut result = InitialTranscriptionResult {
        id,
        revision: 1,
        source,
        transcriber,
        config,
        status,
        decode_spans,
    };
    result.initialize_chunk_transcriptions();
    Ok(result)
}

fn append_paragraph_break_after_last_chunk(decode_spans: &mut [DecodeSpan]) {
    for span in decode_spans.iter_mut().rev() {
        let Some(index) = span
            .content
            .iter()
            .rposition(|item| matches!(item, DecodeSpanItem::Chunk(_)))
        else {
            continue;
        };
        if !matches!(
            span.content.get(index + 1),
            Some(DecodeSpanItem::ParagraphBreak(_))
        ) {
            span.content
                .insert(index + 1, DecodeSpanItem::ParagraphBreak(ParagraphBreak));
        }
        return;
    }
}

fn validate_config(
    source: &SourceFacts,
    samples: &[f32],
    config: &TranscriptionConfig,
) -> Result<(), TranscriptionError> {
    if source.sample_rate_hz != WHISPER_SAMPLE_RATE_HZ || source.channels != 1 {
        return Err(TranscriptionError::InvalidConfiguration(
            "Whisper input must be canonical mono 16 kHz audio".into(),
        ));
    }
    if source.decoded_sample_count != u64::try_from(samples.len()).unwrap_or(u64::MAX) {
        return Err(TranscriptionError::InvalidConfiguration(
            "source facts do not match decoded samples".into(),
        ));
    }
    if config.decode_span_samples == 0 || config.decode_span_samples > WHISPER_MAX_SAMPLES {
        return Err(TranscriptionError::InvalidConfiguration(
            "decode span must be between 1 and 480000 samples".into(),
        ));
    }
    if config.continuation_search_samples >= config.decode_span_samples {
        return Err(TranscriptionError::InvalidConfiguration(
            "continuation search must be shorter than a decode span".into(),
        ));
    }
    let chunking = &config.chunking;
    if chunking.minimum_tokens == 0
        || chunking.minimum_tokens > chunking.target_tokens
        || chunking.target_tokens > chunking.maximum_tokens
    {
        return Err(TranscriptionError::InvalidConfiguration(
            "chunk token limits must be positive and ordered".into(),
        ));
    }
    if chunking.usable_pause_ms > chunking.strong_pause_ms
        || chunking.strong_pause_ms > chunking.long_pause_ms
    {
        return Err(TranscriptionError::InvalidConfiguration(
            "chunk pause limits must be ordered".into(),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct PauseCandidate {
    end: usize,
    token_count: usize,
    pause_samples: u64,
    pause_ms: u64,
}

#[derive(Debug, Clone, Copy)]
struct ChunkChoice {
    end: usize,
    reason: ChunkBoundaryReason,
    pause_samples: Option<u64>,
}

fn build_chunks(
    segments: &[DecodedSegment],
    sample_rate_hz: u32,
    config: &ChunkConstructionConfig,
    terminal_reason: ChunkBoundaryReason,
    next_ordinal: &mut u32,
    previous_chunk_end: &mut Option<u64>,
) -> Vec<Chunk> {
    let mut chunks: Vec<Chunk> = Vec::new();
    let mut start = 0_usize;

    while start < segments.len() {
        let mut token_count = 0_usize;
        let mut usable = Vec::new();
        let mut boundaries = Vec::new();
        let mut choice = None;

        for end in (start + 1)..=segments.len() {
            token_count = token_count.saturating_add(normal_token_count(&segments[end - 1]));
            let pause_samples = pause_after(segments, end);
            let pause_ms = pause_samples
                .map(|samples| samples.saturating_mul(1_000) / u64::from(sample_rate_hz));
            boundaries.push((end, token_count, pause_samples));

            if pause_ms.is_some_and(|pause| pause >= config.long_pause_ms) {
                choice = Some(ChunkChoice {
                    end,
                    reason: ChunkBoundaryReason::LongPause,
                    pause_samples,
                });
                break;
            }
            if token_count >= config.minimum_tokens
                && pause_ms.is_some_and(|pause| pause >= config.strong_pause_ms)
            {
                choice = Some(ChunkChoice {
                    end,
                    reason: ChunkBoundaryReason::StrongPause,
                    pause_samples,
                });
                break;
            }
            if token_count >= config.minimum_tokens
                && pause_ms.is_some_and(|pause| pause >= config.usable_pause_ms)
            {
                usable.push(PauseCandidate {
                    end,
                    token_count,
                    pause_samples: pause_samples.unwrap_or(0),
                    pause_ms: pause_ms.unwrap_or(0),
                });
            }
            if token_count >= config.target_tokens && !usable.is_empty() {
                let candidate = best_pause(&usable, config);
                choice = Some(ChunkChoice {
                    end: candidate.end,
                    reason: ChunkBoundaryReason::ScoredPause,
                    pause_samples: Some(candidate.pause_samples),
                });
                break;
            }
            if token_count >= config.maximum_tokens {
                let end = boundaries
                    .iter()
                    .filter(|(_, count, _)| *count <= config.maximum_tokens)
                    .min_by_key(|(_, count, _)| count.abs_diff(config.target_tokens))
                    .map_or(end, |(boundary, _, _)| *boundary);
                choice = Some(ChunkChoice {
                    end,
                    reason: ChunkBoundaryReason::MaximumTokens,
                    pause_samples: pause_after(segments, end),
                });
                break;
            }
            if end == segments.len() {
                choice = Some(ChunkChoice {
                    end,
                    reason: ChunkBoundaryReason::SourceEnd,
                    pause_samples: None,
                });
                break;
            }
        }

        let mut choice = choice.expect("a non-empty segment suffix always produces a chunk");
        if choice.end == segments.len() {
            choice.reason = terminal_reason;
            choice.pause_samples = None;
        }
        let selected = &segments[start..choice.end];
        let first_range = selected[0]
            .audio_range
            .expect("chunk segments always have a canonical range");
        let last_range = selected[selected.len() - 1]
            .audio_range
            .expect("chunk segments always have a canonical range");
        let range_start = previous_chunk_end.map_or(first_range.start_sample, |previous| {
            previous.max(first_range.start_sample)
        });
        let range_end = last_range.end_sample;
        if range_start >= range_end {
            if let Some(previous) = chunks.last_mut() {
                previous
                    .segment_ids
                    .extend(selected.iter().map(|segment| segment.id.clone()));
                previous
                    .text
                    .extend(selected.iter().map(|segment| segment.text.as_str()));
                previous.token_count = previous
                    .token_count
                    .saturating_add(selected.iter().map(normal_token_count).sum());
                previous.boundary = ChunkBoundary {
                    reason: choice.reason,
                    pause_samples: choice.pause_samples,
                };
                start = choice.end;
                continue;
            }
        }
        let ordinal = *next_ordinal;
        *next_ordinal = next_ordinal.saturating_add(1);
        let chunk = Chunk {
            id: format!("chunk-{ordinal}"),
            ordinal,
            segment_ids: selected.iter().map(|segment| segment.id.clone()).collect(),
            audio_range: SampleRange {
                start_sample: range_start,
                end_sample: range_end,
            },
            text: selected
                .iter()
                .map(|segment| segment.text.as_str())
                .collect(),
            token_count: selected.iter().map(normal_token_count).sum(),
            boundary: ChunkBoundary {
                reason: choice.reason,
                pause_samples: choice.pause_samples,
            },
            transcriptions: Vec::new(),
            current_transcription_id: String::new(),
        };
        *previous_chunk_end = Some(chunk.audio_range.end_sample);
        chunks.push(chunk);
        start = choice.end;
    }

    chunks
}

fn normal_token_count(segment: &DecodedSegment) -> usize {
    segment
        .tokens
        .iter()
        .filter(|token| !token.is_special)
        .count()
}

fn pause_after(segments: &[DecodedSegment], end: usize) -> Option<u64> {
    (end < segments.len()).then(|| {
        let next = segments[end]
            .audio_range
            .expect("chunk segments always have a canonical range");
        let previous = segments[end - 1]
            .audio_range
            .expect("chunk segments always have a canonical range");
        next.start_sample.saturating_sub(previous.end_sample)
    })
}

fn best_pause(candidates: &[PauseCandidate], config: &ChunkConstructionConfig) -> PauseCandidate {
    *candidates
        .iter()
        .max_by_key(|candidate| {
            let pause = i128::from(candidate.pause_ms);
            let distance = i128::try_from(candidate.token_count.abs_diff(config.target_tokens))
                .unwrap_or(i128::MAX);
            let penalty = i128::from(config.distance_penalty_ms).saturating_mul(distance);
            (pause.saturating_sub(penalty), candidate.end)
        })
        .expect("best_pause requires at least one candidate")
}

fn normalize_segments(
    segments: Vec<DecodeSpanSegment>,
    submitted: SampleRange,
    span_ordinal: u32,
) -> Vec<DecodedSegment> {
    segments
        .into_iter()
        .enumerate()
        .map(|(index, segment)| {
            let audio_range = segment
                .raw_timestamps
                .and_then(|timestamps| canonical_range(timestamps, submitted));
            DecodedSegment {
                id: format!("decode-span-{span_ordinal}-segment-{}", index + 1),
                raw_timestamps: segment.raw_timestamps,
                audio_range,
                text: segment.text,
                no_speech_probability: segment.no_speech_probability,
                tokens: segment
                    .tokens
                    .into_iter()
                    .map(|mut token| {
                        token.audio_range = token
                            .raw_timestamps
                            .and_then(|timestamps| canonical_range(timestamps, submitted));
                        token
                    })
                    .collect(),
            }
        })
        .collect()
}

pub(crate) fn canonical_range(
    timestamps: DecoderTimestamps,
    submitted: SampleRange,
) -> Option<SampleRange> {
    let start = u64::try_from(timestamps.start)
        .ok()?
        .checked_mul(timestamps.samples_per_unit)?
        .checked_add(submitted.start_sample)?;
    let end = u64::try_from(timestamps.end)
        .ok()?
        .checked_mul(timestamps.samples_per_unit)?
        .checked_add(submitted.start_sample)?;
    (start < end && start >= submitted.start_sample && end <= submitted.end_sample).then_some(
        SampleRange {
            start_sample: start,
            end_sample: end,
        },
    )
}

#[derive(Debug, Clone, Copy)]
struct BoundaryDecision {
    boundary: u64,
    reason: AdvanceReason,
    pause_samples: Option<u64>,
    accepted_len: usize,
    valid_prefix_len: usize,
}

fn choose_continuation_boundary(
    submitted: SampleRange,
    total: u64,
    segments: &[DecodedSegment],
    config: &TranscriptionConfig,
) -> BoundaryDecision {
    let mut previous_end = submitted.start_sample;
    let valid_prefix_len = segments
        .iter()
        .take_while(|segment| {
            let Some(range) = segment.audio_range else {
                return false;
            };
            let valid =
                range.start_sample >= previous_end && range.end_sample > submitted.start_sample;
            if valid {
                previous_end = range.end_sample;
            }
            valid
        })
        .count();
    if submitted.end_sample == total {
        return BoundaryDecision {
            boundary: total,
            reason: AdvanceReason::SourceEnd,
            pause_samples: None,
            accepted_len: valid_prefix_len,
            valid_prefix_len,
        };
    }
    let search_start = submitted
        .end_sample
        .saturating_sub(config.continuation_search_samples)
        .max(submitted.start_sample.saturating_add(1));
    let pause_after_candidate = |index: usize| -> Option<u64> {
        let current = segments[index].audio_range?;
        if index + 1 < valid_prefix_len {
            let next = segments[index + 1].audio_range?;
            Some(next.start_sample.saturating_sub(current.end_sample))
        } else if index + 1 == segments.len() {
            Some(submitted.end_sample.saturating_sub(current.end_sample))
        } else {
            None
        }
    };
    let latest_strong = (0..valid_prefix_len).rev().find(|index| {
        let range = segments[*index]
            .audio_range
            .expect("valid prefix has canonical ranges");
        range.end_sample >= search_start
            && pause_after_candidate(*index).is_some_and(|pause| {
                pause.saturating_mul(1_000) / u64::from(WHISPER_SAMPLE_RATE_HZ)
                    >= config.continuation_strong_pause_ms
            })
    });
    let latest_in_search = (0..valid_prefix_len).rev().find(|index| {
        segments[*index]
            .audio_range
            .is_some_and(|range| range.end_sample >= search_start)
    });
    let choice = latest_strong
        .map(|index| (index, AdvanceReason::StrongPause))
        .or_else(|| latest_in_search.map(|index| (index, AdvanceReason::LatestTimestamp)))
        .or_else(|| {
            valid_prefix_len
                .checked_sub(1)
                .map(|index| (index, AdvanceReason::EarlyTimestampFallback))
        });
    let Some((index, reason)) = choice else {
        return BoundaryDecision {
            boundary: submitted.end_sample,
            reason: AdvanceReason::NoUsableTimestamp,
            pause_samples: None,
            accepted_len: 0,
            valid_prefix_len,
        };
    };
    let range = segments[index]
        .audio_range
        .expect("selected candidates have canonical ranges");
    BoundaryDecision {
        boundary: range.end_sample,
        reason,
        pause_samples: (reason == AdvanceReason::StrongPause)
            .then(|| pause_after_candidate(index))
            .flatten(),
        accepted_len: index + 1,
        valid_prefix_len,
    }
}

fn segment_has_text_evidence(segment: &DecodedSegment) -> bool {
    !segment.text.is_empty() || segment.tokens.iter().any(|token| !token.is_special)
}

fn run_id(
    source: &SourceFacts,
    transcriber: &TranscriberIdentity,
    config: &TranscriptionConfig,
    decode_spans: &[DecodeSpan],
) -> String {
    let encoded = serde_json::to_vec(&(source, transcriber, config, decode_spans))
        .expect("transcription identity values are serializable");
    let digest = Sha256::digest(encoded);
    format!("transcription-{}", hex::encode(&digest[..16]))
}

pub struct WhisperDecoder {
    context: Arc<WhisperContext>,
    identity: TranscriberIdentity,
    language: String,
    threads: usize,
    top_candidates: usize,
}

impl WhisperDecoder {
    pub fn load(model: &Path, config: &TranscriptionConfig) -> Result<Self, TranscriptionError> {
        let model_sha256 = hash_file(model)?;
        let path = model
            .to_str()
            .ok_or_else(|| TranscriptionError::Model("model path is not valid UTF-8".into()))?;
        let context = Arc::new(
            WhisperContext::new_with_params(path, WhisperContextParameters::default())
                .map_err(|error| TranscriptionError::Model(error.to_string()))?,
        );
        Ok(Self {
            context,
            identity: TranscriberIdentity {
                name: "whisper.cpp".into(),
                implementation: format!(
                    "whisper-rs-{}/whisper.cpp-{}",
                    whisper_rs::get_version(),
                    whisper_rs::get_whisper_cpp_version()
                ),
                model_sha256,
            },
            language: config.language.clone(),
            threads: config.threads,
            top_candidates: config.top_candidates,
        })
    }

    fn extract_segments(&self, state: &WhisperState) -> Result<Vec<DecodeSpanSegment>, String> {
        let mut output = Vec::new();
        for segment_index in 0..state.full_n_segments() {
            let segment = state
                .get_segment(segment_index)
                .ok_or_else(|| format!("Whisper returned invalid segment {segment_index}"))?;
            let mut tokens = Vec::new();
            for token_index in 0..segment.n_tokens() {
                let token = segment
                    .get_token(token_index)
                    .ok_or_else(|| format!("Whisper returned invalid token {token_index}"))?;
                let data = token.token_data();
                let raw_timestamps = Some(DecoderTimestamps {
                    start: data.t0,
                    end: data.t1,
                    samples_per_unit: SAMPLES_PER_CENTISECOND,
                });
                let alternatives = token
                    .get_all_top_candidates()
                    .into_iter()
                    .map(|candidate| TokenAlternative {
                        token_id: candidate.id,
                        text: self
                            .context
                            .token_to_string(candidate.id)
                            .unwrap_or_default(),
                        probability: candidate.p,
                    })
                    .collect();
                tokens.push(WhisperToken {
                    token_id: token.token_id(),
                    text: token.to_string().unwrap_or_default(),
                    probability: token.token_probability(),
                    is_special: token.token_id() >= self.context.token_eot(),
                    raw_timestamps,
                    audio_range: None,
                    alternatives,
                });
            }
            output.push(DecodeSpanSegment {
                raw_timestamps: Some(DecoderTimestamps {
                    start: segment.start_timestamp(),
                    end: segment.end_timestamp(),
                    samples_per_unit: SAMPLES_PER_CENTISECOND,
                }),
                text: segment.to_string().unwrap_or_default(),
                no_speech_probability: segment.no_speech_probability(),
                tokens,
            });
        }
        Ok(output)
    }

    fn interactive_segments(
        &self,
        session: &InteractiveSession,
        transcription: &DecoderTranscription,
    ) -> Result<Vec<DecodeSpanSegment>, TranscriptionError> {
        Ok(transcription
            .segments
            .iter()
            .map(|segment| {
                let tokens = segment
                    .tokens
                    .iter()
                    .map(|token| {
                        let alternatives = session
                            .candidates_at(token.position, self.top_candidates)
                            .unwrap_or_default()
                            .into_iter()
                            .map(|candidate| TokenAlternative {
                                token_id: candidate.token_id,
                                text: candidate.text,
                                probability: candidate.probability,
                            })
                            .collect();
                        WhisperToken {
                            token_id: token.token_id,
                            text: token.text.clone(),
                            probability: token.probability,
                            is_special: token.token_id >= self.context.token_eot(),
                            raw_timestamps: Some(DecoderTimestamps {
                                start: token.start_time_ms,
                                end: token.end_time_ms,
                                samples_per_unit: 16,
                            }),
                            audio_range: None,
                            alternatives,
                        }
                    })
                    .collect();
                DecodeSpanSegment {
                    raw_timestamps: Some(DecoderTimestamps {
                        start: segment.start_time_ms,
                        end: segment.end_time_ms,
                        samples_per_unit: 16,
                    }),
                    text: segment.text.clone(),
                    no_speech_probability: segment.no_speech_prob,
                    tokens,
                }
            })
            .collect())
    }

    fn params(&self) -> FullParams<'_, '_> {
        self.params_with_sampling(SamplingStrategy::BeamSearch {
            beam_size: 5,
            patience: -1.0,
        })
    }

    fn params_with_sampling(&self, sampling: SamplingStrategy) -> FullParams<'_, '_> {
        let mut params = FullParams::new(sampling);
        params.set_n_threads(i32::try_from(self.threads).unwrap_or(i32::MAX));
        params.set_language(Some(&self.language));
        params.set_translate(false);
        params.set_no_context(true);
        params.set_no_timestamps(false);
        params.set_token_timestamps(true);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_print_special(false);
        params.set_capture_top_candidates(self.top_candidates > 0);
        params.set_n_top_candidates(i32::try_from(self.top_candidates).unwrap_or(i32::MAX));
        params
    }
}

impl DecodeSpanDecoder for WhisperDecoder {
    fn identity(&self) -> TranscriberIdentity {
        self.identity.clone()
    }

    fn prompt_capacity(&self) -> usize {
        usize::try_from(self.context.n_text_ctx())
            .unwrap_or(0)
            .saturating_div(2)
            .saturating_sub(1)
    }

    fn decode(
        &mut self,
        audio: &[f32],
        prompt_token_ids: &[i32],
    ) -> Result<Vec<DecodeSpanSegment>, String> {
        let mut state = self
            .context
            .create_state()
            .map_err(|error| error.to_string())?;
        let mut params = self.params();
        if !prompt_token_ids.is_empty() {
            params.set_tokens(prompt_token_ids);
        }
        state
            .full(params, audio)
            .map_err(|error| error.to_string())?;
        self.extract_segments(&state)
    }
}

fn hash_file(path: &Path) -> Result<String, std::io::Error> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(hex::encode(digest.finalize()))
}
