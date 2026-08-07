/*
Interactive whisper shell (whsh) - Rust implementation

Usage: cargo run --example whsh [-l lang] <model_path> <audio_file>

After transcription, enters interactive mode with commands:
- help, ?          : Show available commands
- pos N top [K]    : Show top K candidate tokens at position N (default K=10)
- pos N id TID     : Force token TID at position N and re-transcribe
- lang [code]      : Show/change language and re-transcribe (e.g., 'en', 'de', 'de-en' for translation)
- quit, exit       : Exit the shell

Architecture: Encode-once, decode-many
- Audio is encoded once at startup
- Re-transcriptions use skip_encode=true to reuse the encoding
*/

use std::io::{self, BufRead, Write};
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState,
    WhisperTokenId,
};

/// Maps global token position to segment/token indices
#[derive(Debug, Clone)]
struct TokenPosition {
    segment_idx: i32,
    token_idx: i32,
    token_id: WhisperTokenId,
}

fn print_help() {
    println!("\nAvailable commands:");
    println!("  help, ?           - Show this help message");
    println!("  pos N top [K]     - Show top K candidates at position N (default K=10)");
    println!("  pos N id TID      - Force token TID at position N and re-transcribe");
    println!("  lang [code]       - Show/change language and re-transcribe");
    println!("                      Use language code (e.g., 'en', 'de', 'fr') for transcription");
    println!("                      Use 'code-en' (e.g., 'de-en') for translation to English");
    println!("  quit, exit        - Exit the shell");
    println!();
}

fn format_timestamp(t: i64) -> String {
    let ms = t * 10;
    let s = ms / 1000;
    let ms = ms % 1000;
    let m = s / 60;
    let s = s % 60;
    format!("{:02}:{:02}.{:03}", m, s, ms)
}

