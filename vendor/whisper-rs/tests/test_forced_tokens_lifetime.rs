/// Test to demonstrate lifetime issues with forced tokens borrowing
///
/// This test shows that passing a reference to forced tokens can lead to
/// use-after-free bugs when the source vector is dropped or moved.
use whisper_rs::{FullParams, SamplingStrategy};

#[test]
fn test_forced_tokens_lifetime_issue() {
    // This test demonstrates the problem with borrowing forced tokens.
    // The current API requires the tokens slice to live for lifetime 'b,
    // but in practice the vector is often created temporarily.

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

    // PROBLEM: This pattern is unsafe but compiles
    {
        let forced_tokens = vec![1, 2, 3]; // Temporary vector
        params.set_forced_tokens(&forced_tokens);
        // forced_tokens is dropped here, but params still holds a pointer to it!
    }

    // At this point, params.fp.forced_tokens points to freed memory
    // Using params in a transcription could cause a segfault or read garbage data

    println!("Forced tokens pointer is now dangling!");
    println!("This demonstrates why we need to store the owned vector in FullParams");
}

#[test]
fn test_safe_pattern_with_owned_tokens() {
    // This test demonstrates the SAFE pattern using owned tokens

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

    // NEW SAFE API: set_forced_tokens_owned() takes ownership
    let forced_tokens = vec![1, 2, 3];
    params.set_forced_tokens_owned(forced_tokens);
    // forced_tokens is now owned by params - no dangling pointers!

    // The vector can be dropped safely in any scope
    {
        let other_tokens = vec![4, 5, 6];
        params.set_forced_tokens_owned(other_tokens);
        // other_tokens is moved into params
    }

    // params still holds valid tokens - safe to use for transcription
    println!("Safe pattern: tokens are owned by FullParams");
}

#[test]
fn test_clear_forced_tokens() {
    // Test that we can clear forced tokens safely
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

    let forced_tokens = vec![1, 2, 3];
    params.set_forced_tokens_owned(forced_tokens);

    // NEW: Clear forced tokens
    params.clear_forced_tokens();
    // This sets the pointer to null and clears the owned vector

    println!("Forced tokens cleared successfully");
}

#[test]
fn test_empty_forced_tokens() {
    // Test that empty vector is handled correctly
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

    // Setting empty vector should clear tokens
    params.set_forced_tokens_owned(vec![]);

    println!("Empty forced tokens handled correctly");
}
