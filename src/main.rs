//! datacartridge - JSON/YAML/CSV format conversion plugin for MachineFabricEngine
//!
//! Converts between JSON, YAML, and CSV formats where structurally compatible:
//! - JSON <-> YAML: value, record, list, list-of-records
//! - JSON list-of-records <-> CSV
//! - YAML list-of-records <-> CSV

use anyhow::Result;
use capdag::{
    async_trait, ArgSource, Cap, CapArg, CapManifest, DryContext, Op, OpError, OpResult,
    PluginRuntime, Request, WetContext, WET_KEY_REQUEST,
};
use std::sync::Arc;

// =============================================================================
// MANIFEST
// =============================================================================

fn build_manifest() -> CapManifest {
    let mut all_caps = vec![capdag::identity_cap()];

    for (in_media, out_media) in capdag::all_format_conversion_paths() {
        let urn = capdag::format_conversion_urn(in_media, out_media);
        let title = format!(
            "Convert {} to {}",
            format_display_name(in_media),
            format_display_name(out_media),
        );
        let description = format!(
            "Convert data from {} to {}",
            format_display_name(in_media),
            format_display_name(out_media),
        );

        let mut cap = Cap::with_description(urn, title, "convert_format".to_string(), description);
        cap.add_arg(CapArg::with_description(
            in_media,
            true,
            vec![
                ArgSource::Stdin {
                    stdin: in_media.to_string(),
                },
                ArgSource::Position { position: 0 },
            ],
            "Input data to convert".to_string(),
        ));
        cap.set_output(capdag::CapOutput::new(out_media, "Converted data"));
        all_caps.push(cap);
    }

    CapManifest::new(
        "datacartridge".to_string(),
        env!("CARGO_PKG_VERSION").to_string(),
        "JSON/YAML/CSV format conversion".to_string(),
        all_caps,
    )
    .with_author("https://github.com/machinefabric".to_string())
    .with_page_url("https://github.com/machinefabric/datacartridge".to_string())
}

fn format_display_name(media_urn: &str) -> &'static str {
    // Parse the URN properly — never inspect the string directly
    let urn = capdag::MediaUrn::from_string(media_urn)
        .expect("format_display_name called with invalid media URN");

    let is_list = urn.is_list();
    let is_record = urn.is_record();

    if urn.is_csv() {
        "CSV"
    } else if urn.is_json() {
        match (is_list, is_record) {
            (true, true) => "JSON Array of Objects",
            (true, false) => "JSON Array",
            (false, true) => "JSON Object",
            _ => "JSON Value",
        }
    } else if urn.is_yaml() {
        match (is_list, is_record) {
            (true, true) => "YAML List of Mappings",
            (true, false) => "YAML List",
            (false, true) => "YAML Mapping",
            _ => "YAML Value",
        }
    } else {
        panic!("Unrecognized data format in media URN: {}", media_urn)
    }
}

// =============================================================================
// OP IMPLEMENTATION
// =============================================================================

struct ConvertFormatOp {
    in_media: &'static str,
    out_media: &'static str,
}

#[async_trait]
impl Op<()> for ConvertFormatOp {
    async fn perform(&self, _dry: &mut DryContext, wet: &mut WetContext) -> OpResult<()> {
        let req: Arc<Request> = wet
            .get_required(WET_KEY_REQUEST)
            .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;
        let input = req
            .take_input()
            .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;
        let output = req.output();

        let streams = input
            .collect_streams()
            .await
            .map_err(|e| OpError::ExecutionFailed(format!("Stream error: {}", e)))?;

        // Find input data by the expected media URN — fail hard if not supplied
        let data = capdag::require_stream(&streams, self.in_media)
            .map_err(|e| OpError::ExecutionFailed(format!(
                "Expected input stream '{}' not found: {}", self.in_media, e
            )))?;

        // Determine conversion from the registered in/out media (not from content)
        let from = format_of_str(self.in_media)
            .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;
        let to = format_of_str(self.out_media)
            .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;

        output.start(false)
            .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;
        output.progress(0.10, "Converting format");
        let result = convert(from, to, data)
            .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;

        output.progress(0.90, "Encoding output");
        let cbor_value = ciborium::Value::Text(
            String::from_utf8(result)
                .map_err(|e| OpError::ExecutionFailed(format!("Output is not valid UTF-8: {}", e)))?,
        );
        output
            .emit_cbor(&cbor_value)
            .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;

        Ok(())
    }

