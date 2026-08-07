// Test to demonstrate CString memory leak in set_language() and set_initial_prompt()
//
// Run with Valgrind to see leaks:
// cargo build --example test_memory_leak --release
// valgrind --leak-check=full --show-leak-kinds=all ./target/release/examples/test_memory_leak
//
// Expected: Memory leaks from repeated CString::into_raw() calls

use whisper_rs::{FullParams, SamplingStrategy};

fn main() {
    println!("Testing CString memory leaks in FullParams...\n");

    // Test 1: Repeated set_language() calls
    println!("[Test 1] Calling set_language() 1000 times...");
    {
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        for i in 0..1000 {
            if i % 100 == 0 {
                println!("  Iteration {}/1000", i);
            }
            params.set_language(Some("en"));
            params.set_language(Some("es"));
            params.set_language(Some("fr"));
        }
    }
    println!("  ✓ Completed (but leaked memory!)\n");

    // Test 2: Repeated set_initial_prompt() calls
    println!("[Test 2] Calling set_initial_prompt() 1000 times...");
    {
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        for i in 0..1000 {
            if i % 100 == 0 {
                println!("  Iteration {}/1000", i);
            }
            params.set_initial_prompt("First prompt");
            params.set_initial_prompt("Second prompt");
            params.set_initial_prompt("Third prompt");
        }
    }
    println!("  ✓ Completed (but leaked memory!)\n");

    println!("Tests complete.");
    println!("\nRun with Valgrind to see leaked memory:");
    println!("  cargo build --example test_memory_leak --release");
    println!("  valgrind --leak-check=full ./target/release/examples/test_memory_leak");
}
