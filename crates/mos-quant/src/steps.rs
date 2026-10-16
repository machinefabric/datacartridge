//! What the steps of a pipeline do to series: pure, deterministic functions of their
//! inputs, so that a step run again on the same inputs gives the same output.
//!
//! A series is its points in time order: `(when, value)`. Two series are taken together on
//! the instants both have.

use chrono::Datelike;
use mos_core::time::Timestamp;
use serde::{Deserialize, Serialize};

use mos_core::error::{Error, Result};

/// One value of a series, at the instant it is of.
pub type Point = (Timestamp, f64);

fn bad(reason: impl Into<String>) -> Error {
    Error::invalid("step", reason.into())
}

/// Frequencies a series can be made at.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Every {
    Daily,
    Weekly,
    Monthly,
    Quarterly,
    Annual,
}

/// How the values in one period become one.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Aggregate {
    Last,
    Mean,
    Sum,
}

/// The period an instant falls in: its year and, within it, the day, week, month or quarter.
fn period(at: Timestamp, every: Every) -> (i32, u32) {
    let date = at.date();
    match every {
        Every::Daily => (date.year(), date.ordinal()),
        Every::Weekly => (date.iso_week().year(), date.iso_week().week()),
        Every::Monthly => (date.year(), date.month()),
        Every::Quarterly => (date.year(), (date.month() - 1) / 3 + 1),
        Every::Annual => (date.year(), 0),
    }
}

/// The series at another frequency: one value for each period it has values in, at the last
/// instant of the period it has a value at.
pub fn resample(points: &[Point], every: Every, how: Aggregate) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::new();
    let mut group: Vec<f64> = Vec::new();
    let mut current: Option<((i32, u32), Timestamp)> = None;
    let close = |group: &[f64], at: Timestamp, out: &mut Vec<Point>| {
        let value = match how {
            Aggregate::Last => group[group.len() - 1],
            Aggregate::Mean => group.iter().sum::<f64>() / group.len() as f64,
            Aggregate::Sum => group.iter().sum::<f64>(),
        };
        out.push((at, value));
    };
    for &(at, value) in points {
        let key = period(at, every);
        match current {
            Some((k, last)) if k != key => {
                close(&group, last, &mut out);
                group.clear();
            }
            _ => {}
        }
        group.push(value);
        current = Some((key, at));
    }
    if let Some((_, last)) = current {
        close(&group, last, &mut out);
    }
    out
}

/// How a change is measured.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeHow {
    /// In percent of the value it changed from.
    Percent,
    /// The natural logarithm of the ratio.
    Log,
    /// The value less the value it changed from.
    Difference,
}

/// How each value changed from the value `over` before it, at the later instant. A change
/// from zero in percent, or from or to a value not above zero as a logarithm, has no size and
/// gives no point; how many were left out is said.
pub fn change(points: &[Point], how: ChangeHow, over: usize) -> Result<(Vec<Point>, usize)> {
    if over == 0 {
        return Err(bad("a change is measured from at least one value back"));
    }
    let mut out = Vec::new();
    let mut left_out = 0;
    for i in over..points.len() {
        let (from, (at, to)) = (points[i - over].1, points[i]);
        let changed = match how {
            ChangeHow::Percent if from != 0.0 => Some((to / from - 1.0) * 100.0),
            ChangeHow::Log if from > 0.0 && to > 0.0 => Some((to / from).ln()),
            ChangeHow::Difference => Some(to - from),
            _ => None,
        };
        match changed {
            Some(value) => out.push((at, value)),
            None => left_out += 1,
        }
    }
    Ok((out, left_out))
}

/// Each instant given the value `by` values before it.
pub fn lag(points: &[Point], by: usize) -> Vec<Point> {
    (by..points.len()).map(|i| (points[i].0, points[i - by].1)).collect()
}

/// A statistic over a moving window.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Statistic {
    Mean,
    /// The sample standard deviation.
    Sd,
    /// How far the latest value stands from the window's mean, in the window's deviations.
    Zscore,
    /// The window's standard deviation scaled to a year: for a series of changes, its
    /// realised volatility.
    Vol,
}

