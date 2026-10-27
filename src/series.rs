//! Steps over time series: resample, change, lag, rolling statistics, align, combine,
//! filter, summarize, describe, regress and correlate. Each is MarketOS's own pipeline step
//! (`datacartridge_quant::steps`, `datacartridge_quant::stats`, in crates/), over series as the
//! time-series media carries them: `[{time, value}]`, oldest first.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use capdag::{async_trait, DryContext, Op, OpError, OpResult, Request, WetContext, WET_KEY_REQUEST};
use datacartridge_core::error::Error;
use datacartridge_core::time::Timestamp;
use datacartridge_quant::steps::{self, Aggregate, ChangeHow, Every, Keep, Operation, Point, Statistic, Summary};
use serde_json::{json, Value};

use crate::input_arg_error;

pub const TIME_SERIES: &str = "media:fmt=json;list;record;time-series";
pub const RIGHT_SERIES: &str = "media:fmt=json;list;record;right-series";
pub const REGRESSOR_SERIES: &str = "media:fmt=json;list;record;regressor-series";
pub const RESIDUALS: &str = "media:fmt=json;list;record;regression-residuals;time-series";
pub const DROPPED: &str = "media:filtered-out;fmt=json;list;record";
const EVERY: &str = "media:enc=utf-8;resample-every";
const AGGREGATE: &str = "media:enc=utf-8;resample-aggregate";
const CHANGE_HOW: &str = "media:change-how;enc=utf-8";
const CHANGE_OVER: &str = "media:change-over;integer;numeric";
const LAG_BY: &str = "media:integer;lag-by;numeric";
const WINDOW: &str = "media:integer;numeric;rolling-window";
const STATISTIC: &str = "media:enc=utf-8;rolling-statistic-kind";
const PERIODS_PER_YEAR: &str = "media:numeric;periods-per-year";
const OPERATION: &str = "media:combine-operation;enc=utf-8";
const COMPARISON: &str = "media:comparison;enc=utf-8";
const THRESHOLD: &str = "media:numeric;threshold";
const SUMMARY_KIND: &str = "media:enc=utf-8;summary-kind";

/// A step's refusal (too few values, values it is not defined for): decided by what was given.
fn refused(error: Error, arg: &str) -> OpError {
    input_arg_error("NOT_DEFINED", error.to_string(), arg)
}

/// A series as the time-series media carries it; one that is not that, or whose instants
/// are out of order or repeat, is refused.
pub fn parse(bytes: &[u8], arg: &str) -> Result<Vec<Point>, OpError> {
    let bad = |why: String| input_arg_error("NOT_A_SERIES", why, arg);
    let items: Vec<Value> = serde_json::from_slice(bytes).map_err(|e| bad(format!("the series is not a JSON array: {e}")))?;
    let mut points: Vec<Point> = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        let time = item.get("time").and_then(Value::as_str).ok_or_else(|| bad(format!("item {i} has no time")))?;
        let time = Timestamp::parse(time).map_err(|_| bad(format!("item {i}: '{time}' is not an RFC 3339 time")))?;
        let value = item.get("value").and_then(Value::as_f64).filter(|v| v.is_finite()).ok_or_else(|| bad(format!("item {i} has no finite value")))?;
        if let Some((previous, _)) = points.last() {
            if time <= *previous {
                return Err(bad(format!("item {i} is at {time}, not after the item before it ({previous})")));
            }
        }
        points.push((time, value));
    }
    Ok(points)
}

/// A series as the time-series media carries it.
pub fn to_json(points: &[Point]) -> Value {
    Value::Array(points.iter().map(|(time, value)| json!({ "time": time.to_string(), "value": value })).collect())
}

/// A choice given by its name, as the option's media allows it.
fn choice<T: serde::de::DeserializeOwned>(value: &str, arg: &str) -> Result<T, OpError> {
    serde_json::from_value(json!(value)).map_err(|_| input_arg_error("NOT_A_CHOICE", format!("'{value}' is not one of the choices of {arg}"), arg))
}

