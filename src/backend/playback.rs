use crate::chunking::SampleRange;
use std::{
    io,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackSpeed {
    Normal,
    Slow,
}

impl PlaybackSpeed {
    pub(crate) fn atempo(self) -> &'static str {
        match self {
            Self::Normal => "1.0",
            Self::Slow => "0.75",
        }
    }
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

fn samples_as_seconds(samples: u64, sample_rate_hz: u32) -> String {
    let rate = u64::from(sample_rate_hz);
    format!(
        "{}.{:09}",
        samples / rate,
        (samples % rate) * 1_000_000_000 / rate
    )
}

#[cfg(test)]
mod tests {
    use super::samples_as_seconds;

    #[test]
    fn ffplay_seconds_preserve_sample_precision() {
        assert_eq!(samples_as_seconds(1, 16_000), "0.000062500");
        assert_eq!(samples_as_seconds(480_001, 16_000), "30.000062500");
    }
}
