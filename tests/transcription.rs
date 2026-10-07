//! Initial transcription, decode-span evidence, and chunk presentation.

use std::{
    collections::VecDeque,
    io::{self, Cursor},
    path::{Path, PathBuf},
};

use running_drafts_editor::{
    chunking::{SampleRange, SourceFacts},
    persistence::load_project,
    project::Project,
    session::{run_session, AudioPlayer, PlaybackError, SessionContext},
    transcription::{
        transcribe_initial, AdvanceReason, ChunkBoundaryReason, ChunkConstructionConfig,
        DecodeSpanDecoder, DecodeSpanItem, DecodeSpanSegment, DecoderTimestamps,
        TranscriberIdentity, TranscriptionConfig, TranscriptionStatus, WhisperToken,
    },
};

#[allow(clippy::too_many_arguments)]
fn open_audio(
    run: &running_drafts_editor::transcription::InitialTranscriptionResult,
    source: &Path,
    project_path: Option<&Path>,
    input: &mut impl io::BufRead,
    output: &mut impl io::Write,
    errors: &mut impl io::Write,
    player: &mut FakePlayer,
    replay_context_samples: u64,
) -> io::Result<()> {
    let document = Project::from_initial_transcription_with_source(run, Some(source));
    run_session(
        &document,
        SessionContext::transcribed_audio(run, source, project_path, None),
        input,
        output,
        errors,
        player,
        replay_context_samples,
    )
}

#[derive(Default)]
struct FakeDecoder {
    calls: Vec<(usize, Vec<i32>)>,
    results: VecDeque<Result<Vec<DecodeSpanSegment>, String>>,
    prompt_capacity: Option<usize>,
}

impl DecodeSpanDecoder for FakeDecoder {
    fn identity(&self) -> TranscriberIdentity {
        TranscriberIdentity {
            name: "fake".into(),
            implementation: "test".into(),
            model_sha256: "00".repeat(32),
        }
    }

    fn prompt_capacity(&self) -> usize {
        self.prompt_capacity.unwrap_or(223)
    }

    fn decode(
        &mut self,
        audio: &[f32],
        prompt_token_ids: &[i32],
    ) -> Result<Vec<DecodeSpanSegment>, String> {
        self.calls.push((audio.len(), prompt_token_ids.to_vec()));
        self.results.pop_front().unwrap_or_else(|| Ok(Vec::new()))
    }
}

fn segment(start: u64, end: u64, text: &str) -> DecodeSpanSegment {
    segment_with_tokens(start, end, text, &[])
}

fn segment_with_raw_timestamps(
    raw_timestamps: Option<DecoderTimestamps>,
    text: &str,
) -> DecodeSpanSegment {
    DecodeSpanSegment {
        raw_timestamps,
        text: text.into(),
        no_speech_probability: 0.1,
        tokens: Vec::new(),
    }
}

fn segment_with_tokens(start: u64, end: u64, text: &str, token_ids: &[i32]) -> DecodeSpanSegment {
    DecodeSpanSegment {
        raw_timestamps: Some(DecoderTimestamps {
            start: i64::try_from(start).unwrap(),
            end: i64::try_from(end).unwrap(),
            samples_per_unit: 1,
        }),
        text: text.into(),
        no_speech_probability: 0.1,
        tokens: token_ids
            .iter()
            .enumerate()
            .map(|(index, token_id)| WhisperToken {
                token_id: *token_id,
                text: if index == 0 {
                    text.into()
                } else {
                    String::new()
                },
                probability: 0.9,
                is_special: false,
                raw_timestamps: Some(DecoderTimestamps {
                    start: 0,
                    end: 0,
                    samples_per_unit: 1,
                }),
                audio_range: None,
                alternatives: Vec::new(),
            })
            .collect(),
    }
}

