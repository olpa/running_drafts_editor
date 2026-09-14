use std::fs;

use running_drafts_editor::{
    document::{VisibleTokenId, VisibleTokenOrigin},
    persistence::{
        export_text, load_document, load_project, save_document, save_project, DocumentIoError,
    },
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
fn attention_marks_persist_export_exactly_and_follow_history() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.json");
    let saved = directory.path().join("saved.json");
    let exported = directory.path().join("draft.txt");
    baseline(&input, "missing.wav");
    let mut document = load_document(&input).unwrap();

    document.mark_attention(1, 1).unwrap();
    document.mark_attention(1, 2).unwrap();
    assert!(document
        .mark_attention(1, 1)
        .unwrap_err()
        .contains("already marked"));
    export_text(&exported, &document).unwrap();
    assert_eq!(
        fs::read_to_string(&exported).unwrap(),
        "⚑hello⚑ exact pseudo text "
    );
    document.split_paragraph(1, 1).unwrap();
    assert_eq!(document.attention_marks().len(), 2);
    assert!(document.is_attention_marked(document.token(2, 1).unwrap().id()));
    document.merge_paragraphs(1).unwrap();

    save_document(&saved, &document).unwrap();
    let mut reopened = load_document(&saved).unwrap();
    assert_eq!(reopened.attention_marks().len(), 2);
    reopened.replace_text(1, 1, 1, 1, "fixed".into()).unwrap();
    assert_eq!(reopened.attention_marks().len(), 1);
    assert_eq!(reopened.undo(1), 1);
    assert_eq!(reopened.attention_marks().len(), 2);
    assert_eq!(reopened.redo(1), 1);
    assert_eq!(reopened.attention_marks().len(), 1);
    reopened.unmark_attention(1, 2).unwrap();
    assert!(reopened
        .unmark_attention(1, 2)
        .unwrap_err()
        .contains("not marked"));
}

#[test]
fn export_writes_only_exact_visible_text_with_blank_lines_between_paragraphs() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.json");
    let exported = directory.path().join("draft.txt");
    baseline(&input, "missing.wav");
    let mut document = load_document(&input).unwrap();

    // Recognition tokens, chunk markers, confidence, alternatives, and audio
    // mappings remain stored, but none of them are rendered into the export.
    document.split_paragraph(1, 1).unwrap();
    export_text(&exported, &document).unwrap();

    assert_eq!(
        fs::read_to_string(exported).unwrap(),
        "hello\n\n exact pseudo text "
    );
}

#[test]
fn malformed_and_duplicate_attention_marks_are_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.json");
    baseline(&input, "missing.wav");
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&input).unwrap()).unwrap();
    let unknown = json!({"token_id":{"kind":"pseudo","id":"missing"}});
    value["attention_marks"] = json!([unknown]);
    fs::write(&input, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(load_document(&input)
        .unwrap_err()
        .to_string()
        .contains("unknown visible token"));

    let current = json!({"token_id":{"kind":"pseudo","id":"user:1"}});
    value["attention_marks"] = json!([current.clone(), current]);
    fs::write(&input, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(load_document(&input)
        .unwrap_err()
        .to_string()
        .contains("more than one attention mark"));
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
fn project_exposes_a_document_without_supporting_work_state() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.json");
    let output = directory.path().join("output.json");
    baseline(&input, "missing.wav");

    let project = load_project(&input).unwrap();
    let document = serde_json::to_value(project.document()).unwrap();
    assert_eq!(document["id"], "document:run");
    assert!(document.get("paragraphs").is_some());
    assert!(document.get("schema").is_none());
    assert!(document.get("audio_sources").is_none());
    assert!(document.get("recognition_token_evidence").is_none());
    assert!(document.get("edit_history").is_none());

    save_project(&output, &project).unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
    assert_eq!(saved["schema"], "rde-document/v1-experimental");
    assert_eq!(saved["id"], "document:run");
    assert!(saved.get("document").is_none());
}

#[test]
fn edit_history_survives_save_and_reopen_without_copying_recognition_backing() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.json");
    let output = directory.path().join("output.json");
    baseline(&input, "missing.wav");
    let mut document = load_document(&input).unwrap();

    document
        .replace_text(1, 1, 1, 1, "corrected".into())
        .unwrap();
    assert_eq!(document.edit_history_len(), 1);
    save_document(&output, &document).unwrap();

    let mut reopened = load_document(&output).unwrap();
    assert_eq!(reopened.edit_history_len(), 1);
    assert_eq!(
        reopened.paragraphs()[0].text(),
        "corrected exact pseudo text "
    );
    let encoded: serde_json::Value = serde_json::from_slice(&fs::read(&output).unwrap()).unwrap();
    assert!(encoded["edit_history"][0]["before"]
        .get("recognition_runs")
        .is_none());
    assert!(encoded["edit_history"][0]["before"]
        .get("recognition_token_evidence")
        .is_none());

    assert_eq!(reopened.undo(4), 1);
    assert_eq!(reopened.paragraphs()[0].text(), "hello exact pseudo text ");
    save_document(&output, &reopened).unwrap();
    let mut reopened = load_document(&output).unwrap();
    assert_eq!(reopened.redo_history_len(), 1);
    assert_eq!(reopened.redo(4), 1);
    assert_eq!(
        reopened.paragraphs()[0].text(),
        "corrected exact pseudo text "
    );
}

