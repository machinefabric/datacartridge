//! Forecasts scored, and ways of forecasting tried on a series' history: MarketOS's scoring
//! and hindcast, decided by its proved model (`Datacartridge.Forecast`, `Datacartridge.Hindcast`, generated into
//! crates/datacartridge-formal).

use std::collections::BTreeMap;
use std::sync::Arc;

use capdag::{async_trait, DryContext, Op, OpError, OpResult, Request, WetContext, WET_KEY_REQUEST};
use datacartridge_core::time::{Duration, Timestamp};
use datacartridge_formal::spec::forecast as spec;
use datacartridge_formal::spec::hindcast as trial_spec;
use datacartridge_formal::{Int, List, Nat};
use datacartridge_quant::steps::Point;
use serde_json::{json, Value};

use crate::input_arg_error;

pub const OUTCOMES: &str = "media:fmt=json;forecast-outcomes;list;record";
const COMPARISON: &str = "media:comparison;enc=utf-8";
const CHANGE_PCT: &str = "media:change-pct;numeric";
const WINDOW: &str = "media:enc=utf-8;statement-window";
const DAYS: &str = "media:horizon-days;integer;numeric";
const EVERY_DAYS: &str = "media:every-days;integer;numeric";
const STRETCHES: &str = "media:integer;numeric;stretches";

fn nat(n: u64) -> Nat {
    Nat::from(n)
}

fn small(n: Nat, what: &str) -> Result<u64, OpError> {
    u64::try_from(&n).map_err(|_| OpError::ExecutionFailed(format!("{what} out of range")))
}

/// A likelihood as a fraction, in basis points; one outside 0 to 1 is refused.
fn bps(probability: f64, i: usize) -> Result<u64, OpError> {
    if !(0.0..=1.0).contains(&probability) {
        return Err(input_arg_error("NOT_A_PROBABILITY", format!("forecast {i} gives {probability}, not a probability from 0 to 1"), OUTCOMES));
    }
    Ok((probability * 10000.0).round() as u64)
}

/// Settled forecasts scored as MarketOS scores a forecaster: the mean Brier score
/// (`Datacartridge.Forecast.brier`, `mean`), against the 0.25 that saying 50% every time earns, whether
/// it beats that (`Datacartridge.Forecast.skill` above nothing), and how the forecasts in each tenth of
/// the scale (`Datacartridge.Forecast.tenth`) came out. An empty record is refused: there is nothing
/// to score.
pub fn score(outcomes: &[u8]) -> Result<Value, OpError> {
    let items: Vec<Value> = serde_json::from_slice(outcomes).map_err(|e| input_arg_error("NOT_OUTCOMES", format!("the outcomes are not a JSON array: {e}"), OUTCOMES))?;
    let mut tried = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        let probability = item.get("probability").and_then(Value::as_f64).ok_or_else(|| input_arg_error("NOT_OUTCOMES", format!("forecast {i} has no probability"), OUTCOMES))?;
        let came_true = item.get("came_true").and_then(Value::as_bool).ok_or_else(|| input_arg_error("NOT_OUTCOMES", format!("forecast {i} does not say whether it came true"), OUTCOMES))?;
        tried.push((bps(probability, i)?, came_true));
    }
    if tried.is_empty() {
        return Err(input_arg_error("NOTHING_TO_SCORE", "no forecasts were given", OUTCOMES));
    }
    let scores: List<Nat> = tried.iter().map(|(said, came)| spec::brier(nat(*said), *came)).collect();
    let mean = spec::mean(scores).ok_or_else(|| OpError::ExecutionFailed("forecasts were given and have no mean score".to_string()))?;
    let beats_even = small(spec::skill(mean.clone()), "a skill")? > 0;
    let mut tenths: BTreeMap<u64, (u64, u64, u64)> = BTreeMap::new();
    for (said, came) in &tried {
        let group = tenths.entry(small(spec::tenth(nat(*said)), "a tenth")?).or_default();
        group.0 += 1;
        group.1 += said;
        group.2 += u64::from(*came);
    }
    Ok(json!({
        "n": tried.len(),
        "brier": small(mean, "a mean score")? as f64 / 10000.0,
        "even_brier": 0.25,
        "beats_even": beats_even,
        "calibration": tenths.into_iter().map(|(tenth, (forecasts, said, came_true))| json!({ "tenth": tenth, "forecasts": forecasts, "said": (said as f64 / forecasts as f64).round() / 10000.0, "came_true": came_true })).collect::<Vec<_>>(),
    }))
}

#[derive(Default)]
pub struct ScoreForecastsOp;

