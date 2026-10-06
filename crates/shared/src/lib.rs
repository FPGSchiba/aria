pub mod error;

pub use error::{Error, Result};

// ARIA-146 proof: a deliberate clippy warning (bool_comparison). Never merged.
pub fn is_enabled(flag: bool) -> bool {
    flag == true
}