    fn metadata(&self) -> capdag::OpMetadata {
        capdag::OpMetadata::builder("ConvertFormatOp").build()
    }
}

// =============================================================================
// MAIN
// =============================================================================

#[tokio::main]
async fn main() -> Result<()> {
    let manifest = build_manifest();
    let mut runtime = PluginRuntime::with_manifest(manifest);

    for (in_media, out_media) in capdag::all_format_conversion_paths() {
        let urn = capdag::format_conversion_urn(in_media, out_media);
        runtime.register_op(&urn.to_string(), move || {
            Box::new(ConvertFormatOp { in_media, out_media })
        });
    }

    if let Err(e) = runtime.run().await {
        tracing::error!(target: "datacartridge", "Runtime error: {}", e);
        std::process::exit(1);
    }

    Ok(())
}

// =============================================================================
// FORMAT DETECTION AND DISPATCH
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq)]
enum Fmt {
    Json,
    Yaml,
    Csv,
}

/// Identify the data format from a media URN string.
/// Parses the URN properly and checks marker tags — never inspects the string directly.
fn format_of_str(media_urn: &str) -> Result<Fmt> {
    let urn = capdag::MediaUrn::from_string(media_urn)
        .map_err(|e| anyhow::anyhow!("Invalid media URN '{}': {}", media_urn, e))?;
    if urn.is_json() {
        Ok(Fmt::Json)
    } else if urn.is_yaml() {
        Ok(Fmt::Yaml)
    } else if urn.is_csv() {
        Ok(Fmt::Csv)
    } else {
        anyhow::bail!("Media URN '{}' is not a recognized data format (json, yaml, or csv)", media_urn)
    }
}

fn convert(from: Fmt, to: Fmt, data: &[u8]) -> Result<Vec<u8>> {
    match (from, to) {
        (Fmt::Json, Fmt::Yaml) => json_to_yaml(data),
        (Fmt::Yaml, Fmt::Json) => yaml_to_json(data),
        (Fmt::Json, Fmt::Csv) => json_records_to_csv(data),
        (Fmt::Csv, Fmt::Json) => csv_to_json_records(data),
        (Fmt::Yaml, Fmt::Csv) => yaml_records_to_csv(data),
        (Fmt::Csv, Fmt::Yaml) => csv_to_yaml_records(data),
        (f, t) => anyhow::bail!("Unsupported conversion: {:?} -> {:?}", f, t),
    }
}

// =============================================================================
// CONVERSION FUNCTIONS
// =============================================================================

/// JSON -> YAML: parse as serde_json::Value, serialize to YAML string.
fn json_to_yaml(data: &[u8]) -> Result<Vec<u8>> {
    let value: serde_json::Value =
        serde_json::from_slice(data).map_err(|e| anyhow::anyhow!("Invalid JSON input: {}", e))?;
    let yaml_str = serde_yaml::to_string(&value)
        .map_err(|e| anyhow::anyhow!("Failed to serialize to YAML: {}", e))?;
    Ok(yaml_str.into_bytes())
}

/// YAML -> JSON: parse as serde_yaml::Value, convert recursively to serde_json::Value.
fn yaml_to_json(data: &[u8]) -> Result<Vec<u8>> {
    let value: serde_yaml::Value =
        serde_yaml::from_slice(data).map_err(|e| anyhow::anyhow!("Invalid YAML input: {}", e))?;
    let json_value = yaml_value_to_json_value(value)?;
    serde_json::to_vec_pretty(&json_value)
        .map_err(|e| anyhow::anyhow!("Failed to serialize to JSON: {}", e))
}

