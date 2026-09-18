//! Canonical audio input and sample-based transcription positions.

mod model;
mod wav;

pub use model::*;
pub use wav::{read_canonical_wav, WavError, WavInput};
