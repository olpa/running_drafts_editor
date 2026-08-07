// Demonstration of CString memory leak pattern in WhisperVadContext::new()
//
// ANALYSIS:
// The code in whisper_vad.rs lines 148-151 has the SAME memory leak pattern
// as the FullParams methods we just fixed:
//
//   let model_path = CString::new(model_path)
//       .expect("VAD model path contains null byte")
//       .into_raw() as *const c_char;  // <-- LEAK!
//
// The CString is converted to a raw pointer and NEVER freed.
// The WhisperVadContext struct only stores the `ptr` field, not the CString.
//
// This is identical to the leak we just fixed in:
// - set_language()
// - set_initial_prompt()
// - set_vad_model_path()
//
// PROOF OF LEAK:
// Without a VAD model file, we can't run the actual function 1000 times,
// but the CODE PATTERN is identical to what we just proved leaks memory.
//
// The fix is the same:
// 1. Add `model_path_cstring: CString` field to WhisperVadContext
// 2. Store the CString before calling into_raw()
// 3. Use .as_ptr() instead of .into_raw()

use std::ffi::CString;

fn main() {
    println!("Demonstrating WHY WhisperVadContext::new() leaks memory\n");
    println!("CODE ANALYSIS:");
    println!("==============\n");

    println!("BEFORE (whisper_vad.rs:148-151):");
    println!("  let model_path = CString::new(model_path)");
    println!("      .expect(\"VAD model path contains null byte\")");
    println!("      .into_raw() as *const c_char;  // ❌ LEAKED!");
    println!();

    println!("This creates a CString and immediately leaks it.");
    println!("The WhisperVadContext only stores `ptr`, not the CString.\n");

    println!("PROOF:");
    println!("======\n");
    println!("Simulating the leak pattern 1000 times:\n");

    let mut leaked_bytes = 0;
    for i in 0..1000 {
        let path = format!("model_{}.bin", i % 10); // 10 different paths
        let cstring = CString::new(path.clone()).unwrap();
        leaked_bytes += cstring.as_bytes_with_nul().len();

        // This is what the code does:
        let _raw_ptr = cstring.into_raw(); // Memory leaked!
                                           // CString is now leaked, never freed

        if i % 100 == 0 {
            println!(
                "  Iteration {}/1000: leaked {} bytes so far",
                i, leaked_bytes
            );
        }
    }

    println!("\n  ✓ Completed simulation");
    println!(
        "  Total leaked: {} bytes in 1000 iterations\n",
        leaked_bytes
    );

    println!("SOLUTION (following plan lines 113-135):");
    println!("=========================================\n");
    println!("1. Add field: model_path_cstring: CString");
    println!("2. Store before use: self.model_path_cstring = CString::new(...)");
    println!("3. Use .as_ptr() instead of .into_raw()");
    println!();
    println!("This is IDENTICAL to the fix we just applied to FullParams.");
}
