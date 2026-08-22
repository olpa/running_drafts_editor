//! End-to-end tests for the shared CLI session entered through `rde edit`.

use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
};

use serde_json::json;

#[test]
fn attention_commands_use_addresses_caret_and_selection_start() {
    let directory = tempfile::tempdir().unwrap();
    let document = directory.path().join("attention.json");
    let exported = directory.path().join("attention.txt");
    let value = json!({"schema":"rde-document/v1-experimental","id":"document:attention","paragraphs":[{
        "id":"p","revision":1,"tokens":[
            {"id":{"kind":"pseudo","id":"a"},"text":"one","origin":{"kind":"pseudo","reason":"test"}},
            {"id":{"kind":"pseudo","id":"b"},"text":" two","origin":{"kind":"pseudo","reason":"test"}},
            {"id":{"kind":"pseudo","id":"c"},"text":" three","origin":{"kind":"pseudo","reason":"test"}}
        ],"chunk_boundaries":[{"chunk_id":"chunk","after_tokens":3}]
    }]});
    fs::write(&document, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_rde"))
        .args(["edit", document.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let commands = format!(
        "1.2mark\n1.2mark\n1.1,1.3select\nmark\nundo\nredo\np\n1@1\nmark\n1select\nunmark\n1.1unmark\nexport {}\nsave\nq\n",
        exported.display()
    );
    child
        .stdin
        .take()
        .unwrap()
        .write_all(commands.as_bytes())
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success());
    let output = String::from_utf8(result.stdout).unwrap();
    let errors = String::from_utf8(result.stderr).unwrap();
    assert!(output.contains("marked 1.1"));
    assert!(output.contains("⚑one"));
    assert!(output.contains("⚑ two"));
    assert!(errors.contains("token 1.2 is already marked"));
    assert!(errors.contains("mark requires a current token"));
    assert!(errors.contains("unmark requires a current token"));
    assert_eq!(fs::read_to_string(exported).unwrap(), "one⚑ two three");
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(document).unwrap()).unwrap();
    assert_eq!(saved["attention_marks"].as_array().unwrap().len(), 1);
}

#[test]
fn edit_defers_loading_a_configured_model_until_recognition_is_used() {
    let directory = tempfile::tempdir().unwrap();
    let document = directory.path().join("lazy-model.json");
    let model = directory.path().join("fake-model.bin");
    fs::write(&model, b"readable but not a whisper model").unwrap();
    let value = json!({"schema":"rde-document/v1-experimental","id":"document:lazy","paragraphs":[{
        "id":"p","revision":1,"tokens":[{"id":{"kind":"pseudo","id":"t"},"text":"text","origin":{"kind":"pseudo","reason":"test"}}],
        "chunk_boundaries":[{"chunk_id":"c","after_tokens":1}]
    }]});
    fs::write(&document, serde_json::to_vec_pretty(&value).unwrap()).unwrap();

    let run = |commands: &[u8]| {
        let mut child = Command::new(env!("CARGO_BIN_EXE_rde"))
            .args([
                "edit",
                document.to_str().unwrap(),
                "--model",
                model.to_str().unwrap(),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(commands).unwrap();
        child.wait_with_output().unwrap()
    };
    let idle = run(b"model\nq\n");
    assert!(idle.status.success());
    assert!(String::from_utf8(idle.stdout)
        .unwrap()
        .contains(&format!("model {}", model.display())));
    assert!(!String::from_utf8(idle.stderr)
        .unwrap()
        .contains("could not load model"));

    let used = run(b"1@1refresh\nq\n");
    assert!(used.status.success());
    assert!(String::from_utf8(used.stderr)
        .unwrap()
        .contains("could not load model"));
}

#[test]
fn confidence_issues_navigate_resolve_persist_and_undo_without_color_on_redirect() {
    let directory = tempfile::tempdir().unwrap();
    let document = directory.path().join("issues.rde.json");
    let rid = |segment: &str, token: usize| json!({"kind":"recognition","run_id":"run","segment_id":segment,"token_index":token});
    let token = |id: serde_json::Value, text: &str| json!({"id":id,"text":text,"origin":{"kind":"recognition"}});
    let value = json!({
        "schema":"rde-document/v1-experimental", "id":"document:issues",
        "paragraphs":[
          {"id":"p1","revision":1,"tokens":[token(rid("s",0),"bad\n"),token(rid("s",1),"two"),token(rid("s",2)," orange"),token(rid("s",3)," other")],"chunk_boundaries":[{"chunk_id":"c1","after_tokens":3},{"chunk_id":"c2","after_tokens":4}]},
          {"id":"p2","revision":1,"tokens":[token(rid("t",0)," last")],"chunk_boundaries":[{"chunk_id":"c3","after_tokens":1}]}
        ],
        "recognition_token_evidence":[
          {"token_id":rid("s",0),"recognition_token_id":1,"probability":0.149,"alternatives":[]},
          {"token_id":rid("s",1),"recognition_token_id":2,"probability":0.10,"alternatives":[]},
          {"token_id":rid("s",2),"recognition_token_id":3,"probability":0.15,"alternatives":[]},
          {"token_id":rid("s",3),"recognition_token_id":4,"probability":0.01,"alternatives":[]},
          {"token_id":rid("t",0),"recognition_token_id":5,"probability":0.01,"alternatives":[]}
        ]
    });
    fs::write(&document, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_rde"))
        .args(["edit", document.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"issues\nnext\nnext\nnext\nprev\n1.1,1.2select\nresolve\nissues\nundo\nissues\nredo\nissues\n1unignore\nissue-prob red 0.1\nissues\nissue-prob orange 0.1\nissue-prob\n1resolve\nsave\nq\n").unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success());
    let output = String::from_utf8(result.stdout).unwrap();
    let errors = String::from_utf8(result.stderr).unwrap();
    assert!(output.contains("1  open  \"bad\\ntwo\""));
    assert!(output.contains("selected 1.4,1.4"));
    assert!(output.matches("selected 1.4,1.4").count() >= 2);
    assert!(output.contains("selected 1.1,1.2"));
    assert!(output.contains("selected 1.1,1.2 (wrapped)"));
    assert!(output.contains("selected 2.1,2.1 (wrapped)"));
    assert!(output.contains("1  resolved  \"bad\\ntwo\""));
    assert!(output.contains("reopened 1.1,1.2"));
    assert!(output.contains("issue-prob red 0.1 orange 0.5"));
    assert!(!output.contains("\u{1b}["));
    assert!(errors.contains("issue-prob red must be less than orange"));
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(document).unwrap()).unwrap();
    assert_eq!(saved["resolved_issues"].as_array().unwrap().len(), 1);
    assert!(!saved.to_string().contains("issue-prob"));
}

#[test]
fn lists_every_alternative_but_requires_a_model_before_choose() {
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
    assert!(errors.contains("start with --model MODEL or use: model PATH"));

    let saved: serde_json::Value = serde_json::from_slice(&fs::read(&document).unwrap()).unwrap();
    assert_eq!(saved["paragraphs"][0]["tokens"][0]["text"], "hello");
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
fn bare_tokens_lists_the_selection_with_five_tokens_of_numbered_context() {
    let directory = tempfile::tempdir().unwrap();
    let document = directory.path().join("token-context.json");
    let make_tokens = |paragraph: usize, count: usize| {
        (1..=count)
            .map(|token| {
                json!({
                    "id":{"kind":"pseudo","id":format!("p{paragraph}t{token}")},
                    "text":format!(" {paragraph}:{token}"),
                    "origin":{"kind":"pseudo","reason":"test"}
                })
            })
            .collect::<Vec<_>>()
    };
    let value = json!({"schema":"rde-document/v1-experimental","id":"document:tokens","paragraphs":[
        {"id":"p1","revision":1,"tokens":make_tokens(1,8),"chunk_boundaries":[{"chunk_id":"c1","after_tokens":8}]},
        {"id":"p2","revision":1,"tokens":make_tokens(2,8),"chunk_boundaries":[{"chunk_id":"c2","after_tokens":8}]}
    ]});
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
        .write_all(b"tokens\n1.7,2.2select\ntokens\nq\n")
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success());
    let output = String::from_utf8(result.stdout).unwrap();
    let errors = String::from_utf8(result.stderr).unwrap();
    assert!(errors.contains("tokens requires an active token selection or a paragraph address M"));
    assert!(output.contains("1.2      -"));
    assert!(output.contains("2.7      -"));
    assert!(!output.contains("1.1      -"));
    assert!(!output.contains("2.8      -"));
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
    assert!(!output.contains("inserted at"));
    assert!(errors.contains("start with --model MODEL or use: model PATH"));
    assert!(errors.contains("delete is disabled"));

    let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let first = &saved["paragraphs"][0];
    assert_eq!(first["revision"], 1);
    assert_eq!(first["tokens"].as_array().unwrap().len(), 3);
    assert_eq!(first["tokens"][0]["text"], "a");
    assert_eq!(first["tokens"][1]["text"], " b");
    assert_eq!(first["chunk_boundaries"][0]["after_tokens"], 1);
    assert_eq!(first["chunk_boundaries"][1]["after_tokens"], 3);
    assert_eq!(saved["token_audio_mappings"].as_array().unwrap().len(), 2);
    assert_eq!(saved["token_audio_mappings"][0]["paragraph_revision"], 1);
    assert_eq!(
        saved["token_audio_mappings"][0]["token_id"],
        value["paragraphs"][0]["tokens"][0]["id"]
    );
}

#[test]
fn unaddressed_replace_requires_a_model_and_keeps_the_selection_text() {
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
    assert!(String::from_utf8(result.stderr)
        .unwrap()
        .contains("start with --model MODEL"));
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(
        saved["paragraphs"][0]["tokens"].as_array().unwrap().len(),
        2
    );
    assert_eq!(saved["paragraphs"][0]["tokens"][0]["text"], "wrong");
}

#[test]
fn replacement_without_a_model_does_not_change_boundary_whitespace() {
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
    assert!(String::from_utf8(result.stderr)
        .unwrap()
        .contains("start with --model MODEL"));
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(
        saved["paragraphs"][0]["tokens"][1]["text"],
        "\t old text \u{2003}"
    );
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
        .write_all(b"1.3split\n1@1parasplit\n1merge\n1@1merge\n3undo\n2redo\n9redo\nsave\nq\n")
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
    assert!(output.contains("undid 3 edits"));
    assert!(output.contains("redid 2 edits"));
    assert!(output.contains("redid 1 edit"));

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
