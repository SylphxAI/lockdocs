//! A tiny async runtime.
//!
//! # Spawning
//!
//! Use [`spawn`] to run a future in the background.

mod task;
pub use task::spawn;
