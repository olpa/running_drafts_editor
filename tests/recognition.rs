use std::{
    collections::VecDeque,
    io::Cursor,
    path::{Path, PathBuf},
};

use running_drafts_editor::{
    audition::{run_recognition_session, AudioPlayer, PlaybackError},
    chunking::{SampleRange, SourceFacts},
    recognition::{
        recognize, AdvanceReason, RecognitionConfig, RecognitionStatus, RecognizerIdentity,
        WindowDecoder, WindowSegment,
    },
};

#[derive(Default)]
struct FakeDecoder {
    calls: Vec<(usize, String)>,
    results: VecDeque<Result<Vec<WindowSegment>, String>>,
}

impl WindowDecoder for FakeDecoder {
    fn identity(&self) -> RecognizerIdentity {
        RecognizerIdentity {
            name: "fake".into(),
            implementation: "test".into(),
            model_sha256: "00".repeat(32),
        }
    }

    fn decode(&mut self, audio: &[f32], prompt: &str) -> Result<Vec<WindowSegment>, String> {
        self.calls.push((audio.len(), prompt.into()));
        self.results.pop_front().unwrap_or_else(|| Ok(Vec::new()))
    }
}

fn segment(start: u64, end: u64, text: &str) -> WindowSegment {
    WindowSegment {
        audio_range: SampleRange {
            start_sample: start,
            end_sample: end,
        },
        text: text.into(),
        no_speech_probability: 0.1,
        tokens: Vec::new(),
    }
}

fn source(samples: u64) -> SourceFacts {
    SourceFacts {
        sha256: "11".repeat(32),
        sample_rate_hz: 16_000,
        channels: 1,
        decoded_sample_count: samples,
    }
}

fn small_config() -> RecognitionConfig {
    RecognitionConfig {
        max_window_samples: 30,
        target_core_samples: 24,
        left_context_samples: 3,
        max_prompt_chars: 3,
        right_context_samples: 3,
        language: "de".into(),
        threads: 1,
        top_candidates: 2,
    }
}

#[test]
fn timestamps_drive_overlapping_windows_and_prompts_without_duplicate_segments() {
    let mut decoder = FakeDecoder {
        results: VecDeque::from([
            Ok(vec![
                segment(0, 10, "A"),
                segment(10, 23, "B"),
                segment(23, 27, "tail"),
            ]),
            Ok(vec![
                segment(0, 4, "old-B"),
                segment(3, 20, "C"),
                segment(20, 29, "D"),
            ]),
            Ok(vec![segment(0, 3, "old-D"), segment(3, 24, "E")]),
        ]),
        ..FakeDecoder::default()
    };

    let run = recognize(source(70), &[0.0; 70], small_config(), &mut decoder).unwrap();

    assert_eq!(run.status, RecognitionStatus::Succeeded);
    assert_eq!(
        decoder.calls,
        vec![(27, String::new()), (30, "ail".into()), (20, "lCD".into())]
    );
    assert_eq!(
        run.windows
            .iter()
            .map(|window| window.submitted)
            .collect::<Vec<_>>(),
        vec![
            SampleRange {
                start_sample: 0,
                end_sample: 27,
            },
            SampleRange {
                start_sample: 24,
                end_sample: 54,
            },
            SampleRange {
                start_sample: 50,
                end_sample: 70,
            },
        ]
    );
    assert_eq!(
        run.windows
            .iter()
            .map(|window| window.core)
            .collect::<Vec<_>>(),
        vec![
            SampleRange {
                start_sample: 0,
                end_sample: 27,
            },
            SampleRange {
                start_sample: 27,
                end_sample: 53,
            },
            SampleRange {
                start_sample: 53,
                end_sample: 70,
            },
        ]
    );
    assert_eq!(
        run.windows
            .iter()
            .map(|window| window.advance_reason)
            .collect::<Vec<_>>(),
        vec![
            AdvanceReason::WhisperTimestamp,
            AdvanceReason::WhisperTimestamp,
            AdvanceReason::SourceEnd,
        ]
    );
    assert_eq!(
        run.segments
            .iter()
            .map(|segment| segment.text.as_str())
            .collect::<Vec<_>>(),
        vec!["A", "B", "tail", "C", "D", "E"]
    );
    assert!(run
        .windows
        .iter()
        .all(|window| window.submitted.len() <= 30));
}

