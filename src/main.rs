use std::{path::PathBuf, process::ExitCode};

use clap::{Args, Parser, Subcommand};
use running_drafts_editor::audition::{run_recognition_session, Ffplay};
use running_drafts_editor::chunking::{read_canonical_wav, SourceFacts};
use running_drafts_editor::document::Document;
use running_drafts_editor::editor::run_editor_session;
use running_drafts_editor::persistence::{load_document, save_document};
use running_drafts_editor::recognition::{
    recognize, PostChunkConfig, RecognitionConfig, WhisperDecoder,
};

#[derive(Debug, Parser)]
#[command(
    name = "rde",
    version,
    about = "Running Drafts Editor (experimental)",
    after_help = "Get started:\n  rde transcribe recording.wav --model ggml-tiny.bin --output draft.rde.json\n  rde edit draft.rde.json\n\nDeveloper recognition inspection:\n  rde audition --input recording.wav --model ggml-tiny.bin\n\nRun a command with '--help' for its options."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Transcribe audio to a saved document and exit.
    Transcribe(TranscribeArgs),
    /// Decode, list, and interactively replay recognition chunks.
    Audition(AuditionArgs),
    /// Open a saved visible document without running recognition.
    Edit(EditArgs),
}

#[derive(Debug, Args)]
#[command(
    after_help = "Example:\n  rde transcribe recording.wav --model ggml-tiny.bin --output draft.rde.json"
)]
struct TranscribeArgs {
    /// PCM WAV audio; channels and sample rate are converted automatically.
    input: PathBuf,
    /// Destination for the versioned JSON document.
    #[arg(long)]
    output: PathBuf,
    #[command(flatten)]
    recognition: RecognitionArgs,
}

#[derive(Debug, Args)]
struct EditArgs {
    /// Versioned JSON document to open and save.
    document: PathBuf,
    /// ffplay-compatible playback executable.
    #[arg(long, default_value = "ffplay")]
    player: PathBuf,
    /// Context added before and after text replay.
    #[arg(long, default_value_t = 750)]
    replay_context_ms: u64,
}

#[derive(Debug, Args)]
#[command(
    after_help = "Example:\n  rde audition --input recording.wav --model ggml-tiny.bin --language de\n\nAfter recognition, type 'help' at the 'rde>' prompt to see session commands."
)]
struct AuditionArgs {
    /// PCM WAV audio; channels and sample rate are converted automatically.
    #[arg(long)]
    input: PathBuf,
    #[command(flatten)]
    recognition: RecognitionArgs,
    /// Save the recognized visible document before entering the session.
    #[arg(long)]
    output: Option<PathBuf>,
    /// ffplay-compatible playback executable.
    #[arg(long, default_value = "ffplay")]
    player: PathBuf,
    /// Context added before and after text replay.
    #[arg(long, default_value_t = 750)]
    replay_context_ms: u64,
}

#[derive(Debug, Args)]
struct RecognitionArgs {
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
    #[arg(long, default_value_t = 5)]
    top_candidates: usize,
    /// Minimum normal text tokens before a strong or usable pause may split a chunk.
    #[arg(long, default_value_t = 8)]
    chunk_minimum_tokens: usize,
    /// Preferred number of normal text tokens in a chunk.
    #[arg(long, default_value_t = 32)]
    chunk_target_tokens: usize,
    /// Token limit that forces a split at a whole-segment boundary.
    #[arg(long, default_value_t = 64)]
    chunk_maximum_tokens: usize,
    /// Smallest pause considered when choosing a boundary near the target.
    #[arg(long, default_value_t = 300)]
    chunk_usable_pause_ms: u64,
    /// Pause that splits a chunk once it has the minimum token count.
    #[arg(long, default_value_t = 800)]
    chunk_strong_pause_ms: u64,
    /// Pause that always splits a chunk, even before the minimum token count.
    #[arg(long, default_value_t = 2_000)]
    chunk_long_pause_ms: u64,
    /// Score penalty per token of distance from the target size.
    #[arg(long, default_value_t = 20)]
    chunk_distance_penalty_ms: u64,
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
        Command::Transcribe(args) => run_transcribe(args),
        Command::Audition(args) => run_audition(args),
        Command::Edit(args) => run_edit(args),
    }
}

fn run_transcribe(args: TranscribeArgs) -> Result<(), Box<dyn std::error::Error>> {
    validate_output_target(&args.output)?;
    let run = recognize_audio(&args.input, &args.recognition)?;
    let document = Document::from_run_with_source(&run, Some(&args.input));
    save_document(&args.output, &document)?;
    println!("saved {}", args.output.display());
    Ok(())
}

fn validate_output_target(path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    if !parent.is_dir() {
        return Err(format!("output directory '{}' does not exist", parent.display()).into());
    }
    if path.is_dir() {
        return Err(format!("output path '{}' is a directory", path.display()).into());
    }
    Ok(())
}

fn run_edit(args: EditArgs) -> Result<(), Box<dyn std::error::Error>> {
    let document = load_document(&args.document)?;
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    let mut input = stdin.lock();
    let mut output = stdout.lock();
    let mut errors = stderr.lock();
    let mut player = Ffplay::new(args.player);
    run_editor_session(
        &document,
        &args.document,
        &mut input,
        &mut output,
        &mut errors,
        &mut player,
        args.replay_context_ms.saturating_mul(16),
    )?;
    Ok(())
}

