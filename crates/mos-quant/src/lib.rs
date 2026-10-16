//! Deterministic quantitative library (CMP-002).
//!
//! Copied from MarketOS (crates/mos-quant): the statistics, linear algebra and pipeline
//! steps the series caps use. Its other modules (cash flows, counterfactuals, event
//! studies, Monte Carlo, options, regimes, scenarios, calculations) are not served here.
//!
//! Every number MarketOS shows comes from here or from SQL — never from a language
//! model. Functions are pure, operate on `f64`, validate their inputs and fail explicitly
//! rather than return NaN.

pub mod linalg;
pub mod stats;
pub mod steps;

/// Version of the numerical code, recorded in every run that uses it.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

use mos_core::error::{Error, Result};

pub(crate) fn finite(what: &'static str, xs: &[f64]) -> Result<()> {
    match xs.iter().position(|x| !x.is_finite()) {
        Some(i) => Err(Error::invalid(what, format!("element {i} is not finite"))),
        None => Ok(()),
    }
}

pub(crate) fn need(what: &'static str, xs: &[f64], min: usize) -> Result<()> {
    if xs.len() < min {
        return Err(Error::invalid(what, format!("needs at least {min} values, got {}", xs.len())));
    }
    finite(what, xs)
}
