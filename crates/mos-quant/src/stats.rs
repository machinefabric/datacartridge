//! Descriptive statistics, risk measures and regression.

use mos_core::error::{Error, Result};
use serde::{Deserialize, Serialize};

use crate::linalg::{Matrix, solve_spd};
use crate::{finite, need};

pub fn mean(xs: &[f64]) -> Result<f64> {
    need("mean", xs, 1)?;
    Ok(xs.iter().sum::<f64>() / xs.len() as f64)
}

/// Sample variance (divisor `n - 1`).
pub fn variance(xs: &[f64]) -> Result<f64> {
    need("variance", xs, 2)?;
    let m = mean(xs)?;
    Ok(xs.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (xs.len() - 1) as f64)
}

pub fn stdev(xs: &[f64]) -> Result<f64> {
    Ok(variance(xs)?.sqrt())
}

fn same_len(what: &'static str, x: &[f64], y: &[f64]) -> Result<()> {
    if x.len() != y.len() {
        return Err(Error::invalid(what, format!("series lengths differ ({} vs {})", x.len(), y.len())));
    }
    Ok(())
}

/// Sample covariance.
pub fn covariance(x: &[f64], y: &[f64]) -> Result<f64> {
    same_len("covariance", x, y)?;
    need("covariance", x, 2)?;
    finite("covariance", y)?;
    let (mx, my) = (mean(x)?, mean(y)?);
    Ok(x.iter().zip(y).map(|(a, b)| (a - mx) * (b - my)).sum::<f64>() / (x.len() - 1) as f64)
}

pub fn correlation(x: &[f64], y: &[f64]) -> Result<f64> {
    let (sx, sy) = (stdev(x)?, stdev(y)?);
    if sx == 0.0 || sy == 0.0 {
        return Err(Error::invalid("correlation", "a series has zero variance".to_string()));
    }
    Ok(covariance(x, y)? / (sx * sy))
}

/// Sample covariance matrix of several equally long series.
pub fn covariance_matrix(series: &[Vec<f64>]) -> Result<Matrix> {
    if series.is_empty() {
        return Err(Error::invalid("covariance matrix", "no series".to_string()));
    }
    let n = series.len();
    let mut out = vec![vec![0.0; n]; n];
    for i in 0..n {
        for j in 0..=i {
            let c = covariance(&series[i], &series[j])?;
            out[i][j] = c;
            out[j][i] = c;
        }
    }
    Ok(out)
}

fn positive_prices(what: &'static str, prices: &[f64]) -> Result<()> {
    need(what, prices, 2)?;
    match prices.iter().position(|p| *p <= 0.0) {
        Some(i) => Err(Error::invalid(what, format!("price {i} is not positive"))),
        None => Ok(()),
    }
}

pub fn simple_returns(prices: &[f64]) -> Result<Vec<f64>> {
    positive_prices("returns", prices)?;
    Ok(prices.windows(2).map(|w| w[1] / w[0] - 1.0).collect())
}

pub fn log_returns(prices: &[f64]) -> Result<Vec<f64>> {
    positive_prices("returns", prices)?;
    Ok(prices.windows(2).map(|w| (w[1] / w[0]).ln()).collect())
}

fn periods(what: &'static str, periods_per_year: f64) -> Result<()> {
    if periods_per_year.is_finite() && periods_per_year > 0.0 {
        Ok(())
    } else {
        Err(Error::invalid(what, format!("periods per year must be positive, got {periods_per_year}")))
    }
}

pub fn annualized_volatility(returns: &[f64], periods_per_year: f64) -> Result<f64> {
    periods("volatility", periods_per_year)?;
    Ok(stdev(returns)? * periods_per_year.sqrt())
}

/// Annualized Sharpe ratio of per-period returns over a per-period risk-free rate.
pub fn sharpe(returns: &[f64], risk_free_per_period: f64, periods_per_year: f64) -> Result<f64> {
    periods("sharpe", periods_per_year)?;
    let excess: Vec<f64> = returns.iter().map(|r| r - risk_free_per_period).collect();
    let sd = stdev(&excess)?;
    if sd == 0.0 {
        return Err(Error::invalid("sharpe", "returns have zero variance".to_string()));
    }
    Ok(mean(&excess)? / sd * periods_per_year.sqrt())
}

