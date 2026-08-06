use proptest::prelude::*;
use running_drafts_editor::chunking::*;

const HASH_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const HASH_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn detector_identity() -> DetectorIdentity {
    DetectorIdentity {
        name: "fixture-detector".into(),
        version: "1".into(),
        model_sha256: HASH_B.into(),
        frame_samples: 100,
        sample_rate_hz: 16_000,
        runtime: "fixture-cpu".into(),
    }
}

fn recognizer(max: u64) -> RecognizerContract {
    RecognizerContract {
        name: "fixture-recognizer".into(),
        version: "1".into(),
        max_submitted_samples: max,
    }
}

fn config() -> PlannerConfig {
    PlannerConfig {
        search_back_samples: 300,
        minimum_chunk_samples: 200,
        speech_threshold: 0.5,
        minimum_low_speech_samples: 100,
        left_padding_samples: 0,
        right_padding_samples: 0,
    }
}

fn audio(samples: &[f32]) -> CanonicalAudio<'_> {
    CanonicalAudio {
        samples,
        sample_rate_hz: 16_000,
        channels: 1,
        source_sha256: HASH_A,
    }
}

fn unavailable() -> Result<Vec<FrameEvidence>, DetectorError> {
    Err(DetectorError::new(
        DetectorErrorCode::InferenceFailed,
        "fixture failure with a private path that must not be persisted",
    ))
}

#[test]
fn empty_audio_is_a_valid_empty_plan() {
    let plan = plan(
        &audio(&[]),
        recognizer(480_000),
        config(),
        detector_identity(),
        Ok(vec![]),
    )
    .unwrap();
    assert!(plan.chunks.is_empty());
    assert!(plan.failures.is_empty());
    validate_plan(&plan).unwrap();
}

#[test]
fn exact_limit_and_one_over_preserve_every_sample() {
    let exact = vec![0.0; 480_000];
    let exact_plan = plan(
        &audio(&exact),
        recognizer(480_000),
        config(),
        detector_identity(),
        unavailable(),
    )
    .unwrap();
    assert_eq!(exact_plan.chunks.len(), 1);
    assert_eq!(exact_plan.chunks[0].core.end_sample, 480_000);

    let over = vec![0.0; 480_001];
    let over_plan = plan(
        &audio(&over),
        recognizer(480_000),
        config(),
        detector_identity(),
        unavailable(),
    )
    .unwrap();
    assert_eq!(over_plan.chunks.len(), 2);
    assert_eq!(
        over_plan.chunks[0].core,
        SampleRange {
            start_sample: 0,
            end_sample: 480_000
        }
    );
    assert_eq!(
        over_plan.chunks[1].core,
        SampleRange {
            start_sample: 480_000,
            end_sample: 480_001
        }
    );
    assert_eq!(
        over_plan.chunks[0].boundary.kind,
        BoundaryKind::HardLimitDetectorUnavailable
    );
}

#[test]
fn qualifying_valley_midpoint_is_selected() {
    let samples = vec![0.0; 1_000];
    let evidence = (0..10)
        .map(|index| FrameEvidence {
            start_sample: index * 100,
            end_sample: (index + 1) * 100,
            speech_probability: if (4..6).contains(&index) { 0.1 } else { 0.9 },
        })
        .collect();
    let plan = plan(
        &audio(&samples),
        recognizer(600),
        config(),
        detector_identity(),
        Ok(evidence),
    )
    .unwrap();
    assert_eq!(plan.chunks[0].core.end_sample, 500);
    assert_eq!(plan.chunks[0].boundary.kind, BoundaryKind::VadValley);
    assert_eq!(
        plan.chunks[0]
            .boundary
            .valley
            .as_ref()
            .unwrap()
            .selected_run,
        SampleRange {
            start_sample: 400,
            end_sample: 600
        }
    );
}

#[test]
fn lowest_mean_then_nearest_hard_end_wins() {
    let samples = vec![0.0; 1_000];
    let probabilities = [0.9, 0.9, 0.9, 0.1, 0.9, 0.1, 0.9, 0.9, 0.9, 0.9];
    let evidence = probabilities
        .iter()
        .enumerate()
        .map(|(index, probability)| FrameEvidence {
            start_sample: index as u64 * 100,
            end_sample: (index as u64 + 1) * 100,
            speech_probability: *probability,
        })
        .collect();
    let mut cfg = config();
    cfg.search_back_samples = 400;
    let plan = plan(
        &audio(&samples),
        recognizer(700),
        cfg,
        detector_identity(),
        Ok(evidence),
    )
    .unwrap();
    assert_eq!(plan.chunks[0].core.end_sample, 550);
}

#[test]
fn malformed_evidence_is_recorded_and_falls_back() {
    let samples = vec![0.0; 1_000];
    let plan = plan(
        &audio(&samples),
        recognizer(600),
        config(),
        detector_identity(),
        Ok(vec![FrameEvidence {
            start_sample: 500,
            end_sample: 400,
            speech_probability: 0.1,
        }]),
    )
    .unwrap();
    assert_eq!(plan.detector.status, DetectorStatus::Unavailable);
    assert_eq!(
        plan.detector.error_code,
        Some(DetectorErrorCode::InvalidEvidence)
    );
    assert_eq!(plan.chunks[0].core.end_sample, 600);
    assert!(plan.detector.evidence.is_empty());
}

#[test]
fn invalid_audio_and_unsupported_padding_are_rejected() {
    let invalid = [f32::NAN];
    assert!(matches!(
        plan(
            &audio(&invalid),
            recognizer(10),
            config(),
            detector_identity(),
            Ok(vec![])
        ),
        Err(PlanError::InvalidAudio(_))
    ));
    let samples = [0.0];
    let mut cfg = config();
    cfg.left_padding_samples = 1;
    assert!(matches!(
        plan(
            &audio(&samples),
            recognizer(500),
            cfg,
            detector_identity(),
            Ok(vec![])
        ),
        Err(PlanError::InvalidConfiguration(_))
    ));
}

#[test]
fn serialization_and_ids_are_stable_and_path_free() {
    let samples = vec![0.0; 601];
    let first = plan(
        &audio(&samples),
        recognizer(600),
        config(),
        detector_identity(),
        unavailable(),
    )
    .unwrap();
    let second = plan(
        &audio(&samples),
        recognizer(600),
        config(),
        detector_identity(),
        unavailable(),
    )
    .unwrap();
    let first_json = serde_json::to_string_pretty(&first).unwrap();
    let golden: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/fallback_plan.json")).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&first_json).unwrap(),
        golden
    );
    assert_eq!(first, second);
    assert_eq!(first_json, serde_json::to_string_pretty(&second).unwrap());
    assert!(!first_json.contains("private path"));
    assert!(first.id.starts_with("plan_"));
    assert!(first
        .chunks
        .iter()
        .all(|chunk| chunk.id.starts_with("chunk_")));
}

proptest! {
    #[test]
    fn fixed_fallback_always_partitions_and_terminates(sample_count in 0usize..20_000, max in 1u64..2_000) {
        let samples = vec![0.0; sample_count];
        let mut cfg = config();
        cfg.minimum_chunk_samples = 1.min(max);
        let result = plan(&audio(&samples), recognizer(max), cfg, detector_identity(), unavailable()).unwrap();
        prop_assert!(result.chunks.len() <= sample_count.saturating_add(1));
        prop_assert!(result.chunks.iter().all(|chunk| chunk.core.len() <= max));
        prop_assert_eq!(result.chunks.last().map(|chunk| chunk.core.end_sample).unwrap_or(0), sample_count as u64);
        validate_plan(&result).unwrap();
    }
}