/// The statistic over each window of `window` values, at the window's last instant. A window
/// whose deviation is zero gives no z-score; how many were left out is said.
pub fn rolling(points: &[Point], window: usize, statistic: Statistic, periods_per_year: f64) -> Result<(Vec<Point>, usize)> {
    if window < 2 {
        return Err(bad("a moving window holds at least two values"));
    }
    let mut out = Vec::new();
    let mut left_out = 0;
    for end in window..=points.len() {
        let values: Vec<f64> = points[end - window..end].iter().map(|p| p.1).collect();
        let mean = values.iter().sum::<f64>() / window as f64;
        let sd = (values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (window - 1) as f64).sqrt();
        let value = match statistic {
            Statistic::Mean => Some(mean),
            Statistic::Sd => Some(sd),
            Statistic::Zscore if sd > 0.0 => Some((values[window - 1] - mean) / sd),
            Statistic::Zscore => None,
            Statistic::Vol => Some(sd * periods_per_year.sqrt()),
        };
        match value {
            Some(value) => out.push((points[end - 1].0, value)),
            None => left_out += 1,
        }
    }
    Ok((out, left_out))
}

/// How two series are combined, value by value.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Add,
    Subtract,
    Multiply,
    Divide,
}

/// The values both series have at the same instants, side by side.
pub fn align(a: &[Point], b: &[Point]) -> Vec<(Timestamp, f64, f64)> {
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        match a[i].0.cmp(&b[j].0) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                out.push((a[i].0, a[i].1, b[j].1));
                i += 1;
                j += 1;
            }
        }
    }
    out
}

/// The two series combined at the instants both have. Divided by zero gives no point; how
/// many were left out is said.
pub fn combine(a: &[Point], b: &[Point], operation: Operation) -> (Vec<Point>, usize) {
    let mut out = Vec::new();
    let mut left_out = 0;
    for (at, x, y) in align(a, b) {
        let value = match operation {
            Operation::Add => Some(x + y),
            Operation::Subtract => Some(x - y),
            Operation::Multiply => Some(x * y),
            Operation::Divide if y != 0.0 => Some(x / y),
            Operation::Divide => None,
        };
        match value {
            Some(value) => out.push((at, value)),
            None => left_out += 1,
        }
    }
    (out, left_out)
}

/// Which values a filter keeps.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Keep {
    Above,
    Below,
    AtLeast,
    AtMost,
}

/// The values that meet the condition.
pub fn filter(points: &[Point], keep: Keep, threshold: f64) -> Vec<Point> {
    points
        .iter()
        .copied()
        .filter(|(_, v)| match keep {
            Keep::Above => *v > threshold,
            Keep::Below => *v < threshold,
            Keep::AtLeast => *v >= threshold,
            Keep::AtMost => *v <= threshold,
        })
        .collect()
}

/// A series' values summed up as one number: as they stand (the first summaries), or as the
/// path of a price or a level (the rest), whose returns are the changes from each value to
/// the next.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Summary {
    Last,
    First,
    Mean,
    Stdev,
    Min,
    Max,
    Count,
    /// From the first value to the last, as a fraction: 0.25 is up a quarter.
    TotalReturn,
    /// The growth a year that would take the first value to the last over the dates between.
    Cagr,
    /// The largest fall from a high to a later low, as a fraction: 0.3 is down 30%.
    MaxDrawdown,
    /// How much the returns swing, as a year's standard deviation.
    Volatility,
    /// The returns' mean over their swings, for a year, against nothing earned without risk.
    Sharpe,
    /// The same against their swings below nothing only.
    Sortino,
    /// The loss of one period not exceeded nineteen times in twenty, as a fraction.
    ValueAtRisk95,
    /// The average loss of one period in the worst twentieth, as a fraction.
    ExpectedShortfall95,
}

