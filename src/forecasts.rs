//! Forecasts scored, and ways of forecasting tried on a series' history: MarketOS's scoring
//! and hindcast, decided by its proved model (`Mos.Forecast`, `Mos.Hindcast`, generated into
//! crates/mos-formal).

use std::collections::BTreeMap;
use std::sync::Arc;

use capdag::{async_trait, DryContext, Op, OpError, OpResult, Request, WetContext, WET_KEY_REQUEST};
use mos_formal::spec::forecast as spec;
use mos_formal::{List, Nat};
use serde_json::{json, Value};

use crate::input_arg_error;

pub const OUTCOMES: &str = "media:fmt=json;forecast-outcomes;list;record";

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
/// (`Mos.Forecast.brier`, `mean`), against the 0.25 that saying 50% every time earns, whether
/// it beats that (`Mos.Forecast.skill` above nothing), and how the forecasts in each tenth of
/// the scale (`Mos.Forecast.tenth`) came out. An empty record is refused: there is nothing
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
}
