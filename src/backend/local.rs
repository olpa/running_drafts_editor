use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::{
    chunking::read_canonical_wav,
    document::{AlignmentState, AudioSource, ChunkAudioMapping},
    transcription::{
        transcribe_initial, ChunkTranscriber, ChunkTranscriptionRequest, TranscriberSession,
        TranscriptionConfig, TranscriptionError, TranscriptionStatus, WhisperDecoder,
    },
};

use super::*;

struct LocalRecording {
    metadata: RecordingMetadata,
    path: Option<PathBuf>,
}

/// IDs are proxies to current local files and canonical ranges, not snapshots.
/// Saved hashes identify sources but are never used to reject changed files.
#[derive(Default)]
pub struct LocalAudioBackend {
    recordings: HashMap<String, LocalRecording>,
    chunks: HashMap<String, ChunkMetadata>,
}

impl LocalAudioBackend {
    pub fn new() -> Self {
        Self::default()
    }

    fn path(&self, id: &str) -> Result<&Path, BackendError> {
        self.recordings
            .get(id)
            .ok_or_else(|| BackendError::UnknownRecording(id.into()))?
            .path
            .as_deref()
            .ok_or_else(|| BackendError::NoLocalPath(id.into()))
    }
}

impl AudioBackend for LocalAudioBackend {
    fn upload(&mut self, path: &Path) -> Result<String, BackendError> {
        let wav = read_canonical_wav(path)?;
        // Match the supported project's source identity; paths are not IDs.
        let id = format!("audio:{}", wav.source_sha256);
        self.recordings.insert(
            id.clone(),
            LocalRecording {
                metadata: RecordingMetadata {
                    id: id.clone(),
                    sha256: Some(wav.source_sha256),
                    canonical_sample_count: Some(wav.samples.len() as u64),
                },
                path: Some(path.into()),
            },
        );
        Ok(id)
    }

    fn recording(&self, id: &str) -> Result<RecordingMetadata, BackendError> {
        self.recordings
            .get(id)
            .map(|r| r.metadata.clone())
            .ok_or_else(|| BackendError::UnknownRecording(id.into()))
    }

    fn chunk(&self, id: &str) -> Result<ChunkMetadata, BackendError> {
        self.chunks
            .get(id)
            .cloned()
            .ok_or_else(|| BackendError::UnknownChunk(id.into()))
    }

    fn read_recording(&mut self, id: &str) -> Result<RecordingAudio, BackendError> {
        let wav = read_canonical_wav(self.path(id)?)?;
        Ok(RecordingAudio {
            source: SourceFacts {
                sha256: wav.source_sha256,
                sample_rate_hz: wav.sample_rate_hz,
                channels: wav.channels,
                decoded_sample_count: wav.samples.len() as u64,
            },
            samples: wav.samples,
        })
    }

    fn read_chunk(&mut self, id: &str) -> Result<ChunkAudio, BackendError> {
        let chunk = self.chunk(id)?;
        if chunk.alignment == AlignmentState::Unavailable {
            return Err(BackendError::Other(
                "chunk audio mapping is unavailable".into(),
            ));
        }
        Ok(ChunkAudio {
            audio: self.read_recording(&chunk.recording_id)?,
            range: chunk.range,
        })
    }

    fn register_chunks(&mut self, chunks: &[ChunkMetadata]) -> Result<(), BackendError> {
        let mut staged = self.chunks.clone();
        for chunk in chunks {
            self.recording(&chunk.recording_id)?;
            if staged
                .get(&chunk.id)
                .is_some_and(|existing| existing != chunk)
            {
                return Err(BackendError::Other(
                    "chunk identity or boundaries changed".into(),
                ));
            }
            staged.insert(chunk.id.clone(), chunk.clone());
        }
        self.chunks = staged;
        Ok(())
    }

    fn restore(&mut self, sources: &[AudioSource], chunks: &[ChunkAudioMapping]) {
        self.recordings = sources
            .iter()
            .map(|source| {
                (
                    source.id().to_owned(),
                    LocalRecording {
                        metadata: RecordingMetadata {
                            id: source.id().into(),
                            sha256: source.sha256().map(str::to_owned),
                            canonical_sample_count: source.canonical_sample_count(),
                        },
                        path: source.path().map(Path::to_path_buf),
                    },
                )
            })
            .collect();
        self.chunks = chunks
            .iter()
            .map(|chunk| {
                (
                    chunk.chunk_id().into(),
                    ChunkMetadata {
                        id: chunk.chunk_id().into(),
                        recording_id: chunk.source_id().into(),
                        range: chunk.range(),
                        alignment: chunk.alignment(),
                    },
                )
            })
            .collect();
    }

