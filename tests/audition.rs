use std::{
    io::Cursor,
    path::{Path, PathBuf},
};

use running_drafts_editor::{
    audition::{run_session, AudioPlayer, Ffplay, PlaybackError},
    chunking::{RecognitionPlan, SampleRange},
};

fn fixture_plan() -> RecognitionPlan {
    serde_json::from_str(include_str!("fixtures/fallback_plan.json")).unwrap()
}

#[derive(Default)]
struct FakePlayer {
    calls: Vec<(PathBuf, u32, SampleRange)>,
    fail: bool,
}

impl AudioPlayer for FakePlayer {
    fn play(
        &mut self,
        source: &Path,
        sample_rate_hz: u32,
        range: SampleRange,
    ) -> Result<(), PlaybackError> {
        self.calls
            .push((source.to_path_buf(), sample_rate_hz, range));
        if self.fail {
            Err(PlaybackError::Other("fixture failure".into()))
        } else {
            Ok(())
        }
    }
}

#[test]
fn session_lists_chunks_and_plays_exact_submitted_range() {
    let plan = fixture_plan();
    let mut input = Cursor::new(b"1play\nlist\nhelp\nquit\n");
    let mut output = Vec::new();
    let mut errors = Vec::new();
    let mut player = FakePlayer::default();

    run_session(
        &plan,
        Path::new("recording.wav"),
        &mut input,
        &mut output,
        &mut errors,
        &mut player,
    )
    .unwrap();

    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("Planned 2 chunks from recording.wav"));
    assert!(output.contains("00:00:00.000 – 00:00:00.037"));
    assert!(output.contains("0.037 s"));
    assert!(output.contains("hard limit (detector unavailable)"));
    assert!(output.contains("source end"));
    assert_eq!(
        player.calls,
        vec![(
            PathBuf::from("recording.wav"),
            16_000,
            SampleRange {
                start_sample: 0,
                end_sample: 600,
            },
        )]
    );
    assert!(errors.is_empty());
}

#[test]
fn session_reports_errors_and_remains_usable_until_eof() {
    let plan = fixture_plan();
    let mut input = Cursor::new(b"9play\nwat\n2play\n");
    let mut output = Vec::new();
    let mut errors = Vec::new();
    let mut player = FakePlayer {
        fail: true,
        ..FakePlayer::default()
    };

    run_session(
        &plan,
        Path::new("recording.wav"),
        &mut input,
        &mut output,
        &mut errors,
        &mut player,
    )
    .unwrap();

    let errors = String::from_utf8(errors).unwrap();
    assert!(errors.contains("invalid chunk number 9; expected 1..=2"));
    assert!(errors.contains("unknown command 'wat'"));
    assert!(errors.contains("playback failed for chunk 2: fixture failure"));
    assert_eq!(player.calls.len(), 1);
}

#[test]
fn empty_plan_lists_and_exits_without_prompt() {
    let mut plan = fixture_plan();
    plan.chunks.clear();
    plan.source.decoded_sample_count = 0;
    let mut input = Cursor::new(Vec::<u8>::new());
    let mut output = Vec::new();
    let mut errors = Vec::new();
    let mut player = FakePlayer::default();

    run_session(
        &plan,
        Path::new("empty.wav"),
        &mut input,
        &mut output,
        &mut errors,
        &mut player,
    )
    .unwrap();

    assert_eq!(
        String::from_utf8(output).unwrap(),
        "Planned 0 chunks from empty.wav\n"
    );
    assert!(player.calls.is_empty());
}

#[cfg(unix)]
#[test]
fn ffplay_backend_passes_precise_times_to_fake_executable() {
    use std::{fs, os::unix::fs::PermissionsExt};

    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join("fake-ffplay");
    let arguments = directory.path().join("arguments");
    fs::write(
        &executable,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n",
            arguments.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();

    let mut player = Ffplay::new(&executable);
    player
        .play(
            Path::new("recording.wav"),
            16_000,
            SampleRange {
                start_sample: 1,
                end_sample: 480_002,
            },
        )
        .unwrap();

    let arguments = fs::read_to_string(arguments).unwrap();
    assert!(arguments.contains("-ss\n0.000062500\n"));
    assert!(arguments.contains("-t\n30.000062500\n"));
    assert!(arguments.ends_with("-i\nrecording.wav\n"));
}
