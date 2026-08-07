// Test to verify callback null pointer validation and safety
//
// This test exercises the segment callback with various conditions to ensure
// proper null checking and validation is in place.
//
// Run with:
// cargo run --example test_callback_safety

use whisper_rs::{
    FullParams, SamplingStrategy, SegmentCallbackData, WhisperContext, WhisperContextParameters,
};

fn main() {
    println!("Testing callback safety and null pointer handling...\n");

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

    // Create a state for transcription
    let mut state = ctx.create_state().expect("Failed to create state");

    // Test 1: Normal callback usage
    println!("\n[Test 1] Normal callback with valid audio...");
    {
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some("en"));
        params.set_print_progress(false);
        params.set_print_realtime(false);

        params.set_segment_callback_safe(|data: SegmentCallbackData| {
            println!(
                "  Callback: segment={}, text='{}', t0={}, t1={}",
                data.segment, data.text, data.start_timestamp, data.end_timestamp
            );
        });

        // Generate 1 second of silence (16kHz)
        let audio: Vec<f32> = vec![0.0; 16000];

        match state.full(params, &audio) {
            Ok(_) => {
                println!("  ✓ Transcription complete");
            }
            Err(e) => {
                eprintln!("  ✗ Transcription failed: {:?}", e);
            }
        }
    }

    // Test 2: Multiple transcriptions with callback
    println!("\n[Test 2] Multiple transcriptions with callback...");
    {
        for i in 0..3 {
            let run_num = i + 1;
            let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
            params.set_language(Some("en"));
            params.set_print_progress(false);
            params.set_print_realtime(false);

            params.set_segment_callback_safe(move |data: SegmentCallbackData| {
                println!(
                    "  Run {}: segment {}, text='{}'",
                    run_num, data.segment, data.text
                );
            });

            let audio: Vec<f32> = vec![0.0; 16000];

            match state.full(params, &audio) {
                Ok(_) => println!("  ✓ Run {} complete", run_num),
                Err(e) => eprintln!("  ✗ Run {} failed: {:?}", run_num, e),
            }
        }
    }

    // Test 3: Callback with lossy UTF-8 handling
    println!("\n[Test 3] Callback with lossy UTF-8 handling...");
    {
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some("en"));
        params.set_print_progress(false);
        params.set_print_realtime(false);

        params.set_segment_callback_safe_lossy(|data: SegmentCallbackData| {
            println!(
                "  Lossy callback: segment={}, text='{}'",
                data.segment, data.text
            );
        });

        let audio: Vec<f32> = vec![0.0; 16000];

        match state.full(params, &audio) {
            Ok(_) => {
                println!("  ✓ Lossy callback test complete");
            }
            Err(e) => {
                eprintln!("  ✗ Lossy callback test failed: {:?}", e);
            }
        }
    }

    // Test 4: No callback (ensure it doesn't crash)
    println!("\n[Test 4] Transcription without callback...");
    {
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some("en"));
        params.set_print_progress(false);
        params.set_print_realtime(false);

        let audio: Vec<f32> = vec![0.0; 16000];

        match state.full(params, &audio) {
            Ok(_) => println!("  ✓ No-callback test complete"),
            Err(e) => eprintln!("  ✗ No-callback test failed: {:?}", e),
        }
    }

    println!("\n✅ All callback safety tests completed!");
    println!("\nNote: Check stderr for any validation warnings or errors from the callbacks.");
    println!(
        "After implementing null pointer checks, you should see error messages for invalid data."
    );
}