/// A whole number of at least 1.
fn count(value: f64, arg: &str) -> Result<usize, OpError> {
    if value < 1.0 || value.fract() != 0.0 {
        return Err(input_arg_error("NOT_A_COUNT", format!("{value} is not a whole number of at least 1"), arg));
    }
    Ok(value as usize)
}

/// The series resampled to a calendar period (MarketOS's `steps::resample`).
pub fn resample(points: &[Point], every: &str, aggregate: &str) -> Result<Vec<Point>, OpError> {
    let every: Every = choice(every, EVERY)?;
    let aggregate: Aggregate = choice(aggregate, AGGREGATE)?;
    Ok(steps::resample(points, every, aggregate))
}

/// The series' changes over so many observations (MarketOS's `steps::change`).
pub fn change(points: &[Point], how: &str, over: f64) -> Result<Vec<Point>, OpError> {
    let how: ChangeHow = choice(how, CHANGE_HOW)?;
    steps::change(points, how, count(over, CHANGE_OVER)?).map(|(points, _)| points).map_err(|e| refused(e, TIME_SERIES))
}

/// The series moved later by so many observations (MarketOS's `steps::lag`).
pub fn lag(points: &[Point], by: f64) -> Result<Vec<Point>, OpError> {
    Ok(steps::lag(points, count(by, LAG_BY)?))
}

/// A statistic over a moving window (MarketOS's `steps::rolling`): mean, sd, zscore, or vol —
/// the window's standard deviation scaled to a year by `periods_per_year`, so of the series
/// as given and in its units (for a series of changes, its realised volatility).
///
/// `periods_per_year` belongs to vol and to nothing else: vol without it is refused, naming
/// it, and it given to another statistic is refused rather than ignored.
pub fn rolling(points: &[Point], window: f64, statistic: &str, periods_per_year: Option<f64>) -> Result<Vec<Point>, OpError> {
    let statistic: Statistic = choice(statistic, STATISTIC)?;
    let scale = match (statistic, periods_per_year) {
        (Statistic::Vol, None) => {
            return Err(input_arg_error("INPUT_REQUIRED", "vol is the window's standard deviation scaled to a year, and needs periods-per-year: how many observations a year holds".to_string(), PERIODS_PER_YEAR));
        }
        (Statistic::Vol, Some(periods)) if periods > 0.0 => periods,
        (Statistic::Vol, Some(periods)) => {
            return Err(input_arg_error("NOT_DEFINED", format!("{periods} is not a number of observations in a year"), PERIODS_PER_YEAR));
        }
        (_, Some(_)) => {
            return Err(input_arg_error("NOT_DEFINED", "periods-per-year scales vol to a year, and mean, sd and zscore are not scaled".to_string(), PERIODS_PER_YEAR));
        }
        (_, None) => 1.0,
    };
    let window = count(window, WINDOW)?;
    steps::rolling(points, window, statistic, scale).map(|(points, _)| points).map_err(|e| refused(e, TIME_SERIES))
}

/// The instants every series has, and each series' value then, in the order given.
fn shared(series: &[Vec<Point>]) -> Vec<(Timestamp, Vec<f64>)> {
    let maps: Vec<BTreeMap<Timestamp, f64>> = series.iter().map(|s| s.iter().copied().collect()).collect();
    let Some(first) = maps.first() else { return Vec::new() };
    first.keys().filter(|t| maps.iter().all(|m| m.contains_key(t))).map(|t| (*t, maps.iter().map(|m| m[t]).collect())).collect()
}