/// Annualized Sortino ratio: mean excess return over downside deviation below `target`.
pub fn sortino(returns: &[f64], target_per_period: f64, periods_per_year: f64) -> Result<f64> {
    periods("sortino", periods_per_year)?;
    need("sortino", returns, 2)?;
    let downside = returns.iter().map(|r| (r - target_per_period).min(0.0).powi(2)).sum::<f64>() / returns.len() as f64;
    if downside == 0.0 {
        return Err(Error::invalid("sortino", "no return falls below the target".to_string()));
    }
    Ok((mean(returns)? - target_per_period) / downside.sqrt() * periods_per_year.sqrt())
}

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct Drawdown {
    /// Largest peak-to-trough decline as a positive fraction (0.25 = -25%).
    pub depth: f64,
    pub peak_index: usize,
    pub trough_index: usize,
}

/// Maximum drawdown of a strictly positive value path (prices or equity).
pub fn max_drawdown(values: &[f64]) -> Result<Drawdown> {
    need("drawdown", values, 1)?;
    if values.iter().any(|v| *v <= 0.0) {
        return Err(Error::invalid("drawdown", "values must be positive".to_string()));
    }
    let mut best = Drawdown { depth: 0.0, peak_index: 0, trough_index: 0 };
    let mut peak = 0;
    for (i, v) in values.iter().enumerate() {
        if *v > values[peak] {
            peak = i;
        }
        let depth = 1.0 - v / values[peak];
        if depth > best.depth {
            best = Drawdown { depth, peak_index: peak, trough_index: i };
        }
    }
    Ok(best)
}

