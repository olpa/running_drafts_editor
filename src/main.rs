use std::{
    fs::File,
    io::{BufWriter, Read, Write},
    path::PathBuf,
    process::ExitCode,
};

use clap::{Args, Parser, Subcommand};
use running_drafts_editor::audition::{run_session, Ffplay};
use running_drafts_editor::chunking::{
    plan_from_source, stream_canonical_wav, DetectorErrorCode, DetectorIdentity, DetectorStatus,
    FrameEvidence, PlanFailure, PlannerConfig, PlannerRun, RecognitionChunk, RecognitionPlan,
    RecognizerContract, SileroConfig, SileroDetector, SourceFacts, SpeechDetector,
};
use serde::Serialize;
use sha2::{Digest, Sha256};

const JSONL_SCHEMA: &str = "recognition-plan-jsonl/v1-experimental";

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
    /// List planned chunks and interactively play them for development.
    Audition(AuditionArgs),
}

#[derive(Debug, Args)]
struct PlanArgs {
    #[arg(long)]
    input: PathBuf,
    #[command(flatten)]
    planning: PlanningArgs,
}

#[derive(Debug, Args)]
struct AuditionArgs {
    /// Canonical mono 16 kHz float WAV audio.
    #[arg(long)]
    input: PathBuf,
    #[command(flatten)]
    planning: PlanningArgs,
    /// ffplay-compatible playback executable.
    #[arg(long, default_value = "ffplay")]
    player: PathBuf,
}

#[derive(Debug, Args)]
struct PlanningArgs {
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

#[derive(Serialize)]
struct PlanStarted<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    schema: &'static str,
    detector: &'a DetectorIdentity,
}

#[derive(Serialize)]
struct EvidenceEvent<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    evidence: &'a FrameEvidence,
}

#[derive(Serialize)]
struct DetectorSummary<'a> {
    identity: &'a DetectorIdentity,
    status: &'a DetectorStatus,
    error_code: &'a Option<DetectorErrorCode>,
    evidence_records_emitted: u64,
    evidence_records_used: usize,
}

#[derive(Serialize)]
struct PlanComplete<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    schema: &'static str,
    plan_schema: &'a str,
    id: &'a str,
    plan_inputs_hash: &'a str,
    revision: u64,
    source: &'a SourceFacts,
    recognizer: &'a RecognizerContract,
    detector: DetectorSummary<'a>,
    planner: &'a PlannerRun,
    chunks: &'a [RecognitionChunk],
    failures: &'a [PlanFailure],
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
        Command::Chunk {
            command: ChunkCommand::Audition(args),
        } => run_audition(args),
    }
}

fn run_plan(args: PlanArgs) -> Result<(), Box<dyn std::error::Error>> {
    let stdout = std::io::stdout();
    let mut output = BufWriter::new(stdout.lock());
    let (result, evidence_records_emitted) =
        create_plan(&args.input, &args.planning, Some(&mut output))?;
    emit_plan_complete(&mut output, &result, evidence_records_emitted)?;
    output.flush()?;
    eprintln!(
        "planned {} samples into {} chunks ({} fallbacks)",
        result.source.decoded_sample_count,
        result.chunks.len(),
        result.failures.len()
    );
    Ok(())
}

fn run_audition(args: AuditionArgs) -> Result<(), Box<dyn std::error::Error>> {
    let (plan, _) = create_plan(&args.input, &args.planning, None)?;
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    let mut input = stdin.lock();
    let mut output = stdout.lock();
    let mut errors = stderr.lock();
    let mut player = Ffplay::new(args.player);
    run_session(
        &plan,
        &args.input,
        &mut input,
        &mut output,
        &mut errors,
        &mut player,
    )?;
    Ok(())
}

