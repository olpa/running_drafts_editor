use std::{path::PathBuf, process::ExitCode};

use clap::{Args, Parser, Subcommand};
use running_drafts_editor::audition::{run_recognition_session, Ffplay};
use running_drafts_editor::chunking::{read_canonical_wav, SourceFacts};
use running_drafts_editor::recognition::{recognize, RecognitionConfig, WhisperDecoder};

#[derive(Debug, Parser)]
#[command(name = "rde", version, about = "Running Drafts Editor (experimental)")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Experimental recognition-chunk operations.
    Chunk {
        #[command(subcommand)]
        command: ChunkCommand,
    },
}

#[derive(Debug, Subcommand)]
enum ChunkCommand {
    /// Decode, list, and interactively replay recognition chunks.
    Audition(AuditionArgs),
}

#[derive(Debug, Args)]
struct AuditionArgs {
    /// Canonical mono 16 kHz float WAV audio.
    #[arg(long)]
    input: PathBuf,
    /// Whisper ggml model.
    #[arg(long)]
    model: PathBuf,
    #[arg(long, default_value = "auto")]
    language: String,
    #[arg(long, default_value_t = 4)]
    threads: usize,
    #[arg(long, default_value_t = 384_000)]
    target_core_samples: u64,
    #[arg(long, default_value_t = 48_000)]
    left_context_samples: u64,
    #[arg(long, default_value_t = 48_000)]
    right_context_samples: u64,
    #[arg(long, default_value_t = 160_000)]
    minimum_advance_samples: u64,
    #[arg(long, default_value_t = 5)]
    top_candidates: usize,
    #[arg(long, default_value_t = 1_000)]
    max_prompt_chars: usize,
    /// ffplay-compatible playback executable.
    #[arg(long, default_value = "ffplay")]
    player: PathBuf,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("rde: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let Cli { command } = Cli::parse();
    match command {
        Command::Chunk {
            command: ChunkCommand::Audition(args),
        } => run_audition(args),
    }
}

fn run_audition(args: AuditionArgs) -> Result<(), Box<dyn std::error::Error>> {
    let wav = read_canonical_wav(&args.input)?;
    let source = SourceFacts {
        sha256: wav.source_sha256,
        sample_rate_hz: wav.sample_rate_hz,
        channels: wav.channels,
        decoded_sample_count: u64::try_from(wav.samples.len())?,
    };
    let config = RecognitionConfig {
        target_core_samples: args.target_core_samples,
        left_context_samples: args.left_context_samples,
        right_context_samples: args.right_context_samples,
        minimum_advance_samples: args.minimum_advance_samples,
        language: args.language,
        threads: args.threads,
        top_candidates: args.top_candidates,
        max_prompt_chars: args.max_prompt_chars,
        ..RecognitionConfig::default()
    };
    let mut decoder = WhisperDecoder::load(&args.model, &config)?;
    let run = recognize(source, &wav.samples, config, &mut decoder)?;

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    let mut input = stdin.lock();
    let mut output = stdout.lock();
    let mut errors = stderr.lock();
    let mut player = Ffplay::new(args.player);
    run_recognition_session(
        &run,
        &args.input,
        &mut input,
        &mut output,
        &mut errors,
        &mut player,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audition_has_inspectable_whisper_window_defaults() {
        let cli = Cli::try_parse_from([
            "rde",
            "chunk",
            "audition",
            "--input",
            "audio.wav",
            "--model",
            "whisper.bin",
        ])
        .unwrap();
        let Command::Chunk {
            command: ChunkCommand::Audition(args),
        } = cli.command;

        assert_eq!(args.input, PathBuf::from("audio.wav"));
        assert_eq!(args.player, PathBuf::from("ffplay"));
        assert_eq!(args.model, PathBuf::from("whisper.bin"));
        assert_eq!(args.language, "auto");
        assert_eq!(args.threads, 4);
        assert_eq!(args.target_core_samples, 384_000);
        assert_eq!(args.left_context_samples, 48_000);
        assert_eq!(args.right_context_samples, 48_000);
        assert_eq!(args.minimum_advance_samples, 160_000);
        assert_eq!(args.top_candidates, 5);
        assert_eq!(args.max_prompt_chars, 1_000);
    }
}