    fn availability(&self, id: &str) -> Result<(), BackendError> {
        let path = self.path(id)?;
        if !path.is_file() {
            return Err(BackendError::Unavailable {
                id: id.into(),
                path: path.into(),
            });
        }
        Ok(())
    }

    fn start_replay(
        &mut self,
        request: ReplayRequest<'_>,
        player: &mut dyn AudioPlayer,
    ) -> Result<(), BackendError> {
        for id in request.chunk_ids {
            self.chunk(id)?;
        }
        self.availability(request.recording_id)?;
        player.start(
            self.path(request.recording_id)?,
            16_000,
            request.range,
            request.speed,
        )?;
        Ok(())
    }
}

// Low-level decoder seam stays inside the implementation. Tests can provide a
// deterministic decoder without putting model loading back in session code.
pub(crate) trait TranscriberFactory {
    fn load(
        &mut self,
        model: &Path,
        config: &TranscriptionConfig,
    ) -> Result<Box<dyn ChunkTranscriber>, TranscriptionError>;
    fn initial(
        &mut self,
        model: &Path,
        config: &TranscriptionConfig,
        input: RecordingAudio,
    ) -> Result<(InitialTranscriptionResult, Box<dyn ChunkTranscriber>), BackendError> {
        let mut decoder = WhisperDecoder::load(model, config).map_err(BackendError::Model)?;
        let run = transcribe_initial(input.source, &input.samples, config.clone(), &mut decoder)?;
        Ok((
            run,
            Box::new(TranscriberSession::from_decoder(decoder, model)),
        ))
    }
}
struct WhisperFactory;
impl TranscriberFactory for WhisperFactory {
    fn load(
        &mut self,
        model: &Path,
        config: &TranscriptionConfig,
    ) -> Result<Box<dyn ChunkTranscriber>, TranscriptionError> {
        TranscriberSession::load(model, config).map(|s| Box::new(s) as Box<dyn ChunkTranscriber>)
    }
}

pub struct LocalRecognitionBackend {
    engine: Option<Box<dyn ChunkTranscriber>>,
    settings: Option<TranscriptionSettings>,
    factory: Box<dyn TranscriberFactory>,
}

impl Default for LocalRecognitionBackend {
    fn default() -> Self {
        Self {
            engine: None,
            settings: None,
            factory: Box::new(WhisperFactory),
        }
    }
}
impl LocalRecognitionBackend {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    pub(crate) fn with_factory(factory: Box<dyn TranscriberFactory>) -> Self {
        Self {
            factory,
            ..Self::default()
        }
    }
}

impl RecognitionBackend for LocalRecognitionBackend {
    fn transcribe_recording(
        &mut self,
        audio: &mut dyn AudioBackend,
        recording_id: &str,
        model: &Path,
        config: TranscriptionConfig,
    ) -> Result<InitialTranscriptionResult, BackendError> {
        let input = audio.read_recording(recording_id)?;
        let (mut run, engine) = self.factory.initial(model, &config, input)?;
        register_initial_chunks(audio, recording_id, &mut run)?;
        self.engine = Some(engine);
        self.settings = Some(TranscriptionSettings {
            model: Some(model.into()),
            language: config.language,
        });
        Ok(run)
    }