/// The series summed up as one number; refused for a series with too few values for it, or
/// values the measure is not defined for (a path through zero or below for a drawdown or a
/// growth rate). `periods_per_year` is how many values a year holds, for measures for a year;
/// `risk_free` the rate a year earned without risk, as a fraction, that the Sharpe and
/// Sortino ratios measure against (0 for nothing).
pub fn summarize(points: &[Point], summary: Summary, periods_per_year: f64, risk_free: f64) -> Result<f64> {
    let values: Vec<f64> = points.iter().map(|p| p.1).collect();
    if values.is_empty() && summary != Summary::Count {
        return Err(bad("the series has no values to sum up"));
    }
    let returns = || crate::stats::simple_returns(&values);
    // The rate earned without risk for one period, compounded to a year's.
    let per_period = (1.0 + risk_free).powf(1.0 / periods_per_year) - 1.0;
    Ok(match summary {
        Summary::Last => values[values.len() - 1],
        Summary::First => values[0],
        Summary::Mean => crate::stats::mean(&values)?,
        Summary::Stdev => crate::stats::stdev(&values)?,
        Summary::Min => values.iter().copied().fold(f64::INFINITY, f64::min),
        Summary::Max => values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        Summary::Count => values.len() as f64,
        Summary::TotalReturn => {
            if values[0] == 0.0 {
                return Err(bad("a return from a first value of zero is not defined"));
            }
            values[values.len() - 1] / values[0] - 1.0
        }
        Summary::Cagr => {
            let (first, last) = (points[0], points[points.len() - 1]);
            let years = last.0.since(first.0).as_days_f64() / 365.25;
            if years <= 0.0 || first.1 <= 0.0 || last.1 <= 0.0 {
                return Err(bad("a growth rate needs positive first and last values, a while apart"));
            }
            (last.1 / first.1).powf(1.0 / years) - 1.0
        }
        Summary::MaxDrawdown => crate::stats::max_drawdown(&values)?.depth,
        Summary::Volatility => crate::stats::annualized_volatility(&returns()?, periods_per_year)?,
        Summary::Sharpe => crate::stats::sharpe(&returns()?, per_period, periods_per_year)?,
        Summary::Sortino => crate::stats::sortino(&returns()?, per_period, periods_per_year)?,
        Summary::ValueAtRisk95 => crate::stats::value_at_risk(&returns()?, 0.95)?,
        Summary::ExpectedShortfall95 => crate::stats::expected_shortfall(&returns()?, 0.95)?,
    })
}

/// A series described.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Description {
    pub count: usize,
    pub from: Timestamp,
    pub to: Timestamp,
    pub first: f64,
    pub last: f64,
    pub mean: f64,
    pub sd: Option<f64>,
    pub min: f64,
    pub max: f64,
}

/// A series described; refused for one with no values.
pub fn describe(points: &[Point]) -> Result<Description> {
    let (Some(first), Some(last)) = (points.first(), points.last()) else { return Err(bad("the series has no values to describe")) };
    let values: Vec<f64> = points.iter().map(|p| p.1).collect();
    Ok(Description {
        count: values.len(),
        from: first.0,
        to: last.0,
        first: first.1,
        last: last.1,
        mean: crate::stats::mean(&values)?,
        sd: if values.len() > 1 { Some(crate::stats::stdev(&values)?) } else { None },
        min: values.iter().copied().fold(f64::INFINITY, f64::min),
        max: values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    })
}

/// How much one series moves with another: the line through their values at the instants
/// both have, `y = alpha + beta x`, how much of `y`'s spread it accounts for, and on how many.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Exposure {
    pub beta: f64,
    pub alpha: f64,
    pub r_squared: f64,
    pub n: usize,
}