#[test]
fn legacy_chunk_boundary_history_remains_readable_and_reachable() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("legacy-split.json");
    let output = directory.path().join("saved.json");
    let first_id = json!({
        "kind": "recognition", "run_id": "run", "segment_id": "s", "token_index": 0
    });
    let second_id = json!({
        "kind": "recognition", "run_id": "run", "segment_id": "s", "token_index": 1
    });
    let tokens = json!([
        {"id": first_id, "text": "old", "origin": {"kind": "recognition"}},
        {"id": second_id, "text": " text", "origin": {"kind": "recognition"}}
    ]);
    let parent_paragraph = json!([{
        "id": "paragraph", "revision": 1, "tokens": tokens,
        "chunk_boundaries": [{"chunk_id": "parent", "after_tokens": 2}]
    }]);
    let parent_mapping = json!([{
        "chunk_id": "parent", "source_id": "audio",
        "range": {"start_sample": 0, "end_sample": 200}
    }]);
    let value = json!({
        "schema": "rde-document/v1-experimental",
        "id": "document:legacy-split",
        "paragraphs": [{
            "id": "paragraph", "revision": 2, "tokens": tokens,
            "chunk_boundaries": [
                {"chunk_id": "left", "after_tokens": 1},
                {"chunk_id": "right", "after_tokens": 2}
            ]
        }],
        "audio_sources": [{"id": "audio", "canonical_sample_count": 200}],
        "chunk_audio_mappings": [
            {"chunk_id": "left", "source_id": "audio", "range": {"start_sample": 0, "end_sample": 100}},
            {"chunk_id": "right", "source_id": "audio", "range": {"start_sample": 100, "end_sample": 200}}
        ],
        "replay_chunks": [
            {"id": "parent", "parent_ids": [], "token_ids": [first_id, second_id]},
            {"id": "left", "parent_ids": ["parent"], "token_ids": [first_id]},
            {"id": "right", "parent_ids": ["parent"], "token_ids": [second_id]}
        ],
        "recognition_token_evidence": [
            {"token_id": first_id, "recognition_token_id": 10, "probability": 0.8, "alternatives": []},
            {"token_id": second_id, "recognition_token_id": 11, "probability": 0.7, "alternatives": []}
        ],
        "next_structure_id": 2,
        "edit_history": [{"before": {
            "paragraphs": parent_paragraph,
            "chunk_audio_mappings": parent_mapping,
            "token_audio_mappings": [],
            "replay_chunks": [],
            "next_structure_id": 0
        }}]
    });
    fs::write(&input, serde_json::to_vec_pretty(&value).unwrap()).unwrap();

    let mut document = load_document(&input).unwrap();
    assert_eq!(document.paragraphs()[0].text(), "old text");
    assert_eq!(document.paragraphs()[0].chunk_boundaries().len(), 2);
    assert_eq!(document.recognition_token_evidence().len(), 2);

    assert_eq!(document.undo(1), 1);
    assert_eq!(document.paragraphs()[0].text(), "old text");
    assert_eq!(document.paragraphs()[0].chunk_boundaries().len(), 1);
    assert_eq!(
        document.paragraphs()[0].chunk_boundaries()[0].chunk_id(),
        "parent"
    );
    assert_eq!(document.recognition_token_evidence().len(), 2);
    save_document(&output, &document).unwrap();

    let mut reopened = load_document(&output).unwrap();
    assert_eq!(reopened.redo(1), 1);
    assert_eq!(reopened.paragraphs()[0].text(), "old text");
    assert_eq!(
        reopened.paragraphs()[0]
            .chunk_boundaries()
            .iter()
            .map(|marker| marker.chunk_id())
            .collect::<Vec<_>>(),
        vec!["left", "right"]
    );
    assert_eq!(reopened.recognition_token_evidence().len(), 2);
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
