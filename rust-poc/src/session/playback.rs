pub use crate::backend::{AudioPlayer, Ffplay, PlaybackError, PlaybackSpeed};
use crate::{
    backend::{AudioBackend, BackendError, ReplayRequest},
    chunking::SampleRange,
    navigation::{Address, NavigationState},
    project::Project,
    replay::{resolve as resolve_replay, ResolvedReplay},
};
use std::io::{self, Write};

#[derive(Debug, Clone)]
pub(crate) struct LastPlayback {
    source_id: String,
    chunk_ids: Vec<String>,
    range: SampleRange,
}

#[derive(Clone, Copy)]
pub(crate) struct ReplayStart {
    pub context_samples: u64,
    pub speed: PlaybackSpeed,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn start_document_replay(
    document: &Project,
    navigation: &NavigationState,
    address: Option<&Address>,
    start: ReplayStart,
    audio: &mut dyn AudioBackend,
    player: &mut impl AudioPlayer,
    output: &mut impl Write,
    errors: &mut impl Write,
) -> io::Result<Option<LastPlayback>> {
    let resolved = match resolve_replay(document, navigation, address, start.context_samples) {
        Ok(resolved) => resolved,
        Err(error) => {
            writeln!(errors, "replay unavailable: {error}")?;
            return Ok(None);
        }
    };
    start_resolved(audio, &resolved, start.speed, player, output, errors)
}

fn start_resolved(
    audio: &mut dyn AudioBackend,
    resolved: &ResolvedReplay,
    speed: PlaybackSpeed,
    player: &mut impl AudioPlayer,
    output: &mut impl Write,
    errors: &mut impl Write,
) -> io::Result<Option<LastPlayback>> {
    if let Err(error) = audio.availability(&resolved.source_id) {
        match error {
            BackendError::UnknownRecording(_) => writeln!(errors, "replay unavailable: {error}")?,
            _ => writeln!(errors, "{error}")?,
        }
        return Ok(None);
    }
    if resolved.partial {
        writeln!(errors, "replay uses partial token alignment")?;
    }
    if resolved.alignment != crate::document::AlignmentState::Exact {
        writeln!(errors, "replay alignment is {}", resolved.alignment)?;
    }
    match audio.start_replay(
        ReplayRequest {
            recording_id: &resolved.source_id,
            chunk_ids: &resolved.chunk_ids,
            range: resolved.range,
            speed,
        },
        player,
    ) {
        Ok(()) => {
            writeln!(
                output,
                "playing [{}, {}) at {} speed",
                resolved.range.start_sample,
                resolved.range.end_sample,
                speed.atempo()
            )?;
            Ok(Some(LastPlayback {
                source_id: resolved.source_id.clone(),
                chunk_ids: resolved.chunk_ids.clone(),
                range: resolved.range,
            }))
        }
        Err(error) => {
            match error {
                BackendError::UnknownRecording(_) => {
                    writeln!(errors, "replay unavailable: {error}")?
                }
                _ => writeln!(errors, "{error}")?,
            }
            Ok(None)
        }
    }
}

pub(crate) fn repeat_document_replay(
    last: Option<&LastPlayback>,
    speed: PlaybackSpeed,
    audio: &mut dyn AudioBackend,
    player: &mut impl AudioPlayer,
    output: &mut impl Write,
    errors: &mut impl Write,
) -> io::Result<()> {
    let Some(last) = last else {
        writeln!(errors, "there is no previous replay")?;
        return Ok(());
    };
    let resolved = ResolvedReplay {
        source_id: last.source_id.clone(),
        chunk_ids: last.chunk_ids.clone(),
        range: last.range,
        alignment: crate::document::AlignmentState::Exact,
        partial: false,
    };
    let _ = start_resolved(audio, &resolved, speed, player, output, errors)?;
    Ok(())
}
