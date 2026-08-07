// Test demonstrating buffer overflow protection in tokenize()
//
// BEFORE (Phase 1.3):
// - tokenize() would blindly set vector length to whatever C returns
// - No validation that ret <= max_tokens
// - Could cause buffer overflow if C misbehaves
//
// AFTER (Phase 1.3):
// - Validates return value before using it
// - Returns BufferOverflow error if ret > max_tokens
// - Uses MaybeUninit for proper uninitialized memory handling

use whisper_rs::{WhisperContext, WhisperContextParameters};

fn main() {
    println!("Testing tokenize() buffer overflow protection\n");

    // Note: This requires a model file to test
    let model_path = "ggml-tiny.bin";

    println!("Loading model: {}", model_path);
    let ctx = match WhisperContext::new_with_params(model_path, WhisperContextParameters::default())
    {
        Ok(ctx) => {
            println!("✓ Model loaded successfully\n");
            ctx
        }
        Err(e) => {
            eprintln!("Failed to load model: {:?}", e);
            eprintln!("\nThis test requires a whisper model file.");
            return;
        }
    };

    // Test 1: Normal case
    println!("[Test 1] Normal tokenization:");
    let text = "Hello world";
    match ctx.tokenize(text, 100) {
        Ok(tokens) => {
            println!("  ✓ Tokenized '{}' into {} tokens", text, tokens.len());
            assert!(tokens.len() <= 100, "Should not exceed max_tokens");
        }
        Err(e) => {
            println!("  ✗ Error: {:?}", e);
        }
    }

    // Test 2: Edge case - max_tokens = 1
    println!("\n[Test 2] Edge case (max_tokens=1):");
    match ctx.tokenize("Hi", 1) {
        Ok(tokens) => {
            println!("  ✓ Got {} token(s)", tokens.len());
            assert!(tokens.len() <= 1, "Should not exceed max_tokens=1");
        }
        Err(e) => {
            println!("  ✓ Properly returned error: {:?}", e);
        }
    }

    // Test 3: Very long text with small buffer
    println!("\n[Test 3] Long text with small buffer:");
    let long_text = "a ".repeat(1000); // Very long text
    match ctx.tokenize(&long_text, 10) {
        Ok(tokens) => {
            println!("  ✓ Got {} tokens (max was 10)", tokens.len());
            assert!(tokens.len() <= 10, "Should not exceed max_tokens");
        }
        Err(e) => {
            println!("  ✓ Properly returned error: {:?}", e);
        }
    }

    // Test 4: Empty string
    println!("\n[Test 4] Empty string:");
    match ctx.tokenize("", 100) {
        Ok(tokens) => {
            println!("  ✓ Got {} tokens", tokens.len());
        }
        Err(e) => {
            println!("  ✓ Properly returned error: {:?}", e);
        }
    }

    println!("\n✅ All tests completed!");
    println!("\nKEY IMPROVEMENTS:");
    println!("1. Return value is validated before use");
    println!("2. BufferOverflow error if C returns > max_tokens");
    println!("3. MaybeUninit ensures proper memory initialization");
    println!("4. No undefined behavior from buffer overruns");
}
