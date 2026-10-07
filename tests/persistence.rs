mod common;
use running_drafts_editor::{
    persistence::{export_text, load_project, save_project},
    project::{Project, TranscriptionSettings},
    transcription::{
        ChunkBoundaryReason, DecodeSpanItem, DecoderTimestamps, ParagraphBreak, TranscriptionStatus,
    },
};
use std::fs;

#[test]
fn representative_v4_fixture_is_readable_and_recoverable() {
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/readable-v4.rde.json"
    );
    let encoded = fs::read_to_string(fixture).unwrap();
    let project = load_project(std::path::Path::new(fixture)).unwrap();

    assert!(encoded.find("\"document\"").unwrap() < encoded.find("\"_inspection\"").unwrap());
    assert!(!encoded.contains("\"kind\""));
    assert!(!encoded.contains("\"transcriber\""));
    assert!(encoded.contains("\"text\": \"First chunk.\""));
    assert!(encoded.contains("\"text\": \"Previous spoken context\""));
    let compact_alternative = concat!(
        "\"alternatives\": [\n",
        "                    {\"token_id\":50364,\"text\":\"[_BEG_]\",\"probability\":0.68888783},\n",
        "                    {\"token_id\":1562,\"text\":\" ever\",\"probability\":0.28583738}\n",
        "                  ]"
    );
    assert!(encoded.contains(compact_alternative));
    assert!(encoded.contains("example trailing decode observation"));
    assert_eq!(project.paragraphs().len(), 2);
    assert_eq!(project.paragraph(1).unwrap().text(), "First chunk.");
    assert_eq!(project.paragraph(2).unwrap().text(), "Second chunk.");
    assert_eq!(project.edit_history_len(), 3);
    assert_eq!(project.attention_marks().len(), 1);

    let directory = tempfile::tempdir().unwrap();
    let round_trip = directory.path().join("round-trip.rde.json");
    save_project(&round_trip, &project).unwrap();
    assert_eq!(load_project(&round_trip).unwrap(), project);
    assert!(fs::read_to_string(round_trip)
        .unwrap()
        .contains(compact_alternative));
}

#[test]
fn one_current_transcription_per_chunk_and_optional_inspection_export() {
    let mut result = common::batch("initial", &[" hello ", "\t世界"]);
    let chunk = result.chunks_mut().next().unwrap();
    chunk.boundary.reason = ChunkBoundaryReason::LongPause;
    chunk.transcription.as_mut().unwrap().boundary.reason = ChunkBoundaryReason::LongPause;
    result.decode_spans[0]
        .content
        .insert(1, DecodeSpanItem::ParagraphBreak(ParagraphBreak));
    let project = Project::from_initial_transcription(&result);
    assert_eq!(project.transcriptions().len(), 2);
    assert_ne!(
        project.current_transcription(1, 1).unwrap().id,
        project.current_transcription(2, 1).unwrap().id
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("project.json");
    save_project(&path, &project).unwrap();
    assert_eq!(load_project(&path).unwrap(), project);
    let encoded = fs::read_to_string(&path).unwrap();
    let value: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    assert!(!encoded.contains("recognition"));
    assert!(!encoded.contains("pseudo"));
    assert!(!encoded.contains("transcription_runs"));
    assert!(value.get("transcriptions").is_none());
    assert_eq!(value["document"]["content"][0]["type"], "chunk");
    assert!(value.get("paragraphs").is_none());
    assert!(value.get("chunk_audio_mappings").is_none());
    assert!(value.get("attention_marks").is_none());
    assert_eq!(
        value["document"]["content"][0]["audio"]["source_id"],
        value["audio_sources"][0]["id"]
    );
    assert_eq!(
        value["document"]["content"][0]["audio"]["range"]["start_sample"],
        0
    );
    assert_eq!(value["document"]["content"][1]["type"], "paragraph_break");
    assert_eq!(
        value["document"]["content"][2]["previous_chunk_id"],
        value["document"]["content"][0]["id"]
    );
    assert_eq!(
        value["document"]["content"][0]["transcription"]["text"],
        " hello "
    );
    assert_eq!(
        value["document"]["content"][0]["transcription"]["profile_id"],
        value["transcription_profiles"][0]["id"]
    );
    assert!(value["document"]["content"][0]["transcription"]
        .get("config")
        .is_none());
    assert!(value["document"]["content"][0]["transcription"]
        .get("source")
        .is_none());
    assert!(value["document"]["content"][0]["transcription"]
        .get("transcriber")
        .is_none());
    assert_eq!(
        value["transcription_profiles"][0]["config"]["language"],
        "auto"
    );
    assert_eq!(
        value["document"]["content"][2]["transcription"]["text"],
        "\t世界"
    );
    assert!(value["_inspection"]["decode_spans"].is_array());
    export_text(&dir.path().join("text"), &project).unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join("text")).unwrap(),
        " hello \n\n\t世界"
    );
}

