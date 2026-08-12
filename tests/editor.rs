use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
};

use serde_json::json;

#[test]
fn lists_every_alternative_and_allows_selecting_an_empty_special_token() {
    let directory = tempfile::tempdir().unwrap();
    let document = directory.path().join("alternatives.rde.json");
    let recognition_id = json!({
        "kind": "recognition", "run_id": "run", "segment_id": "s1", "token_index": 0
    });
    let value = json!({
        "schema": "rde-document/v1-experimental",
        "id": "document:test",
        "paragraphs": [{
            "id": "paragraph:test:1",
            "revision": 1,
            "tokens": [{
                "id": recognition_id,
                "text": "hello",
                "origin": {"kind": "recognition"}
            }],
            "chunk_boundaries": [{"chunk_id": "c1", "after_tokens": 1}]
        }],
        "recognition_token_evidence": [{
            "token_id": recognition_id,
            "recognition_token_id": 100,
            "probability": 0.7,
            "alternatives": [
                {"token_id": 100, "text": "hello", "probability": 0.7},
                {"token_id": 101, "text": "hello", "probability": 0.2},
                {"token_id": 50257, "text": "", "probability": 0.1}
            ]
        }]
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
        .write_all(b"1.1alts\n1.1choose 3\n1.1alts\nsave\nq\n")
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success());
    let output = String::from_utf8(result.stdout).unwrap();
    let errors = String::from_utf8(result.stderr).unwrap();
    assert!(output.contains("1  id=100  probability=0.700000  text=\"hello\""));
    assert!(output.contains("2  id=101  probability=0.200000  text=\"hello\""));
    assert!(output.contains("3  id=50257  probability=0.100000  text=\"\""));
    assert!(output.contains("chose alternative 3 for 1.1"));
    assert!(errors.contains("alternatives unavailable for 1.1"));

    let saved: serde_json::Value = serde_json::from_slice(&fs::read(&document).unwrap()).unwrap();
    assert_eq!(saved["paragraphs"][0]["tokens"][0]["text"], "");
    assert_eq!(
        saved["paragraphs"][0]["tokens"][0]["origin"]["reason"],
        "recognition alternative"
    );
    assert_eq!(
        saved["recognition_token_evidence"][0]["alternatives"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}

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

#[test]
fn chunk_and_paragraph_structure_commands_preserve_text_and_persist_provenance() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("structure.json");
    let token = |index: usize, text: &str| {
        json!({
            "id": {"kind": "recognition", "run_id": "run", "segment_id": "s", "token_index": index},
            "text": text,
            "origin": {"kind": "recognition"}
        })
    };
    let mapping = |index: usize, start: u64, end: u64| {
        json!({
            "paragraph_id": "paragraph:original",
            "paragraph_revision": 1,
            "token_id": {"kind": "recognition", "run_id": "run", "segment_id": "s", "token_index": index},
            "source_id": "audio:run",
            "range": {"start_sample": start, "end_sample": end},
            "alignment": "exact"
        })
    };
    let value = json!({
        "schema": "rde-document/v1-experimental",
        "id": "document:structure",
        "paragraphs": [{
            "id": "paragraph:original",
            "revision": 1,
            "tokens": [token(0, "one"), token(1, " two"), token(2, " three"), token(3, " four")],
            "chunk_boundaries": [{"chunk_id": "chunk:original", "after_tokens": 4}]
        }],
        "audio_sources": [{"id": "audio:run", "canonical_sample_count": 400}],
        "chunk_audio_mappings": [{
            "chunk_id": "chunk:original",
            "source_id": "audio:run",
            "range": {"start_sample": 0, "end_sample": 400}
        }],
        "token_audio_mappings": [
            mapping(0, 0, 100), mapping(1, 100, 200),
            mapping(2, 200, 300), mapping(3, 300, 400)
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
        .write_all(b"1.3split\n1@1parasplit\n1merge\n1@1merge\nsave\nq\n")
        .unwrap();
    let result = child.wait_with_output().unwrap();

    assert!(
        result.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let output = String::from_utf8(result.stdout).unwrap();
    assert!(output.contains("split chunk before 1.3; new boundary 1@1"));
    assert!(output.contains("split paragraph 1 after 1@1"));
    assert!(output.contains("merged paragraphs 1 and 2"));
    assert!(output.contains("merged chunks at 1@1"));

    let saved: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(saved["paragraphs"].as_array().unwrap().len(), 1);
    assert_eq!(
        saved["paragraphs"][0]["tokens"].as_array().unwrap().len(),
        4
    );
    assert_eq!(
        saved["paragraphs"][0]["chunk_boundaries"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(saved["replay_chunks"].as_array().unwrap().len(), 4);
    assert_eq!(saved["chunk_audio_mappings"].as_array().unwrap().len(), 1);
    assert_eq!(saved["chunk_audio_mappings"][0]["range"]["start_sample"], 0);
    assert_eq!(saved["chunk_audio_mappings"][0]["range"]["end_sample"], 400);
    assert_eq!(saved["chunk_audio_mappings"][0]["alignment"], "exact");
    assert_eq!(
        saved["paragraphs"][0]["tokens"]
            .as_array()
            .unwrap()
            .iter()
            .map(|token| token["text"].as_str().unwrap())
            .collect::<String>(),
        "one two three four"
    );
}