/// Several series side by side on the instants they all have.
pub fn align(series: &[Vec<Point>]) -> Result<Value, OpError> {
    if series.len() < 2 {
        return Err(input_arg_error("TOO_FEW_SERIES", format!("aligning takes two series or more, and {} was given", series.len()), TIME_SERIES));
    }
    Ok(Value::Array(shared(series).into_iter().map(|(time, values)| json!({ "time": time.to_string(), "values": values })).collect()))
}

/// Two series combined at the instants both have (MarketOS's `steps::combine`).
pub fn combine(left: &[Point], right: &[Point], operation: &str) -> Result<Vec<Point>, OpError> {
    let operation: Operation = choice(operation, OPERATION)?;
    Ok(steps::combine(left, right, operation).0)
}

/// The observations that pass a comparison with a threshold, and those that do not
/// (MarketOS's `steps::filter` and what it leaves).
pub fn filter(points: &[Point], keep: &str, threshold: f64) -> Result<(Vec<Point>, Vec<Point>), OpError> {
    let keep: Keep = choice(keep, COMPARISON)?;
    let kept = steps::filter(points, keep, threshold);
    let kept_at: BTreeSet<Timestamp> = kept.iter().map(|(t, _)| *t).collect();
    let dropped = points.iter().filter(|(t, _)| !kept_at.contains(t)).copied().collect();
    Ok((kept, dropped))
}

/// One summary of a series (MarketOS's `steps::summarize`): last, first, mean, stdev, min,
/// max or count.
pub fn summarize(points: &[Point], summary: &str) -> Result<Value, OpError> {
    let kind: Summary = choice(summary, SUMMARY_KIND)?;
    if !matches!(kind, Summary::Last | Summary::First | Summary::Mean | Summary::Stdev | Summary::Min | Summary::Max | Summary::Count) {
        return Err(input_arg_error("NOT_A_CHOICE", format!("'{summary}' is a measure of a price path, marketcartridge's measure-returns; this takes last, first, mean, stdev, min, max or count"), SUMMARY_KIND));
    }
    let value = steps::summarize(points, kind, 1.0, 0.0).map_err(|e| refused(e, TIME_SERIES))?;
    Ok(json!({ "summary": summary, "value": value }))
}

/// What a series holds (MarketOS's `steps::describe`).
pub fn describe(points: &[Point]) -> Result<Value, OpError> {
    let d = steps::describe(points).map_err(|e| refused(e, TIME_SERIES))?;
    Ok(json!({ "count": d.count, "from": d.from.to_string(), "to": d.to.to_string(), "first": d.first, "last": d.last, "mean": d.mean, "sd": d.sd, "min": d.min, "max": d.max }))
}

/// One series fitted on others by ordinary least squares at the instants they all have
/// (MarketOS's `stats::ols`), and what the fit leaves at each of them.
pub fn regress(y: &[Point], xs: &[Vec<Point>]) -> Result<(Value, Vec<Point>), OpError> {
    if xs.is_empty() {
        return Err(input_arg_error("REGRESSOR_REQUIRED", "a regression takes one regressor or more (x)".to_string(), REGRESSOR_SERIES));
    }
    let mut all = vec![y.to_vec()];
    all.extend(xs.iter().cloned());
    let rows = shared(&all);
    let ys: Vec<f64> = rows.iter().map(|(_, v)| v[0]).collect();
    let regressors: Vec<Vec<f64>> = (1..all.len()).map(|j| rows.iter().map(|(_, v)| v[j]).collect()).collect();
    let fit = datacartridge_quant::stats::ols(&ys, &regressors).map_err(|e| refused(e, TIME_SERIES))?;
    let residuals = rows.iter().map(|(time, v)| (*time, v[0] - fit.alpha - fit.betas.iter().zip(&v[1..]).map(|(b, x)| b * x).sum::<f64>())).collect();
    let value = json!({ "alpha": fit.alpha, "betas": fit.betas, "r_squared": fit.r_squared, "residual_std": fit.residual_std, "observations": fit.observations });
    Ok((value, residuals))
}

