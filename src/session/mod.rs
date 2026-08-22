//! Shared line-oriented session commands, rendering, and audio playback.

mod command;
mod editing;
mod issues;
mod playback;
mod render;
mod shell;

pub use playback::{AudioPlayer, Ffplay, PlaybackError, PlaybackSpeed};
pub use render::render_recognition_chunks;
pub use shell::{run_readline_session, run_session, SessionContext};
