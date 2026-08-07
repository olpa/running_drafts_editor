// Demonstration of potential buffer overflow in tokenize() - BEFORE FIX
//
// THE PROBLEM (Current Code - whisper_ctx.rs:94-110):
// ====================================================
//
// Line 95: let mut tokens: Vec<WhisperTokenId> = Vec::with_capacity(max_tokens);
// Line 108: unsafe { tokens.set_len(ret as usize) };
//
// ISSUE: No validation that `ret <= max_tokens`!
//
// If whisper_tokenize() returns a value > max_tokens, we'll set the vector
// length beyond its capacity, causing undefined behavior.
//
// SCENARIO:
// - Vec::with_capacity(10) allocates space for 10 items
// - C function writes 15 items (buffer overflow in C)
// - C function returns 15
// - Line 108 sets length to 15, but capacity is only 10!
// - Accessing elements 10-14 = UNDEFINED BEHAVIOR
//
// This test DOCUMENTS the issue but cannot easily trigger it because:
// - whisper.cpp's tokenize is usually well-behaved
// - We can't easily mock the C function
// - The actual overflow would happen in C code
//
// Instead, we analyze the CODE to show the vulnerability.

use whisper_rs::{WhisperContext, WhisperContextParameters};

fn main() {
    println!("ANALYSIS: Buffer Overflow Vulnerability in tokenize()\n");
    println!("======================================================\n");

    println!("CURRENT CODE (whisper_ctx.rs:94-110):");
    println!("  let mut tokens: Vec<WhisperTokenId> = Vec::with_capacity(max_tokens);");
    println!("  let ret = unsafe {{ whisper_tokenize(..., max_tokens) }};");
    println!("  if ret == -1 {{");
    println!("      Err(WhisperError::InvalidText)");
    println!("  }} else {{");
    println!("      unsafe {{ tokens.set_len(ret as usize) }};  // ❌ NO VALIDATION!");
    println!("      Ok(tokens)");
    println!("  }}\n");

    println!("THE VULNERABILITY:");
    println!("==================");
    println!("❌ Line 108: No check that `ret <= max_tokens`");
    println!("❌ If C returns ret > max_tokens:");
    println!("   - Vec has capacity for max_tokens");
    println!("   - But length is set to ret");
    println!("   - Accessing beyond capacity = UNDEFINED BEHAVIOR\n");

    println!("ATTACK SCENARIO:");
    println!("================");
    println!("1. Allocate Vec with capacity 10");
    println!("2. C function misbehaves, writes 15 tokens");
    println!("3. C function returns 15");
    println!("4. set_len(15) but capacity is 10");
    println!("5. Access to tokens[10..14] reads uninitialized memory\n");

    println!("DEMONSTRATION:");
    println!("==============");

    // Try to demonstrate with actual API (though whisper.cpp is usually safe)
    let model_path = "ggml-tiny.bin";

    match WhisperContext::new_with_params(model_path, WhisperContextParameters::default()) {
        Ok(ctx) => {
            println!("✓ Loaded model: {}\n", model_path);

            // Test with very restrictive max_tokens
            println!("[Test] Tokenizing with max_tokens=1:");
            let text = "Hello world this is a long sentence";
            println!("  Text: '{}'", text);
            println!("  Max tokens: 1");

            match ctx.tokenize(text, 1) {
                Ok(tokens) => {
                    println!("  Returned: {} tokens", tokens.len());

                    if tokens.len() > 1 {
                        println!("\n  ⚠️  VULNERABILITY DEMONSTRATED!");
                        println!(
                            "  C function returned {} tokens but max was 1!",
                            tokens.len()
                        );
                        println!("  This would be undefined behavior!");
                    } else {
                        println!("  ℹ️  whisper.cpp respected max_tokens (good)");
                        println!("  But the CODE still has no validation!");
                    }
                }
                Err(e) => {
                    println!("  Error: {:?}", e);
                }
            }
        }
        Err(_) => {
            println!("Could not load model (this is okay for demonstration)\n");
        }
    }

    println!("\n");
    println!("CONCLUSION:");
    println!("===========");
    println!("Even if whisper.cpp is well-behaved NOW, the Rust code should:");
    println!("✅ Validate return value: if ret > max_tokens => error");
    println!("✅ Use MaybeUninit for proper uninitialized memory handling");
    println!("✅ Only assume_init() for elements actually written by C");
    println!("\nThis follows Rust's safety principle: VERIFY, don't TRUST.");
}