#[async_trait]
impl Op<()> for ScoreForecastsOp {
    async fn perform(&self, _dry: &mut DryContext, wet: &mut WetContext) -> OpResult<()> {
        let req: Arc<Request> = wet.get_required(WET_KEY_REQUEST).map_err(|e| OpError::ExecutionFailed(e.to_string()))?;
        let streams = req.take_input().map_err(|e| OpError::ExecutionFailed(e.to_string()))?.collect_streams().await.map_err(|e| OpError::ExecutionFailed(format!("Stream error: {e}")))?;
        let outcomes = capdag::require_stream(&streams, OUTCOMES).map_err(|_| input_arg_error("INPUT_REQUIRED", "the settled forecasts are required", OUTCOMES))?;
        let scored = score(outcomes)?;
        let output = req.output();
        output.start(false, None).map_err(|e| OpError::ExecutionFailed(e.to_string()))?;
        output.emit_cbor(&ciborium::Value::Text(scored.to_string())).await.map_err(|e| OpError::ExecutionFailed(e.to_string()))
    }

    fn metadata(&self) -> capdag::OpMetadata {
        capdag::OpMetadata::builder("ScoreForecastsOp").description("Score settled forecasts: Brier score, against saying 50% every time, and calibration by tenths").build()
    }
}


// --- trying ways of forecasting on a series' history (MarketOS's hindcast) ---------------
//
// MarketOS tries a statement about a series it holds, its history as it was published. Here
// the series is given, each value taken as known at its own instant: a series given as it
// stands now (a revised figure in place of what was first said) is tried as if its
// revisions had been known all along.

/// How a value is compared with a threshold (MarketOS's `forecast::Comparison`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Comparison {
    Above,
    Below,
    AtLeast,
    AtMost,
}

impl Comparison {
    fn holds(self, measured: f64, threshold: f64) -> bool {
        match self {
            Comparison::Above => measured > threshold,
            Comparison::Below => measured < threshold,
            Comparison::AtLeast => measured >= threshold,
            Comparison::AtMost => measured <= threshold,
        }
    }

    fn words(self) -> &'static str {
        match self {
            Comparison::Above => "above",
            Comparison::Below => "below",
            Comparison::AtLeast => "at least",
            Comparison::AtMost => "at most",
        }
    }
}

/// What a trial asks at each moment: a change of `change_pct` percent from where the series
/// stands, `days` ahead, judged at the deadline or at any time before it.
#[derive(Clone, Copy, Debug)]
pub struct Asked {
    pub comparison: Comparison,
    pub change_pct: f64,
    pub window: spec::Window,
    pub days: u32,
    pub every_days: u32,
    pub stretches: u32,
}

/// The kinds of way tried (MarketOS's `hindcast::Kind`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    /// How often the statement held.
    Often,
    /// How often it held after going the same way.
    Going,
    /// How often it held when on the same side of the average.
    Side,
    /// How likely a move that size is, from how much it has swung.
    Swings,
}

/// The ways tried, each with the days it looks back over; none is all history known.
const WAYS: [(Kind, Option<u32>); 12] = [
    (Kind::Often, Some(30)),
    (Kind::Often, Some(90)),
    (Kind::Often, Some(180)),
    (Kind::Often, Some(365)),
    (Kind::Often, None),
    (Kind::Going, Some(5)),
    (Kind::Going, Some(20)),
    (Kind::Going, Some(60)),
    (Kind::Side, Some(20)),
    (Kind::Side, Some(60)),
    (Kind::Swings, Some(30)),
    (Kind::Swings, Some(90)),
];

fn named(kind: Kind, days: Option<u32>) -> String {
    match (kind, days) {
        (Kind::Often, None) => "how often it held in all history known".to_string(),
        (Kind::Often, Some(days)) => format!("how often it held in the last {days} days"),
        (Kind::Going, Some(days)) => format!("how often it held after going the same way over {days} days"),
        (Kind::Side, Some(days)) => format!("how often it held on the same side of the {days}-day average"),
        (Kind::Swings, Some(days)) => format!("how likely a move that size is, from the swings of the last {days} days"),
        (_, None) => unreachable!("only how often it held is counted over all history"),
    }
}

fn int_at(t: Timestamp) -> Int {
    Int::from(t.as_nanos())
}

fn small32(n: Nat, what: &str) -> Result<u32, OpError> {
    u32::try_from(&n).map_err(|_| OpError::ExecutionFailed(format!("{what} out of range")))
}

/// The history known at a moment, and each stretch of it as long as the statement looks
/// ahead, by where it begins, with whether the statement held over it (MarketOS's
/// `forecast::past`). None when nothing was known to measure a change from.
struct Past {
    history: Vec<Point>,
    stretches: Vec<(usize, bool)>,
}

