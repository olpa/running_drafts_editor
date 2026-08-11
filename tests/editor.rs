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

    assert!(result.status.success());
    let output = String::from_utf8(result.stdout).unwrap();
    let errors = String::from_utf8(result.stderr).unwrap();
    assert!(output.contains("hello world"));
    assert!(output.contains("caret 1.2"));
    assert!(output.contains(&format!("saved {}", document.display())));
    assert!(errors.contains("text remains editable"));
}