#[test]
fn unavailable_token_alignment_preserves_text_evidence_and_structural_history() {
    let mut result = common::batch("initial", &[" exact text \t", "other"]);
    result
        .chunks_mut()
        .next()
        .unwrap()
        .transcription
        .as_mut()
        .unwrap()
        .segments[0]
        .tokens[0]
        .text = "mismatched".into();
    let mut project = Project::from_initial_transcription(&result);
    assert_eq!(project.chunk_has_tokens(1, 1), Some(false));
    assert_eq!(project.paragraph(1).unwrap().text(), " exact text \tother");
    assert_eq!(
        project.transcriptions()[0].segments[0].tokens[0].text,
        "mismatched"
    );
    project.split_paragraph(1, 1).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("project");
    save_project(&path, &project).unwrap();
    let mut reopened = load_project(&path).unwrap();
    assert_eq!(reopened, project);
    export_text(&dir.path().join("text"), &reopened).unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join("text")).unwrap(),
        " exact text \t\n\nother"
    );
    reopened.undo(1);
    assert_eq!(reopened.paragraph(1).unwrap().text(), " exact text \tother");
    reopened.redo(1);
    assert_eq!(reopened.paragraph(1).unwrap().text(), " exact text \t");
}

#[test]
fn settings_transcription_marks_issues_and_redo_survive_save_reopen() {
    let mut initial = common::batch("initial", &["old"]);
    initial.config.language = "en".into();
    common::synchronize_initial_transcriptions(&mut initial);
    let mut project = Project::from_initial_transcription(&initial);
    project
        .configure_initial_settings(Some("old-model".into()), "en".into())
        .unwrap();
    let old_id = project.chunk_token(1, 1, 1).unwrap().id().clone();
    project.mark_attention(1, 1).unwrap();
    project.resolve_issue(vec![old_id.clone()]);
    let mut next = common::batch("later", &["new"]);
    next.config.language = "de".into();
    let next = common::proposal(&project, next);
    project
        .install_transcription(
            1,
            1,
            next,
            TranscriptionSettings {
                model: Some("new-model".into()),
                language: "de".into(),
            },
        )
        .unwrap();
    assert!(project.attention_marks().is_empty());
    assert!(project.resolved_issues().is_empty());
    let current = project.current_transcription(1, 1).unwrap();
    assert_eq!(
        current.previous_id.as_deref(),
        Some(old_id.transcription_id.as_str())
    );
    project.undo(1);
    assert_eq!(project.settings().language, "en");
    assert_eq!(
        project.settings().model.as_deref(),
        Some(std::path::Path::new("old-model"))
    );
    assert_eq!(project.attention_marks()[0].token_identity(), &old_id);
    assert_eq!(project.resolved_issues().len(), 1);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("project");
    save_project(&path, &project).unwrap();
    let mut reopened = load_project(&path).unwrap();
    reopened.redo(1);
    assert_eq!(reopened.paragraph(1).unwrap().text(), "new");
    assert_eq!(reopened.settings().language, "de");
    assert_eq!(
        reopened.settings().model.as_deref(),
        Some(std::path::Path::new("new-model"))
    );
    assert_eq!(reopened.current_transcription(1, 1).unwrap().chunk_id, "c0");
}

#[test]
fn malformed_current_and_historical_references_are_rejected_without_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("project");
    let mut project = common::project(&["one", "two"]);
    project.split_paragraph(1, 1).unwrap();
    let encoded = serde_json::to_value(&project).unwrap();
    for target in ["current", "history"] {
        let mut invalid = encoded.clone();
        let content = if target == "current" {
            &mut invalid["document"]["content"]
        } else {
            &mut invalid["edit_history"][0]["before"]["document"]["content"]
        };
        content[0]["transcription"]["chunk_id"] = "missing".into();
        fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
        assert!(load_project(&path).is_err());
    }
    save_project(&path, &project).unwrap();
    let good = fs::read(&path).unwrap();
    let mut invalid = encoded;
    invalid["document"]["content"][0]["transcription"]["audio_range"]["end_sample"] = 99.into();
    fs::write(
        dir.path().join("invalid"),
        serde_json::to_vec(&invalid).unwrap(),
    )
    .unwrap();
    assert!(load_project(&dir.path().join("invalid")).is_err());
    assert_eq!(fs::read(&path).unwrap(), good);

    let mut invalid = serde_json::to_value(&project).unwrap();
    invalid["document"]["content"][2]["previous_chunk_id"] = "missing".into();
    fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
    assert!(load_project(&path).is_err());

    let mut invalid = serde_json::to_value(&project).unwrap();
    invalid["misspelled_authoritative_field"] = true.into();
    fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
    assert!(load_project(&path).is_err());

    let mut invalid = serde_json::to_value(&project).unwrap();
    invalid["document"]["content"][0]["misspelled_field"] = true.into();
    fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
    assert!(load_project(&path).is_err());

    let mut invalid = serde_json::to_value(&project).unwrap();
    invalid["document"]["content"][1]["misspelled_field"] = true.into();
    fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
    assert!(load_project(&path).is_err());

    let mut allowed = serde_json::to_value(&project).unwrap();
    allowed["_inspection"] = serde_json::json!({
        "arbitrary_future_diagnostic": { "explanation": "ignored" }
    });
    allowed["document"]["content"][0]["transcription"]["_inspection"] =
        serde_json::json!({ "future_prompt_view": [1, 2, 3] });
    fs::write(&path, serde_json::to_vec(&allowed).unwrap()).unwrap();
    assert!(load_project(&path).is_ok());
}