fn create_plan(
    input: &std::path::Path,
    args: &PlanningArgs,
    mut jsonl: Option<&mut dyn Write>,
) -> Result<(RecognitionPlan, u64), Box<dyn std::error::Error>> {
    let derived_model_sha256 = hash_model(&args.model)?;
    let model_sha256 = args.model_sha256.clone().unwrap_or(derived_model_sha256);
    let recognizer = RecognizerContract {
        name: args.recognizer_name.clone(),
        version: args.recognizer_version.clone(),
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
    let mut evidence_records_emitted = 0_u64;
    let (wav, detector_identity, detector_result) =
        match SileroDetector::load(&args.model, detector_config) {
            Ok(mut detector) => {
                let identity = detector.identity();
                if let Some(output) = jsonl.as_deref_mut() {
                    emit_jsonl(
                        output,
                        &PlanStarted {
                            kind: "plan_started",
                            schema: JSONL_SCHEMA,
                            detector: &identity,
                        },
                    )?;
                    output.flush()?;
                }
                let mut output_error = None;
                let (wav, result) = detector.detect_streamed_wav(input, |evidence| {
                    let Some(output) = jsonl.as_deref_mut() else {
                        return;
                    };
                    if output_error.is_none() {
                        match emit_jsonl(
                            output,
                            &EvidenceEvent {
                                kind: "detector_evidence",
                                evidence,
                            },
                        ) {
                            Ok(()) => evidence_records_emitted += 1,
                            Err(error) => output_error = Some(error),
                        }
                    }
                })?;
                if let Some(error) = output_error {
                    return Err(error);
                }
                (wav, identity, result)
            }
            Err(error) => {
                let identity = DetectorIdentity {
                    name: "silero-vad-onnx".into(),
                    version: args.model_version.clone(),
                    model_sha256,
                    frame_samples: 512,
                    sample_rate_hz: 16_000,
                    runtime: format!("ort-2.0.0-rc.10/cpu/intra-threads-{}", args.intra_threads),
                };
                if let Some(output) = jsonl {
                    emit_jsonl(
                        output,
                        &PlanStarted {
                            kind: "plan_started",
                            schema: JSONL_SCHEMA,
                            detector: &identity,
                        },
                    )?;
                    output.flush()?;
                }
                let wav = stream_canonical_wav(input, |_, _| {})?;
                (wav, identity, Err(error))
            }
        };
    let result = plan_from_source(
        SourceFacts {
            sha256: wav.source_sha256,
            sample_rate_hz: wav.sample_rate_hz,
            channels: wav.channels,
            decoded_sample_count: wav.decoded_sample_count,
        },
        recognizer,
        planner,
        detector_identity,
        detector_result,
    )?;
    Ok((result, evidence_records_emitted))
}

fn emit_plan_complete(
    output: &mut impl Write,
    plan: &RecognitionPlan,
    evidence_records_emitted: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    emit_jsonl(
        output,
        &PlanComplete {
            kind: "plan_complete",
            schema: JSONL_SCHEMA,
            plan_schema: &plan.schema,
            id: &plan.id,
            plan_inputs_hash: &plan.plan_inputs_hash,
            revision: plan.revision,
            source: &plan.source,
            recognizer: &plan.recognizer,
            detector: DetectorSummary {
                identity: &plan.detector.identity,
                status: &plan.detector.status,
                error_code: &plan.detector.error_code,
                evidence_records_emitted,
                evidence_records_used: plan.detector.evidence.len(),
            },
            planner: &plan.planner,
            chunks: &plan.chunks,
            failures: &plan.failures,
        },
    )
}

fn emit_jsonl(
    output: &mut (impl Write + ?Sized),
    value: &impl Serialize,
) -> Result<(), Box<dyn std::error::Error>> {
    serde_json::to_writer(&mut *output, value)?;
    output.write_all(b"\n")?;
    Ok(())
}

fn hash_model(path: &std::path::Path) -> Result<String, ModelReadError> {
    let mut file = File::open(path).map_err(|source| ModelReadError {
        path: path.to_path_buf(),
        source,
    })?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|source| ModelReadError {
            path: path.to_path_buf(),
            source,
        })?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(hex::encode(digest.finalize()))
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
        } = cli.command
        else {
            panic!("expected plan command");
        };

        assert_eq!(args.planning.model_version, "v5");
        assert_eq!(args.planning.search_back_samples, 80_000);
        assert_eq!(args.planning.minimum_chunk_samples, 160_000);
        assert_eq!(args.planning.speech_threshold, 0.5);
        assert_eq!(args.planning.minimum_low_speech_samples, 1_600);
        assert_eq!(args.planning.recognizer_version, "unspecified");
        assert_eq!(args.planning.max_submitted_samples, 480_000);
        assert!(args.planning.model_sha256.is_none());
    }

    #[test]
    fn audition_matches_plan_input_flag_and_uses_ffplay_by_default() {
        let cli = Cli::try_parse_from([
            "rde",
            "chunk",
            "audition",
            "--input",
            "audio.wav",
            "--model",
            "silero.onnx",
        ])
        .unwrap();
        let Command::Chunk {
            command: ChunkCommand::Audition(args),
        } = cli.command
        else {
            panic!("expected audition command");
        };

        assert_eq!(args.input, PathBuf::from("audio.wav"));
        assert_eq!(args.player, PathBuf::from("ffplay"));
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
