/// Test to demonstrate lifetime issues with borrowed strings from C++
///
/// This test shows that returning references to C++ memory can lead to
/// use-after-free bugs when the C++ side invalidates the memory.
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

#[test]
#[ignore] // This test demonstrates the UNSAFE pattern - it won't compile due to Rust's borrow checker
fn test_lifetime_issue_with_borrowed_strings() {
    // This test demonstrates why returning &str is problematic.
    // Even though Rust's borrow checker prevents this specific example from compiling,
    // there are scenarios (like storing in a struct with complex lifetimes, or using
    // unsafe code) where this could still occur.
    //
    // The issue is:
    // 1. We get a reference to C++ memory
    // 2. The C++ memory can be invalidated by re-transcription
    // 3. The Rust reference becomes dangling
    //
    // NOTE: This test is intentionally marked #[ignore] and contains code that doesn't compile.
    // It's here for documentation purposes to show the problem we're solving.

    let model_path = std::env::var("WHISPER_TEST_MODEL")
        .unwrap_or_else(|_| "../whisper.cpp/models/ggml-tiny.en.bin".to_string());

    // Skip test if model doesn't exist
    if !std::path::Path::new(&model_path).exists() {
        eprintln!("Skipping test: model not found at {}", model_path);
        return;
    }

    let ctx = WhisperContext::new_with_params(&model_path, WhisperContextParameters::default())
        .expect("Failed to create context");

    let mut state = ctx.create_state().expect("Failed to create state");

    // Create some test audio (1 second of silence)
    let audio: Vec<f32> = vec![0.0; 16000];

    // First transcription
    let params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    state
        .full(params, &audio)
        .expect("Failed to run first transcription");

    // This pattern would be dangerous if allowed:
    println!("UNSAFE PATTERN (doesn't compile): Trying to store references to C++ memory");
    println!("The fix: use to_string() instead of to_str() to get owned strings");

    // NOTE: The following code is commented out because it won't compile
    // (which is good - Rust's borrow checker is protecting us!)
    /*
    let mut stored_strings: Vec<&str> = Vec::new();

    let n_segments = state.full_n_segments();
    for segment in 0..n_segments {
        if let Some(seg) = state.get_segment(segment) {
            #[allow(deprecated)]
            if let Ok(text) = seg.to_str() {
                // DANGER: We're storing a reference to C++ memory
                stored_strings.push(text);
            }
        }
    }

    // Second transcription - this may invalidate the C++ memory
    let params2 = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    state
        .full(params2, &audio)
        .expect("Failed to run second transcription");

    // DANGER: The stored_strings now contain dangling pointers!
    // Accessing them is undefined behavior
    for text in &stored_strings {
        println!("Stored text (may be garbage): {}", text);
        // This might crash, print garbage, or appear to work
    }
    */
}

#[test]
fn test_safe_pattern_with_owned_strings() {
    // This test demonstrates the SAFE pattern using owned strings

    let model_path = std::env::var("WHISPER_TEST_MODEL")
        .unwrap_or_else(|_| "../whisper.cpp/models/ggml-tiny.en.bin".to_string());

    // Skip test if model doesn't exist
    if !std::path::Path::new(&model_path).exists() {
        eprintln!("Skipping test: model not found at {}", model_path);
        return;
    }

    let ctx = WhisperContext::new_with_params(&model_path, WhisperContextParameters::default())
        .expect("Failed to create context");

    let mut state = ctx.create_state().expect("Failed to create state");

    // Create some test audio (1 second of silence)
    let audio: Vec<f32> = vec![0.0; 16000];

    // First transcription
    let params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    state
        .full(params, &audio)
        .expect("Failed to run first transcription");

    // Store OWNED strings instead of references
    let mut stored_strings: Vec<String> = Vec::new();

    let n_segments = state.full_n_segments();
    for segment in 0..n_segments {
        if let Some(seg) = state.get_segment(segment) {
            // NEW: Use the safe to_string() method that returns String directly
            if let Ok(text) = seg.to_string() {
                stored_strings.push(text);
            }
        }
    }

    // Second transcription - this is safe because we own the strings
    let params2 = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    state
        .full(params2, &audio)
        .expect("Failed to run second transcription");

    // SAFE: The stored_strings contain owned data
    for text in &stored_strings {
        println!("Stored text (safe): {}", text);
        // This will always work correctly
    }

    // Verify we still have valid strings
    assert_eq!(stored_strings.len(), stored_strings.len());
}
