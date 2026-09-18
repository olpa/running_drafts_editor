mod common;
use running_drafts_editor::{
    persistence::{export_text, load_project, save_project},
    project::{Project, TranscriptionSettings},
    transcription::ChunkBoundaryReason,
};
use std::fs;

#[test]
fn one_transcription_per_chunk_and_exact_evidence_round_trip() {
    let mut result = common::batch("initial", &[" hello ", "\t世界"]);
    result.chunks[0].boundary.reason = ChunkBoundaryReason::LongPause;
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
    assert!(!encoded.contains("recognition"));
    assert!(!encoded.contains("pseudo"));
    assert!(!encoded.contains("transcription_runs"));
    export_text(&dir.path().join("text"), &project).unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join("text")).unwrap(),
        " hello \n\n\t世界"
    );
}

#[test]
fn unavailable_token_alignment_preserves_text_evidence_and_structural_history() {
    let mut result = common::batch("initial", &[" exact text \t", "other"]);
    result.segments[0].tokens[0].text = "mismatched".into();
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
        let paragraphs = if target == "current" {
            &mut invalid["paragraphs"]
        } else {
            &mut invalid["edit_history"][0]["before"]["paragraphs"]
        };
        paragraphs[0]["chunk_boundaries"][0]["transcription_id"] = "missing".into();
        fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
        assert!(load_project(&path).is_err());
    }
    save_project(&path, &project).unwrap();
    let good = fs::read(&path).unwrap();
    let mut invalid = encoded;
    invalid["paragraphs"][0]["tokens"][0]["vocabulary_id"] = 999.into();
    fs::write(
        dir.path().join("invalid"),
        serde_json::to_vec(&invalid).unwrap(),
    )
    .unwrap();
    assert!(load_project(&dir.path().join("invalid")).is_err());
    assert_eq!(fs::read(&path).unwrap(), good);
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
    let mut value = serde_json::to_value(&project).unwrap();
    value["schema"] = "rde-document/v1-experimental".into();
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(load_project(&path)
        .unwrap_err()
        .to_string()
        .contains("unsupported project schema"));
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
    altered.chunks[0].audio_range.end_sample = 99;
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
fn duplicate_attention_targets_and_audio_mappings_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("project");
    let mut project = common::project(&["text"]);
    project.mark_attention(1, 1).unwrap();
    let mut value = serde_json::to_value(&project).unwrap();
    let mark = value["attention_marks"][0].clone();
    value["attention_marks"].as_array_mut().unwrap().push(mark);
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
fn special_tokens_do_not_get_addresses_but_empty_text_tokens_do() {
    let mut result = common::batch("initial", &[""]);
    let project = Project::from_initial_transcription(&result);
    assert_eq!(project.chunk_has_tokens(1, 1), Some(true));
    assert_eq!(project.chunk_token(1, 1, 1).unwrap().text(), "");
    result.segments[0].tokens[0].is_special = true;
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
    result.chunks.clear();
    result.segments.clear();
    result.status = running_drafts_editor::transcription::TranscriptionStatus::Failed;
    result.config.language = "de".into();
    result.windows[0].hypotheses.clear();
    result.windows[0].accepted_segment_ids.clear();
    result.windows[0].error = Some("synthetic decode failure".into());
    let project = Project::from_initial_transcription(&result);
    assert!(project.transcriptions().is_empty());
    let dir = tempfile::tempdir().unwrap();
    save_project(&dir.path().join("project"), &project).unwrap();
    let reopened = load_project(&dir.path().join("project")).unwrap();
    assert_eq!(reopened, project);
    let value = serde_json::to_value(&reopened).unwrap();
    assert_eq!(value["initial_evidence"]["config"]["language"], "de");
    assert_eq!(
        value["initial_evidence"]["source"]["decoded_sample_count"],
        100
    );
    assert_eq!(value["initial_evidence"]["status"], "failed");
}