fn segment_with_token_kinds(
    start: u64,
    end: u64,
    text: &str,
    tokens: &[(i32, bool)],
) -> DecodeSpanSegment {
    DecodeSpanSegment {
        raw_timestamps: Some(DecoderTimestamps {
            start: i64::try_from(start).unwrap(),
            end: i64::try_from(end).unwrap(),
            samples_per_unit: 1,
        }),
        text: text.into(),
        no_speech_probability: 0.1,
        tokens: tokens
            .iter()
            .map(|(token_id, is_special)| WhisperToken {
                token_id: *token_id,
                text: format!("token-{token_id}"),
                probability: 0.9,
                is_special: *is_special,
                raw_timestamps: Some(DecoderTimestamps {
                    start: 0,
                    end: 0,
                    samples_per_unit: 1,
                }),
                audio_range: None,
                alternatives: Vec::new(),
            })
            .collect(),
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

fn token_ids(count: usize, first: i32) -> Vec<i32> {
    (0..count)
        .map(|offset| first + i32::try_from(offset).unwrap())
        .collect()
}

fn transcribe_initial_chunks(
    segments: Vec<DecodeSpanSegment>,
    total: u64,
    chunking: ChunkConstructionConfig,
) -> running_drafts_editor::transcription::InitialTranscriptionResult {
    let mut decoder = FakeDecoder {
        results: VecDeque::from([Ok(segments)]),
        ..FakeDecoder::default()
    };
    transcribe_initial(
        source(total),
        &vec![0.0; usize::try_from(total).unwrap()],
        TranscriptionConfig {
            decode_span_samples: total,
            continuation_search_samples: 0,
            chunking,
            ..small_config()
        },
        &mut decoder,
    )
    .unwrap()
}

fn small_config() -> TranscriptionConfig {
    TranscriptionConfig {
        decode_span_samples: 30,
        continuation_search_samples: 3,
        continuation_strong_pause_ms: 800,
        language: "de".into(),
        threads: 1,
        top_candidates: 2,
        chunking: ChunkConstructionConfig::default(),
    }
}

#[test]
fn continuation_boundaries_drive_overlapping_decode_spans_and_exact_prompts() {
    let mut decoder = FakeDecoder {
        results: VecDeque::from([
            Ok(vec![
                segment(0, 10, "A"),
                segment(10, 23, "B"),
                segment_with_token_kinds(
                    23,
                    27,
                    "tail",
                    &[(50_364, true), (30, false), (31, false), (50_464, true)],
                ),
            ]),
            Ok(vec![
                segment(0, 3, "old-B"),
                segment(3, 20, "C"),
                segment_with_token_kinds(
                    20,
                    29,
                    "D",
                    &[
                        (50_364, true),
                        (40, false),
                        (41, false),
                        (42, false),
                        (50_464, true),
                    ],
                ),
            ]),
            Ok(vec![segment(0, 3, "old-D"), segment(3, 14, "E")]),
        ]),
        ..FakeDecoder::default()
    };

    let run = transcribe_initial(source(70), &[0.0; 70], small_config(), &mut decoder).unwrap();

    assert_eq!(run.status, TranscriptionStatus::Succeeded);
    assert_eq!(
        decoder.calls,
        vec![
            (30, vec![]),
            (30, vec![30, 31]),
            (14, vec![30, 31, 40, 41, 42])
        ]
    );
    assert_eq!(
        run.decode_spans
            .iter()
            .map(|span| span.submitted)
            .collect::<Vec<_>>(),
        vec![
            SampleRange {
                start_sample: 0,
                end_sample: 30,
            },
            SampleRange {
                start_sample: 27,
                end_sample: 57,
            },
            SampleRange {
                start_sample: 56,
                end_sample: 70,
            },
        ]
    );
    assert_eq!(
        run.decode_spans
            .iter()
            .map(|span| span.continuation_boundary)
            .collect::<Vec<_>>(),
        vec![27, 56, 70]
    );
    assert_eq!(
        run.decode_spans
            .iter()
            .map(|span| span.advance_reason)
            .collect::<Vec<_>>(),
        vec![
            AdvanceReason::LatestTimestamp,
            AdvanceReason::LatestTimestamp,
            AdvanceReason::SourceEnd,
        ]
    );
    assert_eq!(
        run.accepted_segments()
            .map(|segment| segment.text.as_str())
            .collect::<Vec<_>>(),
        vec!["A", "B", "tail", "old-B", "C", "D", "old-D", "E"]
    );
    assert_eq!(run.decode_spans[0].prompt_token_ids, Vec::<i32>::new());
    assert_eq!(run.decode_spans[1].prompt_token_ids, vec![30, 31]);
    assert_eq!(
        run.decode_spans[2].prompt_token_ids,
        vec![30, 31, 40, 41, 42]
    );
    let tail = run
        .accepted_segments()
        .find(|segment| segment.text == "tail")
        .unwrap();
    assert_eq!(
        tail.tokens
            .iter()
            .map(|token| (token.token_id, token.is_special))
            .collect::<Vec<_>>(),
        vec![(50_364, true), (30, false), (31, false), (50_464, true)]
    );
    assert!(run
        .decode_spans
        .iter()
        .all(|span| span.submitted.len() <= 30));
    let chunks = run.chunks().collect::<Vec<_>>();
    assert_eq!(
        chunks
            .iter()
            .map(|chunk| chunk.audio_range)
            .collect::<Vec<_>>(),
        vec![
            SampleRange {
                start_sample: 0,
                end_sample: 27,
            },
            SampleRange {
                start_sample: 27,
                end_sample: 56,
            },
            SampleRange {
                start_sample: 56,
                end_sample: 70,
            },
        ]
    );
    assert_eq!(
        chunks
            .iter()
            .map(|chunk| chunk.boundary.reason)
            .collect::<Vec<_>>(),
        vec![
            ChunkBoundaryReason::Continuation,
            ChunkBoundaryReason::Continuation,
            ChunkBoundaryReason::SourceEnd,
        ]
    );
    assert!(chunks
        .windows(2)
        .all(|pair| { pair[0].audio_range.end_sample <= pair[1].audio_range.start_sample }));
    assert!(chunks
        .iter()
        .all(|chunk| { chunk.current_transcription().is_some() }));
}

#[test]
fn latest_timestamp_in_search_area_wins_and_trailing_silence_uses_last_segment_end() {
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

    let run = transcribe_initial(source(50), &[0.0; 50], small_config(), &mut decoder).unwrap();

    assert_eq!(
        run.decode_spans
            .iter()
            .map(|span| span.continuation_boundary)
            .collect::<Vec<_>>(),
        vec![26, 50]
    );
    assert_eq!(
        run.decode_spans[0].advance_reason,
        AdvanceReason::EarlyTimestampFallback
    );

    let mut early_only = FakeDecoder {
        results: VecDeque::from([Ok(vec![segment(0, 2, "early")]), Ok(Vec::new())]),
        ..FakeDecoder::default()
    };
    let run = transcribe_initial(source(50), &[0.0; 50], small_config(), &mut early_only).unwrap();

    assert_eq!(run.decode_spans[0].continuation_boundary, 2);
    assert_eq!(
        run.decode_spans[0].advance_reason,
        AdvanceReason::EarlyTimestampFallback
    );
}

#[test]
fn continuation_prefers_the_latest_strong_pause_over_later_ordinary_timestamps() {
    let mut decoder = FakeDecoder {
        results: VecDeque::from([
            Ok(vec![
                segment(0, 10_000, "a"),
                segment(30_000, 40_000, "b"),
                segment_with_tokens(40_000, 50_000, "c", &[7]),
                segment(65_000, 70_000, "d"),
                segment(70_000, 99_000, "later"),
            ]),
            Ok(Vec::new()),
            Ok(Vec::new()),
        ]),
        ..FakeDecoder::default()
    };
    let config = TranscriptionConfig {
        decode_span_samples: 100_000,
        continuation_search_samples: 96_000,
        continuation_strong_pause_ms: 800,
        ..small_config()
    };

    let run = transcribe_initial(source(180_000), &[0.0; 180_000], config, &mut decoder).unwrap();

    assert_eq!(run.decode_spans[0].continuation_boundary, 50_000);
    assert_eq!(
        run.decode_spans[0].advance_reason,
        AdvanceReason::StrongPause
    );
    assert_eq!(run.decode_spans[0].continuation_pause_samples, Some(15_000));
    assert_eq!(run.decode_spans[0].accepted_segment_ids.len(), 3);
    assert_eq!(decoder.calls[1].1, vec![7]);
}

#[test]
fn timestamp_defects_stop_the_prefix_but_preserve_all_raw_evidence() {
    let later = DecoderTimestamps {
        start: 20,
        end: 25,
        samples_per_unit: 1,
    };
    let defects = [
        (None, None),
        (
            Some(DecoderTimestamps {
                start: 20,
                end: 15,
                samples_per_unit: 1,
            }),
            None,
        ),
        (
            Some(DecoderTimestamps {
                start: 10,
                end: 10,
                samples_per_unit: 1,
            }),
            None,
        ),
        (
            Some(DecoderTimestamps {
                start: 5,
                end: 12,
                samples_per_unit: 1,
            }),
            Some(SampleRange {
                start_sample: 5,
                end_sample: 12,
            }),
        ),
        (
            Some(DecoderTimestamps {
                start: 20,
                end: 31,
                samples_per_unit: 1,
            }),
            None,
        ),
    ];
    for (defect, expected_range) in defects {
        let mut decoder = FakeDecoder {
            results: VecDeque::from([Ok(vec![
                segment(0, 10, "accepted"),
                segment_with_raw_timestamps(defect, "broken"),
                segment_with_raw_timestamps(Some(later), "must-not-resume"),
            ])]),
            ..FakeDecoder::default()
        };

        let run = transcribe_initial(source(30), &[0.0; 30], small_config(), &mut decoder).unwrap();

        assert_eq!(run.status, TranscriptionStatus::Partial);
        assert_eq!(run.decode_spans[0].accepted_segment_ids.len(), 1);
        assert_eq!(run.decode_spans[0].hypotheses.len(), 3);
        assert_eq!(run.decode_spans[0].hypotheses[1].raw_timestamps, defect);
        assert_eq!(
            run.decode_spans[0].hypotheses[1].audio_range,
            expected_range
        );
        assert_eq!(
            run.decode_spans[0].hypotheses[2].audio_range,
            Some(SampleRange {
                start_sample: 20,
                end_sample: 25,
            })
        );
    }
}

#[test]
fn rolling_prompt_is_truncated_oldest_first_and_resets_after_failure() {
    let mut decoder = FakeDecoder {
        results: VecDeque::from([
            Ok(vec![segment_with_tokens(0, 27, "a", &[1, 2])]),
            Ok(vec![segment_with_tokens(0, 29, "b", &[3, 4])]),
            Err("gap".into()),
            Ok(vec![segment_with_tokens(0, 14, "c", &[5])]),
        ]),
        prompt_capacity: Some(3),
        ..FakeDecoder::default()
    };

    let run = transcribe_initial(source(100), &[0.0; 100], small_config(), &mut decoder).unwrap();

    assert_eq!(
        decoder.calls,
        vec![
            (30, vec![]),
            (30, vec![1, 2]),
            (30, vec![2, 3, 4]),
            (14, vec![]),
        ]
    );
    assert_eq!(run.decode_spans[2].prompt_omitted_token_count, 1);
    assert_eq!(
        run.decode_spans[3].empty_prompt_reason,
        Some(running_drafts_editor::transcription::EmptyPromptReason::ResetAfterDecodeFailure)
    );
    assert_eq!(run.status, TranscriptionStatus::Partial);
    assert!(run.chunks().all(|chunk| chunk.transcription.iter().all(|transcription| {
        transcription.forced_token_ids.is_empty()
            && transcription.prompt_token_ids
                == run.decode_spans
                    .iter()
                    .find(|span| span.content.iter().any(|item| matches!(item, DecodeSpanItem::Chunk(candidate) if candidate.id == chunk.id)))
                    .unwrap()
                    .prompt_token_ids
    })));
}

#[test]
fn complete_fallback_without_text_resets_prompt_and_empty_text_tokens_remain_evidence() {
    let mut decoder = FakeDecoder {
        results: VecDeque::from([
            Ok(vec![segment_with_tokens(0, 27, "", &[9])]),
            Ok(Vec::new()),
            Ok(Vec::new()),
        ]),
        ..FakeDecoder::default()
    };

    let run = transcribe_initial(source(80), &[0.0; 80], small_config(), &mut decoder).unwrap();

    assert_eq!(decoder.calls[1].1, vec![9]);
    assert!(decoder.calls[2].1.is_empty());
    assert_eq!(
        run.decode_spans[2].empty_prompt_reason,
        Some(running_drafts_editor::transcription::EmptyPromptReason::ResetAfterUnknownGap)
    );
    assert_eq!(run.chunks().next().unwrap().token_count, 1);
}

#[test]
fn decode_failures_still_advance_by_complete_bounded_decode_spans() {
    let mut decoder = FakeDecoder {
        results: VecDeque::from([Err("one".into()), Err("two".into()), Err("three".into())]),
        ..FakeDecoder::default()
    };

    let run = transcribe_initial(source(55), &[0.0; 55], small_config(), &mut decoder).unwrap();

    assert_eq!(run.status, TranscriptionStatus::Failed);
    assert_eq!(run.accepted_segments().count(), 0);
    assert_eq!(
        run.decode_spans
            .iter()
            .map(|span| span.submitted)
            .collect::<Vec<_>>(),
        vec![
            SampleRange {
                start_sample: 0,
                end_sample: 30,
            },
            SampleRange {
                start_sample: 30,
                end_sample: 55,
            },
        ]
    );
    assert!(run.decode_spans.iter().all(|span| {
        span.advance_reason == AdvanceReason::DecodeFailure && span.error.is_some()
    }));
}

#[test]
fn silence_only_span_advances_without_a_chunk_and_later_speech_is_finalized() {
    let mut decoder = FakeDecoder {
        results: VecDeque::from([
            Ok(Vec::new()),
            Ok(vec![segment_with_tokens(5, 10, "speech", &[10])]),
        ]),
        ..FakeDecoder::default()
    };

    let run = transcribe_initial(source(55), &[0.0; 55], small_config(), &mut decoder).unwrap();

    assert_eq!(run.decode_spans.len(), 2);
    assert_eq!(
        run.decode_spans[0].advance_reason,
        AdvanceReason::NoUsableTimestamp
    );
    assert!(run.decode_spans[0].content.is_empty());
    assert_eq!(run.decode_spans[1].submitted.start_sample, 30);
    let chunks = run.chunks().collect::<Vec<_>>();
    assert_eq!(chunks.len(), 1);
    assert_eq!(
        chunks[0].audio_range,
        SampleRange {
            start_sample: 35,
            end_sample: 40,
        }
    );
    assert_eq!(chunks[0].boundary.reason, ChunkBoundaryReason::SourceEnd);
}

#[test]
fn long_pause_discovered_by_the_next_decode_span_adds_a_paragraph_break() {
    let mut decoder = FakeDecoder {
        results: VecDeque::from([
            Ok(vec![segment_with_tokens(0, 40_000, "before", &[10])]),
            Ok(vec![segment_with_tokens(32_000, 40_000, "after", &[20])]),
        ]),
        ..FakeDecoder::default()
    };
    let config = TranscriptionConfig {
        decode_span_samples: 48_000,
        continuation_search_samples: 8_000,
        ..small_config()
    };

    let run = transcribe_initial(source(88_000), &[0.0; 88_000], config, &mut decoder).unwrap();

    assert!(matches!(
        run.decode_spans[0].content.as_slice(),
        [DecodeSpanItem::Chunk(_), DecodeSpanItem::ParagraphBreak(_)]
    ));
    assert!(matches!(
        run.decode_spans[1].content.as_slice(),
        [DecodeSpanItem::Chunk(_)]
    ));
    let project = Project::from_initial_transcription(&run);
    assert_eq!(project.paragraphs().len(), 2);
    assert_eq!(project.paragraph(1).unwrap().text(), "before");
    assert_eq!(project.paragraph(2).unwrap().text(), "after");
}

#[test]
fn long_pause_splits_without_minimum_tokens_and_strong_pause_respects_minimum() {
    let one = token_ids(1, 10);
    let four_a = token_ids(4, 20);
    let four_b = token_ids(4, 30);
    let run = transcribe_initial_chunks(
        vec![
            segment_with_tokens(0, 16_000, "one", &one),
            segment_with_tokens(48_000, 64_000, "four-a", &four_a),
            segment_with_tokens(76_800, 92_800, "four-b", &four_b),
            segment_with_tokens(105_600, 121_600, "tail", &one),
        ],
        121_600,
        ChunkConstructionConfig::default(),
    );

    let chunks = run.chunks().collect::<Vec<_>>();
    assert_eq!(chunks.len(), 3);
    assert_eq!(chunks[0].token_count, 1);
    assert_eq!(chunks[0].boundary.reason, ChunkBoundaryReason::LongPause);
    assert_eq!(chunks[1].token_count, 8);
    assert_eq!(chunks[1].boundary.reason, ChunkBoundaryReason::StrongPause);
    assert_eq!(chunks[2].boundary.reason, ChunkBoundaryReason::SourceEnd);
    assert!(matches!(
        run.decode_spans[0].content.as_slice(),
        [
            DecodeSpanItem::Chunk(_),
            DecodeSpanItem::ParagraphBreak(_),
            DecodeSpanItem::Chunk(_),
            DecodeSpanItem::Chunk(_)
        ]
    ));
}

#[test]
fn usable_pauses_are_scored_near_target_and_maximum_uses_whole_segment_boundary() {
    let twenty = token_ids(20, 100);
    let ten = token_ids(10, 200);
    let two = token_ids(2, 300);
    let run = transcribe_initial_chunks(
        vec![
            segment_with_tokens(0, 16_000, "a", &twenty),
            segment_with_tokens(24_000, 40_000, "b", &ten),
            segment_with_tokens(46_400, 62_400, "c", &two),
            segment_with_tokens(62_400, 78_400, "d", &two),
        ],
        78_400,
        ChunkConstructionConfig::default(),
    );

    let chunks = run.chunks().collect::<Vec<_>>();
    assert_eq!(chunks[0].segment_ids.len(), 2);
    assert_eq!(chunks[0].token_count, 30);
    assert_eq!(chunks[0].boundary.reason, ChunkBoundaryReason::ScoredPause);

    let twenty_a = token_ids(20, 400);
    let twenty_b = token_ids(20, 500);
    let twenty_c = token_ids(20, 600);
    let twenty_d = token_ids(20, 700);
    let run = transcribe_initial_chunks(
        vec![
            segment_with_tokens(0, 16_000, "a", &twenty_a),
            segment_with_tokens(16_000, 32_000, "b", &twenty_b),
            segment_with_tokens(32_000, 48_000, "c", &twenty_c),
            segment_with_tokens(48_000, 64_000, "d", &twenty_d),
        ],
        64_000,
        ChunkConstructionConfig::default(),
    );

    let chunks = run.chunks().collect::<Vec<_>>();
    assert_eq!(chunks[0].token_count, 40);
    assert_eq!(
        chunks[0].boundary.reason,
        ChunkBoundaryReason::MaximumTokens
    );
    assert_eq!(chunks[0].segment_ids.len(), 2);
}

#[test]
fn chunk_settings_must_be_ordered() {
    let mut config = small_config();
    config.chunking.minimum_tokens = 33;
    config.chunking.target_tokens = 32;
    let mut decoder = FakeDecoder::default();

    let error = transcribe_initial(source(1), &[0.0], config, &mut decoder).unwrap_err();

    assert!(error.to_string().contains("token limits"));
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
fn decoded_open_audio_shows_text_and_replays_exact_timestamp_range() {
    let mut decoder = FakeDecoder {
        results: VecDeque::from([Ok(vec![segment_with_tokens(
            160,
            480,
            " decoded words",
            &[1],
        )])]),
        ..FakeDecoder::default()
    };
    let run = transcribe_initial(
        source(640),
        &[0.0; 640],
        TranscriptionConfig {
            decode_span_samples: 640,
            continuation_search_samples: 0,
            ..small_config()
        },
        &mut decoder,
    )
    .unwrap();
    let mut input = Cursor::new(b"1.1info\n1.1play\nquit\n");
    let mut output = Vec::new();
    let mut errors = Vec::new();
    let mut player = FakePlayer::default();
    let audio = tempfile::NamedTempFile::new().unwrap();

    open_audio(
        &run,
        audio.path(),
        None,
        &mut input,
        &mut output,
        &mut errors,
        &mut player,
        12_000,
    )
    .unwrap();

    let output = String::from_utf8(output).unwrap();
    assert!(output.contains(&format!("Built 1 chunks from {}", audio.path().display())));
    assert!(output.contains("source end"));
    assert!(output.contains("decoded words"));
    assert!(output.contains("⟦1.1⟧"));
    assert!(output.contains("1.1  00:00:00.010 – 00:00:00.030"));
    assert_eq!(
        player.calls,
        vec![(
            audio.path().to_path_buf(),
            16_000,
            SampleRange {
                start_sample: 160,
                end_sample: 480,
            },
        )]
    );
    assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
}

#[test]
fn open_audio_retains_text_without_inventing_addressable_tokens() {
    let run = transcribe_initial_chunks(
        vec![segment_with_token_kinds(
            0,
            16_000,
            "visible text",
            &[(1, false)],
        )],
        16_000,
        ChunkConstructionConfig::default(),
    );
    let mut input = Cursor::new(b"1tokens\n1.1.1select\n1.1,1.2select\nquit\n");
    let mut output = Vec::new();
    let mut errors = Vec::new();
    let mut player = FakePlayer::default();

    open_audio(
        &run,
        Path::new("audio.wav"),
        None,
        &mut input,
        &mut output,
        &mut errors,
        &mut player,
        12_000,
    )
    .unwrap();

    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("1.1  chunk  no tokens"));
    assert!(output.contains("visible text"));
    assert!(output.contains("selected 1.1,1.2"));
    assert_eq!(
        String::from_utf8(errors).unwrap(),
        "token alignment unavailable for chunk 1.1: normal transcription tokens do not reproduce the chunk text; preserving transcription text without token positions\nchunk 1.1 has no token positions\n"
    );
}

#[test]
fn open_audio_output_becomes_the_default_session_save_path() {
    let run = transcribe_initial_chunks(
        vec![segment_with_tokens(0, 16_000, "visible text", &[1])],
        16_000,
        ChunkConstructionConfig::default(),
    );
    let directory = tempfile::tempdir().unwrap();
    let project_path = directory.path().join("draft.rde.json");
    let mut input = Cursor::new(b"save\nquit\n");
    let mut output = Vec::new();
    let mut errors = Vec::new();
    let mut player = FakePlayer::default();

    open_audio(
        &run,
        Path::new("audio.wav"),
        Some(&project_path),
        &mut input,
        &mut output,
        &mut errors,
        &mut player,
        12_000,
    )
    .unwrap();

    let saved = load_project(&project_path).unwrap();
    assert_eq!(
        saved.paragraph(1).unwrap().tokens()[0].text(),
        "visible text"
    );
    assert!(String::from_utf8(output)
        .unwrap()
        .contains(&format!("saved {}", project_path.display())));
    assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
}

#[test]
fn open_audio_groups_long_pauses_into_paragraphs_and_reports_marker_errors() {
    let one = token_ids(1, 10);
    let four_a = token_ids(4, 20);
    let four_b = token_ids(4, 30);
    let run = transcribe_initial_chunks(
        vec![
            segment_with_tokens(0, 16_000, "one", &one),
            segment_with_tokens(48_000, 64_000, "four-a", &four_a),
            segment_with_tokens(76_800, 92_800, "four-b", &four_b),
            segment_with_tokens(105_600, 121_600, "tail", &one),
        ],
        121_600,
        ChunkConstructionConfig::default(),
    );
    let mut input = Cursor::new(
        b"2p\n2.1.1\n2p\n1.1.1,2.1.1select\np\n2.1\n2p\n2.1select\n2p\n2select\n2p\n2.1,2.2select\n2p\nplay\n2.2play\n2tokens\n3p\n1.1info\n2.2info\n3.1info\nquit\n",
    );
    let mut output = Vec::new();
    let mut errors = Vec::new();
    let mut player = FakePlayer::default();
    let audio = tempfile::NamedTempFile::new().unwrap();

    open_audio(
        &run,
        audio.path(),
        None,
        &mut input,
        &mut output,
        &mut errors,
        &mut player,
        12_000,
    )
    .unwrap();

    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("⟦1.1⟧one\n\n⟦2.1⟧four-afour-b ⟦2.2⟧tail"));
    assert!(output.contains("position 2.1.1"));
    assert!(output.contains("selected 1.1.1,2.1.1"));
    assert!(output.contains("position 2.1"));
    assert!(output.contains("selected 2.1"));
    assert!(output.contains("selected 2"));
    assert!(output.contains("selected 2.1,2.2"));
    assert!(output.contains("2.1.1  0.900"));
    assert!(output.contains("2.1  chunk  has_tokens"));
    assert!(output.contains("1.1  00:00:00.000 – 00:00:01.000"));
    assert!(output.contains("long pause (2.000 s)"));
    assert!(output.contains("2.2  00:00:06.600 – 00:00:07.600"));
    assert!(output.contains("source end"));
    assert_eq!(
        String::from_utf8(errors).unwrap(),
        "unknown paragraph 3\nunknown chunk 3.1\n"
    );
    assert_eq!(
        player.calls,
        vec![
            (
                audio.path().to_path_buf(),
                16_000,
                SampleRange {
                    start_sample: 48_000,
                    end_sample: 92_800
                }
            ),
            (
                audio.path().to_path_buf(),
                16_000,
                SampleRange {
                    start_sample: 105_600,
                    end_sample: 121_600
                }
            ),
        ]
    );
}
