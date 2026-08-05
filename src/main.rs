use std::{path::PathBuf, process::ExitCode};

use clap::{Args, Parser, Subcommand};
use running_drafts_editor::chunking::{
    plan, plan_with_detector, read_canonical_wav, CanonicalAudio, DetectorIdentity, PlannerConfig,
    RecognizerContract, SileroConfig, SileroDetector,
};
use sha2::{Digest, Sha256};

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
    model_sha256: Option<String>,
    #[arg(long, default_value = "v5")]
    model_version: String,
    #[arg(long, default_value_t = 80_000)]
    search_back_samples: u64,
    #[arg(long, default_value_t = 160_000)]
    minimum_chunk_samples: u64,
    #[arg(long, default_value_t = 0.5)]
    speech_threshold: f32,
    #[arg(long, default_value_t = 1_600)]
    minimum_low_speech_samples: u64,
    #[arg(long, default_value_t = 1)]
    intra_threads: usize,
    #[arg(long, default_value = "whisper-rs/backtrack")]
    recognizer_name: String,
    #[arg(long, default_value = "unspecified")]
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
    let derived_model_sha256 = hash_model(&args.model)?;
    let model_sha256 = args.model_sha256.unwrap_or(derived_model_sha256);
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
        expected_model_sha256: model_sha256.clone(),
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
                model_sha256,
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

fn hash_model(path: &std::path::Path) -> Result<String, ModelReadError> {
    let bytes = std::fs::read(path).map_err(|source| ModelReadError {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

#[derive(Debug, thiserror::Error)]
#[error("could not read model '{}': {source}", path.display())]
struct ModelReadError {
    path: PathBuf,
    #[source]
    source: std::io::Error,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_requires_only_input_and_model() {
        let cli = Cli::try_parse_from([
            "rde",
            "chunk",
            "plan",
            "--input",
            "audio.wav",
            "--model",
            "silero.onnx",
        ])
        .unwrap();
        let Command::Chunk {
            command: ChunkCommand::Plan(args),
        } = cli.command;

        assert_eq!(args.model_version, "v5");
        assert_eq!(args.search_back_samples, 80_000);
        assert_eq!(args.minimum_chunk_samples, 160_000);
        assert_eq!(args.speech_threshold, 0.5);
        assert_eq!(args.minimum_low_speech_samples, 1_600);
        assert_eq!(args.recognizer_version, "unspecified");
        assert_eq!(args.max_submitted_samples, 480_000);
        assert!(args.model_sha256.is_none());
    }

    #[test]
    fn absent_model_fails_preflight() {
        let error = hash_model(std::path::Path::new(
            "/path/which/does/not/exist/silero.onnx",
        ))
        .unwrap_err();

        assert_eq!(error.source.kind(), std::io::ErrorKind::NotFound);
        assert!(error.to_string().contains("could not read model"));
        assert!(error.to_string().contains("silero.onnx"));
    }
}
