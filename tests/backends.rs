//! Sessions use backend identities even when the client has no local audio.

mod common;

use running_drafts_editor::{
    backend::{
        AudioBackend, AudioPlayer, BackendError, ChunkAudio, ChunkMetadata,
        ChunkRecognitionRequest, LocalAudioBackend, PlaybackError, RecognitionBackend,
        RecordingAudio, RecordingMetadata, ReplayRequest,
    },
    chunking::SampleRange,
    document::{AudioSource, ChunkAudioMapping},
    persistence::{load_project, save_project},
    project::Project,
    session::{run_session, SessionContext},
    transcription::{InitialTranscriptionResult, Transcription, TranscriptionConfig},
};
use std::{cell::RefCell, io::Cursor, path::Path, rc::Rc};

#[derive(Default)]
struct Calls {
    recognition: Vec<ChunkRecognitionRequest>,
    replay: Vec<(String, Vec<String>, SampleRange)>,
}

struct ServerAudio {
    metadata: LocalAudioBackend,
    calls: Rc<RefCell<Calls>>,
}
impl AudioBackend for ServerAudio {
    fn upload(&mut self, _: &Path) -> Result<String, BackendError> {
        unreachable!()
    }
    fn recording(&self, id: &str) -> Result<RecordingMetadata, BackendError> {
        self.metadata.recording(id)
    }
    fn chunk(&self, id: &str) -> Result<ChunkMetadata, BackendError> {
        self.metadata.chunk(id)
    }
    fn read_recording(&mut self, _: &str) -> Result<RecordingAudio, BackendError> {
        panic!("client must not request samples from the server")
    }
    fn read_chunk(&mut self, _: &str) -> Result<ChunkAudio, BackendError> {
        panic!("client must not request samples from the server")
    }
    fn register_chunks(&mut self, chunks: &[ChunkMetadata]) -> Result<(), BackendError> {
        self.metadata.register_chunks(chunks)
    }
    fn restore(&mut self, sources: &[AudioSource], chunks: &[ChunkAudioMapping]) {
        self.metadata.restore(sources, chunks);
    }
    fn availability(&self, id: &str) -> Result<(), BackendError> {
        self.recording(id).map(|_| ())
    }
    fn start_replay(
        &mut self,
        request: ReplayRequest<'_>,
        _: &mut dyn AudioPlayer,
    ) -> Result<(), BackendError> {
        // Serving retained chunks does not require the original recording.
        for id in request.chunk_ids {
            self.chunk(id)?;
        }
        self.calls.borrow_mut().replay.push((
            request.recording_id.into(),
            request.chunk_ids.into(),
            request.range,
        ));
        Ok(())
    }
}

struct ServerRecognition {
    initial: InitialTranscriptionResult,
    calls: Rc<RefCell<Calls>>,
}
impl RecognitionBackend for ServerRecognition {
    fn transcribe_recording(
        &mut self,
        _: &mut dyn AudioBackend,
        _: &str,
        _: &Path,
        _: TranscriptionConfig,
    ) -> Result<InitialTranscriptionResult, BackendError> {
        unreachable!()
    }
    fn transcribe_chunk(
        &mut self,
        audio: &mut dyn AudioBackend,
        request: ChunkRecognitionRequest,
    ) -> Result<Transcription, BackendError> {
        let metadata = audio.chunk(&request.chunk_id)?;
        let text = request
            .correction
            .as_ref()
            .map_or("server text", |correction| correction.prefix.as_str());
        let mut result = common::batch(&format!("server-{}", request.revision), &[text]);
        result.source = self.initial.source.clone();
        result.config.language = request.settings.language.clone();
        result.chunks[0].audio_range = metadata.range;
        let transcription = result.transcription_for(
            &result.chunks[0],
            &request.chunk_id,
            Some(request.previous_id.clone()),
        );
        self.calls.borrow_mut().recognition.push(request);
        Ok(transcription)
    }
}

struct NoLocalPlayer;
impl AudioPlayer for NoLocalPlayer {
    fn play(&mut self, _: &Path, _: u32, _: SampleRange) -> Result<(), PlaybackError> {
        panic!("server audio must not require a local file player")
    }
}

#[test]
fn reopened_server_project_replays_corrects_and_restores_history_without_local_audio_or_models() {
    let dir = tempfile::tempdir().unwrap();
    let saved = dir.path().join("server.json");
    let mut initial = common::batch("server-initial", &["old"]);
    initial.config.language = "en".into();
    let mut project = Project::from_initial_transcription_with_recording_id(
        &initial,
        "server-recording-42",
        None::<&Path>,
    );
    project
        .configure_initial_settings(Some("server-model".into()), "en".into())
        .unwrap();
    save_project(&saved, &project).unwrap();
    let project = load_project(&saved).unwrap();
    let calls = Rc::new(RefCell::new(Calls::default()));
    let audio = ServerAudio {
        metadata: LocalAudioBackend::new(),
        calls: calls.clone(),
    };
    let recognition = ServerRecognition {
        initial,
        calls: calls.clone(),
    };
    let context = SessionContext::saved_project(&saved, None)
        .with_backends(Box::new(audio), Box::new(recognition));
    let final_path = dir.path().join("final.json");
    let commands = format!("play\nreplay\n1.1.1,1.1.2replace corrected\nlanguage de\n2undo\n2redo\nsave {}\nload {}\nplay\nquit\n", final_path.display(), final_path.display());
    let mut output = Vec::new();
    let mut errors = Vec::new();
    run_session(
        &project,
        context,
        &mut Cursor::new(commands.as_bytes()),
        &mut output,
        &mut errors,
        &mut NoLocalPlayer,
        0,
    )
    .unwrap();
    assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
    let calls = calls.borrow();
    assert_eq!(calls.recognition.len(), 2);
    assert_eq!(calls.recognition[0].chunk_id, "c0");
    assert_eq!(
        calls.recognition[0].correction.as_ref().unwrap().prefix,
        "corrected"
    );
    assert_eq!(calls.recognition[1].settings.language, "de");
    assert_eq!(calls.replay.len(), 3);
    assert!(calls
        .replay
        .iter()
        .all(|(id, chunks, _)| id == "server-recording-42" && chunks == &["c0"]));
    let final_project = load_project(&final_path).unwrap();
    assert_eq!(final_project.audio_sources()[0].id(), "server-recording-42");
    assert_eq!(final_project.settings().language, "de");
    assert_eq!(
        final_project.chunk_audio_mapping("c0").unwrap().range(),
        SampleRange {
            start_sample: 0,
            end_sample: 100
        }
    );
}

#[test]
fn accepting_changed_source_circumstances_does_not_allow_changed_chunk_boundaries() {
    let mut project = common::project(&["original"]);
    let before = project.clone();
    let mut result = common::batch("changed", &["changed"]);
    result.source.sha256 = "33".repeat(32);
    result.chunks[0].audio_range.start_sample = 1;
    let transcription = common::proposal(&project, result);
    assert!(project
        .install_transcription(1, 1, transcription, project.settings().clone())
        .unwrap_err()
        .contains("chunk audio boundaries changed"));
    assert_eq!(project, before);
}
