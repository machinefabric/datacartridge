//! MarketOS core domain model: the part of it the series steps use.
//!
//! Copied from MarketOS's mos-core unchanged, module for module: its error type and its
//! clocks and instants. The rest (identities, facts, evidence, policy, trading) is about
//! MarketOS's own world model, and nothing copied here uses it.

pub mod error;
pub mod time;

pub use error::{Error, Result};
