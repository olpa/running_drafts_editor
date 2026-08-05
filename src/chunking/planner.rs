use std::cmp::Ordering;

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::{
    BoundaryDecision, BoundaryKind, CanonicalAudio, DetectorError, DetectorErrorCode,
    DetectorIdentity, DetectorRun, DetectorStatus, FrameEvidence, PlanError, PlanFailure,
    PlannerConfig, PlannerRun, PreflightResult, RecognitionChunk, RecognitionPlan,
    RecognizerContract, SampleRange, SourceFacts, SpeechDetector, ValleyEvidence, PLANNER_VERSION,
    PLAN_SCHEMA,
};

#[derive(Serialize)]
struct IdentityMaterial<'a> {
    schema: &'a str,
    revision: u64,
    source: &'a SourceFacts,
    recognizer: &'a RecognizerContract,
    detector: &'a DetectorRun,
    planner: &'a PlannerRun,
    chunks: Vec<ChunkIdentityMaterial<'a>>,
    failures: &'a [PlanFailure],
}

#[derive(Serialize)]
struct ChunkIdentityMaterial<'a> {
    ordinal: u32,
    core: SampleRange,
    submitted: SampleRange,
    boundary: &'a BoundaryDecision,
    preflight: &'a PreflightResult,
}

#[derive(Debug)]
struct Candidate {
    run: SampleRange,
    sample: u64,
    mean: f32,
    frame_count: u64,
}

pub fn plan_with_detector(
    audio: &CanonicalAudio<'_>,
    recognizer: RecognizerContract,
    config: PlannerConfig,
    detector: &mut dyn SpeechDetector,
) -> Result<RecognitionPlan, PlanError> {
    validate_inputs(audio, &recognizer, &config)?;
    let identity = detector.identity();
    if audio.samples.is_empty() {
        return plan(audio, recognizer, config, identity, Ok(Vec::new()));
    }
    let evidence = detector.detect(audio);
    plan(audio, recognizer, config, identity, evidence)
}

pub fn plan(
    audio: &CanonicalAudio<'_>,
    recognizer: RecognizerContract,
    config: PlannerConfig,
    detector_identity: DetectorIdentity,
    detector_result: Result<Vec<FrameEvidence>, DetectorError>,
) -> Result<RecognitionPlan, PlanError> {
    validate_inputs(audio, &recognizer, &config)?;
    validate_detector_identity(&detector_identity)?;
    let sample_count =
        u64::try_from(audio.samples.len()).map_err(|_| PlanError::IntegerOverflow)?;

    let (status, evidence, error_code) = match detector_result {
        Ok(evidence) => match validate_evidence(&evidence, sample_count) {
            Ok(()) => (DetectorStatus::Available, evidence, None),
            Err(_) => (
                DetectorStatus::Unavailable,
                Vec::new(),
                Some(DetectorErrorCode::InvalidEvidence),
            ),
        },
        Err(error) => (DetectorStatus::Unavailable, Vec::new(), Some(error.code)),
    };
    let detector = DetectorRun {
        identity: detector_identity,
        status,
        evidence,
        error_code,
    };
    let planner = PlannerRun {
        version: PLANNER_VERSION.to_owned(),
        config,
    };
    let source = SourceFacts {
        sha256: audio.source_sha256.to_owned(),
        sample_rate_hz: audio.sample_rate_hz,
        channels: audio.channels,
        decoded_sample_count: sample_count,
    };

    let mut chunks = build_chunks(sample_count, &recognizer, &detector, &planner.config)?;
    let failures = detector
        .error_code
        .map(|code| PlanFailure {
            code: format!("detector_{}", detector_code_name(code)),
            recoverable: true,
        })
        .into_iter()
        .collect::<Vec<_>>();

    let material = IdentityMaterial {
        schema: PLAN_SCHEMA,
        revision: 0,
        source: &source,
        recognizer: &recognizer,
        detector: &detector,
        planner: &planner,
        chunks: chunk_material(&chunks),
        failures: &failures,
    };
    let plan_inputs_hash = canonical_hash(&material)?;
    let id = format!("plan_{plan_inputs_hash}");
    assign_chunk_ids(&mut chunks, &id)?;

    let plan = RecognitionPlan {
        schema: PLAN_SCHEMA.to_owned(),
        id,
        plan_inputs_hash,
        revision: 0,
        source,
        recognizer,
        detector,
        planner,
        chunks,
        failures,
    };
    validate_plan(&plan)?;
    Ok(plan)
}