/// How much `y` moves with `x`; refused with fewer than three instants in common, or when
/// `x` does not move.
pub fn exposure(y: &[Point], x: &[Point]) -> Result<Exposure> {
    let pairs = align(x, y);
    if pairs.len() < 3 {
        return Err(bad(format!("the two series have {} instants in common; at least three are needed", pairs.len())));
    }
    let n = pairs.len() as f64;
    let mx = pairs.iter().map(|p| p.1).sum::<f64>() / n;
    let my = pairs.iter().map(|p| p.2).sum::<f64>() / n;
    let sxx = pairs.iter().map(|p| (p.1 - mx).powi(2)).sum::<f64>();
    let sxy = pairs.iter().map(|p| (p.1 - mx) * (p.2 - my)).sum::<f64>();
    let syy = pairs.iter().map(|p| (p.2 - my).powi(2)).sum::<f64>();
    if sxx == 0.0 {
        return Err(bad("the series it is measured against does not move"));
    }
    let beta = sxy / sxx;
    Ok(Exposure { beta, alpha: my - beta * mx, r_squared: if syy == 0.0 { 1.0 } else { (sxy * sxy) / (sxx * syy) }, n: pairs.len() })
}

/// How a rate series is written.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RateIn {
    /// 5.2 for 5.2% a year, as most sources publish yields.
    Percent,
    /// 0.052 for the same.
    Fraction,
}

/// A price path with what a rate earned without risk taken out, on the dates both have: it
/// starts where the path does, and from each such date to the next grows by the path's
/// return less what the rate, as it stood at the earlier date, earned over the days between.
/// Refused with fewer than two dates in common or a path at zero or below.
pub fn excess(path: &[Point], rate: &[Point], rate_in: RateIn) -> Result<Vec<Point>> {
    let pairs = align(path, rate);
    if pairs.len() < 2 {
        return Err(bad(format!("the path and the rate have {} dates in common; at least two are needed", pairs.len())));
    }
    if pairs.iter().any(|(_, p, _)| *p <= 0.0) {
        return Err(bad("a path has what a rate earned taken out of it as a price: every value above zero"));
    }
    let scale = match rate_in {
        RateIn::Percent => 0.01,
        RateIn::Fraction => 1.0,
    };
    let mut value = pairs[0].1;
    let mut out = vec![(pairs[0].0, value)];
    for w in pairs.windows(2) {
        let years = w[1].0.since(w[0].0).as_days_f64() / 365.25;
        let earned = (1.0 + w[0].2 * scale).powf(years) - 1.0;
        value *= 1.0 + (w[1].1 / w[0].1 - 1.0) - earned;
        out.push((w[1].0, value));
    }
    Ok(out)
}

/// A price path measured against a benchmark, on the dates both have.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Relative {
    /// How many dates both have.
    pub n: usize,
    /// The path's total return less the benchmark's, as a fraction.
    pub excess_return: f64,
    /// How much the difference of their returns swings, for a year.
    pub tracking_error: f64,
    /// The difference of their returns, on average, over its swings, for a year.
    pub information_ratio: Option<f64>,
    /// How much the path's returns move with the benchmark's, and how closely.
    pub beta: f64,
    pub correlation: f64,
    /// Of the benchmark's rises, how much the path rose with it on average, as a fraction of
    /// the benchmark's; and of its falls; none where it had none.
    pub up_capture: Option<f64>,
    pub down_capture: Option<f64>,
}