#[test]
fn historical_format_is_not_supported_and_missing_audio_is_harmless() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("project");
    let project = Project::from_initial_transcription_with_source(
        &common::batch("initial", &["text"]),
        Some(std::path::Path::new("/missing/audio.wav")),
    );
    save_project(&path, &project).unwrap();
    assert_eq!(
        load_project(&path).unwrap().paragraph(1).unwrap().text(),
        "text"
    );
    let value = serde_json::to_value(&project).unwrap();
    for old_schema in [
        "rde-project/v1-experimental",
        "rde-document/v1-experimental",
    ] {
        let mut old = value.clone();
        old["schema"] = old_schema.into();
        fs::write(&path, serde_json::to_vec(&old).unwrap()).unwrap();
        assert!(load_project(&path)
            .unwrap_err()
            .to_string()
            .contains("unsupported project schema"));
    }
}

#[test]
fn failed_install_preserves_redo_and_current_state() {
    let mut project = common::project(&["one", "two"]);
    project.split_paragraph(1, 1).unwrap();
    project.undo(1);
    let before = project.clone();
    let mut wrong_target = common::proposal(&project, common::batch("bad", &["a", "b"]));
    wrong_target.chunk_id = "other-chunk".into();
    assert!(project
        .install_transcription(1, 1, wrong_target, TranscriptionSettings::default())
        .is_err());
    assert_eq!(project, before);
    let mut altered = common::batch("bad-boundary", &["a"]);
    altered.chunks_mut().next().unwrap().audio_range.end_sample = 99;
    let altered = common::proposal(&project, altered);
    assert!(project
        .install_transcription(1, 1, altered, TranscriptionSettings::default())
        .is_err());
    assert_eq!(project, before);
}

#[test]
fn new_action_after_undo_clears_redo_and_follows_current_transcription() {
    let mut project = common::project(&["old"]);
    project
        .install_transcription(
            1,
            1,
            common::proposal(&project, common::batch("first", &["first"])),
            TranscriptionSettings::default(),
        )
        .unwrap();
    project.undo(1);
    let previous = project.current_transcription(1, 1).unwrap().id.clone();
    project
        .install_transcription(
            1,
            1,
            common::proposal(&project, common::batch("second", &["second"])),
            TranscriptionSettings::default(),
        )
        .unwrap();
    assert_eq!(project.redo_history_len(), 0);
    assert_eq!(
        project
            .current_transcription(1, 1)
            .unwrap()
            .previous_id
            .as_deref(),
        Some(previous.as_str())
    );
    assert_eq!(
        project
            .chunk_audio_mapping("c0")
            .unwrap()
            .range()
            .end_sample,
        100
    );
}

#[test]
fn duplicate_attention_targets_and_token_audio_mappings_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("project");
    let mut project = common::project(&["text"]);
    project.mark_attention(1, 1).unwrap();
    let mut value = serde_json::to_value(&project).unwrap();
    let annotations = value["document"]["content"][0]["annotations"]
        .as_array_mut()
        .unwrap();
    annotations.push(annotations[0].clone());
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(load_project(&path).is_err());
    let mut value = serde_json::to_value(&project).unwrap();
    let mapping = value["token_audio_mappings"][0].clone();
    value["token_audio_mappings"]
        .as_array_mut()
        .unwrap()
        .push(mapping);
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(load_project(&path).is_err());
}

#[test]
fn overlapping_finalized_chunk_ranges_are_rejected() {
    let mut result = common::batch("overlap", &["one", "two"]);
    let second = result.chunks_mut().nth(1).unwrap();
    second.audio_range.start_sample = 50;
    second
        .transcription
        .as_mut()
        .unwrap()
        .audio_range
        .start_sample = 50;
    let project = Project::from_initial_transcription(&result);
    let dir = tempfile::tempdir().unwrap();
    let error = save_project(&dir.path().join("project"), &project).unwrap_err();

    assert!(error
        .to_string()
        .contains("invalid Chunk in Document content"));
}