    fn transcribe_chunk(
        &mut self,
        audio: &mut dyn AudioBackend,
        request: ChunkRecognitionRequest,
    ) -> Result<Transcription, BackendError> {
        let model = request
            .settings
            .model
            .as_deref()
            .ok_or(BackendError::MissingModel)?;
        let replace = self.settings.as_ref() != Some(&request.settings) || self.engine.is_none();
        let mut candidate = if replace {
            Some(
                self.factory
                    .load(
                        model,
                        &TranscriptionConfig {
                            language: request.settings.language.clone(),
                            ..TranscriptionConfig::default()
                        },
                    )
                    .map_err(BackendError::Model)?,
            )
        } else {
            None
        };
        let engine = if let Some(engine) = candidate.as_mut() {
            engine
        } else {
            self.engine
                .as_mut()
                .expect("matching settings have an engine")
        };
        let forced = prepare_correction(engine.as_ref(), request.correction)?;
        let input = audio.read_chunk(&request.chunk_id)?;
        let run = engine.transcribe_chunk(
            ChunkTranscriptionRequest {
                chunk_id: request.chunk_id,
                previous_id: request.previous_id,
                source: input.audio.source,
                chunk_range: input.range,
                language: request.settings.language.clone(),
                forced_tokens: forced.clone(),
                revision: request.revision,
            },
            &input.audio.samples,
        )?;
        let decoded = run
            .segments
            .iter()
            .flat_map(|s| &s.tokens)
            .map(|t| t.token_id)
            .collect::<Vec<_>>();
        if !decoded.starts_with(&forced) {
            return Err(BackendError::Other(
                "decoder did not preserve the forced prefix".into(),
            ));
        }
        if let Some(engine) = candidate {
            self.engine = Some(engine);
        }
        self.settings = Some(request.settings);
        Ok(run)
    }
}

fn prepare_correction(
    engine: &dyn ChunkTranscriber,
    correction: Option<CorrectionContext>,
) -> Result<Vec<i32>, BackendError> {
    let Some(correction) = correction else {
        return Ok(Vec::new());
    };
    let mut forced = engine.tokenize(&correction.prefix)?;
    if let Some(id) = correction.chosen_token_id {
        forced.push(id);
    } else if engine.render_tokens(&forced)? != correction.prefix {
        return Err(BackendError::Other(
            "tokenizer did not reproduce the forced prefix".into(),
        ));
    }
    forced.insert(0, engine.beginning_timestamp_token());
    Ok(forced)
}