fn past(series: &[Point], asked: &Asked, made_at: Timestamp, deadline: Timestamp) -> Result<Option<Past>, OpError> {
    let history: Vec<Point> = series.iter().copied().filter(|(t, _)| *t <= made_at).collect();
    if history.last().is_none_or(|(_, v)| *v == 0.0) {
        return Ok(None);
    }
    let ahead = deadline.since(made_at);
    let change = |value: f64, from: f64| (value / from - 1.0) * 100.0;
    let last = history.last().map(|(t, _)| *t);
    let mut stretches = Vec::new();
    for (index, (start_at, start)) in history.iter().enumerate() {
        if *start == 0.0 {
            continue;
        }
        let end = start_at.checked_add(ahead).map_err(|e| OpError::ExecutionFailed(e.to_string()))?;
        // Only stretches history covers to their end are counted.
        if last.is_none_or(|last| end > last) {
            break;
        }
        let within: Vec<f64> = history[index + 1..].iter().take_while(|(t, _)| *t <= end).map(|(_, v)| change(*v, *start)).collect();
        if within.is_empty() {
            continue;
        }
        let satisfied = match asked.window {
            spec::Window::AtDeadline => asked.comparison.holds(within[within.len() - 1], asked.change_pct),
            spec::Window::AnyTime => within.iter().any(|c| asked.comparison.holds(*c, asked.change_pct)),
        };
        stretches.push((index, satisfied));
    }
    Ok(Some(Past { history, stretches }))
}

/// How the statement made at `made_at` came out by `deadline` (MarketOS's
/// `forecast::judged_then`): each value after it measured as a change from the value it
/// stood at then, judged by `Datacartridge.Forecast.settle`.
fn judged(series: &[Point], asked: &Asked, made_at: Timestamp, deadline: Timestamp) -> spec::Outcome {
    let from = series.iter().rev().find(|(t, _)| *t <= made_at).map(|(_, v)| *v).filter(|v| *v != 0.0);
    let satisfied: List<bool> = match from {
        Some(from) => series.iter().filter(|(t, _)| *t > made_at && *t <= deadline).map(|(_, v)| asked.comparison.holds((v / from - 1.0) * 100.0, asked.change_pct)).collect(),
        None => List(Vec::new()),
    };
    spec::settle(asked.window, true, satisfied)
}

fn sign(x: f64) -> Int {
    Int::from(if x > 0.0 { 1i64 } else if x < 0.0 { -1 } else { 0 })
}

/// What the series stood at `days` before the value at `index`.
fn before(history: &[Point], index: usize, days: u32) -> Option<f64> {
    let then = history[index].0.checked_sub(Duration::from_days(i64::from(days))).ok()?;
    history[..index].iter().rev().find(|(t, _)| *t <= then).map(|(_, v)| *v)
}

/// The average of the values over the `days` up to the one at `index`.
fn average(history: &[Point], index: usize, days: u32) -> Option<f64> {
    let from = history[index].0.checked_sub(Duration::from_days(i64::from(days))).ok()?;
    let within: Vec<f64> = history[..=index].iter().rev().take_while(|(t, _)| *t > from).map(|(_, v)| *v).collect();
    Some(within.iter().sum::<f64>() / within.len() as f64)
}

/// How things stood at the value at `index`, as a way of the kind looks at them.
fn standing(history: &[Point], index: usize, kind: Kind, days: u32) -> Option<f64> {
    match kind {
        Kind::Going => before(history, index, days).map(|then| history[index].1 - then),
        Kind::Side => before(history, index, days).and_then(|_| average(history, index, days)).map(|mean| history[index].1 - mean),
        Kind::Often | Kind::Swings => unreachable!("only ways that look at how things stand ask how they stood"),
    }
}

/// How likely a change of `change_pct` percent is over `ahead` moves like `moves`, taking
/// the moves to come as drawn from a normal spread as wide as theirs, centred on nothing;
/// at any time before the end twice as likely as at the end (the reflection principle), and
/// never more than certain (MarketOS's `hindcast::swings`).
fn swings(moves: &[f64], ahead: f64, asked: &Asked) -> Option<f64> {
    if moves.len() < 2 || ahead <= 0.0 {
        return None;
    }
    let mean = moves.iter().sum::<f64>() / moves.len() as f64;
    let spread = (moves.iter().map(|m| (m - mean).powi(2)).sum::<f64>() / (moves.len() - 1) as f64).sqrt();
    let up = matches!(asked.comparison, Comparison::Above | Comparison::AtLeast);
    let at_end = if asked.change_pct <= -100.0 {
        if up { 1.0 } else { 0.0 }
    } else if spread == 0.0 {
        if asked.comparison.holds(0.0, asked.change_pct) { 1.0 } else { 0.0 }
    } else {
        let above = 1.0 - datacartridge_quant::options::norm_cdf((1.0 + asked.change_pct / 100.0).ln() / (spread * ahead.sqrt()));
        if up { above } else { 1.0 - above }
    };
    Some(match asked.window {
        spec::Window::AtDeadline => at_end,
        spec::Window::AnyTime => (2.0 * at_end).min(1.0),
    })
}