/// The `q`-quantile (0..=1) by linear interpolation between order statistics.
pub fn quantile(xs: &[f64], q: f64) -> Result<f64> {
    need("quantile", xs, 1)?;
    if !(0.0..=1.0).contains(&q) {
        return Err(Error::invalid("quantile", format!("q must be within [0, 1], got {q}")));
    }
    let mut sorted = xs.to_vec();
    sorted.sort_by(f64::total_cmp);
    let pos = q * (sorted.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    Ok(sorted[lo] + (sorted[hi] - sorted[lo]) * (pos - lo as f64))
}

fn confidence(what: &'static str, c: f64) -> Result<()> {
    if c > 0.0 && c < 1.0 { Ok(()) } else { Err(Error::invalid(what, format!("confidence must be within (0, 1), got {c}"))) }
}

/// Historical value-at-risk: the loss (positive number) not exceeded with `confidence`.
pub fn value_at_risk(returns: &[f64], confidence_level: f64) -> Result<f64> {
    confidence("value at risk", confidence_level)?;
    Ok(-quantile(returns, 1.0 - confidence_level)?)
}

/// Historical expected shortfall: the mean loss in the tail beyond the VaR quantile.
pub fn expected_shortfall(returns: &[f64], confidence_level: f64) -> Result<f64> {
    let threshold = -value_at_risk(returns, confidence_level)?;
    let tail: Vec<f64> = returns.iter().copied().filter(|r| *r <= threshold).collect();
    Ok(-mean(&tail)?)
}

/// Applies `f` to each full window of `window` consecutive values.
pub fn rolling<T>(xs: &[f64], window: usize, f: impl Fn(&[f64]) -> Result<T>) -> Result<Vec<T>> {
    if window == 0 || window > xs.len() {
        return Err(Error::invalid("rolling window", format!("window {window} does not fit {} values", xs.len())));
    }
    xs.windows(window).map(f).collect()
}

pub fn zscores(xs: &[f64]) -> Result<Vec<f64>> {
    let (m, sd) = (mean(xs)?, stdev(xs)?);
    if sd == 0.0 {
        return Err(Error::invalid("z-score", "series has zero variance".to_string()));
    }
    Ok(xs.iter().map(|x| (x - m) / sd).collect())
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Regression {
    pub alpha: f64,
    /// One coefficient per regressor, in input order.
    pub betas: Vec<f64>,
    pub r_squared: f64,
    /// Standard deviation of residuals (divisor `n - k - 1`).
    pub residual_std: f64,
    pub observations: usize,
}

/// Ordinary least squares of `y` on the regressors with an intercept.
pub fn ols(y: &[f64], regressors: &[Vec<f64>]) -> Result<Regression> {
    let k = regressors.len();
    if k == 0 {
        return Err(Error::invalid("regression", "no regressors".to_string()));
    }
    let n = y.len();
    finite("regression", y)?;
    for x in regressors {
        same_len("regression", y, x)?;
        finite("regression", x)?;
    }
    if n < k + 2 {
        return Err(Error::invalid("regression", format!("{n} observations cannot identify {k} coefficients and an intercept")));
    }
    // Normal equations on [1, x1, ..., xk].
    let column = |j: usize, i: usize| if j == 0 { 1.0 } else { regressors[j - 1][i] };
    let dim = k + 1;
    let mut xtx = vec![vec![0.0; dim]; dim];
    let mut xty = vec![0.0; dim];
    for i in 0..n {
        for a in 0..dim {
            xty[a] += column(a, i) * y[i];
            for b in 0..dim {
                xtx[a][b] += column(a, i) * column(b, i);
            }
        }
    }
    let coef = solve_spd(&xtx, &xty)
        .map_err(|_| Error::invalid("regression", "regressors are collinear or constant".to_string()))?;
    let y_mean = mean(y)?;
    let (mut ss_res, mut ss_tot) = (0.0, 0.0);
    for i in 0..n {
        let fitted: f64 = (0..dim).map(|a| coef[a] * column(a, i)).sum();
        ss_res += (y[i] - fitted).powi(2);
        ss_tot += (y[i] - y_mean).powi(2);
    }
    if ss_tot == 0.0 {
        return Err(Error::invalid("regression", "dependent variable is constant".to_string()));
    }
    Ok(Regression {
        alpha: coef[0],
        betas: coef[1..].to_vec(),
        r_squared: 1.0 - ss_res / ss_tot,
        residual_std: (ss_res / (n - k - 1) as f64).sqrt(),
        observations: n,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn moments() {
        let xs = [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        assert_eq!(mean(&xs).unwrap(), 5.0);
        assert!(close(variance(&xs).unwrap(), 32.0 / 7.0, 1e-12));
        assert!(mean(&[]).is_err());
        assert!(variance(&[1.0]).is_err());
        assert!(mean(&[1.0, f64::NAN]).is_err());
    }

    #[test]
    fn covariance_and_correlation() {
        let x = [1.0, 2.0, 3.0, 4.0];
        let y = [2.0, 4.0, 6.0, 8.0];
        assert!(close(correlation(&x, &y).unwrap(), 1.0, 1e-12));
        assert!(close(covariance(&x, &y).unwrap(), 10.0 / 3.0, 1e-12));
        let neg: Vec<f64> = y.iter().map(|v| -v).collect();
        assert!(close(correlation(&x, &neg).unwrap(), -1.0, 1e-12));
        assert!(correlation(&x, &[1.0, 1.0, 1.0, 1.0]).is_err());
        assert!(covariance(&x, &[1.0]).is_err());
        let m = covariance_matrix(&[x.to_vec(), y.to_vec()]).unwrap();
        assert!(close(m[0][1], m[1][0], 0.0) && close(m[1][1], 4.0 * m[0][0], 1e-12));
    }

    #[test]
    fn returns_and_volatility() {
        let prices = [100.0, 110.0, 99.0];
        let r = simple_returns(&prices).unwrap();
        assert!(close(r[0], 0.10, 1e-12) && close(r[1], -0.10, 1e-12));
        let lr = log_returns(&prices).unwrap();
        assert!(close(lr.iter().sum::<f64>(), (99.0f64 / 100.0).ln(), 1e-12), "log returns add up");
        assert!(simple_returns(&[100.0, 0.0]).is_err());
        assert!(simple_returns(&[100.0]).is_err());
        let vol = annualized_volatility(&[0.01, -0.01, 0.01, -0.01], 252.0).unwrap();
        assert!(close(vol, (4.0e-4f64 / 3.0).sqrt() * 252f64.sqrt(), 1e-12));
    }

    #[test]
    fn drawdown() {
        let d = max_drawdown(&[100.0, 120.0, 90.0, 95.0, 130.0, 104.0]).unwrap();
        assert!(close(d.depth, 0.25, 1e-12));
        assert_eq!((d.peak_index, d.trough_index), (1, 2));
        assert_eq!(max_drawdown(&[1.0, 2.0, 3.0]).unwrap().depth, 0.0);
    }

    #[test]
    fn tail_risk() {
        let returns: Vec<f64> = (1..=100).map(|i| (i as f64 - 50.5) / 100.0).collect();
        // 5% quantile of -0.495..0.495 by interpolation is -0.4455.
        assert!(close(value_at_risk(&returns, 0.95).unwrap(), 0.4455, 1e-9));
        let es = expected_shortfall(&returns, 0.95).unwrap();
        assert!(close(es, (0.495 + 0.485 + 0.475 + 0.465 + 0.455) / 5.0, 1e-9));
        assert!(es >= value_at_risk(&returns, 0.95).unwrap());
        assert!(value_at_risk(&returns, 1.0).is_err());
        assert_eq!(quantile(&[3.0, 1.0, 2.0], 0.5).unwrap(), 2.0);
    }

    #[test]
    fn sharpe_and_sortino() {
        let r = [0.02, 0.01, -0.01, 0.03, 0.00];
        let m = 0.01;
        let sd = stdev(&r).unwrap();
        assert!(close(sharpe(&r, 0.0, 12.0).unwrap(), m / sd * 12f64.sqrt(), 1e-12));
        let dd = ((0.01f64).powi(2) / 5.0).sqrt();
        assert!(close(sortino(&r, 0.0, 12.0).unwrap(), m / dd * 12f64.sqrt(), 1e-12));
        assert!(sortino(&[0.01, 0.02], 0.0, 12.0).is_err());
        assert!(sharpe(&[0.01, 0.01], 0.0, 12.0).is_err());
    }

    #[test]
    fn regression_recovers_known_coefficients() {
        let x1: Vec<f64> = (0..50).map(|i| (i as f64 * 0.37).sin()).collect();
        let x2: Vec<f64> = (0..50).map(|i| (i as f64 * 0.11).cos()).collect();
        let y: Vec<f64> = (0..50).map(|i| 0.5 + 1.2 * x1[i] - 0.7 * x2[i]).collect();
        let fit = ols(&y, &[x1.clone(), x2.clone()]).unwrap();
        assert!(close(fit.alpha, 0.5, 1e-9) && close(fit.betas[0], 1.2, 1e-9) && close(fit.betas[1], -0.7, 1e-9));
        assert!(close(fit.r_squared, 1.0, 1e-9) && fit.residual_std < 1e-6);
        // Collinear regressors and underdetermined systems are refused.
        let doubled: Vec<f64> = x1.iter().map(|v| 2.0 * v).collect();
        assert!(ols(&y, &[x1.clone(), doubled]).is_err());
        assert!(ols(&y[..3], &[x1[..3].to_vec(), x2[..3].to_vec()]).is_err());
        // Noise lowers R².
        let noisy: Vec<f64> = y.iter().enumerate().map(|(i, v)| v + if i % 2 == 0 { 0.5 } else { -0.5 }).collect();
        assert!(ols(&noisy, &[x1, x2]).unwrap().r_squared < 0.95);
    }

    #[test]
    fn rolling_and_zscores() {
        let xs = [1.0, 2.0, 3.0, 4.0];
        assert_eq!(rolling(&xs, 2, mean).unwrap(), vec![1.5, 2.5, 3.5]);
        assert!(rolling(&xs, 5, mean).is_err());
        let z = zscores(&xs).unwrap();
        assert!(close(z.iter().sum::<f64>(), 0.0, 1e-12));
        assert!(close(stdev(&z).unwrap(), 1.0, 1e-12));
    }
}