fn validate_inputs(
    audio: &CanonicalAudio<'_>,
    recognizer: &RecognizerContract,
    config: &PlannerConfig,
) -> Result<(), PlanError> {
    if audio.sample_rate_hz != 16_000 {
        return Err(PlanError::InvalidAudio(
            "sample rate must be 16000 Hz".into(),
        ));
    }
    if audio.channels != 1 {
        return Err(PlanError::InvalidAudio("audio must be mono".into()));
    }
    if !valid_sha256(audio.source_sha256) {
        return Err(PlanError::InvalidAudio(
            "source SHA-256 must be 64 lowercase hexadecimal characters".into(),
        ));
    }
    if let Some((index, _)) = audio
        .samples
        .iter()
        .enumerate()
        .find(|(_, sample)| !sample.is_finite() || !(-1.0..=1.0).contains(*sample))
    {
        return Err(PlanError::InvalidAudio(format!(
            "sample {index} is not finite normalized f32 PCM"
        )));
    }
    if recognizer.max_submitted_samples == 0 {
        return Err(PlanError::InvalidConfiguration(
            "recognizer maximum must be non-zero".into(),
        ));
    }
    if config.minimum_chunk_samples == 0 {
        return Err(PlanError::InvalidConfiguration(
            "minimum chunk length must be non-zero".into(),
        ));
    }
    if config.minimum_chunk_samples > recognizer.max_submitted_samples {
        return Err(PlanError::InvalidConfiguration(
            "minimum chunk length exceeds recognizer maximum".into(),
        ));
    }
    if !config.speech_threshold.is_finite() || !(0.0..=1.0).contains(&config.speech_threshold) {
        return Err(PlanError::InvalidConfiguration(
            "speech threshold must be finite and in [0, 1]".into(),
        ));
    }
    if config.minimum_low_speech_samples == 0 {
        return Err(PlanError::InvalidConfiguration(
            "minimum low-speech length must be non-zero".into(),
        ));
    }
    if config.left_padding_samples != 0 || config.right_padding_samples != 0 {
        return Err(PlanError::InvalidConfiguration(
            "padding is unsupported in recognition-plan/v1-experimental".into(),
        ));
    }
    Ok(())
}