/// The likelihood a way gives at `at`, from the history known then, in basis points as a
/// forecast states it (MarketOS's `hindcast::likelihood`). None when it cannot say.
fn likelihood(past: &Past, kind: Kind, days: Option<u32>, asked: &Asked, at: Timestamp) -> Result<Option<u32>, OpError> {
    let history = &past.history;
    let Some(last) = history.len().checked_sub(1) else { return Ok(None) };
    let rate = match (kind, days) {
        (Kind::Often, _) => {
            let since = days.and_then(|days| at.checked_sub(Duration::from_days(i64::from(days))).ok());
            let counted: Vec<bool> = past.stretches.iter().filter(|(index, _)| since.is_none_or(|since| history[*index].0 >= since)).map(|(_, held)| *held).collect();
            spec::base_rate(nat(counted.iter().filter(|h| **h).count() as u64), nat(counted.len() as u64))
        }
        (Kind::Going | Kind::Side, Some(days)) => {
            let Some(now) = standing(history, last, kind, days) else { return Ok(None) };
            let stood: List<(Int, bool)> = past.stretches.iter().filter_map(|(index, held)| standing(history, *index, kind, days).map(|then| (sign(then), *held))).collect();
            trial_spec::alike_rate(sign(now), stood)
        }
        (Kind::Swings, Some(days)) => {
            let from = at.checked_sub(Duration::from_days(i64::from(days))).map_err(|e| OpError::ExecutionFailed(e.to_string()))?;
            let recent: Vec<f64> = history.iter().filter(|(t, _)| *t > from).map(|(_, v)| *v).collect();
            let moves: Vec<f64> = recent.windows(2).filter(|pair| pair[0] > 0.0 && pair[1] > 0.0).map(|pair| (pair[1] / pair[0]).ln()).collect();
            // As many moves as there were in so many days, for as many days as it looks ahead.
            let ahead = moves.len() as f64 * f64::from(asked.days) / f64::from(days);
            swings(&moves, ahead, asked).map(|p| nat((p * 10000.0).round() as u64))
        }
        (_, None) => unreachable!("only how often it held is counted over all history"),
    };
    rate.map(|rate| small32(spec::as_forecast(rate), "a likelihood")).transpose()
}

struct Moment {
    at: Timestamp,
    stated: Vec<u32>,
    came_true: bool,
}

fn tried(stated: u32, came_true: bool) -> trial_spec::Tried {
    trial_spec::Tried { stated_bps: nat(u64::from(stated)), came_true }
}

fn mean_of(of: impl Iterator<Item = (u32, bool)>) -> Result<u32, OpError> {
    let list: List<trial_spec::Tried> = of.map(|(stated, came)| tried(stated, came)).collect();
    small32(trial_spec::mean(list).ok_or_else(|| OpError::ExecutionFailed("a mean score of no forecasts".to_string()))?, "a mean score")
}

fn calibration(chosen: &[(u32, bool)]) -> Result<Vec<Value>, OpError> {
    let mut tenths: BTreeMap<u64, (u64, u64, u64)> = BTreeMap::new();
    for (said, came) in chosen {
        let group = tenths.entry(small(spec::tenth(nat(u64::from(*said))), "a tenth")?).or_default();
        group.0 += 1;
        group.1 += u64::from(*said);
        group.2 += u64::from(*came);
    }
    Ok(tenths.into_iter().map(|(tenth, (forecasts, said, came_true))| json!({ "tenth": tenth, "forecasts": forecasts, "said_bps": said / forecasts, "came_true": came_true })).collect())
}