/// `path` measured against `benchmark` on the dates both have, their returns the changes from
/// each such date to the next; refused with fewer than four dates in common, values of zero
/// or below, or a benchmark whose returns do not move. `periods_per_year` is how many dates
/// a year holds.
pub fn relative(path: &[Point], benchmark: &[Point], periods_per_year: f64) -> Result<Relative> {
    let pairs = align(path, benchmark);
    if pairs.len() < 4 {
        return Err(bad(format!("the path and its benchmark have {} dates in common; at least four are needed", pairs.len())));
    }
    if pairs.iter().any(|(_, p, b)| *p <= 0.0 || *b <= 0.0) {
        return Err(bad("a path and its benchmark are measured as prices: every value above zero"));
    }
    let returns: Vec<(f64, f64)> = pairs.windows(2).map(|w| (w[1].1 / w[0].1 - 1.0, w[1].2 / w[0].2 - 1.0)).collect();
    let (rp, rb): (Vec<f64>, Vec<f64>) = returns.iter().copied().unzip();
    let differences: Vec<f64> = returns.iter().map(|(p, b)| p - b).collect();
    let sd_difference = crate::stats::stdev(&differences)?;
    let variance_b = crate::stats::variance(&rb)?;
    if variance_b == 0.0 {
        return Err(bad("the benchmark's returns do not move"));
    }
    let capture = |rising: bool| {
        let chosen: Vec<&(f64, f64)> = returns.iter().filter(|(_, b)| if rising { *b > 0.0 } else { *b < 0.0 }).collect();
        let mean_b = chosen.iter().map(|(_, b)| b).sum::<f64>() / chosen.len().max(1) as f64;
        (!chosen.is_empty() && mean_b != 0.0).then(|| chosen.iter().map(|(p, _)| p).sum::<f64>() / chosen.len() as f64 / mean_b)
    };
    let (first, last) = (pairs[0], pairs[pairs.len() - 1]);
    Ok(Relative {
        n: pairs.len(),
        excess_return: (last.1 / first.1 - 1.0) - (last.2 / first.2 - 1.0),
        tracking_error: sd_difference * periods_per_year.sqrt(),
        information_ratio: (sd_difference > 0.0).then(|| crate::stats::mean(&differences).map(|m| m / sd_difference * periods_per_year.sqrt())).transpose()?,
        beta: crate::stats::covariance(&rp, &rb)? / variance_b,
        correlation: crate::stats::correlation(&rp, &rb)?,
        up_capture: capture(true),
        down_capture: capture(false),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(d: u32) -> Timestamp {
        Timestamp::from_ymd(2030, 1, d).unwrap()
    }

    fn series(values: &[f64]) -> Vec<Point> {
        values.iter().enumerate().map(|(i, v)| (day(i as u32 + 1), *v)).collect()
    }

    #[test]
    fn changes_lag_and_windows() {
        let s = series(&[100.0, 110.0, 99.0, 0.0, 5.0]);
        let (pct, left) = change(&s, ChangeHow::Percent, 1).unwrap();
        assert_eq!((pct.iter().map(|p| (p.1 * 100.0).round() / 100.0).collect::<Vec<_>>(), left), (vec![10.0, -10.0, -100.0], 1), "from zero has no size");
        assert_eq!(change(&s, ChangeHow::Difference, 2).unwrap().0.iter().map(|p| p.1).collect::<Vec<_>>(), vec![-1.0, -110.0, -94.0]);
        assert_eq!(lag(&s, 1).iter().map(|p| (p.0, p.1)).collect::<Vec<_>>(), vec![(day(2), 100.0), (day(3), 110.0), (day(4), 99.0), (day(5), 0.0)]);
        let (mean, _) = rolling(&series(&[1.0, 2.0, 3.0, 4.0]), 2, Statistic::Mean, 252.0).unwrap();
        assert_eq!(mean, vec![(day(2), 1.5), (day(3), 2.5), (day(4), 3.5)]);
        let (z, left) = rolling(&series(&[1.0, 1.0, 1.0, 3.0]), 3, Statistic::Zscore, 252.0).unwrap();
        assert_eq!((z.len(), left), (1, 1), "a flat window has no z-score");
        assert!(rolling(&s, 1, Statistic::Sd, 252.0).is_err());
    }

    #[test]
    fn two_series_on_the_instants_both_have() {
        let a = series(&[1.0, 2.0, 3.0, 4.0]);
        let b: Vec<Point> = vec![(day(2), 10.0), (day(4), 0.0), (day(9), 1.0)];
        assert_eq!(combine(&a, &b, Operation::Add).0, vec![(day(2), 12.0), (day(4), 4.0)]);
        assert_eq!(combine(&a, &b, Operation::Divide), (vec![(day(2), 0.2)], 1));
        let y = series(&[3.0, 5.0, 7.0, 9.0]);
        let e = exposure(&y, &a).unwrap();
        assert_eq!((e.beta, e.alpha, e.r_squared, e.n), (2.0, 1.0, 1.0, 4));
        assert!(exposure(&y, &series(&[1.0, 1.0, 1.0, 1.0])).is_err());
    }

    #[test]
    fn a_price_path_is_measured_as_the_quant_library_measures_it() {
        let day = |n: i64| Timestamp::from_ymd(2030, 1, 1).unwrap().checked_add(mos_core::time::Duration::from_days(n)).unwrap();
        // 100, up to 120, down to 90, back to 110: returns +20%, -25%, +22.2…%.
        let path: Vec<Point> = [100.0, 120.0, 90.0, 110.0].iter().enumerate().map(|(n, v)| (day(n as i64), *v)).collect();
        let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
        assert!(near(summarize(&path, Summary::TotalReturn, 252.0, 0.0).unwrap(), 0.10));
        assert!(near(summarize(&path, Summary::MaxDrawdown, 252.0, 0.0).unwrap(), 0.25), "from 120 down to 90");
        let returns = [0.2, -0.25, 110.0 / 90.0 - 1.0];
        let mean = returns.iter().sum::<f64>() / 3.0;
        let sd = (returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / 2.0).sqrt();
        assert!(near(summarize(&path, Summary::Volatility, 252.0, 0.0).unwrap(), sd * 252f64.sqrt()));
        assert!(near(summarize(&path, Summary::Sharpe, 252.0, 0.0).unwrap(), mean / sd * 252f64.sqrt()));
        // Worst of the three returns is the loss nineteen times in twenty, near enough: the 5% quantile.
        assert!(summarize(&path, Summary::ValueAtRisk95, 252.0, 0.0).unwrap() > 0.2);
        assert!(near(summarize(&path, Summary::ExpectedShortfall95, 252.0, 0.0).unwrap(), 0.25));
        // Doubling over two years is about 41% a year.
        let doubled = vec![(day(0), 50.0), (day(730), 100.0)];
        assert!((summarize(&doubled, Summary::Cagr, 252.0, 0.0).unwrap() - (2f64.powf(365.25 / 730.0) - 1.0)).abs() < 1e-9);
        // A path through zero has no drawdown or growth rate.
        let through = vec![(day(0), 10.0), (day(1), 0.0), (day(2), 5.0)];
        assert!(summarize(&through, Summary::MaxDrawdown, 252.0, 0.0).is_err());
        assert!(summarize(&[(day(0), 0.0), (day(9), 5.0)], Summary::Cagr, 252.0, 0.0).is_err());
    }

    #[test]
    fn what_a_rate_earned_is_taken_out_of_a_path() {
        // A path flat for a year, at a rate of 5% a year: it earns 5% less than the rate gives.
        let year = Timestamp::from_ymd(2031, 1, 1).unwrap();
        let start = Timestamp::from_ymd(2030, 1, 1).unwrap();
        let path = vec![(start, 100.0), (year, 100.0)];
        let rate = vec![(start, 5.0), (year, 5.0)];
        let out = excess(&path, &rate, RateIn::Percent).unwrap();
        let years = year.since(start).as_days_f64() / 365.25;
        assert!((out[1].1 - 100.0 * (1.0 - (1.05f64.powf(years) - 1.0))).abs() < 1e-9, "{out:?}");
        // The same rate written as a fraction gives the same.
        assert_eq!(excess(&path, &[(start, 0.05), (year, 0.05)], RateIn::Fraction).unwrap(), out);
        // A path earning exactly the rate is flat beyond it.
        let earning = vec![(start, 100.0), (year, 100.0 * 1.05f64.powf(years))];
        assert!((excess(&earning, &rate, RateIn::Percent).unwrap()[1].1 - 100.0).abs() < 1e-9);
        assert!(excess(&path[..1], &rate, RateIn::Percent).is_err());
    }

    #[test]
    fn a_sharpe_ratio_is_measured_against_a_rate() {
        let day = |n: i64| Timestamp::from_ymd(2030, 1, 1).unwrap().checked_add(mos_core::time::Duration::from_days(n)).unwrap();
        let path: Vec<Point> = [100.0, 102.0, 101.0, 104.0].iter().enumerate().map(|(n, v)| (day(n as i64), *v)).collect();
        let returns = [0.02, 101.0 / 102.0 - 1.0, 104.0 / 101.0 - 1.0];
        let rf = 1.0252f64.powf(1.0 / 252.0) - 1.0;
        let excess: Vec<f64> = returns.iter().map(|r| r - rf).collect();
        let mean = excess.iter().sum::<f64>() / 3.0;
        let sd = (excess.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / 2.0).sqrt();
        assert!((summarize(&path, Summary::Sharpe, 252.0, 0.0252).unwrap() - mean / sd * 252f64.sqrt()).abs() < 1e-9);
        assert!(summarize(&path, Summary::Sharpe, 252.0, 0.0252).unwrap() < summarize(&path, Summary::Sharpe, 252.0, 0.0).unwrap(), "less, against a rate");
    }

    #[test]
    fn a_path_is_measured_against_its_benchmark() {
        // The benchmark: 100, 110, 99, 108.9 (+10%, -10%, +10%). The path moves twice as much
        // each time: +20%, -20%, +20%.
        let bench = series(&[100.0, 110.0, 99.0, 108.9]);
        let path = series(&[100.0, 120.0, 96.0, 115.2]);
        let r = relative(&path, &bench, 252.0).unwrap();
        let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
        assert_eq!(r.n, 4);
        assert!(near(r.excess_return, 0.152 - 0.089), "{r:?}");
        assert!(near(r.beta, 2.0) && near(r.correlation, 1.0), "{r:?}");
        // The difference of returns is +10%, -10%, +10%: as much as the benchmark's own returns.
        let d = [0.1, -0.1, 0.1];
        let m = d.iter().sum::<f64>() / 3.0;
        let sd = (d.iter().map(|x| (x - m).powi(2)).sum::<f64>() / 2.0).sqrt();
        assert!(near(r.tracking_error, sd * 252f64.sqrt()) && near(r.information_ratio.unwrap(), m / sd * 252f64.sqrt()));
        // It rose twice as much when the benchmark rose, and fell twice as much when it fell.
        assert!(near(r.up_capture.unwrap(), 2.0) && near(r.down_capture.unwrap(), 2.0));
        // Too few dates in common, and a benchmark that does not move, are refused.
        assert!(relative(&series(&[1.0, 2.0, 3.0]), &series(&[1.0, 2.0, 3.0]), 252.0).is_err());
        assert!(relative(&path, &series(&[100.0, 100.0, 100.0, 100.0]), 252.0).is_err());
    }

    #[test]
    fn periods_summaries_and_filters() {
        let s: Vec<Point> = vec![(Timestamp::from_ymd(2030, 1, 30).unwrap(), 1.0), (Timestamp::from_ymd(2030, 1, 31).unwrap(), 3.0), (Timestamp::from_ymd(2030, 2, 2).unwrap(), 5.0)];
        assert_eq!(resample(&s, Every::Monthly, Aggregate::Last), vec![(Timestamp::from_ymd(2030, 1, 31).unwrap(), 3.0), (Timestamp::from_ymd(2030, 2, 2).unwrap(), 5.0)]);
        assert_eq!(resample(&s, Every::Monthly, Aggregate::Mean)[0].1, 2.0);
        assert_eq!(resample(&s, Every::Annual, Aggregate::Sum), vec![(Timestamp::from_ymd(2030, 2, 2).unwrap(), 9.0)]);
        assert_eq!(summarize(&s, Summary::Max, 252.0, 0.0).unwrap(), 5.0);
        assert_eq!(summarize(&[], Summary::Count, 252.0, 0.0).unwrap(), 0.0);
        assert!(summarize(&[], Summary::Last, 252.0, 0.0).is_err());
        assert_eq!(filter(&s, Keep::AtLeast, 3.0).len(), 2);
        let d = describe(&s).unwrap();
        assert_eq!((d.count, d.first, d.last, d.min, d.max), (3, 1.0, 5.0, 1.0, 5.0));
    }
}