/// How closely two series move together at the instants both have (MarketOS's
/// `stats::correlation`).
pub fn correlate(left: &[Point], right: &[Point]) -> Result<Value, OpError> {
    let rows = shared(&[left.to_vec(), right.to_vec()]);
    let (a, b): (Vec<f64>, Vec<f64>) = rows.iter().map(|(_, v)| (v[0], v[1])).unzip();
    if rows.len() < 3 {
        return Err(input_arg_error("NOT_DEFINED", format!("a correlation needs three shared instants, and the series share {}", rows.len()), RIGHT_SERIES));
    }
    let correlation = datacartridge_quant::stats::correlation(&a, &b).map_err(|e| refused(e, TIME_SERIES))?;
    Ok(json!({ "correlation": correlation, "n": rows.len() }))
}

// --- handlers --------------------------------------------------------------------------

/// The arguments a request gave, by media URN: each stream's items, in the order they came.
struct Args(BTreeMap<String, Vec<Vec<u8>>>);

impl Args {
    async fn read(req: &Request, sequences: &[&str]) -> Result<Self, OpError> {
        let mut input = req.take_input().map_err(|e| OpError::ExecutionFailed(e.to_string()))?;
        let mut by_urn: BTreeMap<String, Vec<Vec<u8>>> = BTreeMap::new();
        while let Some(stream) = input.recv().await {
            let stream = stream.map_err(|e| OpError::ExecutionFailed(format!("Stream error: {e}")))?;
            let urn = stream.media_urn().to_string();
            let values = if sequences.contains(&urn.as_str()) {
                stream.collect_items().await.map_err(|e| OpError::ExecutionFailed(e.to_string()))?.into_iter().map(|(bytes, _)| bytes).collect()
            } else {
                vec![stream.collect_bytes().await.map_err(|e| OpError::ExecutionFailed(e.to_string()))?]
            };
            by_urn.entry(urn).or_default().extend(values);
        }
        Ok(Self(by_urn))
    }

    fn all(&self, urn: &str) -> &[Vec<u8>] {
        self.0.get(urn).map(Vec::as_slice).unwrap_or_default()
    }

    fn one(&self, urn: &str) -> Option<&[u8]> {
        self.0.get(urn).and_then(|v| v.first()).map(Vec::as_slice)
    }

    fn text(&self, urn: &str) -> Result<Option<String>, OpError> {
        self.one(urn).map(|b| std::str::from_utf8(b).map(|s| s.trim().to_string()).map_err(|_| input_arg_error("NOT_TEXT", format!("{urn} is not UTF-8 text"), urn))).transpose()
    }

    fn required_text(&self, urn: &str, what: &str) -> Result<String, OpError> {
        self.text(urn)?.ok_or_else(|| input_arg_error("INPUT_REQUIRED", format!("{what} is required"), urn))
    }

    fn number(&self, urn: &str, what: &str, default: Option<f64>) -> Result<f64, OpError> {
        match self.text(urn)? {
            Some(text) => text.parse::<f64>().ok().filter(|n| n.is_finite()).ok_or_else(|| input_arg_error("NOT_A_NUMBER", format!("'{text}' is not a number"), urn)),
            None => default.ok_or_else(|| input_arg_error("INPUT_REQUIRED", format!("{what} is required"), urn)),
        }
    }

    /// A number given, or `None` when the argument was not given at all.
    fn optional_number(&self, urn: &str) -> Result<Option<f64>, OpError> {
        self.text(urn)?.map(|text| text.parse::<f64>().ok().filter(|n| n.is_finite()).ok_or_else(|| input_arg_error("NOT_A_NUMBER", format!("'{text}' is not a number"), urn))).transpose()
    }

    fn series(&self, urn: &str, what: &str) -> Result<Vec<Point>, OpError> {
        parse(self.one(urn).ok_or_else(|| input_arg_error("INPUT_REQUIRED", format!("{what} is required"), urn))?, urn)
    }
}