#[test]
fn latest_timestamp_in_search_area_wins_and_early_timestamp_does_not_shorten_core() {
    let mut decoder = FakeDecoder {
        results: VecDeque::from([
            Ok(vec![
                segment(0, 2, "early"),
                segment(20, 25, "first"),
                segment(25, 26, "latest"),
            ]),
            Ok(Vec::new()),
        ]),
        ..FakeDecoder::default()
    };

    let run = recognize(source(50), &[0.0; 50], small_config(), &mut decoder).unwrap();

    assert_eq!(
        run.windows
            .iter()
            .map(|window| window.core)
            .collect::<Vec<_>>(),
        vec![
            SampleRange {
                start_sample: 0,
                end_sample: 26,
            },
            SampleRange {
                start_sample: 26,
                end_sample: 50,
            },
        ]
    );
    assert_eq!(
        run.windows[0].advance_reason,
        AdvanceReason::WhisperTimestamp
    );

    let mut early_only = FakeDecoder {
        results: VecDeque::from([Ok(vec![segment(0, 2, "early")]), Ok(Vec::new())]),
        ..FakeDecoder::default()
    };
    let run = recognize(source(50), &[0.0; 50], small_config(), &mut early_only).unwrap();

    assert_eq!(run.windows[0].core.end_sample, 24);
    assert_eq!(
        run.windows[0].advance_reason,
        AdvanceReason::FixedNoTimestamp
    );
}

#[test]
fn decode_failures_still_create_complete_bounded_core_coverage() {
    let mut decoder = FakeDecoder {
        results: VecDeque::from([Err("one".into()), Err("two".into()), Err("three".into())]),
        ..FakeDecoder::default()
    };

    let run = recognize(source(55), &[0.0; 55], small_config(), &mut decoder).unwrap();

    assert_eq!(run.status, RecognitionStatus::Failed);
    assert!(run.segments.is_empty());
    assert_eq!(
        run.windows
            .iter()
            .map(|window| window.core)
            .collect::<Vec<_>>(),
        vec![
            SampleRange {
                start_sample: 0,
                end_sample: 24,
            },
            SampleRange {
                start_sample: 24,
                end_sample: 48,
            },
            SampleRange {
                start_sample: 48,
                end_sample: 55,
            },
        ]
    );
    assert!(run.windows.iter().all(|window| {
        window.advance_reason == AdvanceReason::FixedDecodeFailure && window.error.is_some()
    }));
}

#[derive(Default)]
struct FakePlayer {
    calls: Vec<(PathBuf, u32, SampleRange)>,
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
        Ok(())
    }
}

#[test]
fn decoded_audition_shows_text_and_replays_exact_timestamp_range() {
    let mut decoder = FakeDecoder {
        results: VecDeque::from([Ok(vec![segment(160, 480, " decoded words")])]),
        ..FakeDecoder::default()
    };
    let run = recognize(
        source(640),
        &[0.0; 640],
        RecognitionConfig {
            max_window_samples: 640,
            target_core_samples: 640,
            left_context_samples: 0,
            right_context_samples: 0,
            ..small_config()
        },
        &mut decoder,
    )
    .unwrap();
    let mut input = Cursor::new(b"1p\nquit\n");
    let mut output = Vec::new();
    let mut errors = Vec::new();
    let mut player = FakePlayer::default();

    run_recognition_session(
        &run,
        Path::new("audio.wav"),
        &mut input,
        &mut output,
        &mut errors,
        &mut player,
    )
    .unwrap();

    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("Decoded 1 chunks from audio.wav"));
    assert!(output.contains("whisper timestamp"));
    assert!(output.contains("decoded words"));
    assert_eq!(
        player.calls,
        vec![(
            PathBuf::from("audio.wav"),
            16_000,
            SampleRange {
                start_sample: 160,
                end_sample: 480,
            },
        )]
    );
    assert!(errors.is_empty());
}