/// Ways of forecasting the statement tried on the series' history (MarketOS's hindcast):
/// at moments `every_days` apart, each way says a likelihood from what was known then (and
/// again read through its own record, `Datacartridge.Forecast.recalibrated`); the moments are cut into
/// stretches (`Datacartridge.Hindcast.cut`), the way that did best on what had settled before each
/// stretch is chosen (`Datacartridge.Hindcast.chosen`) and scored on it. Scores are mean Brier scores
/// in basis points; saying 50% every time scores 2500.
pub fn trial(series: &[Point], asked: &Asked) -> Result<Value, OpError> {
    let input = |code: &str, message: String, arg: &str| input_arg_error(code, message, arg);
    if !asked.change_pct.is_finite() {
        return Err(input("NOT_A_NUMBER", "the change asked for is a number".to_string(), CHANGE_PCT));
    }
    if asked.days == 0 || asked.every_days == 0 {
        return Err(input("NOT_A_COUNT", "a forecast looks ahead at least a day, and moments are at least a day apart".to_string(), DAYS));
    }
    if asked.stretches == 0 {
        return Err(input("NOT_A_COUNT", "there is at least one stretch to test on".to_string(), STRETCHES));
    }
    let ways: Vec<(Kind, Option<u32>)> = WAYS.iter().copied().filter(|(kind, days)| trial_spec::speaks(*kind == Kind::Often, days.map(|d| nat(u64::from(d))), nat(u64::from(asked.days)))).collect();
    let (ahead, every) = (Duration::from_days(i64::from(asked.days)), Duration::from_days(i64::from(asked.every_days)));
    let fail = |e: datacartridge_core::error::Error| OpError::ExecutionFailed(e.to_string());
    let (Some(first), Some(last)) = (series.first().map(|(t, _)| *t), series.last().map(|(t, _)| *t)) else {
        return Err(input("NOTHING_TO_TRY", "the series holds no values".to_string(), crate::series::TIME_SERIES));
    };
    let mut moments = Vec::new();
    let mut left_out = 0u32;
    let mut at = first.checked_add(every).map_err(fail)?;
    while at.checked_add(ahead).map_err(fail)? <= last {
        let deadline = at.checked_add(ahead).map_err(fail)?;
        let stated = match past(series, asked, at, deadline)? {
            Some(past) => {
                let said: List<Option<Nat>> = ways.iter().map(|(kind, days)| likelihood(&past, *kind, *days, asked, at).map(|s| s.map(|bps| nat(u64::from(bps))))).collect::<Result<Vec<_>, _>>()?.into_iter().collect();
                trial_spec::moment(said).map(|said| said.0.into_iter().map(|bps| small32(bps, "a likelihood")).collect::<Result<Vec<u32>, _>>()).transpose()?
            }
            None => None,
        };
        match (stated, judged(series, asked, at, deadline)) {
            (Some(stated), outcome @ (spec::Outcome::CameTrue | spec::Outcome::DidNot)) => moments.push(Moment { at, stated, came_true: outcome == spec::Outcome::CameTrue }),
            _ => left_out += 1,
        }
        at = at.checked_add(every).map_err(fail)?;
    }
    // Each way again, read through its own record: what it says taken together with how
    // what it said in the same tenth came out, of its forecasts settled by then, counted as
    // MarketOS counts a forecaster's by default (`Datacartridge.Forecast.provenAt`).
    let counts_as = spec::proven_at();
    let ahead_int = Int::from(ahead.as_nanos());
    let tenth = |bps: u32| spec::tenth(nat(u64::from(bps)));
    let base = ways.len();
    let mut read = Vec::with_capacity(moments.len());
    for (i, moment) in moments.iter().enumerate() {
        let mut through = Vec::with_capacity(base);
        for w in 0..base {
            let said = moment.stated[w];
            let (mut came, mut scored) = (0u64, 0u64);
            for earlier in moments[..i].iter().filter(|m| trial_spec::learnable(int_at(m.at), ahead_int.clone(), int_at(moment.at)) && tenth(m.stated[w]) == tenth(said)) {
                came += u64::from(earlier.came_true);
                scored += 1;
            }
            let reading = spec::recalibrated(nat(u64::from(said)), nat(came), nat(scored), counts_as.clone());
            through.push(small32(spec::as_forecast(reading), "a likelihood")?);
        }
        read.push(through);
    }
    for (moment, through) in moments.iter_mut().zip(read) {
        moment.stated.extend(through);
    }
    let stretches = asked.stretches as usize;
    let times: List<Int> = moments.iter().map(|m| int_at(m.at)).collect();
    if !trial_spec::triable(times.clone(), ahead_int.clone(), nat(u64::from(asked.stretches))) {
        let enough = trial_spec::enough(nat(moments.len() as u64), nat(u64::from(asked.stretches)));
        let most = trial_spec::most_triable(times, ahead_int.clone()).map(|n| small(n, "how many stretches")).transpose()?;
        let fewer = most.map(|most| format!("ask for {most} stretch{} or fewer, ", if most == 1 { "" } else { "es" })).unwrap_or_else(|| "ask for ".to_string());
        let why = if enough {
            format!("the first test stretch would begin before any forecast before it had settled: {fewer}or a shorter look ahead")
        } else {
            let needed = small(trial_spec::needed(nat(u64::from(asked.stretches))), "how many moments are needed")?;
            let per = small(trial_spec::at_least_per_stretch(), "how many moments a stretch holds")?;
            format!("{} moments could be tried, and {} stretches of at least {per} take {needed}: {fewer}moments closer together, or a shorter look ahead", moments.len(), asked.stretches + 1)
        };
        return Err(input("TOO_SHORT", format!("the series' history does not reach far enough for this trial: {why}"), crate::series::TIME_SERIES));
    }
    let cut = |i: usize| small(trial_spec::cut(nat(moments.len() as u64), nat(stretches as u64), nat(i as u64)), "a stretch's start").map(|n| n as usize);
    let n_ways = 2 * base;
    let mut tested = Vec::new();
    let mut chosen_all = Vec::new();
    for i in 1..=stretches {
        let (start, end) = (cut(i)?, cut(i + 1)?);
        let test = &moments[start..end];
        let begins = test[0].at;
        let learned: Vec<&Moment> = moments[..start].iter().filter(|m| trial_spec::learnable(int_at(m.at), ahead_int.clone(), int_at(begins))).collect();
        if learned.is_empty() {
            return Err(OpError::ExecutionFailed(format!("test stretch {i} has nothing settled to learn from, though the trial could be tried")));
        }
        let totals: List<Nat> = (0..n_ways).map(|w| trial_spec::total(learned.iter().map(|m| tried(m.stated[w], m.came_true)).collect())).collect();
        let chosen = small(trial_spec::chosen(totals).ok_or_else(|| OpError::ExecutionFailed("no way of forecasting was chosen among several".to_string()))?, "the way chosen")? as usize;
        chosen_all.extend(test.iter().map(|m| (m.stated[chosen], m.came_true)));
        tested.push(json!({
            "from": begins.to_string(),
            "to": test[test.len() - 1].at.to_string(),
            "learned_from": learned.len(),
            "learned_bps": (0..n_ways).map(|w| mean_of(learned.iter().map(|m| (m.stated[w], m.came_true)))).collect::<Result<Vec<_>, _>>()?,
            "chosen": chosen,
            "tested": test.len(),
            "came_true": test.iter().filter(|m| m.came_true).count(),
            "chosen_bps": mean_of(test.iter().map(|m| (m.stated[chosen], m.came_true)))?,
            "each_bps": (0..n_ways).map(|w| mean_of(test.iter().map(|m| (m.stated[w], m.came_true)))).collect::<Result<Vec<_>, _>>()?,
            "even_bps": mean_of(test.iter().map(|m| (5000, m.came_true)))?,
        }));
    }
    let tested_moments = &moments[cut(1)?..];
    let window = match asked.window {
        spec::Window::AtDeadline => "at its deadline",
        spec::Window::AnyTime => "at any time before",
    };
    let ways_tried: Vec<Value> = [false, true]
        .iter()
        .flat_map(|read| ways.iter().map(move |(kind, days)| json!({ "kind": kind, "days": days, "read": read, "name": if *read { format!("{}, read through its record", named(*kind, *days)) } else { named(*kind, *days) } })))
        .collect();
    Ok(json!({
        "statement": format!("the series is {} {}% from where it stands, {} days ahead, {window}", asked.comparison.words(), asked.change_pct, asked.days),
        "days": asked.days,
        "every_days": asked.every_days,
        "ways": ways_tried,
        "from": moments[0].at.to_string(),
        "to": moments[moments.len() - 1].at.to_string(),
        "moments": moments.len(),
        "left_out": left_out,
        "first": [moments[0].at.to_string(), moments[cut(1)? - 1].at.to_string()],
        "stretches": tested,
        "tested": tested_moments.len(),
        "came_true": tested_moments.iter().filter(|m| m.came_true).count(),
        "calibration": calibration(&chosen_all)?,
        "chosen_bps": mean_of(chosen_all.iter().copied())?,
        "each_bps": (0..n_ways).map(|w| mean_of(tested_moments.iter().map(|m| (m.stated[w], m.came_true)))).collect::<Result<Vec<_>, _>>()?,
        "even_bps": mean_of(tested_moments.iter().map(|m| (5000, m.came_true)))?,
    }))
}

