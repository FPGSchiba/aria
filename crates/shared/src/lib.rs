pub mod error;

pub use error::{Error, Result};

// ARIA-146 proof: see coverage_drop.rs. Never merged.
pub mod coverage_drop;
