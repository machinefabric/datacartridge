//! The base the series steps are built on: an error type, and clocks and instants.
//!
//! datacartridge's own. marketcartridge builds its crates on it, and MarketOS is to take it
//! from here rather than keep its own.

pub mod error;
pub mod time;

pub use error::{Error, Result};