fn register_initial_chunks(
    audio: &mut dyn AudioBackend,
    recording_id: &str,
    run: &mut InitialTranscriptionResult,
) -> Result<(), BackendError> {
    if run.status != TranscriptionStatus::Succeeded {
        let reason = run
            .windows
            .iter()
            .filter_map(|w| w.error.as_deref())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(BackendError::Other(format!(
            "transcription failed: {reason}"
        )));
    }
    // Initial recognition owns finalization. Namespace IDs by recording and
    // result, so initial runs for different recordings/settings cannot retarget
    // an already finalized chunk.
    for chunk in &mut run.chunks {
        chunk.id = format!("chunk:{recording_id}:{}:{}", run.id, chunk.ordinal);
    }
    audio.register_chunks(
        &run.chunks
            .iter()
            .map(|chunk| ChunkMetadata {
                id: chunk.id.clone(),
                recording_id: recording_id.into(),
                range: chunk.audio_range,
                alignment: AlignmentState::Exact,
            })
            .collect::<Vec<_>>(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_audio(path: &Path, sample: f32, count: usize) {
        let mut writer = hound::WavWriter::create(
            path,
            hound::WavSpec {
                channels: 1,
                sample_rate: 16_000,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            },
        )
        .unwrap();
        for _ in 0..count {
            writer.write_sample(sample).unwrap();
        }
        writer.finalize().unwrap();
    }

    struct InitialFactory(TranscriptionStatus);
    impl TranscriberFactory for InitialFactory {
        fn load(
            &mut self,
            _: &Path,
            _: &TranscriptionConfig,
        ) -> Result<Box<dyn ChunkTranscriber>, TranscriptionError> {
            unreachable!()
        }

        fn initial(
            &mut self,
            _: &Path,
            config: &TranscriptionConfig,
            input: RecordingAudio,
        ) -> Result<(InitialTranscriptionResult, Box<dyn ChunkTranscriber>), BackendError> {
            let mut run = crate::test_support::batch("recognized", &["one", "two"]);
            run.source = input.source;
            run.config = config.clone();
            run.status = self.0;
            if self.0 != TranscriptionStatus::Succeeded {
                run.windows[0].error = Some("synthetic recognition failure".into());
            }
            Ok((run, Box::new(UnusedEngine)))
        }
    }
    struct UnusedEngine;
    impl ChunkTranscriber for UnusedEngine {
        fn tokenize(&self, _: &str) -> Result<Vec<i32>, TranscriptionError> {
            unreachable!()
        }
        fn render_tokens(&self, _: &[i32]) -> Result<String, TranscriptionError> {
            unreachable!()
        }
        fn beginning_timestamp_token(&self) -> i32 {
            unreachable!()
        }
        fn transcribe_chunk(
            &mut self,
            _: ChunkTranscriptionRequest,
            _: &[f32],
        ) -> Result<Transcription, TranscriptionError> {
            unreachable!()
        }
    }

    #[test]
    fn initial_recognition_registers_fetchable_chunks_only_on_complete_success() {
        for status in [
            TranscriptionStatus::Succeeded,
            TranscriptionStatus::Partial,
            TranscriptionStatus::Failed,
        ] {
            let file = tempfile::NamedTempFile::new().unwrap();
            write_audio(file.path(), 0.25, 200);
            let mut audio = LocalAudioBackend::new();
            let recording = audio.upload(file.path()).unwrap();
            assert!(matches!(
                audio.chunk("c0"),
                Err(BackendError::UnknownChunk(_))
            ));
            let mut recognition =
                LocalRecognitionBackend::with_factory(Box::new(InitialFactory(status)));
            let result = recognition.transcribe_recording(
                &mut audio,
                &recording,
                Path::new("fake"),
                TranscriptionConfig::default(),
            );
            if status == TranscriptionStatus::Succeeded {
                let result = result.unwrap();
                for chunk in result.chunks {
                    let metadata = audio.chunk(&chunk.id).unwrap();
                    assert_eq!(metadata.recording_id, recording);
                    assert_eq!(metadata.range, chunk.audio_range);
                    let fetched = audio.read_chunk(&chunk.id).unwrap();
                    assert_eq!(fetched.range, chunk.audio_range);
                    assert_eq!(fetched.audio.samples, vec![0.25; 200]);
                }
            } else {
                assert!(result
                    .unwrap_err()
                    .to_string()
                    .contains("synthetic recognition failure"));
                assert!(audio.chunks.is_empty());
                assert!(audio.recording(&recording).is_ok());
            }
        }
    }

    #[test]
    fn finalized_ids_do_not_collide_between_recordings_and_registration_is_atomic() {
        let mut audio = LocalAudioBackend::new();
        let mut recognition = LocalRecognitionBackend::with_factory(Box::new(InitialFactory(
            TranscriptionStatus::Succeeded,
        )));
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut produced = Vec::new();
        for sample in [0.0, 0.5] {
            write_audio(file.path(), sample, 200);
            let id = audio.upload(file.path()).unwrap();
            produced.push(
                recognition
                    .transcribe_recording(
                        &mut audio,
                        &id,
                        Path::new("fake"),
                        TranscriptionConfig::default(),
                    )
                    .unwrap(),
            );
        }
        assert_ne!(produced[0].chunks[0].id, produced[1].chunks[0].id);
        let original = audio.chunk(&produced[0].chunks[0].id).unwrap();
        let mut new = original.clone();
        new.id = "new".into();
        let mut changed = original.clone();
        changed.range.end_sample += 1;
        assert!(audio.register_chunks(&[new, changed]).is_err());
        assert!(audio.chunk("new").is_err());
        assert_eq!(audio.chunk(&original.id).unwrap(), original);
    }

    #[test]
    fn restoring_missing_audio_keeps_ids_and_replaces_previous_mappings_without_reading_files() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let path = file.path().to_owned();
        let project = crate::project::Project::from_initial_transcription_with_recording_id(
            &crate::test_support::batch("saved", &["one"]),
            "server-recording",
            Some(&path),
        );
        file.close().unwrap();
        let mut audio = LocalAudioBackend::new();
        audio.restore(project.audio_sources(), project.chunk_audio_mappings());
        assert_eq!(audio.chunk("c0").unwrap().recording_id, "server-recording");
        assert!(matches!(
            audio.availability("server-recording"),
            Err(BackendError::Unavailable { .. })
        ));
        assert!(audio.read_chunk("c0").is_err());
        let replacement = crate::project::Project::from_initial_transcription_with_recording_id(
            &crate::test_support::batch("other", &["two"]),
            "other-recording",
            None::<&Path>,
        );
        audio.restore(
            replacement.audio_sources(),
            replacement.chunk_audio_mappings(),
        );
        assert!(audio.recording("server-recording").is_err());
        assert_eq!(audio.chunk("c0").unwrap().recording_id, "other-recording");
    }

    #[test]
    fn file_proxies_read_changed_audio_without_retargeting_ids_or_ranges() {
        let file = tempfile::NamedTempFile::new().unwrap();
        write_audio(file.path(), 0.0, 200);
        let mut audio = LocalAudioBackend::new();
        let id = audio.upload(file.path()).unwrap();
        let original_hash = audio.recording(&id).unwrap().sha256;
        let mut recognition = LocalRecognitionBackend::with_factory(Box::new(InitialFactory(
            TranscriptionStatus::Succeeded,
        )));
        let run = recognition
            .transcribe_recording(
                &mut audio,
                &id,
                Path::new("fake"),
                TranscriptionConfig::default(),
            )
            .unwrap();
        let chunk = audio.chunk(&run.chunks[0].id).unwrap();
        write_audio(file.path(), 0.5, 300);
        let fetched = audio.read_chunk(&chunk.id).unwrap();
        assert_eq!(fetched.audio.samples, vec![0.5; 300]);
        assert_ne!(Some(fetched.audio.source.sha256), original_hash);
        assert_eq!(audio.chunk(&chunk.id).unwrap(), chunk);
        assert!(audio.read_recording("unknown").is_err());
        assert!(audio.read_chunk("unknown").is_err());
    }

    struct BrokenCorrectionFactory {
        tokenizer_mismatch: bool,
    }
    impl TranscriberFactory for BrokenCorrectionFactory {
        fn load(
            &mut self,
            _: &Path,
            _: &TranscriptionConfig,
        ) -> Result<Box<dyn ChunkTranscriber>, TranscriptionError> {
            Ok(Box::new(BrokenCorrectionEngine {
                tokenizer_mismatch: self.tokenizer_mismatch,
            }))
        }
    }
    struct BrokenCorrectionEngine {
        tokenizer_mismatch: bool,
    }
    impl ChunkTranscriber for BrokenCorrectionEngine {
        fn tokenize(&self, _: &str) -> Result<Vec<i32>, TranscriptionError> {
            Ok(vec![1])
        }
        fn render_tokens(&self, _: &[i32]) -> Result<String, TranscriptionError> {
            Ok(if self.tokenizer_mismatch {
                "wrong"
            } else {
                "intended"
            }
            .into())
        }
        fn beginning_timestamp_token(&self) -> i32 {
            50_364
        }
        fn transcribe_chunk(
            &mut self,
            request: ChunkTranscriptionRequest,
            _: &[f32],
        ) -> Result<Transcription, TranscriptionError> {
            assert_eq!(request.forced_tokens, vec![50_364, 1]);
            let mut result = crate::test_support::batch("broken", &["ignored prefix"]);
            result.source = request.source;
            Ok(result.transcription_for(
                &result.chunks[0],
                &request.chunk_id,
                Some(request.previous_id),
            ))
        }
    }

    #[test]
    fn correction_backend_rejects_tokenizer_round_trip_and_decoder_prefix_failures() {
        let file = tempfile::NamedTempFile::new().unwrap();
        write_audio(file.path(), 0.0, 100);
        let project = crate::project::Project::from_initial_transcription_with_source(
            &crate::test_support::batch("initial", &["old"]),
            Some(file.path()),
        );
        for tokenizer_mismatch in [true, false] {
            let mut audio = LocalAudioBackend::new();
            audio.restore(project.audio_sources(), project.chunk_audio_mappings());
            let mut recognition =
                LocalRecognitionBackend::with_factory(Box::new(BrokenCorrectionFactory {
                    tokenizer_mismatch,
                }));
            let error = recognition
                .transcribe_chunk(
                    &mut audio,
                    ChunkRecognitionRequest {
                        chunk_id: "c0".into(),
                        previous_id: project.transcriptions()[0].id.clone(),
                        revision: 2,
                        settings: TranscriptionSettings {
                            model: Some("fake".into()),
                            language: "auto".into(),
                        },
                        correction: Some(CorrectionContext {
                            prefix: "intended".into(),
                            chosen_token_id: None,
                        }),
                    },
                )
                .unwrap_err()
                .to_string();
            assert!(
                error.contains(if tokenizer_mismatch {
                    "tokenizer did not reproduce the forced prefix"
                } else {
                    "decoder did not preserve the forced prefix"
                }),
                "{error}"
            );
        }
    }
}