/// Recursive YAML-to-JSON value converter.
/// Handles Mapping (non-string keys → string), Sequence, Tagged (strip tags), scalars.
fn yaml_value_to_json_value(yaml: serde_yaml::Value) -> Result<serde_json::Value> {
    match yaml {
        serde_yaml::Value::Null => Ok(serde_json::Value::Null),
        serde_yaml::Value::Bool(b) => Ok(serde_json::Value::Bool(b)),
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(serde_json::Value::Number(i.into()))
            } else if let Some(f) = n.as_f64() {
                serde_json::Number::from_f64(f)
                    .map(serde_json::Value::Number)
                    .ok_or_else(|| anyhow::anyhow!("YAML number {} is not representable in JSON", f))
            } else {
                anyhow::bail!("Unrepresentable YAML number: {:?}", n)
            }
        }
        serde_yaml::Value::String(s) => Ok(serde_json::Value::String(s)),
        serde_yaml::Value::Sequence(seq) => {
            let items: Result<Vec<_>> = seq.into_iter().map(yaml_value_to_json_value).collect();
            Ok(serde_json::Value::Array(items?))
        }
        serde_yaml::Value::Mapping(map) => {
            let mut obj = serde_json::Map::new();
            for (k, v) in map {
                let key = match k {
                    serde_yaml::Value::String(s) => s,
                    other => serde_yaml::to_string(&other)
                        .map_err(|e| anyhow::anyhow!("Non-string YAML key: {}", e))?
                        .trim()
                        .to_string(),
                };
                obj.insert(key, yaml_value_to_json_value(v)?);
            }
            Ok(serde_json::Value::Object(obj))
        }
        serde_yaml::Value::Tagged(tagged) => yaml_value_to_json_value(tagged.value),
    }
}

/// JSON array of objects -> CSV.
/// Headers are collected from all records, preserving insertion order from the first record.
fn json_records_to_csv(data: &[u8]) -> Result<Vec<u8>> {
    let records: Vec<serde_json::Map<String, serde_json::Value>> = serde_json::from_slice(data)
        .map_err(|e| anyhow::anyhow!("Invalid JSON array of objects: {}", e))?;

    if records.is_empty() {
        return Ok(Vec::new());
    }

    // Collect all unique headers preserving insertion order
    let mut headers: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for record in &records {
        for key in record.keys() {
            if seen.insert(key.clone()) {
                headers.push(key.clone());
            }
        }
    }

    let mut wtr = csv::Writer::from_writer(Vec::new());
    wtr.write_record(&headers)
        .map_err(|e| anyhow::anyhow!("Failed to write CSV headers: {}", e))?;

    for record in &records {
        let row: Vec<String> = headers
            .iter()
            .map(|h| match record.get(h) {
                Some(serde_json::Value::String(s)) => s.clone(),
                Some(serde_json::Value::Null) | None => String::new(),
                Some(v) => v.to_string(),
            })
            .collect();
        wtr.write_record(&row)
            .map_err(|e| anyhow::anyhow!("Failed to write CSV row: {}", e))?;
    }

    wtr.into_inner()
        .map_err(|e| anyhow::anyhow!("Failed to finalize CSV: {}", e))
}

/// CSV -> JSON array of objects.
/// Headers from first row become object keys. Type inference: empty→null, integers, floats,
/// booleans ("true"/"false"), else string.
fn csv_to_json_records(data: &[u8]) -> Result<Vec<u8>> {
    let mut rdr = csv::Reader::from_reader(data);
    let headers: Vec<String> = rdr
        .headers()
        .map_err(|e| anyhow::anyhow!("Failed to read CSV headers: {}", e))?
        .iter()
        .map(|h| h.to_string())
        .collect();

    if headers.is_empty() {
        anyhow::bail!("CSV has no headers");
    }

    let mut records: Vec<serde_json::Value> = Vec::new();
    for result in rdr.records() {
        let row = result.map_err(|e| anyhow::anyhow!("Failed to read CSV row: {}", e))?;
        let mut obj = serde_json::Map::new();
        for (i, field) in row.iter().enumerate() {
            let key = headers
                .get(i)
                .ok_or_else(|| anyhow::anyhow!("CSV row has more fields than headers"))?;
            let value = infer_csv_value(field);
            obj.insert(key.clone(), value);
        }
        records.push(serde_json::Value::Object(obj));
    }

    serde_json::to_vec_pretty(&records)
        .map_err(|e| anyhow::anyhow!("Failed to serialize to JSON: {}", e))
}