fn run_audition(args: AuditionArgs) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(path) = &args.output {
        validate_output_target(path)?;
    }
    let run = recognize_audio(&args.input, &args.recognition)?;
    if let Some(path) = &args.output {
        let document = Document::from_run_with_source(&run, Some(&args.input));
        save_document(path, &document)?;
        println!("saved {}", path.display());
    }

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
        args.replay_context_ms.saturating_mul(16),
    )?;
    Ok(())
}

fn recognize_audio(
    input: &std::path::Path,
    args: &RecognitionArgs,
) -> Result<running_drafts_editor::recognition::RecognitionRun, Box<dyn std::error::Error>> {
    let wav = read_canonical_wav(input)?;
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
        language: args.language.clone(),
        threads: args.threads,
        top_candidates: args.top_candidates,
        post_chunking: PostChunkConfig {
            minimum_tokens: args.chunk_minimum_tokens,
            target_tokens: args.chunk_target_tokens,
            maximum_tokens: args.chunk_maximum_tokens,
            usable_pause_ms: args.chunk_usable_pause_ms,
            strong_pause_ms: args.chunk_strong_pause_ms,
            long_pause_ms: args.chunk_long_pause_ms,
            distance_penalty_ms: args.chunk_distance_penalty_ms,
        },
        ..RecognitionConfig::default()
    };
    let mut decoder = WhisperDecoder::load(&args.model, &config)?;
    let run = recognize(source, &wav.samples, config, &mut decoder)?;
    Ok(run)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn audition_has_inspectable_whisper_window_defaults() {
        let cli = Cli::try_parse_from([
            "rde",
            "audition",
            "--input",
            "audio.wav",
            "--model",
            "whisper.bin",
        ])
        .unwrap();
        let Command::Audition(args) = cli.command else {
            panic!("expected audition")
        };

        assert_eq!(args.input, PathBuf::from("audio.wav"));
        assert_eq!(args.player, PathBuf::from("ffplay"));
        assert_eq!(args.output, None);
        assert_eq!(args.recognition.model, PathBuf::from("whisper.bin"));
        assert_eq!(args.recognition.language, "auto");
        assert_eq!(args.recognition.threads, 4);
        assert_eq!(args.recognition.target_core_samples, 384_000);
        assert_eq!(args.recognition.left_context_samples, 48_000);
        assert_eq!(args.recognition.right_context_samples, 48_000);
        assert_eq!(args.recognition.top_candidates, 5);
        assert_eq!(args.recognition.chunk_minimum_tokens, 8);
        assert_eq!(args.recognition.chunk_target_tokens, 32);
        assert_eq!(args.recognition.chunk_maximum_tokens, 64);
        assert_eq!(args.recognition.chunk_usable_pause_ms, 300);
        assert_eq!(args.recognition.chunk_strong_pause_ms, 800);
        assert_eq!(args.recognition.chunk_long_pause_ms, 2_000);
        assert_eq!(args.recognition.chunk_distance_penalty_ms, 20);
        assert_eq!(args.replay_context_ms, 750);
    }

    #[test]
    fn transcribe_requires_explicit_input_model_and_output() {
        let cli = Cli::try_parse_from([
            "rde",
            "transcribe",
            "audio.wav",
            "--model",
            "whisper.bin",
            "--output",
            "draft.rde.json",
        ])
        .unwrap();
        let Command::Transcribe(args) = cli.command else {
            panic!("expected transcribe");
        };
        assert_eq!(args.input, PathBuf::from("audio.wav"));
        assert_eq!(args.output, PathBuf::from("draft.rde.json"));
        assert_eq!(args.recognition.model, PathBuf::from("whisper.bin"));
        assert!(Cli::try_parse_from(["rde", "transcribe", "audio.wav"]).is_err());
    }

    #[test]
    fn transcribe_output_is_checked_before_recognition() {
        let directory = tempfile::tempdir().unwrap();
        let missing_parent = directory.path().join("missing/draft.rde.json");
        assert!(validate_output_target(&missing_parent)
            .unwrap_err()
            .to_string()
            .contains("does not exist"));
        assert!(validate_output_target(directory.path())
            .unwrap_err()
            .to_string()
            .contains("is a directory"));
        assert!(validate_output_target(&directory.path().join("draft.rde.json")).is_ok());
        assert!(validate_output_target(std::path::Path::new("test.json")).is_ok());
    }

    #[test]
    fn audition_accepts_a_document_output_path() {
        let cli = Cli::try_parse_from([
            "rde",
            "audition",
            "--input",
            "audio.wav",
            "--model",
            "whisper.bin",
            "--output",
            "draft.rde.json",
        ])
        .unwrap();
        let Command::Audition(args) = cli.command else {
            panic!("expected audition");
        };
        assert_eq!(args.output, Some(PathBuf::from("draft.rde.json")));
    }

    #[test]
    fn edit_opens_a_document_without_recognition_arguments() {
        let cli = Cli::try_parse_from(["rde", "edit", "draft.rde.json"]).unwrap();
        let Command::Edit(args) = cli.command else {
            panic!("expected edit");
        };
        assert_eq!(args.document, PathBuf::from("draft.rde.json"));
        assert_eq!(args.player, PathBuf::from("ffplay"));
        assert_eq!(args.replay_context_ms, 750);
    }

    #[test]
    fn top_level_help_points_to_the_runnable_command() {
        let help = Cli::command().render_long_help().to_string();

        assert!(help.contains(
            "rde transcribe recording.wav --model ggml-tiny.bin --output draft.rde.json"
        ));
        assert!(help.contains("rde audition --input recording.wav --model ggml-tiny.bin"));
        assert!(help.contains("rde edit draft.rde.json"));
        assert!(help.contains("Run a command with '--help'"));
    }
}