async fn emit(output: &capdag::OutputStream, value: &Value) -> Result<(), OpError> {
    output.start(false, None).map_err(|e| OpError::ExecutionFailed(e.to_string()))?;
    output.emit_cbor(&ciborium::Value::Text(value.to_string())).await.map_err(|e| OpError::ExecutionFailed(e.to_string()))
}

fn request(wet: &mut WetContext) -> Result<Arc<Request>, OpError> {
    wet.get_required::<Request>(WET_KEY_REQUEST).map_err(|e| OpError::ExecutionFailed(e.to_string()))
}

/// An op that reads its arguments, computes, and writes its main output.
macro_rules! series_op {
    ($name:ident, $description:literal, [$($sequence:expr),*], |$args:ident| $compute:expr) => {
        #[derive(Default)]
        pub struct $name;

        #[async_trait]
        impl Op<()> for $name {
            async fn perform(&self, _dry: &mut DryContext, wet: &mut WetContext) -> OpResult<()> {
                let req = request(wet)?;
                let $args = Args::read(&req, &[$($sequence),*]).await?;
                let value: Value = $compute?;
                emit(req.output(), &value).await
            }

            fn metadata(&self) -> capdag::OpMetadata {
                capdag::OpMetadata::builder(stringify!($name)).description($description).build()
            }
        }
    };
}

series_op!(ResampleOp, "Resample a series to a calendar period", [], |a| resample(&a.series(TIME_SERIES, "a series")?, &a.required_text(EVERY, "every")?, &a.text(AGGREGATE)?.unwrap_or_else(|| "last".to_string())).map(|p| to_json(&p)));
series_op!(ChangeOp, "Measure how a series changes", [], |a| change(&a.series(TIME_SERIES, "a series")?, &a.text(CHANGE_HOW)?.unwrap_or_else(|| "percent".to_string()), a.number(CHANGE_OVER, "over", Some(1.0))?).map(|p| to_json(&p)));
series_op!(LagOp, "Move a series later", [], |a| lag(&a.series(TIME_SERIES, "a series")?, a.number(LAG_BY, "by", None)?).map(|p| to_json(&p)));
series_op!(RollingOp, "Take a statistic over a moving window", [], |a| rolling(&a.series(TIME_SERIES, "a series")?, a.number(WINDOW, "window", None)?, &a.required_text(STATISTIC, "statistic")?, a.optional_number(PERIODS_PER_YEAR)?).map(|p| to_json(&p)));
series_op!(AlignOp, "Line several series up", [TIME_SERIES], |a| a.all(TIME_SERIES).iter().map(|b| parse(b, TIME_SERIES)).collect::<Result<Vec<_>, _>>().and_then(|s| align(&s)));
series_op!(CombineOp, "Combine two series", [], |a| combine(&a.series(TIME_SERIES, "the left series")?, &a.series(RIGHT_SERIES, "the right series")?, &a.required_text(OPERATION, "operation")?).map(|p| to_json(&p)));
series_op!(SummarizeOp, "Sum a series up as one number", [], |a| summarize(&a.series(TIME_SERIES, "a series")?, &a.required_text(SUMMARY_KIND, "summary")?));
series_op!(DescribeOp, "Describe a series", [], |a| describe(&a.series(TIME_SERIES, "a series")?));
series_op!(CorrelateOp, "Measure how closely two series move together", [], |a| correlate(&a.series(TIME_SERIES, "a series")?, &a.series(RIGHT_SERIES, "the other series")?));

#[derive(Default)]
pub struct FilterOp;

