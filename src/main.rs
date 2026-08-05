use std::{path::PathBuf, process::ExitCode};

use clap::{Args, Parser, Subcommand};
use running_drafts_editor::chunking::{
    plan, plan_with_detector, read_canonical_wav, CanonicalAudio, DetectorIdentity, PlannerConfig,
    RecognizerContract, SileroConfig, SileroDetector,
};

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
    /// Produce a recognition plan from canonical float WAV audio.
    Plan(PlanArgs),
}

#[derive(Debug, Args)]
struct PlanArgs {
    #[arg(long)]
    input: PathBuf,
    #[arg(long)]
    model: PathBuf,
    #[arg(long)]
    model_sha256: String,
    #[arg(long)]
    model_version: String,
    #[arg(long)]
    search_back_samples: u64,
    #[arg(long)]
    minimum_chunk_samples: u64,
    #[arg(long)]
    speech_threshold: f32,
    #[arg(long)]
    minimum_low_speech_samples: u64,
    #[arg(long, default_value_t = 1)]
    intra_threads: usize,
    #[arg(long, default_value = "whisper-rs/backtrack")]
    recognizer_name: String,
    #[arg(long)]
    recognizer_version: String,
    #[arg(long, default_value_t = 480_000)]
    max_submitted_samples: u64,
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
            command: ChunkCommand::Plan(args),
        } => run_plan(args),
    }
}

fn run_plan(args: PlanArgs) -> Result<(), Box<dyn std::error::Error>> {
    let wav = read_canonical_wav(&args.input)?;
    let audio = CanonicalAudio {
        samples: &wav.samples,
        sample_rate_hz: wav.sample_rate_hz,
        channels: wav.channels,
        source_sha256: &wav.source_sha256,
    };
    let recognizer = RecognizerContract {
        name: args.recognizer_name,
        version: args.recognizer_version,
        max_submitted_samples: args.max_submitted_samples,
    };
    let planner = PlannerConfig {
        search_back_samples: args.search_back_samples,
        minimum_chunk_samples: args.minimum_chunk_samples,
        speech_threshold: args.speech_threshold,
        minimum_low_speech_samples: args.minimum_low_speech_samples,
        left_padding_samples: 0,
        right_padding_samples: 0,
    };
    let detector_config = SileroConfig {
        model_version: args.model_version.clone(),
        expected_model_sha256: args.model_sha256.clone(),
        intra_threads: args.intra_threads,
    };
    let result = match SileroDetector::load(&args.model, detector_config) {
        Ok(mut detector) => plan_with_detector(&audio, recognizer, planner, &mut detector),
        Err(error) => plan(
            &audio,
            recognizer,
            planner,
            DetectorIdentity {
                name: "silero-vad-onnx".into(),
                version: args.model_version,
                model_sha256: args.model_sha256,
                frame_samples: 512,
                sample_rate_hz: 16_000,
                runtime: format!("ort-2.0.0-rc.10/cpu/intra-threads-{}", args.intra_threads),
            },
            Err(error),
        ),
    }?;
    serde_json::to_writer_pretty(std::io::stdout().lock(), &result)?;
    println!();
    eprintln!(
        "planned {} samples into {} chunks ({} fallbacks)",
        result.source.decoded_sample_count,
        result.chunks.len(),
        result.failures.len()
    );
    Ok(())
}
