//! transform-records: a record program run over a list of records exactly as written.
//!
//! The program language is the one edit-records has the constrained-generation peer write
//! (`crate::transform`); here the program is given directly, and no model takes part.

use std::sync::Arc;

use capdag::{async_trait, DryContext, Op, OpError, OpResult, Request, WetContext, WET_KEY_REQUEST};
use serde_json::Value;

use crate::input_arg_error;

pub const RECORDS: &str = "media:fmt=json;list;record";
pub const PROGRAM: &str = "media:fmt=json;record;record-program";

/// The records transformed by the program. A program that does not parse, names an
/// operation the language does not have, or does not apply to the records is refused as
/// what was given.
pub fn transform(records: &[u8], program: &[u8]) -> Result<Value, OpError> {
    let program = std::str::from_utf8(program).map_err(|_| input_arg_error("NOT_TEXT", "the program is not UTF-8 text", PROGRAM))?;
    let program = crate::transform::parse_program(program).map_err(|e| input_arg_error("NOT_A_PROGRAM", e.to_string(), PROGRAM))?;
    let records: Value = serde_json::from_slice(records).map_err(|e| input_arg_error("NOT_RECORDS", format!("the records are not JSON: {e}"), RECORDS))?;
    crate::transform::apply_program(&program, &records).map_err(|e| input_arg_error("NOT_APPLICABLE", e.to_string(), RECORDS))
}

#[derive(Default)]
pub struct TransformRecordsOp;

#[async_trait]
impl Op<()> for TransformRecordsOp {
    async fn perform(&self, _dry: &mut DryContext, wet: &mut WetContext) -> OpResult<()> {
        let req: Arc<Request> = wet.get_required(WET_KEY_REQUEST).map_err(|e| OpError::ExecutionFailed(e.to_string()))?;
        let streams = req.take_input().map_err(|e| OpError::ExecutionFailed(e.to_string()))?.collect_streams().await.map_err(|e| OpError::ExecutionFailed(format!("Stream error: {e}")))?;
        let records = capdag::require_stream(&streams, RECORDS).map_err(|_| input_arg_error("INPUT_REQUIRED", "the records are required", RECORDS))?;
        let program = capdag::find_stream(&streams, PROGRAM).ok_or_else(|| input_arg_error("INPUT_REQUIRED", "the program (--program) is required", PROGRAM))?;
        let transformed = transform(records, program)?;
        let output = req.output();
        output.start(false, None).map_err(|e| OpError::ExecutionFailed(e.to_string()))?;
        output.emit_cbor(&ciborium::Value::Text(transformed.to_string())).await.map_err(|e| OpError::ExecutionFailed(e.to_string()))
    }

    fn metadata(&self) -> capdag::OpMetadata {
        capdag::OpMetadata::builder("TransformRecordsOp").description("Run a record program over a list of records, exactly as written").build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn code(error: OpError) -> String {
        match error {
            OpError::Classified { code, .. } => code,
            other => panic!("{other:?}"),
        }
    }

    // TEST310: a record program runs exactly as written, every operation of the language in
    // one program: filter, map, set, rename, drop, select, sort, limit, reverse.
    #[test]
    fn test0310_a_program_runs_as_written() {
        let records = json!([
            {"name": " Acme ", "price": "12.5", "country": "US", "note": "x"},
            {"name": "Beta", "price": "8", "country": "DE", "note": "y"},
            {"name": "Gamma", "price": "30", "country": "US", "note": "z"}
        ]);
        let program = json!({"ops": [
            {"op": "filter", "field": "country", "predicate": "eq", "value": "US"},
            {"op": "map_field", "field": "name", "transform": "trim"},
            {"op": "map_field", "field": "price", "transform": "to_number"},
            {"op": "set_field", "field": "listed", "value": true},
            {"op": "rename_field", "from": "price", "to": "usd"},
            {"op": "drop_fields", "fields": ["note"]},
            {"op": "select_fields", "fields": ["name", "usd", "listed"]},
            {"op": "sort_by", "field": "usd", "order": "desc"},
            {"op": "limit", "n": 2},
            {"op": "reverse"}
        ]});
        let out = transform(records.to_string().as_bytes(), program.to_string().as_bytes()).unwrap();
        assert_eq!(out, json!([{"name": "Acme", "usd": 12.5, "listed": true}, {"name": "Gamma", "usd": 30.0, "listed": true}]));
    }

    // TEST311: what cannot run is refused as what was given: a program with no operations,
    // one naming an operation the language does not have, records that are not a list of
    // objects.
    #[test]
    fn test0311_what_cannot_run_is_refused() {
        let records = br#"[{"a": 1}]"#;
        assert_eq!(code(transform(records, br#"{"ops": []}"#).unwrap_err()), "NOT_A_PROGRAM");
        assert_eq!(code(transform(records, br#"{"ops": [{"op": "explode"}]}"#).unwrap_err()), "NOT_A_PROGRAM");
        assert_eq!(code(transform(br#"[1, 2]"#, br#"{"ops": [{"op": "reverse"}]}"#).unwrap_err()), "NOT_APPLICABLE");
        assert_eq!(code(transform(b"not json", br#"{"ops": [{"op": "reverse"}]}"#).unwrap_err()), "NOT_RECORDS");
    }
}
