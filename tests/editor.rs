use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
};

use serde_json::json;

#[test]
fn edit_opens_prints_and_navigates_without_audio_or_recognition() {
    let directory = tempfile::tempdir().unwrap();
    let document = directory.path().join("draft.rde.json");
    let value = json!({
        "schema": "rde-document/v1-experimental",
        "id": "document:test",
        "paragraphs": [{
            "id": "paragraph:test:1",
            "revision": 1,
            "tokens": [
                {"id": {"kind": "pseudo", "id": "p1"}, "text": "hello", "origin": {"kind": "pseudo", "reason": "test"}},
                {"id": {"kind": "pseudo", "id": "p2"}, "text": " world", "origin": {"kind": "pseudo", "reason": "test"}}
            ],
            "chunk_boundaries": [{"chunk_id": "c1", "after_tokens": 2}]
        }],
        "audio_sources": [{"id": "audio:test", "path": "/missing/audio.wav"}]
    });
    fs::write(&document, serde_json::to_vec_pretty(&value).unwrap()).unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_rde"))
        .args(["edit", document.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"1.2\np\nsave\nq\n")
        .unwrap();
    let result = child.wait_with_output().unwrap();

    assert!(
        result.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let output = String::from_utf8(result.stdout).unwrap();
    let errors = String::from_utf8(result.stderr).unwrap();
    assert!(output.contains("hello world"));
    assert!(output.contains("caret 1.2"));
    assert!(output.contains(&format!("saved {}", document.display())));
    assert!(errors.contains("text remains editable"));
}

#[test]
fn session_edit_replaces_document_resets_navigation_and_changes_default_save_path() {
    let directory = tempfile::tempdir().unwrap();
    let first = directory.path().join("first.json");
    let second = directory.path().join("second.json");
    let document = |id: &str, text: &str| {
        json!({
            "schema": "rde-document/v1-experimental",
            "id": format!("document:{id}"),
            "paragraphs": [{
                "id": format!("paragraph:{id}:1"),
                "revision": 1,
                "tokens": [{
                    "id": {"kind": "pseudo", "id": format!("token:{id}")},
                    "text": text,
                    "origin": {"kind": "pseudo", "reason": "test"}
                }],
                "chunk_boundaries": [{"chunk_id": format!("chunk:{id}"), "after_tokens": 1}]
            }]
        })
    };
    fs::write(
        &first,
        serde_json::to_vec_pretty(&document("first", "first text")).unwrap(),
    )
    .unwrap();
    fs::write(
        &second,
        serde_json::to_vec_pretty(&document("second", "second text")).unwrap(),
    )
    .unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_rde"))
        .args(["edit", first.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(format!("1.1\nedit {}\n1.1\nsave\nq\n", second.display()).as_bytes())
        .unwrap();
    let result = child.wait_with_output().unwrap();

    assert!(
        result.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let output = String::from_utf8(result.stdout).unwrap();
    assert!(output.contains(&format!("loaded {}", second.display())));
    assert!(output.contains("second text"));
    assert!(output.contains(&format!("saved {}", second.display())));
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(&second).unwrap()).unwrap();
    assert_eq!(saved["id"], "document:second");
}

#[test]
fn session_edits_exact_pseudo_tokens_preserves_mappings_and_rejects_cross_paragraph_ranges() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("draft.json");
    let value = json!({
        "schema": "rde-document/v1-experimental",
        "id": "document:edit-test",
        "paragraphs": [
            {
                "id": "paragraph:one",
                "revision": 1,
                "tokens": [
                    {"id": {"kind": "recognition", "run_id": "run", "segment_id": "s1", "token_index": 0}, "text": "a", "origin": {"kind": "recognition"}},
                    {"id": {"kind": "recognition", "run_id": "run", "segment_id": "s1", "token_index": 1}, "text": " b", "origin": {"kind": "recognition"}},
                    {"id": {"kind": "recognition", "run_id": "run", "segment_id": "s2", "token_index": 0}, "text": " c", "origin": {"kind": "recognition"}}
                ],
                "chunk_boundaries": [
                    {"chunk_id": "c1", "after_tokens": 1},
                    {"chunk_id": "c2", "after_tokens": 3}
                ]
            },
            {
                "id": "paragraph:two",
                "revision": 1,
                "tokens": [
                    {"id": {"kind": "recognition", "run_id": "run", "segment_id": "s3", "token_index": 0}, "text": "next", "origin": {"kind": "recognition"}}
                ],
                "chunk_boundaries": [{"chunk_id": "c3", "after_tokens": 1}]
            }
        ],
        "audio_sources": [{"id": "audio:run"}],
        "token_audio_mappings": [
            {
                "paragraph_id": "paragraph:one",
                "paragraph_revision": 1,
                "token_id": {"kind": "recognition", "run_id": "run", "segment_id": "s1", "token_index": 0},
                "source_id": "audio:run",
                "range": {"start_sample": 1, "end_sample": 10},
                "alignment": "exact"
            },
            {
                "paragraph_id": "paragraph:one",
                "paragraph_revision": 1,
                "token_id": {"kind": "recognition", "run_id": "run", "segment_id": "s1", "token_index": 1},
                "source_id": "audio:run",
                "range": {"start_sample": 10, "end_sample": 20},
                "alignment": "exact"
            }
        ]
    });
    fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();

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
        .write_all(
            b"1.2insert inserted words\n1.3append appended words\n1.2,1.4replace \" exact text  \"\n1.3,2.1replace forbidden\n1.3,1.3delete\nsave\nq\n",
        )
        .unwrap();
    let result = child.wait_with_output().unwrap();

    assert!(
        result.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let output = String::from_utf8(result.stdout).unwrap();
    let errors = String::from_utf8(result.stderr).unwrap();
    assert!(output.contains("inserted at 1.2"));
    assert!(output.contains("appended at 1.4"));
    assert!(output.contains("replaced 1.2,1.4"));
    assert!(output.contains("deleted 1.3,1.3"));
    assert!(errors.contains("text-edit ranges cannot cross paragraph boundaries"));

    let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let first = &saved["paragraphs"][0];
    assert_eq!(first["revision"], 5);
    assert_eq!(first["tokens"].as_array().unwrap().len(), 2);
    assert_eq!(first["tokens"][0]["text"], "a");
    assert_eq!(first["tokens"][1]["text"], " exact text  ");
    assert_eq!(first["tokens"][1]["origin"]["reason"], "user text");
    assert_eq!(first["chunk_boundaries"][0]["after_tokens"], 1);
    assert_eq!(first["chunk_boundaries"][1]["after_tokens"], 2);
    assert_eq!(saved["token_audio_mappings"].as_array().unwrap().len(), 1);
    assert_eq!(saved["token_audio_mappings"][0]["paragraph_revision"], 5);
    assert_eq!(
        saved["token_audio_mappings"][0]["token_id"],
        value["paragraphs"][0]["tokens"][0]["id"]
    );
}

#[test]
fn unaddressed_replace_uses_the_current_token_selection() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("selection.json");
    let value = json!({
        "schema": "rde-document/v1-experimental",
        "id": "document:selection",
        "paragraphs": [{
            "id": "paragraph:selection",
            "revision": 1,
            "tokens": [
                {"id": {"kind": "pseudo", "id": "one"}, "text": "wrong", "origin": {"kind": "pseudo", "reason": "test"}},
                {"id": {"kind": "pseudo", "id": "two"}, "text": " name", "origin": {"kind": "pseudo", "reason": "test"}}
            ],
            "chunk_boundaries": [{"chunk_id": "chunk", "after_tokens": 2}]
        }]
    });
    fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();

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
        .write_all(b"1.1,1.2select\nreplace Oleg\nsave\nq\n")
        .unwrap();
    let result = child.wait_with_output().unwrap();

    assert!(result.status.success());
    assert!(String::from_utf8(result.stdout)
        .unwrap()
        .contains("replaced 1.1,1.2"));
    assert!(result.stderr.is_empty());
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(
        saved["paragraphs"][0]["tokens"].as_array().unwrap().len(),
        1
    );
    assert_eq!(saved["paragraphs"][0]["tokens"][0]["text"], "Oleg");
}

#[test]
fn replacement_preserves_boundary_whitespace_unless_quoted() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("whitespace.json");
    let value = json!({
        "schema": "rde-document/v1-experimental",
        "id": "document:whitespace",
        "paragraphs": [{
            "id": "paragraph:whitespace",
            "revision": 1,
            "tokens": [
                {"id": {"kind": "pseudo", "id": "before"}, "text": "before", "origin": {"kind": "pseudo", "reason": "test"}},
                {"id": {"kind": "pseudo", "id": "middle"}, "text": "\t old text \u{2003}", "origin": {"kind": "pseudo", "reason": "test"}},
                {"id": {"kind": "pseudo", "id": "after"}, "text": "after", "origin": {"kind": "pseudo", "reason": "test"}}
            ],
            "chunk_boundaries": [{"chunk_id": "chunk", "after_tokens": 3}]
        }]
    });
    fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();

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
        .write_all(b"1.2,1.2replace new text\np\n1.2,1.2replace \"tight\"\nsave\nq\n")
        .unwrap();
    let result = child.wait_with_output().unwrap();

    assert!(result.status.success());
    assert!(result.stderr.is_empty());
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(saved["paragraphs"][0]["tokens"][1]["text"], "tight");
}