#[async_trait]
impl Op<()> for FilterOp {
    async fn perform(&self, _dry: &mut DryContext, wet: &mut WetContext) -> OpResult<()> {
        let req = request(wet)?;
        let a = Args::read(&req, &[]).await?;
        let (kept, dropped) = filter(&a.series(TIME_SERIES, "a series")?, &a.required_text(COMPARISON, "keep")?, a.number(THRESHOLD, "threshold", None)?)?;
        emit(req.output(), &to_json(&kept)).await?;
        // The optional output fires only when something was dropped.
        if !dropped.is_empty() {
            emit(req.output_for(DROPPED).map_err(|e| OpError::ExecutionFailed(e.to_string()))?, &to_json(&dropped)).await?;
        }
        Ok(())
    }

    fn metadata(&self) -> capdag::OpMetadata {
        capdag::OpMetadata::builder("FilterOp").description("Keep the observations that pass a comparison, and give those that fail on their own").build()
    }
}

#[derive(Default)]
pub struct RegressOp;

#[async_trait]
impl Op<()> for RegressOp {
    async fn perform(&self, _dry: &mut DryContext, wet: &mut WetContext) -> OpResult<()> {
        let req = request(wet)?;
        let a = Args::read(&req, &[REGRESSOR_SERIES]).await?;
        let y = a.series(TIME_SERIES, "the dependent series")?;
        let xs = a.all(REGRESSOR_SERIES).iter().map(|b| parse(b, REGRESSOR_SERIES)).collect::<Result<Vec<_>, _>>()?;
        let (fit, residuals) = regress(&y, &xs)?;
        emit(req.output(), &fit).await?;
        emit(req.output_for(RESIDUALS).map_err(|e| OpError::ExecutionFailed(e.to_string()))?, &to_json(&residuals)).await
    }

    fn metadata(&self) -> capdag::OpMetadata {
        capdag::OpMetadata::builder("RegressOp").description("Fit one series on others by ordinary least squares").build()
    }
}

/// A handler, made new for each request.
pub type Factory = fn() -> Box<dyn Op<()>>;