/// Perform transcription and return token position map
/// lang: language code (e.g., "en", "de", "fr") or "code-en" for automatic translation
fn do_transcription(
    state: &mut WhisperState,
    pcm: &[f32],
    lang: &str,
    forced_tokens: Option<&[i32]>,
    skip_encode: bool,
) -> Result<Vec<TokenPosition>, Box<dyn std::error::Error>> {
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

    params.set_print_realtime(false);
    params.set_print_progress(false);
    params.set_print_timestamps(true);
    params.set_print_special(false);

    // Parse language parameter: if it ends with "-en", enable translation
    let (translate, language) = if lang.len() >= 3 && lang.ends_with("-en") {
        // Extract the source language code (before "-en")
        let source_lang = &lang[..lang.len() - 3];
        (true, source_lang)
    } else {
        (false, lang)
    };

    params.set_translate(translate);
    params.set_language(Some(language));
    params.set_n_threads(1);
    params.set_no_timestamps(false);
    params.set_token_timestamps(false);
    params.set_temperature(0.0);
    params.set_temperature_inc(0.0);

    // Capture top candidates for interactive queries
    params.set_capture_top_candidates(true);
    params.set_n_top_candidates(20);

    // Skip encoding if requested (reuse kv_cross from previous encode)
    params.set_skip_encode(skip_encode);

    // Set forced tokens if provided
    if let Some(tokens) = forced_tokens {
        params.set_forced_tokens_owned(tokens.to_vec());
    }

    // Run the transcription
    state.full(params, pcm)?;

    // Print transcription
    println!("\n=== Transcription ===");
    let n_segments = state.full_n_segments();
    for i in 0..n_segments {
        if let Some(segment) = state.get_segment(i) {
            let t0 = segment.start_timestamp();
            let t1 = segment.end_timestamp();
            let text = segment.to_str_lossy().unwrap_or_default();
            println!(
                "[{} --> {}]  {}",
                format_timestamp(t0),
                format_timestamp(t1),
                text
            );
        }
    }
    println!();

    // Build token position map and print token details
    println!();
    let mut token_map = Vec::new();
    let mut global_pos = 0;
    let mut first_token = true;

    for i in 0..n_segments {
        if let Some(segment) = state.get_segment(i) {
            let n_tokens = segment.n_tokens();
            for j in 0..n_tokens {
                if let Some(token) = segment.get_token(j) {
                    let token_id = token.token_id();
                    let token_text = token.to_str_lossy().unwrap_or_default();
                    let token_p = token.token_probability();

                    token_map.push(TokenPosition {
                        segment_idx: i,
                        token_idx: j,
                        token_id,
                    });

                    if !first_token {
                        print!(" | ");
                    }
                    print!("{},{},{},{:.4}", global_pos, token_id, token_text, token_p);
                    first_token = false;
                    global_pos += 1;
                }
            }
        }
    }
    println!("\n");

    Ok(token_map)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "whisper-rs {} | whisper.cpp {}",
        whisper_rs::get_version(),
        whisper_rs::get_whisper_cpp_version()
    );

    let args: Vec<String> = std::env::args().collect();

    // Parse command-line arguments
    let mut model_path: Option<String> = None;
    let mut audio_path: Option<String> = None;
    let mut language = String::from("en"); // Default language (mutable for in-app lang command)

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-l" | "--lang" => {
                if i + 1 >= args.len() {
                    eprintln!("error: -l/--lang requires a language code argument");
                    std::process::exit(1);
                }
                i += 1;
                language = args[i].clone();
            }
            _ => {
                // First positional argument is model_path, second is audio_path
                if model_path.is_none() {
                    model_path = Some(args[i].clone());
                } else if audio_path.is_none() {
                    audio_path = Some(args[i].clone());
                } else {
                    eprintln!("error: unexpected argument '{}'", args[i]);
                    std::process::exit(1);
                }
            }
        }
        i += 1;
    }

    // Check if required arguments are provided
    if model_path.is_none() || audio_path.is_none() {
        eprintln!("Usage: {} [-l lang] <model_path> <audio_file>", args[0]);
        eprintln!();
        eprintln!("Interactive whisper shell - transcribes audio then enters interactive mode.");
        eprintln!("Options:");
        eprintln!("  -l, --lang <code>  Language code for transcription (default: en)");
        eprintln!(
            "                     Use language code (e.g., 'en', 'de', 'fr') for transcription"
        );
        eprintln!("                     Use 'code-en' for automatic translation to English");
        eprintln!("Fixed settings: CPU only, single thread");
        std::process::exit(1);
    }

    let model_path = model_path.unwrap();
    let audio_path = audio_path.unwrap();

    // Read audio file
    let reader = hound::WavReader::open(&audio_path)?;
    let spec = reader.spec();

    let samples: Vec<i16> = reader.into_samples::<i16>().map(|s| s.unwrap()).collect();

    // Convert to f32
    let mut pcm_f32 = vec![0.0f32; samples.len()];
    whisper_rs::convert_integer_to_float_audio(&samples, &mut pcm_f32)?;

    // Convert stereo to mono if needed
    let pcm = if spec.channels == 2 {
        let mut mono = vec![0.0f32; pcm_f32.len() / 2];
        whisper_rs::convert_stereo_to_mono_audio(&pcm_f32, &mut mono)?;
        mono
    } else {
        pcm_f32
    };

    eprintln!(
        "Processing '{}' ({} samples, {:.1} sec)",
        &audio_path,
        pcm.len(),
        pcm.len() as f32 / 16000.0
    );

    // Initialize whisper context
    let mut ctx_params = WhisperContextParameters::default();
    ctx_params.use_gpu(false);

    let ctx = WhisperContext::new_with_params(&model_path, ctx_params)?;

    // Create state that will be reused
    let mut state = ctx.create_state()?;

    // Perform initial transcription
    let mut token_map = do_transcription(&mut state, &pcm, &language, None, false)?;

    if token_map.is_empty() {
        eprintln!("No tokens produced from transcription");
        return Ok(());
    }

    // Enter interactive mode
    println!("Entering interactive mode. Type 'help' or '?' for available commands.");

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("whsh> ");
        stdout.flush()?;

        let mut line = String::new();
        if stdin.lock().read_line(&mut line)? == 0 {
            break; // EOF
        }

        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Parse command
        let parts: Vec<&str> = line.split_whitespace().collect();
        let cmd = parts.first().map(|s| *s).unwrap_or("");

        match cmd {
            "quit" | "exit" => {
                println!("Exiting whsh...");
                break;
            }
            "help" | "?" => {
                print_help();
            }
            "lang" => {
                if parts.len() < 2 {
                    // No argument provided - show current language
                    println!("Current language: {}", language);
                    println!("Usage: lang <code>");
                    println!("  <code> can be a language code (e.g., 'en', 'de', 'fr') for transcription");
                    println!("  or 'code-en' (e.g., 'de-en', 'fr-en') for automatic translation to English");
                    continue;
                }

                let new_lang = parts[1];
                println!(
                    "Changing language from '{}' to '{}' and re-transcribing...",
                    language, new_lang
                );
                language = new_lang.to_string();

                // Re-transcribe with new language (skip_encode=false to get fresh encoding)
                match do_transcription(&mut state, &pcm, &language, None, false) {
                    Ok(new_map) => {
                        if new_map.is_empty() {
                            println!("Re-transcription failed: no tokens produced");
                        } else {
                            token_map = new_map;
                        }
                    }
                    Err(e) => {
                        println!("Re-transcription failed: {}", e);
                    }
                }
            }
            "pos" => {
                if parts.len() < 3 {
                    println!("Usage: pos N top [K] or pos N id TID");
                    continue;
                }

                let pos_n: usize = match parts[1].parse() {
                    Ok(n) => n,
                    Err(_) => {
                        println!("Invalid position: {}", parts[1]);
                        continue;
                    }
                };

                if pos_n >= token_map.len() {
                    println!(
                        "Error: position {} out of range [0, {}]",
                        pos_n,
                        token_map.len() - 1
                    );
                    continue;
                }

                let subcmd = parts[2];

                match subcmd {
                    "top" => {
                        let top_k: usize = parts.get(3).and_then(|s| s.parse().ok()).unwrap_or(10);

                        // Get top candidates for the token at pos_n
                        let pos = &token_map[pos_n];

                        if let Some(segment) = state.get_segment(pos.segment_idx) {
                            if let Some(token) = segment.get_token(pos.token_idx) {
                                let n_candidates = token.n_top_candidates();
                                if n_candidates == 0 {
                                    println!("No candidates available for position {}", pos_n);
                                    continue;
                                }

                                let k = top_k.min(n_candidates as usize);
                                println!("Top {} candidates at position {}:", k, pos_n);

                                for i in 0..k {
                                    let cand = token.get_top_candidate(i as i32);
                                    let token_text = ctx
                                        .token_to_str_lossy(cand.id)
                                        .unwrap_or_else(|_| "<?>".into());
                                    println!(
                                        "  {}: id={} token='{}' prob={:.4} logprob={:.4}",
                                        i + 1,
                                        cand.id,
                                        token_text,
                                        cand.p,
                                        cand.plog
                                    );
                                }
                            }
                        }
                    }
                    "id" => {
                        if parts.len() < 4 {
                            println!("Usage: pos N id TID");
                            continue;
                        }

                        let token_id: i32 = match parts[3].parse() {
                            Ok(id) => id,
                            Err(_) => {
                                println!("Invalid token ID: {}", parts[3]);
                                continue;
                            }
                        };

                        println!(
                            "Re-transcribing with token {} forced at position {} (skip_encode + forced_tokens)...",
                            token_id, pos_n
                        );

                        // Build forced tokens: all tokens up to pos_n, with token_id at pos_n
                        let mut forced_tokens: Vec<i32> = Vec::with_capacity(pos_n + 1);
                        for i in 0..=pos_n {
                            if i == pos_n {
                                forced_tokens.push(token_id);
                            } else {
                                forced_tokens.push(token_map[i].token_id);
                            }
                        }

                        // Re-transcribe with skip_encode=true and forced_tokens
                        match do_transcription(
                            &mut state,
                            &pcm,
                            &language,
                            Some(&forced_tokens),
                            true,
                        ) {
                            Ok(new_map) => {
                                if new_map.is_empty() {
                                    println!("Re-transcription failed: no tokens produced");
                                } else {
                                    token_map = new_map;
                                }
                            }
                            Err(e) => {
                                println!("Re-transcription failed: {}", e);
                            }
                        }
                    }
                    _ => {
                        println!(
                            "Unknown subcommand: '{}'. Usage: pos N top [K] or pos N id TID",
                            subcmd
                        );
                    }
                }
            }
            _ => {
                println!(
                    "Unknown command: '{}'. Type 'help' or '?' for available commands.",
                    cmd
                );
            }
        }
    }

    Ok(())
}