#[test]
fn special_tokens_do_not_get_addresses_but_empty_text_tokens_do() {
    let mut result = common::batch("initial", &[""]);
    let project = Project::from_initial_transcription(&result);
    assert_eq!(project.chunk_has_tokens(1, 1), Some(true));
    assert_eq!(project.chunk_token(1, 1, 1).unwrap().text(), "");
    result
        .chunks_mut()
        .next()
        .unwrap()
        .transcription
        .as_mut()
        .unwrap()
        .segments[0]
        .tokens[0]
        .is_special = true;
    let project = Project::from_initial_transcription(&result);
    assert_eq!(project.chunk_has_tokens(1, 1), Some(false));
    assert!(project.chunk_token(1, 1, 1).is_none());
    let dir = tempfile::tempdir().unwrap();
    save_project(&dir.path().join("project"), &project).unwrap();
    assert_eq!(load_project(&dir.path().join("project")).unwrap(), project);
}

#[test]
fn failed_atomic_replacement_preserves_existing_target_and_cleans_its_temporary_file() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("target");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("keep"), "unchanged").unwrap();
    assert!(save_project(&target, &common::project(&["text"])).is_err());
    assert_eq!(
        fs::read_to_string(target.join("keep")).unwrap(),
        "unchanged"
    );
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn initial_configuration_cannot_bypass_settings_history_after_user_actions() {
    let mut project = common::project(&["one", "two"]);
    project
        .configure_initial_settings(Some("initial-model".into()), "auto".into())
        .unwrap();
    project.split_paragraph(1, 1).unwrap();
    project.undo(1);
    let before = project.clone();
    assert!(project
        .configure_initial_settings(Some("another-model".into()), "auto".into())
        .is_err());
    assert_eq!(project, before);
}

#[test]
fn failed_initial_decoding_keeps_its_circumstances_even_without_finalized_chunks() {
    let mut result = common::batch("failed-initial", &["unused"]);
    result.decode_spans[0].content.clear();
    result.status = running_drafts_editor::transcription::TranscriptionStatus::Failed;
    result.config.language = "de".into();
    result.decode_spans[0].hypotheses.clear();
    result.decode_spans[0].accepted_segment_ids.clear();
    result.decode_spans[0].error = Some("synthetic decode failure".into());
    let project = Project::from_initial_transcription(&result);
    assert!(project.transcriptions().is_empty());
    let dir = tempfile::tempdir().unwrap();
    save_project(&dir.path().join("project"), &project).unwrap();
    let path = dir.path().join("project");
    let exported: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let reopened = load_project(&path).unwrap();
    assert_eq!(reopened, project);
    assert_eq!(exported["_inspection"]["config"]["language"], "de");
    assert_eq!(
        exported["_inspection"]["source"]["decoded_sample_count"],
        100
    );
    assert_eq!(exported["_inspection"]["status"], "failed");
    assert!(reopened.decode_spans().is_empty());
}

#[test]
fn unlocated_raw_timestamp_evidence_is_exported_but_ignored_on_import() {
    let mut result = common::batch("partial-initial", &["unlocated"]);
    let span = &mut result.decode_spans[0];
    span.content.clear();
    span.accepted_segment_ids.clear();
    span.hypotheses[0].raw_timestamps = Some(DecoderTimestamps {
        start: 9,
        end: 4,
        samples_per_unit: 160,
    });
    span.hypotheses[0].audio_range = None;
    span.hypotheses[0].tokens[0].raw_timestamps = Some(DecoderTimestamps {
        start: -1,
        end: -1,
        samples_per_unit: 160,
    });
    span.hypotheses[0].tokens[0].audio_range = None;
    result.status = TranscriptionStatus::Partial;
    let project = Project::from_initial_transcription(&result);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("partial.rde.json");

    save_project(&path, &project).unwrap();
    let exported: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let reopened = load_project(&path).unwrap();

    assert_eq!(reopened, project);
    assert!(reopened.decode_spans().is_empty());
    let evidence = &exported["_inspection"]["decode_spans"][0]["hypotheses"][0];
    assert_eq!(
        evidence["raw_timestamps"]["start"],
        result.decode_spans[0].hypotheses[0]
            .raw_timestamps
            .unwrap()
            .start
    );
    assert!(evidence["raw_timestamps"].get("samples_per_unit").is_none());
    assert!(evidence["tokens"][0]["raw_timestamps"]
        .get("samples_per_unit")
        .is_none());
    assert!(evidence.get("audio_range").is_none());
    assert!(evidence["tokens"][0].get("audio_range").is_none());
}