/// Every series op, transform-records and the forecast caps, by the fabric alias of the cap
/// they answer.
pub fn handlers() -> Vec<(&'static str, Factory)> {
    vec![
        ("resample-series", || Box::new(ResampleOp)),
        ("change-series", || Box::new(ChangeOp)),
        ("lag-series", || Box::new(LagOp)),
        ("rolling-statistic", || Box::new(RollingOp)),
        ("align-series", || Box::new(AlignOp)),
        ("combine-series", || Box::new(CombineOp)),
        ("filter-series", || Box::new(FilterOp)),
        ("summarize-series", || Box::new(SummarizeOp)),
        ("describe-series", || Box::new(DescribeOp)),
        ("regress-series", || Box::new(RegressOp)),
        ("correlate-series", || Box::new(CorrelateOp)),
        ("transform-records", || Box::new(crate::records::TransformRecordsOp)),
        ("score-forecasts", || Box::new(crate::forecasts::ScoreForecastsOp)),
        ("try-forecasting-ways", || Box::new(crate::forecasts::TryForecastingWaysOp)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(n: i64) -> Timestamp {
        Timestamp::from_ymd(2030, 1, 1).unwrap().checked_add(datacartridge_core::time::Duration::from_days(n)).unwrap()
    }

    fn series(values: &[f64]) -> Vec<Point> {
        values.iter().enumerate().map(|(i, v)| (day(i as i64), *v)).collect()
    }

    fn code(error: OpError) -> String {
        refusal(error).0
    }

    /// A refusal's code and what it says.
    fn refusal(error: OpError) -> (String, String) {
        match error {
            OpError::Classified { code, message, .. } => (code, message),
            other => panic!("{other:?}"),
        }
    }

    // TEST305: a series is read as the time-series media carries it and written back the
    // same; one out of order, repeating an instant, or with a value that is no finite number
    // is refused as input.
    #[test]
    fn test0305_series_are_read_in_time_order() {
        let read = parse(br#"[{"time": "2030-01-01T00:00:00Z", "value": 1}, {"time": "2030-01-02T00:00:00Z", "value": 2.5}]"#, TIME_SERIES).unwrap();
        assert_eq!(read, series(&[1.0, 2.5]));
        assert_eq!(to_json(&read), json!([{"time": "2030-01-01T00:00:00Z", "value": 1.0}, {"time": "2030-01-02T00:00:00Z", "value": 2.5}]));
        for bad in [r#"[{"time": "2030-01-02T00:00:00Z", "value": 1}, {"time": "2030-01-01T00:00:00Z", "value": 2}]"#, r#"[{"time": "2030-01-01T00:00:00Z", "value": null}]"#, r#"[{"value": 1}]"#] {
            assert_eq!(code(parse(bad.as_bytes(), TIME_SERIES).unwrap_err()), "NOT_A_SERIES", "{bad}");
        }
    }

    // TEST306: resample, change, lag and rolling are MarketOS's steps: a month's last value,
    // a percent change, values moved one later, a rolling mean; a choice the option does not
    // allow is refused, and vol without the periods-per-year it is scaled by is refused,
    // naming it.
    #[test]
    fn test0306_one_series_steps() {
        let daily: Vec<Point> = (0..62).map(|i| (day(i), i as f64)).collect();
        let monthly = resample(&daily, "monthly", "last").unwrap();
        assert_eq!(monthly.iter().map(|(_, v)| *v).collect::<Vec<_>>(), vec![30.0, 58.0, 61.0]);
        let changed = change(&series(&[100.0, 110.0, 99.0]), "percent", 1.0).unwrap();
        assert!((changed[0].1 - 10.0).abs() < 1e-9 && (changed[1].1 + 10.0).abs() < 1e-9);
        assert_eq!(lag(&series(&[1.0, 2.0, 3.0]), 1.0).unwrap(), vec![(day(1), 1.0), (day(2), 2.0)]);
        assert_eq!(rolling(&series(&[1.0, 2.0, 3.0, 4.0]), 2.0, "mean", None).unwrap().iter().map(|(_, v)| *v).collect::<Vec<_>>(), vec![1.5, 2.5, 3.5]);
        assert_eq!(code(resample(&daily, "hourly", "last").unwrap_err()), "NOT_A_CHOICE");
        let (code_of, said) = refusal(rolling(&daily, 3.0, "vol", None).unwrap_err());
        assert!(code_of == "INPUT_REQUIRED" && said.contains("periods-per-year"), "vol without periods-per-year names it: {code_of} {said}");
        assert_eq!(code(lag(&daily, 0.5).unwrap_err()), "NOT_A_COUNT");
    }

    // TEST1860: vol is the window's sample standard deviation times the square root of the
    // periods in a year, of the series as given — a series of daily changes gives its realised
    // volatility. Worked here from the definition, not from the step. periods-per-year is
    // vol's alone: given to another statistic, or as no number of observations, it is refused.
    #[test]
    fn test1860_vol_is_the_windows_deviation_scaled_to_a_year() {
        let moves = series(&[0.01, -0.02, 0.015, 0.0, -0.005]);
        let got = rolling(&moves, 3.0, "vol", Some(365.0)).unwrap();
        let expected: Vec<f64> = moves
            .windows(3)
            .map(|window| {
                let mean = window.iter().map(|p| p.1).sum::<f64>() / 3.0;
                let variance = window.iter().map(|p| (p.1 - mean).powi(2)).sum::<f64>() / 2.0;
                variance.sqrt() * 365f64.sqrt()
            })
            .collect();
        assert_eq!(got.len(), 3, "one value per full window");
        for ((time, value), (want_time, want)) in got.iter().zip(moves[2..].iter().map(|p| p.0).zip(expected)) {
            assert_eq!(*time, want_time, "each value stands at its window's last instant");
            assert!((value - want).abs() < 1e-12, "{value} is not {want}");
        }
        let (code_of, said) = refusal(rolling(&moves, 3.0, "sd", Some(365.0)).unwrap_err());
        assert!(code_of == "NOT_DEFINED" && said.contains("not scaled"), "{code_of} {said}");
        let (code_of, said) = refusal(rolling(&moves, 3.0, "vol", Some(0.0)).unwrap_err());
        assert!(code_of == "NOT_DEFINED" && said.contains("not a number of observations in a year"), "{code_of} {said}");
    }

    // TEST307: series side by side, combined, correlated and regressed at the instants they
    // all have: aligning keeps only shared instants, combining divides value by value, a
    // regression of a series on two others recovers their coefficients, and its residuals
    // are what the fit leaves at each instant.
    #[test]
    fn test0307_steps_over_several_series() {
        let a = series(&[1.0, 2.0, 3.0, 4.0]);
        let b: Vec<Point> = vec![(day(1), 20.0), (day(2), 30.0), (day(3), 40.0), (day(9), 1.0)];
        assert_eq!(align(&[a.clone(), b.clone()]).unwrap(), json!([
            {"time": day(1).to_string(), "values": [2.0, 20.0]}, {"time": day(2).to_string(), "values": [3.0, 30.0]}, {"time": day(3).to_string(), "values": [4.0, 40.0]}]));
        assert_eq!(code(align(std::slice::from_ref(&a)).unwrap_err()), "TOO_FEW_SERIES");
        assert_eq!(combine(&b, &a, "divide").unwrap(), vec![(day(1), 10.0), (day(2), 10.0), (day(3), 10.0)]);
        let corr = correlate(&a, &b).unwrap();
        assert!((corr["correlation"].as_f64().unwrap() - 1.0).abs() < 1e-9 && corr["n"] == json!(3));
        let x1 = series(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        let x2 = series(&[2.0, 1.0, 4.0, 3.0, 6.0, 5.0]);
        let y: Vec<Point> = x1.iter().zip(&x2).map(|((t, p), (_, q))| (*t, 1.0 + 2.0 * p - 0.5 * q)).collect();
        let (fit, residuals) = regress(&y, &[x1, x2]).unwrap();
        let betas: Vec<f64> = fit["betas"].as_array().unwrap().iter().map(|b| b.as_f64().unwrap()).collect();
        assert!((fit["alpha"].as_f64().unwrap() - 1.0).abs() < 1e-9 && (betas[0] - 2.0).abs() < 1e-9 && (betas[1] + 0.5).abs() < 1e-9, "{fit}");
        assert!(residuals.iter().all(|(_, r)| r.abs() < 1e-9) && residuals.len() == 6);
        assert_eq!(code(regress(&y, &[]).unwrap_err()), "REGRESSOR_REQUIRED");
    }

    // TEST308: a filter keeps what passes and gives what fails apart; a summary is one of the
    // plain ones (a measure of a price path is marketcartridge's); a description holds a
    // series' count, span, first and last, mean, spread and extremes.
    #[test]
    fn test0308_filter_summarize_describe() {
        let s = series(&[1.0, 5.0, 2.0, 7.0]);
        let (kept, dropped) = filter(&s, "above", 2.0).unwrap();
        assert_eq!((kept, dropped), (vec![(day(1), 5.0), (day(3), 7.0)], vec![(day(0), 1.0), (day(2), 2.0)]));
        assert_eq!(summarize(&s, "max").unwrap(), json!({"summary": "max", "value": 7.0}));
        assert_eq!(code(summarize(&s, "sharpe").unwrap_err()), "NOT_A_CHOICE");
        let described = describe(&s).unwrap();
        assert_eq!((described["count"].clone(), described["first"].clone(), described["last"].clone(), described["min"].clone(), described["max"].clone()), (json!(4), json!(1.0), json!(7.0), json!(1.0), json!(7.0)));
        assert_eq!(described["from"], json!(day(0).to_string()));
    }
}