#[derive(Default)]
pub struct TryForecastingWaysOp;

fn text_of(streams: &[(String, Vec<u8>, Option<capdag::StreamMeta>)], urn: &str) -> Result<Option<String>, OpError> {
    capdag::find_stream(streams, urn).map(|b| std::str::from_utf8(b).map(|s| s.trim().to_string()).map_err(|_| input_arg_error("NOT_TEXT", format!("{urn} is not UTF-8 text"), urn))).transpose()
}

fn count_of(streams: &[(String, Vec<u8>, Option<capdag::StreamMeta>)], urn: &str, default: Option<u32>) -> Result<u32, OpError> {
    match text_of(streams, urn)? {
        Some(text) => text.parse::<u32>().ok().filter(|n| *n >= 1).ok_or_else(|| input_arg_error("NOT_A_COUNT", format!("'{text}' is not a whole number of at least 1"), urn)),
        None => default.ok_or_else(|| input_arg_error("INPUT_REQUIRED", format!("{urn} is required"), urn)),
    }
}

#[async_trait]
impl Op<()> for TryForecastingWaysOp {
    async fn perform(&self, _dry: &mut DryContext, wet: &mut WetContext) -> OpResult<()> {
        let req: Arc<Request> = wet.get_required(WET_KEY_REQUEST).map_err(|e| OpError::ExecutionFailed(e.to_string()))?;
        let streams = req.take_input().map_err(|e| OpError::ExecutionFailed(e.to_string()))?.collect_streams().await.map_err(|e| OpError::ExecutionFailed(format!("Stream error: {e}")))?;
        let series = crate::series::parse(capdag::require_stream(&streams, crate::series::TIME_SERIES).map_err(|_| input_arg_error("INPUT_REQUIRED", "a series is required", crate::series::TIME_SERIES))?, crate::series::TIME_SERIES)?;
        let comparison: Comparison = serde_json::from_value(json!(text_of(&streams, COMPARISON)?.ok_or_else(|| input_arg_error("INPUT_REQUIRED", "comparison is required", COMPARISON))?)).map_err(|_| input_arg_error("NOT_A_CHOICE", "comparison is above, below, at_least or at_most", COMPARISON))?;
        let change_pct = text_of(&streams, CHANGE_PCT)?.ok_or_else(|| input_arg_error("INPUT_REQUIRED", "change-pct is required", CHANGE_PCT))?;
        let change_pct: f64 = change_pct.parse().map_err(|_| input_arg_error("NOT_A_NUMBER", format!("'{change_pct}' is not a number"), CHANGE_PCT))?;
        let window = match text_of(&streams, WINDOW)?.as_deref() {
            None | Some("at_deadline") => spec::Window::AtDeadline,
            Some("any_time") => spec::Window::AnyTime,
            Some(other) => return Err(input_arg_error("NOT_A_CHOICE", format!("'{other}' is not at_deadline or any_time"), WINDOW)),
        };
        let asked = Asked { comparison, change_pct, window, days: count_of(&streams, DAYS, None)?, every_days: count_of(&streams, EVERY_DAYS, None)?, stretches: count_of(&streams, STRETCHES, Some(5))? };
        let tried = tokio::task::spawn_blocking(move || trial(&series, &asked)).await.map_err(|e| OpError::ExecutionFailed(e.to_string()))??;
        let output = req.output();
        output.start(false, None).map_err(|e| OpError::ExecutionFailed(e.to_string()))?;
        output.emit_cbor(&ciborium::Value::Text(tried.to_string())).await.map_err(|e| OpError::ExecutionFailed(e.to_string()))
    }