fn validate_detector_identity(identity: &DetectorIdentity) -> Result<(), PlanError> {
    if !valid_sha256(&identity.model_sha256) {
        return Err(PlanError::InvalidConfiguration(
            "detector model SHA-256 must be 64 lowercase hexadecimal characters".into(),
        ));
    }
    if identity.frame_samples == 0 || identity.sample_rate_hz != 16_000 {
        return Err(PlanError::InvalidConfiguration(
            "detector must use non-empty 16 kHz frames".into(),
        ));
    }
    Ok(())
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn validate_evidence(evidence: &[FrameEvidence], sample_count: u64) -> Result<(), PlanError> {
    if sample_count > 0 && evidence.is_empty() {
        return Err(PlanError::InvalidDetectorEvidence(
            "evidence is missing for a non-empty source".into(),
        ));
    }
    let mut previous_end = 0;
    for (index, frame) in evidence.iter().enumerate() {
        if frame.start_sample >= frame.end_sample || frame.end_sample > sample_count {
            return Err(PlanError::InvalidDetectorEvidence(format!(
                "frame {index} has an invalid range"
            )));
        }
        if index > 0 && frame.start_sample != previous_end {
            return Err(PlanError::InvalidDetectorEvidence(format!(
                "frame {index} is not consecutive"
            )));
        }
        if !frame.speech_probability.is_finite() || !(0.0..=1.0).contains(&frame.speech_probability)
        {
            return Err(PlanError::InvalidDetectorEvidence(format!(
                "frame {index} has an invalid probability"
            )));
        }
        previous_end = frame.end_sample;
    }
    if sample_count > 0
        && (!evidence.is_empty())
        && (evidence[0].start_sample != 0 || previous_end != sample_count)
    {
        return Err(PlanError::InvalidDetectorEvidence(
            "evidence does not cover the complete source".into(),
        ));
    }
    Ok(())
}

fn build_chunks(
    sample_count: u64,
    recognizer: &RecognizerContract,
    detector: &DetectorRun,
    config: &PlannerConfig,
) -> Result<Vec<RecognitionChunk>, PlanError> {
    let mut chunks = Vec::new();
    let mut core_start = 0_u64;
    while core_start < sample_count {
        let remaining = sample_count - core_start;
        let (core_end, boundary) = if remaining <= recognizer.max_submitted_samples {
            (
                sample_count,
                BoundaryDecision {
                    kind: BoundaryKind::SourceEnd,
                    valley: None,
                    detector_error_code: None,
                },
            )
        } else {
            let hard_end = core_start
                .checked_add(recognizer.max_submitted_samples)
                .ok_or(PlanError::IntegerOverflow)?;
            match detector.status {
                DetectorStatus::Unavailable => (
                    hard_end,
                    BoundaryDecision {
                        kind: BoundaryKind::HardLimitDetectorUnavailable,
                        valley: None,
                        detector_error_code: detector.error_code,
                    },
                ),
                DetectorStatus::Available => {
                    let minimum_end = core_start
                        .checked_add(config.minimum_chunk_samples)
                        .ok_or(PlanError::IntegerOverflow)?;
                    let search = SampleRange {
                        start_sample: minimum_end
                            .max(hard_end.saturating_sub(config.search_back_samples)),
                        end_sample: hard_end,
                    };
                    if let Some(candidate) = select_candidate(
                        &detector.evidence,
                        search,
                        config.speech_threshold,
                        config.minimum_low_speech_samples,
                    ) {
                        (
                            candidate.sample,
                            BoundaryDecision {
                                kind: BoundaryKind::VadValley,
                                valley: Some(ValleyEvidence {
                                    search,
                                    selected_run: candidate.run,
                                    mean_speech_probability: candidate.mean,
                                    frame_count: candidate.frame_count,
                                }),
                                detector_error_code: None,
                            },
                        )
                    } else {
                        (
                            hard_end,
                            BoundaryDecision {
                                kind: BoundaryKind::HardLimitNoCandidate,
                                valley: None,
                                detector_error_code: None,
                            },
                        )
                    }
                }
            }
        };
        if core_end <= core_start {
            return Err(PlanError::Invariant("planner made no progress".into()));
        }
        let range = SampleRange {
            start_sample: core_start,
            end_sample: core_end,
        };
        let ordinal = u32::try_from(chunks.len()).map_err(|_| PlanError::IntegerOverflow)?;
        chunks.push(RecognitionChunk {
            id: String::new(),
            ordinal,
            core: range,
            submitted: range,
            boundary,
            preflight: PreflightResult::Ready,
        });
        core_start = core_end;
    }
    Ok(chunks)
}

fn select_candidate(
    evidence: &[FrameEvidence],
    search: SampleRange,
    threshold: f32,
    minimum_length: u64,
) -> Option<Candidate> {
    let mut runs: Vec<Vec<&FrameEvidence>> = Vec::new();
    for frame in evidence.iter().filter(|frame| {
        frame.speech_probability < threshold
            && frame.end_sample > search.start_sample
            && frame.start_sample < search.end_sample
    }) {
        match runs.last_mut() {
            Some(run)
                if run
                    .last()
                    .is_some_and(|last| last.end_sample == frame.start_sample) =>
            {
                run.push(frame)
            }
            _ => runs.push(vec![frame]),
        }
    }
    runs.into_iter()
        .filter_map(|frames| {
            let start = frames.first()?.start_sample.max(search.start_sample);
            let end = frames.last()?.end_sample.min(search.end_sample);
            if end - start < minimum_length {
                return None;
            }
            let mean = (frames
                .iter()
                .map(|frame| f64::from(frame.speech_probability))
                .sum::<f64>()
                / frames.len() as f64) as f32;
            let sample = start + (end - start) / 2;
            Some(Candidate {
                run: SampleRange {
                    start_sample: start,
                    end_sample: end,
                },
                sample,
                mean,
                frame_count: frames.len() as u64,
            })
        })
        .min_by(|left, right| compare_candidates(left, right, search.end_sample))
}

// IEEE-754 total ordering is used for the primary comparison. Valid probabilities
// exclude NaN; bit-identical means tie. Ties prefer the point nearest the hard end,
// then the later point.
fn compare_candidates(left: &Candidate, right: &Candidate, hard_end: u64) -> Ordering {
    left.mean
        .total_cmp(&right.mean)
        .then_with(|| (hard_end - left.sample).cmp(&(hard_end - right.sample)))
        .then_with(|| right.sample.cmp(&left.sample))
}

fn chunk_material(chunks: &[RecognitionChunk]) -> Vec<ChunkIdentityMaterial<'_>> {
    chunks
        .iter()
        .map(|chunk| ChunkIdentityMaterial {
            ordinal: chunk.ordinal,
            core: chunk.core,
            submitted: chunk.submitted,
            boundary: &chunk.boundary,
            preflight: &chunk.preflight,
        })
        .collect()
}

