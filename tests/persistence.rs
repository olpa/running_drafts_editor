use std::fs;

use running_drafts_editor::{
    document::{VisibleTokenId, VisibleTokenOrigin},
    persistence::{load_document, save_document, DocumentIoError},
};
use serde_json::json;

fn baseline(path: &std::path::Path, audio_path: &str) {
    let value = json!({
        "schema": "rde-document/v1-experimental",
        "id": "document:run",
        "paragraphs": [{
            "id": "paragraph:run:c1",
            "revision": 1,
            "tokens": [
                {
                    "id": {"kind": "recognition", "run_id": "run", "segment_id": "s1", "token_index": 0},
                    "text": "hello",
                    "origin": {"kind": "recognition"}
                },
                {
                    "id": {"kind": "pseudo", "id": "user:1"},
                    "text": " exact pseudo text ",
                    "origin": {"kind": "pseudo", "reason": "user text"}
                }
            ],
            "chunk_boundaries": [
                {"chunk_id": "c1", "after_tokens": 1},
                {"chunk_id": "c2", "after_tokens": 2}
            ]
        }],
        "audio_sources": [{
            "id": "audio:hash",
            "path": audio_path,
            "sha256": "hash",
            "canonical_sample_count": 32000
        }],
        "chunk_audio_mappings": [
            {"chunk_id": "c1", "source_id": "audio:hash", "range": {"start_sample": 0, "end_sample": 16000}},
            {"chunk_id": "c2", "source_id": "audio:hash", "range": {"start_sample": 16000, "end_sample": 32000}}
        ],
        "token_audio_mappings": [{
            "paragraph_id": "paragraph:run:c1",
            "paragraph_revision": 1,
            "token_id": {"kind": "recognition", "run_id": "run", "segment_id": "s1", "token_index": 0},
            "source_id": "audio:hash",
            "range": {"start_sample": 100, "end_sample": 8000},
            "alignment": "exact"
        }],
        "recognition_token_evidence": [{
            "token_id": {"kind": "recognition", "run_id": "run", "segment_id": "s1", "token_index": 0},
            "recognition_token_id": 100,
            "probability": 0.75,
            "alternatives": [
                {"token_id": 100, "text": "hello", "probability": 0.75},
                {"token_id": 101, "text": "hullo", "probability": 0.2},
                {"token_id": 50257, "text": "", "probability": 0.05}
            ]
        }],
        "ignored_future_field": {"safe": true}
    });
    fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}

#[test]
fn exact_tokens_ids_markers_and_audio_mappings_round_trip() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.json");
    let output = directory.path().join("output.json");
    baseline(&input, "missing.wav");

    let document = load_document(&input).unwrap();
    assert_eq!(document.paragraphs()[0].text(), "hello exact pseudo text ");
    assert!(matches!(
        document.paragraphs()[0].tokens()[0].id(),
        VisibleTokenId::Recognition { token_index: 0, .. }
    ));
    assert!(matches!(
        document.paragraphs()[0].tokens()[1].origin(),
        VisibleTokenOrigin::Pseudo { reason } if reason == "user text"
    ));
    assert_eq!(
        document.paragraphs()[0].chunk_boundaries()[1].after_tokens(),
        2
    );
    assert_eq!(
        document.chunk_audio_mappings()[1].range().start_sample,
        16000
    );
    assert_eq!(document.token_audio_mappings()[0].range().start_sample, 100);
    assert_eq!(document.alternatives(1, 1).unwrap().len(), 3);
    assert_eq!(document.alternatives(1, 1).unwrap()[2].text(), "");

    save_document(&output, &document).unwrap();
    assert_eq!(load_document(&output).unwrap(), document);
}

#[test]
fn missing_audio_does_not_prevent_loading_visible_text() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("document.json");
    baseline(&input, "/definitely/not/present.wav");

    let document = load_document(&input).unwrap();

    assert_eq!(document.paragraphs()[0].tokens().len(), 2);
    assert!(!document.audio_sources()[0].path().unwrap().exists());
}

#[test]
fn rejects_unsupported_schema_and_invalid_authoritative_structure() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("document.json");
    baseline(&input, "missing.wav");
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&input).unwrap()).unwrap();
    value["schema"] = json!("rde-document/v999");
    fs::write(&input, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(matches!(
        load_document(&input),
        Err(DocumentIoError::UnsupportedSchema { .. })
    ));

    value["schema"] = json!("rde-document/v1-experimental");
    value["paragraphs"][0]["chunk_boundaries"][1]["after_tokens"] = json!(3);
    fs::write(&input, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(matches!(
        load_document(&input),
        Err(DocumentIoError::Invalid(_))
    ));
}

#[test]
fn failed_atomic_replacement_keeps_existing_target() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.json");
    let target_directory = directory.path().join("target");
    baseline(&input, "missing.wav");
    fs::create_dir(&target_directory).unwrap();
    fs::write(target_directory.join("sentinel"), "kept").unwrap();
    let document = load_document(&input).unwrap();

    assert!(save_document(&target_directory, &document).is_err());
    assert_eq!(
        fs::read_to_string(target_directory.join("sentinel")).unwrap(),
        "kept"
    );
    assert_eq!(
        fs::read_dir(directory.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp"))
            .count(),
        0
    );
}