/// YAML list of mappings -> CSV. Converts via JSON intermediary.
fn yaml_records_to_csv(data: &[u8]) -> Result<Vec<u8>> {
    let json_bytes = yaml_to_json(data)?;
    json_records_to_csv(&json_bytes)
}

/// CSV -> YAML list of mappings. Converts via JSON intermediary.
fn csv_to_yaml_records(data: &[u8]) -> Result<Vec<u8>> {
    let json_bytes = csv_to_json_records(data)?;
    json_to_yaml(&json_bytes)
}

/// Infer a typed JSON value from a CSV field string.
fn infer_csv_value(field: &str) -> serde_json::Value {
    if field.is_empty() {
        return serde_json::Value::Null;
    }
    if let Ok(n) = field.parse::<i64>() {
        return serde_json::Value::Number(n.into());
    }
    if let Ok(f) = field.parse::<f64>() {
        if let Some(n) = serde_json::Number::from_f64(f) {
            return serde_json::Value::Number(n);
        }
    }
    if field == "true" {
        return serde_json::Value::Bool(true);
    }
    if field == "false" {
        return serde_json::Value::Bool(false);
    }
    serde_json::Value::String(field.to_string())
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_to_yaml_object() {
        let json = br#"{"name": "Alice", "age": 30}"#;
        let yaml = json_to_yaml(json).unwrap();
        let yaml_str = std::str::from_utf8(&yaml).unwrap();
        assert!(yaml_str.contains("name:"));
        assert!(yaml_str.contains("Alice"));
        assert!(yaml_str.contains("age:"));
    }

    #[test]
    fn test_yaml_to_json_object() {
        let yaml = b"name: Alice\nage: 30";
        let json = yaml_to_json(yaml).unwrap();
        let val: serde_json::Value = serde_json::from_slice(&json).unwrap();
        assert_eq!(val["name"], "Alice");
        assert_eq!(val["age"], 30);
    }

    #[test]
    fn test_json_to_yaml_array() {
        let json = br#"[1, 2, 3]"#;
        let yaml = json_to_yaml(json).unwrap();
        let yaml_str = std::str::from_utf8(&yaml).unwrap();
        assert!(yaml_str.contains("- 1"));
        assert!(yaml_str.contains("- 2"));
        assert!(yaml_str.contains("- 3"));
    }

    #[test]
    fn test_yaml_to_json_list() {
        let yaml = b"- 1\n- 2\n- 3";
        let json = yaml_to_json(yaml).unwrap();
        let val: serde_json::Value = serde_json::from_slice(&json).unwrap();
        assert_eq!(val, serde_json::json!([1, 2, 3]));
    }

    #[test]
    fn test_json_to_yaml_scalar() {
        let json = br#""hello world""#;
        let yaml = json_to_yaml(json).unwrap();
        let yaml_str = std::str::from_utf8(&yaml).unwrap();
        assert!(yaml_str.contains("hello world"));
    }

    #[test]
    fn test_json_records_to_csv() {
        let json = br#"[{"a": 1, "b": "x"}, {"a": 2, "b": "y"}]"#;
        let csv_bytes = json_records_to_csv(json).unwrap();
        let csv_str = std::str::from_utf8(&csv_bytes).unwrap();
        // Headers present
        assert!(csv_str.starts_with("a,b") || csv_str.starts_with("b,a"));
        // Data present
        assert!(csv_str.contains('1'));
        assert!(csv_str.contains('x'));
    }

    #[test]
    fn test_csv_to_json_records() {
        let csv = b"name,age\nAlice,30\nBob,25";
        let json = csv_to_json_records(csv).unwrap();
        let records: Vec<serde_json::Value> = serde_json::from_slice(&json).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0]["name"], "Alice");
        assert_eq!(records[0]["age"], 30);
        assert_eq!(records[1]["name"], "Bob");
        assert_eq!(records[1]["age"], 25);
    }

    #[test]
    fn test_csv_to_yaml_records() {
        let csv = b"name,age\nAlice,30";
        let yaml = csv_to_yaml_records(csv).unwrap();
        let yaml_str = std::str::from_utf8(&yaml).unwrap();
        assert!(yaml_str.contains("name:"));
        assert!(yaml_str.contains("Alice"));
    }

    #[test]
    fn test_yaml_records_to_csv() {
        let yaml = b"- name: Alice\n  age: 30\n- name: Bob\n  age: 25";
        let csv_bytes = yaml_records_to_csv(yaml).unwrap();
        let csv_str = std::str::from_utf8(&csv_bytes).unwrap();
        assert!(csv_str.contains("Alice"));
        assert!(csv_str.contains("Bob"));
    }

    #[test]
    fn test_roundtrip_json_yaml_json() {
        let original = br#"{"key": "value", "num": 42, "flag": true}"#;
        let yaml = json_to_yaml(original).unwrap();
        let json = yaml_to_json(&yaml).unwrap();
        let orig_val: serde_json::Value = serde_json::from_slice(original).unwrap();
        let round_val: serde_json::Value = serde_json::from_slice(&json).unwrap();
        assert_eq!(orig_val, round_val);
    }

    #[test]
    fn test_roundtrip_csv_json_csv() {
        let csv = b"name,age\nAlice,30\nBob,25";
        let json = csv_to_json_records(csv).unwrap();
        let csv2 = json_records_to_csv(&json).unwrap();
        // Re-parse both CSVs to compare structurally
        let records1 = csv_to_json_records(csv).unwrap();
        let records2 = csv_to_json_records(&csv2).unwrap();
        assert_eq!(records1, records2);
    }

    #[test]
    fn test_empty_json_array_to_csv() {
        let json = b"[]";
        let csv_bytes = json_records_to_csv(json).unwrap();
        assert!(csv_bytes.is_empty());
    }

    #[test]
    fn test_malformed_json_fails() {
        let bad = b"not json at all";
        let result = json_to_yaml(bad);
        assert!(result.is_err());
    }

    #[test]
    fn test_malformed_yaml_fails() {
        let bad = b"{{{{invalid yaml";
        let result = yaml_to_json(bad);
        assert!(result.is_err());
    }

    #[test]
    fn test_csv_type_inference() {
        assert_eq!(infer_csv_value(""), serde_json::Value::Null);
        assert_eq!(infer_csv_value("42"), serde_json::json!(42));
        assert_eq!(infer_csv_value("3.14"), serde_json::json!(3.14));
        assert_eq!(infer_csv_value("true"), serde_json::json!(true));
        assert_eq!(infer_csv_value("false"), serde_json::json!(false));
        assert_eq!(infer_csv_value("hello"), serde_json::json!("hello"));
    }

    #[test]
    fn test_csv_with_mixed_columns() {
        let csv = b"id,name,active,score\n1,Alice,true,95.5\n2,Bob,false,";
        let json = csv_to_json_records(csv).unwrap();
        let records: Vec<serde_json::Value> = serde_json::from_slice(&json).unwrap();
        assert_eq!(records[0]["id"], 1);
        assert_eq!(records[0]["active"], true);
        assert_eq!(records[0]["score"], 95.5);
        assert_eq!(records[1]["score"], serde_json::Value::Null);
    }

    #[test]
    fn test_yaml_tagged_values_stripped() {
        let yaml = b"!!str 42";
        let json = yaml_to_json(yaml).unwrap();
        let val: serde_json::Value = serde_json::from_slice(&json).unwrap();
        // Tagged !!str should strip the tag and give us the inner value
        assert!(val.is_string() || val.is_number());
    }

    #[test]
    fn test_json_records_superset_headers() {
        // Records with different keys — all keys should appear as headers
        let json = br#"[{"a": 1}, {"b": 2}, {"a": 3, "c": 4}]"#;
        let csv_bytes = json_records_to_csv(json).unwrap();
        let csv_str = std::str::from_utf8(&csv_bytes).unwrap();
        let first_line = csv_str.lines().next().unwrap();
        assert!(first_line.contains('a'));
        assert!(first_line.contains('b'));
        assert!(first_line.contains('c'));
    }
}
