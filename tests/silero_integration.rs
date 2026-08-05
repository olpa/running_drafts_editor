use running_drafts_editor::chunking::{SileroConfig, SileroDetector};

const HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[test]
fn missing_model_has_a_typed_safe_failure() {
    let error = SileroDetector::load(
        "tests/fixtures/does-not-exist.onnx",
        SileroConfig {
            model_version: "v5".into(),
            expected_model_sha256: HASH.into(),
            intra_threads: 1,
        },
    )
    .unwrap_err();
    assert_eq!(
        error.code,
        running_drafts_editor::chunking::DetectorErrorCode::ModelNotFound
    );
    assert!(!error.to_string().contains("does-not-exist"));
}
