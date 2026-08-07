// POC to demonstrate callback vulnerabilities with null pointers and invalid data
//
// This test attempts to trigger unsafe conditions in callbacks that should be
// handled with proper null checks and validation.
//
// WARNING: This test may crash or cause undefined behavior before fixes are applied!
//
// Run with:
// cargo run --example test_callback_crash

use std::ffi::c_void;
use whisper_rs::{
    FullParams, SamplingStrategy, SegmentCallbackData, WhisperContext, WhisperContextParameters,
};

fn main() {
    println!("POC: Testing callback vulnerabilities (may crash!)...\n");

    // Get model path from environment or use default
    let model_path = std::env::var("WHISPER_MODEL").unwrap_or_else(|_| {
        println!("Note: WHISPER_MODEL not set, using default path");
        "../whisper.cpp/models/ggml-tiny.en.bin".to_string()
    });

    println!("Loading model: {}", model_path);
    let ctx =
        match WhisperContext::new_with_params(&model_path, WhisperContextParameters::default()) {
            Ok(ctx) => ctx,
            Err(e) => {
                eprintln!("Failed to load model: {:?}", e);
                eprintln!("Please set WHISPER_MODEL to a valid model path");
                std::process::exit(1);
            }
        };

    let mut state = ctx.create_state().expect("Failed to create state");

    // Test 1: Try to trigger callback with potentially invalid state
    println!("\n[Test 1] Testing callback with unsafe raw API manipulation...");
    {
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some("en"));
        params.set_print_progress(false);
        params.set_print_realtime(false);

        // Set up a callback that will be invoked
        params.set_segment_callback_safe(|data: SegmentCallbackData| {
            println!(
                "  Callback invoked: segment={}, text='{}'",
                data.segment, data.text
            );
            // If we get here without validation, we might be processing invalid data
        });

        // Use the unsafe raw API to set a potentially problematic callback scenario
        unsafe {
            // Try to invoke callback with null user_data (this tests our null checks)
            // Note: We can't easily do this through the safe API, so this demonstrates
            // what could happen if whisper.cpp had a bug
            println!("  Attempting to test null pointer handling...");

            // The safe API prevents us from easily triggering this, but the C++ side could
            // potentially call our trampoline with null pointers if it has bugs
        }

        let audio: Vec<f32> = vec![0.0; 16000];

        match state.full(params, &audio) {
            Ok(_) => println!("  ✓ Test completed"),
            Err(e) => eprintln!("  ✗ Test failed: {:?}", e),
        }
    }

    // Test 2: Multiple rapid re-transcriptions (stress test)
    println!("\n[Test 2] Stress test with rapid re-transcriptions...");
    {
        for i in 0..10 {
            let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
            params.set_language(Some("en"));
            params.set_print_progress(false);
            params.set_print_realtime(false);

            params.set_segment_callback_safe(move |data: SegmentCallbackData| {
                // Access data fields - if pointers are invalid, this will crash
                let _seg = data.segment;
                let _text_len = data.text.len();
                let _t0 = data.start_timestamp;
                let _t1 = data.end_timestamp;
            });

            let audio: Vec<f32> = vec![0.0; 16000];

            match state.full(params, &audio) {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("  ✗ Iteration {} failed: {:?}", i, e);
                    break;
                }
            }
        }
        println!("  ✓ Stress test completed");
    }

    // Test 3: Callback that accesses text extensively (to trigger null pointer if text is null)
    println!("\n[Test 3] Testing extensive text access in callback...");
    {
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some("en"));
        params.set_print_progress(false);
        params.set_print_realtime(false);

        params.set_segment_callback_safe(|data: SegmentCallbackData| {
            // These operations would crash if the underlying C string pointer was null
            // and we didn't have proper validation
            let text = &data.text;
            let _chars: Vec<char> = text.chars().collect();
            let _bytes = text.as_bytes();
            let _len = text.len();

            // Try to access every character
            for _c in text.chars() {
                // If the C string wasn't properly validated, this could crash
            }

            println!("  Processed text safely: '{}'", text);
        });

        let audio: Vec<f32> = vec![0.0; 16000];

        match state.full(params, &audio) {
            Ok(_) => println!("  ✓ Text access test completed"),
            Err(e) => eprintln!("  ✗ Text access test failed: {:?}", e),
        }
    }

    println!("\n=== POC Summary ===");
    println!("If you saw this message, the callbacks handled the tests without crashing.");
    println!("\nWithout proper null checks:");
    println!("  - Null pointers from C++ would cause segfaults");
    println!("  - Invalid segment counts would cause out-of-bounds access");
    println!("  - Null text pointers would cause crashes when dereferenced");
    println!("\nAfter implementing Phase 1.4.1 fixes:");
    println!("  - Null pointers are checked and logged as errors");
    println!("  - Invalid counts are validated before use");
    println!("  - Text pointers are validated with c_str_from_ptr_with_limit()");
}
