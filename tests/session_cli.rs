mod common;
use running_drafts_editor::{
    persistence::{load_project, save_project},
    project::Project,
};
use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
};

fn run(project: &Project, commands: &str) -> (String, String, Project) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("project.json");
    save_project(&path, project).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_rde"))
        .args(["edit", path.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(format!("{commands}\nsave\nq\n").as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    (
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr)
            .unwrap()
            .lines()
            .filter(|line| !line.ends_with("has no local path; replay is unavailable"))
            .collect::<Vec<_>>()
            .join("\n"),
        load_project(&path).unwrap(),
    )
}

#[test]
fn removed_chunk_and_refresh_commands_leave_project_unchanged() {
    let project = common::project(&["one", "two"]);
    let (output, errors, saved) = run(
        &project,
        "refresh\n1.1refresh\n1.1.1split\n1.1isplit\n1.1asplit\n1@1merge\nhelp",
    );
    assert_eq!(saved, project);
    assert!(errors.contains("unknown command 'refresh'"));
    assert!(!output.contains("[N.M]refresh"));
}

#[test]
fn attention_and_issue_commands_follow_actual_tokens_and_history() {
    let project = common::project(&["one", " two"]);
    let (output, errors, saved) = run(
        &project,
        "1.1.1mark\n1.1.1mark\nissues\nnext\nignore\nundo\nredo\np",
    );
    assert!(output.contains("⚑one"));
    assert!(output.contains("undid 1 edit"));
    assert!(errors.contains("already marked"));
    assert_eq!(saved.attention_marks().len(), 1);
    assert_eq!(saved.resolved_issues().len(), 1);
}

#[test]
fn paragraph_operations_preserve_chunks_and_recalculate_issue_addresses() {
    let project = common::project(&["one", " two"]);
    let (output, errors, saved) = run(
        &project,
        "1.2parasplit\nissues\n2tokens\n1merge\nundo\nredo",
    );
    assert!(errors.is_empty(), "{errors}");
    assert!(output.contains("2.1.1"));
    assert_eq!(saved.paragraphs().len(), 1);
    assert_eq!(saved.transcriptions(), project.transcriptions());
    assert_eq!(saved.paragraph(1).unwrap().text(), "one two");
    assert_eq!(
        saved.paragraph(1).unwrap().chunk_boundaries()[1].chunk_id(),
        "c1"
    );
}

#[test]
fn text_without_matching_tokens_is_visible_and_structurally_selectable() {
    let mut result = common::batch("initial", &["exact text", "two"]);
    result.segments[0].tokens[0].text = "bad evidence".into();
    let project = Project::from_initial_transcription(&result);
    let (output, errors, saved) = run(
        &project,
        "p\n1tokens\n1.1.1\n1.1,1.2select\nreplace corrected\n1.1info",
    );
    assert!(output.contains("exact text"));
    assert!(output.contains("1.1  chunk  no tokens"));
    assert!(output.contains("selected 1.1,1.2"));
    assert!(errors.contains("chunk 1.1 has no token positions"));
    assert!(errors.contains("transcription requires a model"));
    assert_eq!(saved, project);
}

#[test]
fn settings_queries_are_read_only_and_failed_changes_keep_history() {
    let mut project = common::project(&["one", "two"]);
    project.split_paragraph(1, 1).unwrap();
    project.undo(1);
    let (output, errors, saved) = run(
        &project,
        "model\nlanguage\nlanguage de\nmodel /missing/model.bin\nmodel\nlanguage",
    );
    assert!(output.contains("model (none)"));
    assert!(output.contains("language auto"));
    assert!(errors.contains("transcription requires a model"));
    assert!(errors.contains("could not load model"));
    assert_eq!(saved, project);
}

#[test]
fn cross_chunk_setting_targets_are_rejected_before_loading_models() {
    let project = common::project(&["one", "two"]);
    let (_, errors, saved) = run(&project, "1,2select\nmodel /missing/model.bin\nlanguage de");
    assert!(errors.contains("setting change requires exactly one current chunk"));
    assert!(!errors.contains("could not load model"));
    assert_eq!(saved, project);
}

#[test]
fn failed_corrections_preserve_whitespace_text_selection_and_redo() {
    let mut project = common::project(&[" \t old text \u{2003}", " next"]);
    project.split_paragraph(1, 1).unwrap();
    project.undo(1);
    let (output, errors, saved) = run(
        &project,
        "1.1.1,1.1.2select\nreplace NEW\np\n1.1.1insert inserted\n1.1.1append appended",
    );
    assert!(output.contains("selected 1.1.1,1.1.2"));
    assert!(output.contains("old text"));
    assert!(errors.contains("transcription requires a model"));
    assert_eq!(saved, project);
}

#[test]
fn save_load_and_export_keep_exact_current_text() {
    let dir = tempfile::tempdir().unwrap();
    let export = dir.path().join("text");
    let project = common::project(&["one", " two"]);
    let (_, errors, saved) = run(
        &project,
        &format!("1.2parasplit\nexport {}", export.display()),
    );
    assert!(errors.is_empty(), "{errors}");
    assert_eq!(fs::read_to_string(export).unwrap(), "one\n\n two");
    assert_eq!(saved.paragraphs().len(), 2);
}
