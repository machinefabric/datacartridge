//! Deterministic quantitative library (CMP-002).
//!
//! datacartridge's statistics, linear algebra, option maths (its normal distribution, which
//! forecast trials use) and pipeline steps, which the series and forecast caps use.
//! marketcartridge takes them from here, and MarketOS is to rather than keep its own.
//!
//! Every number MarketOS shows comes from here or from SQL — never from a language
//! model. Functions are pure, operate on `f64`, validate their inputs and fail explicitly
//! rather than return NaN.

pub mod linalg;
pub mod options;
pub mod stats;
pub mod steps;

/// Version of the numerical code, recorded in every run that uses it.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

use datacartridge_core::error::{Error, Result};

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

/// Finds a root of `f` in `[lo, hi]` by bisection, given a sign change at the ends.
pub(crate) fn bisect(mut lo: f64, mut hi: f64, f: impl Fn(f64) -> f64) -> f64 {
    let mut f_lo = f(lo);
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        let f_mid = f(mid);
        if f_mid == 0.0 || (hi - lo).abs() < 1e-14 * (1.0 + mid.abs()) {
            return mid;
        }
        if (f_mid < 0.0) == (f_lo < 0.0) {
            lo = mid;
            f_lo = f_mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}