fn canonical_hash(value: &impl Serialize) -> Result<String, PlanError> {
    let bytes =
        serde_json::to_vec(value).map_err(|error| PlanError::Serialization(error.to_string()))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn assign_chunk_ids(chunks: &mut [RecognitionChunk], plan_id: &str) -> Result<(), PlanError> {
    #[derive(Serialize)]
    struct ChunkId<'a> {
        plan_id: &'a str,
        ordinal: u32,
        core: SampleRange,
        submitted: SampleRange,
    }
    for chunk in chunks {
        let hash = canonical_hash(&ChunkId {
            plan_id,
            ordinal: chunk.ordinal,
            core: chunk.core,
            submitted: chunk.submitted,
        })?;
        chunk.id = format!("chunk_{hash}");
    }
    Ok(())
}

pub fn validate_plan(plan: &RecognitionPlan) -> Result<(), PlanError> {
    if plan.schema != PLAN_SCHEMA || plan.planner.version != PLANNER_VERSION {
        return Err(PlanError::Invariant(
            "unknown schema or planner version".into(),
        ));
    }
    let material = IdentityMaterial {
        schema: &plan.schema,
        revision: plan.revision,
        source: &plan.source,
        recognizer: &plan.recognizer,
        detector: &plan.detector,
        planner: &plan.planner,
        chunks: chunk_material(&plan.chunks),
        failures: &plan.failures,
    };
    let expected_hash = canonical_hash(&material)?;
    if plan.plan_inputs_hash != expected_hash || plan.id != format!("plan_{expected_hash}") {
        return Err(PlanError::Invariant("plan identity mismatch".into()));
    }
    if plan.source.decoded_sample_count == 0 {
        if !plan.chunks.is_empty() {
            return Err(PlanError::Invariant("empty source has chunks".into()));
        }
        return Ok(());
    }
    let mut expected_start = 0;
    let mut expected_chunks = plan.chunks.clone();
    assign_chunk_ids(&mut expected_chunks, &plan.id)?;
    for (index, (chunk, expected)) in plan.chunks.iter().zip(expected_chunks).enumerate() {
        if chunk.ordinal as usize != index || chunk.id != expected.id {
            return Err(PlanError::Invariant(
                "chunk ordinal or identity mismatch".into(),
            ));
        }
        if chunk.core.is_empty() || chunk.core.start_sample != expected_start {
            return Err(PlanError::Invariant(
                "core ranges do not partition source".into(),
            ));
        }
        if chunk.submitted != chunk.core
            || chunk.submitted.end_sample > plan.source.decoded_sample_count
            || chunk.submitted.len() > plan.recognizer.max_submitted_samples
        {
            return Err(PlanError::Invariant("submitted range is illegal".into()));
        }
        match (
            &chunk.boundary.kind,
            &chunk.boundary.valley,
            chunk.boundary.detector_error_code,
        ) {
            (BoundaryKind::VadValley, Some(valley), None)
                if valley.search.start_sample <= chunk.core.end_sample
                    && chunk.core.end_sample <= valley.search.end_sample
                    && valley.selected_run.start_sample <= chunk.core.end_sample
                    && chunk.core.end_sample <= valley.selected_run.end_sample => {}
            (BoundaryKind::SourceEnd | BoundaryKind::HardLimitNoCandidate, None, None) => {}
            (BoundaryKind::HardLimitDetectorUnavailable, None, Some(_)) => {}
            _ => {
                return Err(PlanError::Invariant(
                    "boundary evidence is inconsistent".into(),
                ))
            }
        }
        expected_start = chunk.core.end_sample;
    }
    if expected_start != plan.source.decoded_sample_count {
        return Err(PlanError::Invariant(
            "final core does not reach source end".into(),
        ));
    }
    Ok(())
}

fn detector_code_name(code: DetectorErrorCode) -> &'static str {
    match code {
        DetectorErrorCode::ModelNotFound => "model_not_found",
        DetectorErrorCode::ModelHashMismatch => "model_hash_mismatch",
        DetectorErrorCode::InvalidModel => "invalid_model",
        DetectorErrorCode::RuntimeInitialization => "runtime_initialization",
        DetectorErrorCode::InferenceFailed => "inference_failed",
        DetectorErrorCode::MalformedOutput => "malformed_output",
        DetectorErrorCode::InvalidEvidence => "invalid_evidence",
    }
}
