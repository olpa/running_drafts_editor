//! Recognition chunk detection, planning, validation, and serialization.

mod model;
mod planner;
mod silero;
mod wav;

pub use model::*;
pub use planner::{plan, plan_with_detector, validate_plan};
pub use silero::{SileroConfig, SileroDetector};
pub use wav::{read_canonical_wav, WavInput};
