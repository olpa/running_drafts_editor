//! Immutable Whisper recognition evidence and bounded-window orchestration.

use std::{fs::File, io::Read, path::Path};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState,
};

use crate::chunking::{SampleRange, SourceFacts};

pub const RECOGNITION_RUN_SCHEMA: &str = "recognition-run/v1-experimental";
const WHISPER_SAMPLE_RATE_HZ: u32 = 16_000;
const SAMPLES_PER_CENTISECOND: u64 = 160;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecognitionConfig {
    pub max_window_samples: u64,
    pub target_core_samples: u64,
    pub left_context_samples: u64,
    pub right_context_samples: u64,
    pub language: String,
    pub threads: usize,
    pub top_candidates: usize,
    pub post_chunking: PostChunkConfig,
}

impl Default for RecognitionConfig {
    fn default() -> Self {
        Self {
            max_window_samples: 480_000,
            target_core_samples: 384_000,
            left_context_samples: 48_000,
            right_context_samples: 48_000,
            language: "auto".into(),
            threads: 4,
            top_candidates: 20,
            post_chunking: PostChunkConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostChunkConfig {
    pub minimum_tokens: usize,
    pub target_tokens: usize,
    pub maximum_tokens: usize,
    pub usable_pause_ms: u64,
    pub strong_pause_ms: u64,
    pub long_pause_ms: u64,
    pub distance_penalty_ms: u64,
}

impl Default for PostChunkConfig {
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
pub struct RecognizerIdentity {
    pub name: String,
    pub implementation: String,
    pub model_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecognitionStatus {
    Succeeded,
    Partial,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdvanceReason {
    WhisperTimestamp,
    SourceEnd,
    FixedNoTimestamp,
    FixedDecodeFailure,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TokenAlternative {
    pub token_id: i32,
    pub text: String,
    pub probability: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecognitionToken {
    pub token_id: i32,
    pub text: String,
    pub probability: f32,
    pub is_special: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_range: Option<SampleRange>,
    pub alternatives: Vec<TokenAlternative>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecodedSegment {
    pub id: String,
    pub audio_range: SampleRange,
    pub text: String,
    pub no_speech_probability: f32,
    pub tokens: Vec<RecognitionToken>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChunkBoundaryReason {
    LongPause,
    StrongPause,
    ScoredPause,
    MaximumTokens,
    SourceEnd,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkBoundary {
    pub reason: ChunkBoundaryReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pause_samples: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecognitionChunk {
    pub id: String,
    pub ordinal: u32,
    pub segment_ids: Vec<String>,
    pub audio_range: SampleRange,
    pub text: String,
    pub token_count: usize,
    pub boundary: ChunkBoundary,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcessingWindow {
    pub ordinal: u32,
    pub submitted: SampleRange,
    pub core: SampleRange,
    pub prompt_token_ids: Vec<i32>,
    pub advance_reason: AdvanceReason,
    pub hypotheses: Vec<DecodedSegment>,
    pub accepted_segment_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecognitionRun {
    pub schema: String,
    pub id: String,
    pub revision: u64,
    pub source: SourceFacts,
    pub recognizer: RecognizerIdentity,
    pub config: RecognitionConfig,
    pub status: RecognitionStatus,
    pub windows: Vec<ProcessingWindow>,
    pub segments: Vec<DecodedSegment>,
    pub chunks: Vec<RecognitionChunk>,
}

#[derive(Debug, Clone)]
pub struct ChunkRefreshRequest {
    pub source: SourceFacts,
    pub chunk_range: SampleRange,
    pub language: String,
    pub forced_tokens: Vec<i32>,
    pub revision: u64,
}

pub struct RecognizerSession {
    decoder: WhisperDecoder,
    model_path: std::path::PathBuf,
}

impl RecognizerSession {
    pub fn load(model: &Path, config: &RecognitionConfig) -> Result<Self, RecognitionError> {
        Ok(Self {
            decoder: WhisperDecoder::load(model, config)?,
            model_path: model.into(),
        })
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

    pub fn tokenize(&self, text: &str) -> Result<Vec<i32>, RecognitionError> {
        let maximum = text.len().saturating_add(256).max(256);
        self.decoder
            .context
            .tokenize(text, maximum)
            .map_err(|error| RecognitionError::Model(error.to_string()))
    }

    pub fn render_tokens(&self, tokens: &[i32]) -> Result<String, RecognitionError> {
        tokens
            .iter()
            .map(|id| {
                self.decoder
                    .context
                    .token_to_string(*id)
                    .map_err(|error| RecognitionError::Model(error.to_string()))
            })
            .collect()
    }

    pub fn refresh_chunk(
        &mut self,
        request: ChunkRefreshRequest,
        samples: &[f32],
    ) -> Result<RecognitionRun, RecognitionError> {
        if request.source.sample_rate_hz != WHISPER_SAMPLE_RATE_HZ
            || request.source.channels != 1
            || request.source.decoded_sample_count != samples.len() as u64
            || request.chunk_range.is_empty()
            || request.chunk_range.end_sample > request.source.decoded_sample_count
            || request.chunk_range.len() > 480_000
        {
            return Err(RecognitionError::InvalidConfiguration(
                "invalid existing chunk audio range".into(),
            ));
        }
        let start = usize::try_from(request.chunk_range.start_sample)
            .map_err(|_| RecognitionError::AudioTooLong)?;
        let end = usize::try_from(request.chunk_range.end_sample)
            .map_err(|_| RecognitionError::AudioTooLong)?;
        self.decoder.language = request.language.clone();
        let relative = self
            .decoder
            .decode_forced(&samples[start..end], request.forced_tokens.clone())
            .map_err(RecognitionError::Model)?;
        let segments = normalize_segments(relative, request.chunk_range, 1);
        let chunk_text = segments
            .iter()
            .flat_map(|s| &s.tokens)
            .filter(|t| !t.is_special)
            .map(|t| t.text.as_str())
            .collect::<String>();
        let segment_ids = segments.iter().map(|s| s.id.clone()).collect::<Vec<_>>();
        let chunk = RecognitionChunk {
            id: "refresh-chunk".into(),
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
        };
        let config = RecognitionConfig {
            language: request.language,
            ..RecognitionConfig::default()
        };
        let window = ProcessingWindow {
            ordinal: 1,
            submitted: request.chunk_range,
            core: request.chunk_range,
            prompt_token_ids: Vec::new(),
            advance_reason: AdvanceReason::SourceEnd,
            hypotheses: segments.clone(),
            accepted_segment_ids: segments.iter().map(|s| s.id.clone()).collect(),
            error: None,
        };
        let recognizer = self.decoder.identity.clone();
        let base_id = run_id(
            &request.source,
            &recognizer,
            &config,
            std::slice::from_ref(&window),
            &segments,
            std::slice::from_ref(&chunk),
        );
        let id = format!("{base_id}-r{}", request.revision);
        Ok(RecognitionRun {
            schema: RECOGNITION_RUN_SCHEMA.into(),
            id,
            revision: request.revision,
            source: request.source,
            recognizer,
            config,
            status: RecognitionStatus::Succeeded,
            windows: vec![window],
            segments,
            chunks: vec![chunk],
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WindowSegment {
    pub audio_range: SampleRange,
    pub text: String,
    pub no_speech_probability: f32,
    pub tokens: Vec<RecognitionToken>,
}

pub trait WindowDecoder {
    fn identity(&self) -> RecognizerIdentity;
    fn decode(
        &mut self,
        audio: &[f32],
        prompt_token_ids: &[i32],
    ) -> Result<Vec<WindowSegment>, String>;
}

#[derive(Debug, thiserror::Error)]
pub enum RecognitionError {
    #[error("invalid recognition configuration: {0}")]
    InvalidConfiguration(String),
    #[error("audio sample count does not fit this platform")]
    AudioTooLong,
    #[error("could not load Whisper model: {0}")]
    Model(String),
    #[error("could not read Whisper model: {0}")]
    ModelRead(#[from] std::io::Error),
}

pub fn recognize<D: WindowDecoder>(
    source: SourceFacts,
    samples: &[f32],
    config: RecognitionConfig,
    decoder: &mut D,
) -> Result<RecognitionRun, RecognitionError> {
    validate_config(&source, samples, &config)?;
    let total = source.decoded_sample_count;
    let mut cursor = 0_u64;
    let mut windows = Vec::new();
    let mut accepted = Vec::new();
    let mut prompt_token_ids = Vec::new();
    let mut failures = 0_usize;

    while cursor < total {
        let submitted_start = cursor.saturating_sub(config.left_context_samples);
        let desired_end = cursor
            .saturating_add(config.target_core_samples)
            .saturating_add(config.right_context_samples);
        let submitted_end = desired_end.min(total);
        let submitted = SampleRange {
            start_sample: submitted_start,
            end_sample: submitted_end,
        };
        let start = usize::try_from(submitted_start).map_err(|_| RecognitionError::AudioTooLong)?;
        let end = usize::try_from(submitted_end).map_err(|_| RecognitionError::AudioTooLong)?;
        let ordinal = u32::try_from(windows.len() + 1).unwrap_or(u32::MAX);
        let window_prompt_token_ids = prompt_token_ids.clone();

        let decoded = decoder.decode(&samples[start..end], &window_prompt_token_ids);
        let (hypotheses, boundary, advance_reason, error) = match decoded {
            Ok(relative) => {
                let hypotheses = normalize_segments(relative, submitted, ordinal);
                let (boundary, reason) = choose_boundary(cursor, total, &hypotheses, &config);
                (hypotheses, boundary, reason, None)
            }
            Err(error) => {
                failures += 1;
                let boundary = target_boundary(cursor, total, &config);
                (
                    Vec::new(),
                    boundary,
                    AdvanceReason::FixedDecodeFailure,
                    Some(error),
                )
            }
        };

        let mut accepted_ids = Vec::new();
        let mut next_prompt_token_ids = None;
        for segment in &hypotheses {
            let midpoint = segment.audio_range.start_sample + segment.audio_range.len() / 2;
            if midpoint >= cursor
                && segment.audio_range.end_sample <= boundary
                && !segment.audio_range.is_empty()
            {
                accepted_ids.push(segment.id.clone());
                next_prompt_token_ids = Some(
                    segment
                        .tokens
                        .iter()
                        .filter(|token| !token.is_special)
                        .map(|token| token.token_id)
                        .collect(),
                );
                accepted.push(segment.clone());
            }
        }
        if let Some(next) = next_prompt_token_ids {
            prompt_token_ids = next;
        }

        windows.push(ProcessingWindow {
            ordinal,
            submitted,
            core: SampleRange {
                start_sample: cursor,
                end_sample: boundary,
            },
            prompt_token_ids: window_prompt_token_ids,
            advance_reason,
            hypotheses,
            accepted_segment_ids: accepted_ids,
            error,
        });
        cursor = boundary;
    }

    let status = if failures == 0 {
        RecognitionStatus::Succeeded
    } else if failures == windows.len() {
        RecognitionStatus::Failed
    } else {
        RecognitionStatus::Partial
    };
    let recognizer = decoder.identity();
    let chunks = build_post_chunks(&accepted, source.sample_rate_hz, &config.post_chunking);
    let id = run_id(&source, &recognizer, &config, &windows, &accepted, &chunks);
    Ok(RecognitionRun {
        schema: RECOGNITION_RUN_SCHEMA.into(),
        id,
        revision: 1,
        source,
        recognizer,
        config,
        status,
        windows,
        segments: accepted,
        chunks,
    })
}

fn validate_config(
    source: &SourceFacts,
    samples: &[f32],
    config: &RecognitionConfig,
) -> Result<(), RecognitionError> {
    if source.sample_rate_hz != WHISPER_SAMPLE_RATE_HZ || source.channels != 1 {
        return Err(RecognitionError::InvalidConfiguration(
            "Whisper input must be canonical mono 16 kHz audio".into(),
        ));
    }
    if source.decoded_sample_count != u64::try_from(samples.len()).unwrap_or(u64::MAX) {
        return Err(RecognitionError::InvalidConfiguration(
            "source facts do not match decoded samples".into(),
        ));
    }
    if config.max_window_samples == 0 || config.target_core_samples == 0 {
        return Err(RecognitionError::InvalidConfiguration(
            "window and target core must be positive".into(),
        ));
    }
    let submitted = config
        .left_context_samples
        .checked_add(config.target_core_samples)
        .and_then(|value| value.checked_add(config.right_context_samples));
    if submitted.is_none_or(|value| value > config.max_window_samples) {
        return Err(RecognitionError::InvalidConfiguration(
            "left context + target core + right context exceeds maximum window".into(),
        ));
    }
    let post = &config.post_chunking;
    if post.minimum_tokens == 0
        || post.minimum_tokens > post.target_tokens
        || post.target_tokens > post.maximum_tokens
    {
        return Err(RecognitionError::InvalidConfiguration(
            "post-chunk token limits must be positive and ordered".into(),
        ));
    }
    if post.usable_pause_ms > post.strong_pause_ms || post.strong_pause_ms > post.long_pause_ms {
        return Err(RecognitionError::InvalidConfiguration(
            "post-chunk pause limits must be ordered".into(),
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

fn build_post_chunks(
    segments: &[DecodedSegment],
    sample_rate_hz: u32,
    config: &PostChunkConfig,
) -> Vec<RecognitionChunk> {
    let mut chunks = Vec::new();
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

        let choice = choice.expect("a non-empty segment suffix always produces a chunk");
        let selected = &segments[start..choice.end];
        let ordinal = u32::try_from(chunks.len() + 1).unwrap_or(u32::MAX);
        chunks.push(RecognitionChunk {
            id: format!("recognition-chunk-{ordinal}"),
            ordinal,
            segment_ids: selected.iter().map(|segment| segment.id.clone()).collect(),
            audio_range: SampleRange {
                start_sample: selected[0].audio_range.start_sample,
                end_sample: selected[selected.len() - 1].audio_range.end_sample,
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
        });
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
        segments[end]
            .audio_range
            .start_sample
            .saturating_sub(segments[end - 1].audio_range.end_sample)
    })
}

fn best_pause(candidates: &[PauseCandidate], config: &PostChunkConfig) -> PauseCandidate {
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
    segments: Vec<WindowSegment>,
    submitted: SampleRange,
    window_ordinal: u32,
) -> Vec<DecodedSegment> {
    segments
        .into_iter()
        .enumerate()
        .filter_map(|(index, segment)| {
            let start = submitted
                .start_sample
                .saturating_add(segment.audio_range.start_sample)
                .min(submitted.end_sample);
            let end = submitted
                .start_sample
                .saturating_add(segment.audio_range.end_sample)
                .min(submitted.end_sample);
            (start < end).then(|| DecodedSegment {
                id: format!("window-{window_ordinal}-segment-{}", index + 1),
                audio_range: SampleRange {
                    start_sample: start,
                    end_sample: end,
                },
                text: segment.text,
                no_speech_probability: segment.no_speech_probability,
                tokens: segment
                    .tokens
                    .into_iter()
                    .map(|mut token| {
                        token.audio_range = token.audio_range.and_then(|range| {
                            let token_start = submitted
                                .start_sample
                                .saturating_add(range.start_sample)
                                .min(submitted.end_sample);
                            let token_end = submitted
                                .start_sample
                                .saturating_add(range.end_sample)
                                .min(submitted.end_sample);
                            (token_start < token_end).then_some(SampleRange {
                                start_sample: token_start,
                                end_sample: token_end,
                            })
                        });
                        token
                    })
                    .collect(),
            })
        })
        .collect()
}

fn choose_boundary(
    cursor: u64,
    total: u64,
    segments: &[DecodedSegment],
    config: &RecognitionConfig,
) -> (u64, AdvanceReason) {
    let submitted_end = cursor
        .saturating_add(config.target_core_samples)
        .saturating_add(config.right_context_samples)
        .min(total);
    if submitted_end == total {
        return (total, AdvanceReason::SourceEnd);
    }
    let target = cursor.saturating_add(config.target_core_samples).min(total);
    let candidate = segments
        .iter()
        .map(|segment| segment.audio_range.end_sample)
        .filter(|end| *end >= target && *end <= submitted_end)
        .max();
    candidate
        .map(|boundary| (boundary, AdvanceReason::WhisperTimestamp))
        .unwrap_or_else(|| {
            (
                target_boundary(cursor, total, config),
                AdvanceReason::FixedNoTimestamp,
            )
        })
}

fn target_boundary(cursor: u64, total: u64, config: &RecognitionConfig) -> u64 {
    cursor.saturating_add(config.target_core_samples).min(total)
}

fn run_id(
    source: &SourceFacts,
    recognizer: &RecognizerIdentity,
    config: &RecognitionConfig,
    windows: &[ProcessingWindow],
    segments: &[DecodedSegment],
    chunks: &[RecognitionChunk],
) -> String {
    let encoded = serde_json::to_vec(&(source, recognizer, config, windows, segments, chunks))
        .expect("recognition identity values are serializable");
    let digest = Sha256::digest(encoded);
    format!("recognition-{}", hex::encode(&digest[..16]))
}

pub struct WhisperDecoder {
    context: WhisperContext,
    identity: RecognizerIdentity,
    language: String,
    threads: usize,
    top_candidates: usize,
}

impl WhisperDecoder {
    pub fn load(model: &Path, config: &RecognitionConfig) -> Result<Self, RecognitionError> {
        let model_sha256 = hash_file(model)?;
        let path = model
            .to_str()
            .ok_or_else(|| RecognitionError::Model("model path is not valid UTF-8".into()))?;
        let context = WhisperContext::new_with_params(path, WhisperContextParameters::default())
            .map_err(|error| RecognitionError::Model(error.to_string()))?;
        Ok(Self {
            context,
            identity: RecognizerIdentity {
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

    fn extract_segments(&self, state: &WhisperState) -> Result<Vec<WindowSegment>, String> {
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
                let audio_range = centiseconds_range(data.t0, data.t1);
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
                tokens.push(RecognitionToken {
                    token_id: token.token_id(),
                    text: token.to_string().unwrap_or_default(),
                    probability: token.token_probability(),
                    is_special: token.token_id() >= self.context.token_eot(),
                    audio_range,
                    alternatives,
                });
            }
            let Some(audio_range) =
                centiseconds_range(segment.start_timestamp(), segment.end_timestamp())
            else {
                continue;
            };
            output.push(WindowSegment {
                audio_range,
                text: segment.to_string().unwrap_or_default(),
                no_speech_probability: segment.no_speech_probability(),
                tokens,
            });
        }
        Ok(output)
    }

    fn decode_forced(
        &mut self,
        audio: &[f32],
        forced_tokens: Vec<i32>,
    ) -> Result<Vec<WindowSegment>, String> {
        let mut state = self
            .context
            .create_state()
            .map_err(|error| error.to_string())?;
        let mut params = self.params_with_sampling(SamplingStrategy::Greedy { best_of: 1 });
        params.set_single_segment(true);
        params.set_forced_tokens_owned(forced_tokens);
        state
            .full(params, audio)
            .map_err(|error| error.to_string())?;
        self.extract_segments(&state)
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

impl WindowDecoder for WhisperDecoder {
    fn identity(&self) -> RecognizerIdentity {
        self.identity.clone()
    }

    fn decode(
        &mut self,
        audio: &[f32],
        prompt_token_ids: &[i32],
    ) -> Result<Vec<WindowSegment>, String> {
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

fn centiseconds_range(start: i64, end: i64) -> Option<SampleRange> {
    let start = u64::try_from(start)
        .ok()?
        .checked_mul(SAMPLES_PER_CENTISECOND)?;
    let end = u64::try_from(end)
        .ok()?
        .checked_mul(SAMPLES_PER_CENTISECOND)?;
    (start < end).then_some(SampleRange {
        start_sample: start,
        end_sample: end,
    })
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