    fn metadata(&self) -> capdag::OpMetadata {
        capdag::OpMetadata::builder("TryForecastingWaysOp").description("Try ways of forecasting a change in a series on its history").build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(error: OpError) -> String {
        match error {
            OpError::Classified { code, .. } => code,
            other => panic!("{other:?}"),
        }
    }

    // TEST312: settled forecasts are scored as MarketOS scores a forecaster: the mean of their
    // Brier scores, whether that beats saying 50% every time, and by tenths how many said
    // about that and how often they came true; a record no better than even odds says so.
    #[test]
    fn test0312_forecasts_are_scored() {
        let good = br#"[{"probability": 0.9, "came_true": true}, {"probability": 0.8, "came_true": true}, {"probability": 0.1, "came_true": false}, {"probability": 0.85, "came_true": false}]"#;
        let scored = score(good).unwrap();
        // (0.01 + 0.04 + 0.01 + 0.7225) / 4
        assert!((scored["brier"].as_f64().unwrap() - 0.195625).abs() < 1e-4, "{scored}");
        assert_eq!((scored["n"].clone(), scored["even_brier"].clone(), scored["beats_even"].clone()), (json!(4), json!(0.25), json!(true)));
        assert_eq!(scored["calibration"], json!([
            {"tenth": 1, "forecasts": 1, "said": 0.1, "came_true": 0},
            {"tenth": 8, "forecasts": 2, "said": 0.825, "came_true": 1},
            {"tenth": 9, "forecasts": 1, "said": 0.9, "came_true": 1},
        ]));
        let even = score(br#"[{"probability": 0.5, "came_true": true}, {"probability": 0.5, "came_true": false}]"#).unwrap();
        assert_eq!((even["brier"].clone(), even["beats_even"].clone()), (json!(0.25), json!(false)), "even odds do not beat even odds");
    }

    // TEST313: what cannot be scored is refused as what was given: nothing to score, a
    // probability outside 0 to 1, a forecast that does not say how it came out.
    #[test]
    fn test0313_what_cannot_be_scored_is_refused() {
        assert_eq!(code(score(b"[]").unwrap_err()), "NOTHING_TO_SCORE");
        assert_eq!(code(score(br#"[{"probability": 1.5, "came_true": true}]"#).unwrap_err()), "NOT_A_PROBABILITY");
        assert_eq!(code(score(br#"[{"probability": 0.5}]"#).unwrap_err()), "NOT_OUTCOMES");
    }

    fn daily(values: &[f64]) -> Vec<Point> {
        values.iter().enumerate().map(|(i, v)| (Timestamp::from_ymd(2025, 1, 1).unwrap().checked_add(Duration::from_days(i as i64)).unwrap(), *v)).collect()
    }

    fn asked(comparison: Comparison, change_pct: f64, days: u32, every_days: u32, stretches: u32) -> Asked {
        Asked { comparison, change_pct, window: spec::Window::AtDeadline, days, every_days, stretches }
    }

    // TEST314: ways of forecasting are tried on a series' history as MarketOS tries them: a
    // series that rises every day comes true at every moment, the ways that count how often
    // it held learn that and say it near certainly, and the way chosen on each stretch beats
    // saying 50% every time; every way is tried as it is and read through its record.
    #[test]
    fn test0314_ways_are_tried_on_history() {
        let rising: Vec<f64> = (0..400).map(|i| 100.0 * 1.001f64.powi(i)).collect();
        let tried = trial(&daily(&rising), &asked(Comparison::Above, 0.5, 7, 7, 3)).unwrap();
        let moments = tried["moments"].as_u64().unwrap();
        assert_eq!(tried["came_true"], tried["tested"], "it rose every week: {tried}");
        assert!(moments >= 20, "{moments}");
        assert_eq!(tried["stretches"].as_array().unwrap().len(), 3);
        assert!(tried["chosen_bps"].as_u64().unwrap() < tried["even_bps"].as_u64().unwrap(), "the way chosen beats even odds: {}", tried["chosen_bps"]);
        let ways = tried["ways"].as_array().unwrap();
        assert_eq!(ways.len() % 2, 0);
        assert!(ways.iter().any(|w| w["read"] == json!(true)));
        assert!(ways.iter().all(|w| w["kind"] != json!("often") || w["days"].as_u64().is_none_or(|d| d > 7)), "a count shorter than the look ahead does not speak (Datacartridge.Hindcast.speaks)");
    }

    // TEST315: a history too short for the stretches asked is refused, saying how many it
    // could be tried on; a trial asking nothing ahead is refused.
    #[test]
    fn test0315_too_short_a_history_is_refused() {
        let short = daily(&(0..40).map(|i| 100.0 + i as f64).collect::<Vec<_>>());
        match trial(&short, &asked(Comparison::Above, 1.0, 7, 7, 5)).unwrap_err() {
            OpError::Classified { code, message, .. } => assert!(code == "TOO_SHORT" && message.contains("does not reach far enough"), "{message}"),
            other => panic!("{other:?}"),
        }
        match trial(&short, &asked(Comparison::Above, 1.0, 0, 7, 5)).unwrap_err() {
            OpError::Classified { code, .. } => assert_eq!(code, "NOT_A_COUNT"),
            other => panic!("{other:?}"),
        }
    }

    // TEST316: a moment's statement comes out as MarketOS judges it (`Datacartridge.Forecast.settle`):
    // at the deadline on the last value by then, at any time on the first that holds.
    #[test]
    fn test0316_a_statement_is_judged_as_marketos_judges_it() {
        let s = daily(&[100.0, 103.0, 99.0]);
        let at = s[0].0;
        let deadline = s[2].0;
        assert_eq!(judged(&s, &asked(Comparison::Above, 2.0, 2, 1, 1), at, deadline), spec::Outcome::DidNot, "at the deadline it stood 1% down");
        let any = Asked { window: spec::Window::AnyTime, ..asked(Comparison::Above, 2.0, 2, 1, 1) };
        assert_eq!(judged(&s, &any, at, deadline), spec::Outcome::CameTrue, "it was 3% up the day after");
    }
}
