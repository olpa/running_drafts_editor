use std::sync::{Arc, Mutex};
use std::thread;
/// Test to demonstrate and verify thread safety guarantees
///
/// WhisperState is Send but NOT Sync, which means:
/// - You CAN move it between threads
/// - You CANNOT share &WhisperState between threads (won't compile)
/// - You MUST use Mutex if you need to share
use whisper_rs::{WhisperContext, WhisperContextParameters};

#[test]
fn test_whisper_state_is_send() {
    // WhisperState is Send: can be moved to another thread
    let model_path = std::env::var("WHISPER_TEST_MODEL")
        .unwrap_or_else(|_| "../whisper.cpp/models/ggml-tiny.en.bin".to_string());

    if !std::path::Path::new(&model_path).exists() {
        eprintln!("Skipping test: model not found at {}", model_path);
        return;
    }

    let ctx = WhisperContext::new_with_params(&model_path, WhisperContextParameters::default())
        .expect("Failed to create context");

    let state = ctx.create_state().expect("Failed to create state");

    // Move state to another thread - this works because WhisperState: Send
    let handle = thread::spawn(move || {
        let n_segments = state.full_n_segments();
        assert_eq!(n_segments, 0); // No transcription yet
    });

    handle.join().unwrap();
}

#[test]
fn test_shared_state_with_mutex() {
    // If you need to share WhisperState, you MUST use Mutex
    let model_path = std::env::var("WHISPER_TEST_MODEL")
        .unwrap_or_else(|_| "../whisper.cpp/models/ggml-tiny.en.bin".to_string());

    if !std::path::Path::new(&model_path).exists() {
        eprintln!("Skipping test: model not found at {}", model_path);
        return;
    }

    let ctx = WhisperContext::new_with_params(&model_path, WhisperContextParameters::default())
        .expect("Failed to create context");

    let state = ctx.create_state().expect("Failed to create state");

    // Wrap in Mutex to share across threads
    let state = Arc::new(Mutex::new(state));
    let state_clone = Arc::clone(&state);

    let handle = thread::spawn(move || {
        let state = state_clone.lock().unwrap();
        let n_segments = state.full_n_segments();
        assert_eq!(n_segments, 0);
    });

    handle.join().unwrap();

    // Main thread can still access
    let state = state.lock().unwrap();
    assert_eq!(state.full_n_segments(), 0);
}

#[test]
fn test_recommended_pattern_separate_states() {
    // RECOMMENDED: Create separate states per thread for best performance
    let model_path = std::env::var("WHISPER_TEST_MODEL")
        .unwrap_or_else(|_| "../whisper.cpp/models/ggml-tiny.en.bin".to_string());

    if !std::path::Path::new(&model_path).exists() {
        eprintln!("Skipping test: model not found at {}", model_path);
        return;
    }

    let ctx = Arc::new(
        WhisperContext::new_with_params(&model_path, WhisperContextParameters::default())
            .expect("Failed to create context"),
    );

    let mut handles = vec![];

    // Spawn multiple threads, each with its own state
    for i in 0..4 {
        let ctx_clone = Arc::clone(&ctx);
        let handle = thread::spawn(move || {
            // Each thread creates its own state - no locking needed!
            let state = ctx_clone.create_state().expect("Failed to create state");

            let n_segments = state.full_n_segments();
            assert_eq!(n_segments, 0, "Thread {} got wrong segment count", i);
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }
}

// This test would NOT compile if WhisperState implemented Sync:
//
// #[test]
// fn test_cannot_share_without_mutex() {
//     let ctx = WhisperContext::new_with_params(...)?;
//     let state = Arc::new(ctx.create_state()?); // ERROR: WhisperState is not Sync
//
//     let state_clone = Arc::clone(&state);
//     thread::spawn(move || {
//         // This won't compile!
//     });
// }
//
// The compiler prevents this unsafe pattern at compile-time!
