use std::{
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

use crate::{
    chunking::SampleRange,
    document::Document,
    navigation::{Address, NavigationState},
    replay::{resolve as resolve_replay, ResolvedReplay},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackSpeed {
    Normal,
    Slow,
}

impl PlaybackSpeed {
    fn atempo(self) -> &'static str {
        match self {
            Self::Normal => "1.0",
            Self::Slow => "0.75",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LastPlayback {
    source_id: String,
    range: SampleRange,
    require_file: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct ReplayStart {
    pub context_samples: u64,
    pub speed: PlaybackSpeed,
    pub require_file: bool,
}

pub(crate) fn start_document_replay(
    document: &Document,
    navigation: &NavigationState,
    address: Option<&Address>,
    start: ReplayStart,
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
    start_resolved(
        document,
        &resolved,
        start.speed,
        start.require_file,
        player,
        output,
        errors,
    )
}

fn start_resolved(
    document: &Document,
    resolved: &ResolvedReplay,
    speed: PlaybackSpeed,
    require_file: bool,
    player: &mut impl AudioPlayer,
    output: &mut impl Write,
    errors: &mut impl Write,
) -> io::Result<Option<LastPlayback>> {
    let Some(source) = document.audio_source(&resolved.source_id) else {
        writeln!(
            errors,
            "replay unavailable: audio source '{}' is missing",
            resolved.source_id
        )?;
        return Ok(None);
    };
    let Some(path) = source.path() else {
        writeln!(errors, "audio source '{}' has no local path", source.id())?;
        return Ok(None);
    };
    if require_file && !path.is_file() {
        writeln!(
            errors,
            "audio source '{}' is unavailable at {}",
            source.id(),
            path.display()
        )?;
        return Ok(None);
    }
    if resolved.partial {
        writeln!(errors, "replay uses partial token alignment")?;
    }
    if resolved.alignment != crate::document::AlignmentState::Exact {
        writeln!(errors, "replay alignment is {}", resolved.alignment)?;
    }
    match player.start(path, 16_000, resolved.range, speed) {
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
                range: resolved.range,
                require_file,
            }))
        }
        Err(error) => {
            writeln!(errors, "playback failed: {error}")?;
            Ok(None)
        }
    }
}

pub(crate) fn repeat_document_replay(
    document: &Document,
    last: Option<&LastPlayback>,
    speed: PlaybackSpeed,
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
        range: last.range,
        alignment: crate::document::AlignmentState::Exact,
        partial: false,
    };
    let _ = start_resolved(
        document,
        &resolved,
        speed,
        last.require_file,
        player,
        output,
        errors,
    )?;
    Ok(())
}

pub trait AudioPlayer {
    fn play(
        &mut self,
        source: &Path,
        sample_rate_hz: u32,
        range: SampleRange,
    ) -> Result<(), PlaybackError>;

    fn start(
        &mut self,
        source: &Path,
        sample_rate_hz: u32,
        range: SampleRange,
        _speed: PlaybackSpeed,
    ) -> Result<(), PlaybackError> {
        self.play(source, sample_rate_hz, range)
    }

    fn stop(&mut self) -> Result<bool, PlaybackError> {
        Ok(false)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PlaybackError {
    #[error("could not start playback program '{}': {source}", program.display())]
    Start {
        program: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("playback program '{}' exited with {status}", program.display())]
    Failed {
        program: PathBuf,
        status: std::process::ExitStatus,
    },
    #[error("cannot play a range at a zero sample rate")]
    ZeroSampleRate,
    #[error("invalid playback range [{}, {})", .0.start_sample, .0.end_sample)]
    InvalidRange(SampleRange),
    #[error("{0}")]
    Other(String),
}

#[derive(Debug)]
pub struct Ffplay {
    program: PathBuf,
    child: Option<Child>,
}

impl Default for Ffplay {
    fn default() -> Self {
        Self::new("ffplay")
    }
}

impl Ffplay {
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            child: None,
        }
    }
}

impl AudioPlayer for Ffplay {
    fn play(
        &mut self,
        source: &Path,
        sample_rate_hz: u32,
        range: SampleRange,
    ) -> Result<(), PlaybackError> {
        if sample_rate_hz == 0 {
            return Err(PlaybackError::ZeroSampleRate);
        }
        if range.start_sample >= range.end_sample {
            return Err(PlaybackError::InvalidRange(range));
        }
        let start = samples_as_seconds(range.start_sample, sample_rate_hz);
        let duration = samples_as_seconds(range.len(), sample_rate_hz);
        let status = Command::new(&self.program)
            .args([
                "-nodisp",
                "-autoexit",
                "-loglevel",
                "error",
                "-ss",
                &start,
                "-t",
                &duration,
                "-i",
            ])
            .arg(source)
            .stdin(Stdio::null())
            .status()
            .map_err(|source| PlaybackError::Start {
                program: self.program.clone(),
                source,
            })?;
        if status.success() {
            Ok(())
        } else {
            Err(PlaybackError::Failed {
                program: self.program.clone(),
                status,
            })
        }
    }

    fn start(
        &mut self,
        source: &Path,
        sample_rate_hz: u32,
        range: SampleRange,
        speed: PlaybackSpeed,
    ) -> Result<(), PlaybackError> {
        self.stop()?;
        if sample_rate_hz == 0 {
            return Err(PlaybackError::ZeroSampleRate);
        }
        if range.start_sample >= range.end_sample {
            return Err(PlaybackError::InvalidRange(range));
        }
        let start = samples_as_seconds(range.start_sample, sample_rate_hz);
        let duration = samples_as_seconds(range.len(), sample_rate_hz);
        let child = Command::new(&self.program)
            .args([
                "-nodisp",
                "-autoexit",
                "-loglevel",
                "error",
                "-ss",
                &start,
                "-t",
                &duration,
                "-af",
            ])
            .arg(format!("atempo={}", speed.atempo()))
            .arg("-i")
            .arg(source)
            .stdin(Stdio::null())
            .spawn()
            .map_err(|source| PlaybackError::Start {
                program: self.program.clone(),
                source,
            })?;
        self.child = Some(child);
        Ok(())
    }

    fn stop(&mut self) -> Result<bool, PlaybackError> {
        let Some(mut child) = self.child.take() else {
            return Ok(false);
        };
        if child
            .try_wait()
            .map_err(|error| PlaybackError::Other(error.to_string()))?
            .is_some()
        {
            return Ok(false);
        }
        child
            .kill()
            .map_err(|error| PlaybackError::Other(error.to_string()))?;
        child
            .wait()
            .map_err(|error| PlaybackError::Other(error.to_string()))?;
        Ok(true)
    }
}

impl Drop for Ffplay {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

pub(crate) fn samples_as_seconds(samples: u64, sample_rate_hz: u32) -> String {
    let rate = u64::from(sample_rate_hz);
    format!(
        "{}.{:09}",
        samples / rate,
        (samples % rate) * 1_000_000_000 / rate
    )
}
