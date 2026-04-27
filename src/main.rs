//! datacartridge - Data format conversion and type coercion cartridge
//!
//! Format conversion (JSON/YAML/CSV where structurally compatible):
//! - JSON <-> YAML: value, record, list, list-of-records
//! - JSON list-of-records <-> CSV
//! - YAML list-of-records <-> CSV
//!
//! Type coercion (scalar type conversions):
//! - To string: from integer, number, boolean, object, and array types
//! - To integer: from string, number, boolean
//! - To number: from string, integer, boolean
//! - To object: from string, integer, number, boolean

mod adapter;

use anyhow::Result;
use capdag::{
    async_trait, ArgSource, Cap, CapArg, CapGroup, CapManifest, DryContext, Op, OpError, OpResult,
    CartridgeRuntime, Request, WetContext, WET_KEY_REQUEST, CAP_ADAPTER_SELECTION,
};
use std::sync::Arc;

// =============================================================================
// MANIFEST
// =============================================================================

fn build_manifest() -> CapManifest {
    let mut all_caps = vec![capdag::identity_cap()];

    for path in capdag::all_format_conversion_paths() {
        let urn = capdag::format_conversion_urn(path.in_media, path.out_media);

        let mut cap = Cap::with_description(urn, path.title.to_string(), "convert_format".to_string(), path.description.to_string());
        cap.add_arg(CapArg::with_description(
            path.in_media,
            true,
            vec![
                ArgSource::Stdin {
                    stdin: path.in_media.to_string(),
                },
                ArgSource::Position { position: 0 },
            ],
            "Input data to convert".to_string(),
        ));
        cap.set_output(capdag::CapOutput::new(path.out_media, "Converted data"));
        all_caps.push(cap);
    }

    // Coercion caps
    for (source_type, target_type) in capdag::all_coercion_paths() {
        let urn = capdag::coercion_urn(source_type, target_type);
        let in_media = capdag::media_urn_for_type(source_type);
        let out_media = capdag::media_urn_for_type(target_type);
        let title = format!("Coerce {} to {}", source_type, target_type);
        let description = format!("Coerce data from {} to {}", source_type, target_type);

        let mut cap = Cap::with_description(urn, title, "coerce".to_string(), description);
        cap.add_arg(CapArg::with_description(
            in_media,
            true,
            vec![
                ArgSource::Stdin {
                    stdin: in_media.to_string(),
                },
                ArgSource::Position { position: 0 },
            ],
            "Input data to coerce".to_string(),
        ));
        cap.set_output(capdag::CapOutput::new(out_media, "Coerced data"));
        all_caps.push(cap);
    }

    // Collect JSON objects caps
    for out_media in &["media:json;list;record;textable", "media:csv;list;record;textable", "media:list;record;textable;yaml"] {
        let urn = capdag::CapUrnBuilder::new()
            .tag("op", "collect_json_objects")
            .in_spec("media:json;record;textable")
            .out_spec(out_media)
            .build().expect("collect_json_objects URN");
        let mut cap = Cap::with_description(urn, format!("Collect JSON Objects into {}", out_media), "collect_json_objects".to_string(), format!("Collect individual JSON objects into {}", out_media));
        let mut arg = CapArg::with_description(
            "media:json;record;textable", true,
            vec![ArgSource::Stdin { stdin: "media:json;record;textable".to_string() }, ArgSource::Position { position: 0 }],
            "JSON objects to collect",
        );
        arg.is_sequence = true;
        cap.add_arg(arg);
        cap.set_output(capdag::CapOutput::new(*out_media, "Collected output"));
        all_caps.push(cap);
    }

    // Collect CSV records caps
    for out_media in &["media:csv;list;record;textable", "media:json;list;record;textable", "media:list;record;textable;yaml"] {
        let urn = capdag::CapUrnBuilder::new()
            .tag("op", "collect_records")
            .in_spec("media:csv;list;record;textable")
            .out_spec(out_media)
            .build().expect("collect_records csv URN");
        let mut cap = Cap::with_description(urn, format!("Merge CSV into {}", out_media), "collect_records".to_string(), format!("Merge CSV files into {}", out_media));
        let mut arg = CapArg::with_description(
            "media:csv;list;record;textable", true,
            vec![ArgSource::Stdin { stdin: "media:csv;list;record;textable".to_string() }, ArgSource::Position { position: 0 }],
            "CSV files to merge",
        );
        arg.is_sequence = true;
        cap.add_arg(arg);
        cap.set_output(capdag::CapOutput::new(*out_media, "Merged output"));
        all_caps.push(cap);
    }

    // Collect YAML mappings caps
    for out_media in &["media:list;record;textable;yaml", "media:json;list;record;textable", "media:csv;list;record;textable"] {
        let urn = capdag::CapUrnBuilder::new()
            .tag("op", "collect_records")
            .in_spec("media:record;textable;yaml")
            .out_spec(out_media)
            .build().expect("collect_records yaml URN");
        let mut cap = Cap::with_description(urn, format!("Merge YAML into {}", out_media), "collect_records".to_string(), format!("Merge YAML mapping files into {}", out_media));
        let mut arg = CapArg::with_description(
            "media:record;textable;yaml", true,
            vec![ArgSource::Stdin { stdin: "media:record;textable;yaml".to_string() }, ArgSource::Position { position: 0 }],
            "YAML mapping files to merge",
        );
        arg.is_sequence = true;
        cap.add_arg(arg);
        cap.set_output(capdag::CapOutput::new(*out_media, "Merged output"));
        all_caps.push(cap);
    }

    // All caps in a single cap group with data format adapter URNs
    let data_group = CapGroup {
        name: "data-formats".to_string(),
        caps: all_caps, // identity_cap() is already first in all_caps
        adapter_urns: vec![
            "media:json".to_string(),
            "media:ndjson".to_string(),
            "media:csv".to_string(),
            "media:tsv".to_string(),
            "media:psv".to_string(),
            "media:yaml".to_string(),
            "media:xml".to_string(),
            "media:toml".to_string(),
        ],
    };

    CapManifest::new(
        "datacartridge".to_string(),
        env!("CARGO_PKG_VERSION").to_string(),
        capdag::CartridgeChannel::from_build_env(env!("MFR_CARTRIDGE_CHANNEL")),
        // Registry URL is baked at compile time via
        // `MFR_REGISTRY_URL`. Unset (plain `cargo build`) ⇒ dev
        // build, manifest emits null.
        option_env!("MFR_REGISTRY_URL").map(|s| s.to_string()),
        "Data format conversion, type coercion, and data format content inspection".to_string(),
        vec![data_group],
    )
    .with_author("https://github.com/machinefabric".to_string())
    .with_page_url("https://github.com/machinefabric/datacartridge".to_string())
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

        output.log("INFO", &format!(
            "[convert_format] collect_streams returned {} streams", streams.len()
        ));
        for (i, (urn, bytes, _meta)) in streams.iter().enumerate() {
            output.log("INFO", &format!(
                "[convert_format]   stream[{}]: urn='{}', {} bytes, preview={:?}",
                i, urn, bytes.len(),
                String::from_utf8_lossy(&bytes[..bytes.len().min(100)])
            ));
        }

        // Find input data by the expected media URN — fail hard if not supplied
        let data = capdag::require_stream(&streams, self.in_media)
            .map_err(|e| OpError::ExecutionFailed(format!(
                "Expected input stream '{}' not found: {}", self.in_media, e
            )))?;

        output.log("INFO", &format!(
            "[convert_format] require_stream('{}') -> {} bytes, preview={:?}",
            self.in_media, data.len(),
            String::from_utf8_lossy(&data[..data.len().min(100)])
        ));

        // Determine conversion from the registered in/out media (not from content)
        let from = format_of_str(self.in_media)
            .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;
        let to = format_of_str(self.out_media)
            .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;

        // Scalar→Scalar: propagate input stream meta to output
        let input_meta = capdag::find_stream_meta(&streams, self.in_media).cloned();
        output.start(false, input_meta)
            .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;
        output.progress(0.10, "Converting format");
        output.log("INFO", &format!(
            "[convert_format] converting {:?} -> {:?}, {} bytes input",
            from, to, data.len()
        ));
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
// COERCION OP
// =============================================================================

struct CoerceOp {
    source_type: &'static str,
    target_type: &'static str,
}

#[async_trait]
impl Op<()> for CoerceOp {
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

        let in_media = capdag::media_urn_for_type(self.source_type);
        let data = capdag::require_stream(&streams, in_media)
            .map_err(|e| OpError::ExecutionFailed(format!(
                "Expected input stream '{}' not found: {}", in_media, e
            )))?;

        // Scalar→Scalar: propagate input stream meta to output
        let input_meta = capdag::find_stream_meta(&streams, in_media).cloned();
        output.start(false, input_meta)
            .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;
        output.progress(0.10, "Coercing type");

        let result = coerce(data, self.source_type, self.target_type)
            .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;

        output.progress(0.90, "Encoding output");
        let cbor_value = ciborium::Value::Text(
            String::from_utf8(result)
                .map_err(|e| OpError::ExecutionFailed(format!("Coercion output is not valid UTF-8: {}", e)))?,
        );
        output
            .emit_cbor(&cbor_value)
            .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;

        Ok(())
    }

    fn metadata(&self) -> capdag::OpMetadata {
        capdag::OpMetadata::builder("CoerceOp").build()
    }
}

// =============================================================================
// COLLECT OPS — sequence of items → single merged output
// =============================================================================

struct CollectJsonObjectsOp {
    out_media: &'static str,
}

#[async_trait]
impl Op<()> for CollectJsonObjectsOp {
    async fn perform(&self, _dry: &mut DryContext, wet: &mut WetContext) -> OpResult<()> {
        let req: Arc<Request> = wet
            .get_required(WET_KEY_REQUEST)
            .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;
        let mut input = req
            .take_input()
            .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;
        let output = req.output();

        // Sequence input: collect each item separately (one JSON object per item)
        let mut json_items: Vec<Vec<u8>> = Vec::new();
        while let Some(stream_result) = input.recv().await {
            let stream = stream_result
                .map_err(|e| OpError::ExecutionFailed(format!("Stream error: {}", e)))?;
            let items = stream.collect_items().await
                .map_err(|e| OpError::ExecutionFailed(format!("Collect items error: {}", e)))?;
            for (bytes, _meta) in items {
                json_items.push(bytes);
            }
        }

        output.start(false, None)
            .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;
        output.progress(0.50, "Collecting objects");

        let result = collect_json_objects_to(&json_items, self.out_media)
            .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;

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
        capdag::OpMetadata::builder("CollectJsonObjectsOp").build()
    }
}

struct CollectRecordsOp {
    in_media: &'static str,
    out_media: &'static str,
}

#[async_trait]
impl Op<()> for CollectRecordsOp {
    async fn perform(&self, _dry: &mut DryContext, wet: &mut WetContext) -> OpResult<()> {
        let req: Arc<Request> = wet
            .get_required(WET_KEY_REQUEST)
            .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;
        let mut input = req
            .take_input()
            .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;
        let output = req.output();

        // Sequence input: collect each item separately
        let mut raw_items: Vec<Vec<u8>> = Vec::new();
        while let Some(stream_result) = input.recv().await {
            let stream = stream_result
                .map_err(|e| OpError::ExecutionFailed(format!("Stream error: {}", e)))?;
            let items = stream.collect_items().await
                .map_err(|e| OpError::ExecutionFailed(format!("Collect items error: {}", e)))?;
            for (bytes, _meta) in items {
                raw_items.push(bytes);
            }
        }

        output.start(false, None)
            .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;
        output.progress(0.50, "Collecting records");

        let result = collect_records_to(&raw_items, self.in_media, self.out_media)
            .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;

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
        capdag::OpMetadata::builder("CollectRecordsOp").build()
    }
}

/// Collect JSON objects into the target format.
fn collect_json_objects_to(items: &[Vec<u8>], out_media: &str) -> Result<Vec<u8>> {
    // Parse each item as a JSON object
    let objects: Vec<serde_json::Value> = items.iter()
        .enumerate()
        .map(|(i, bytes)| {
            serde_json::from_slice(bytes)
                .map_err(|e| anyhow::anyhow!("Item {} is not valid JSON: {}", i, e))
        })
        .collect::<Result<Vec<_>>>()?;

    if out_media.contains("csv") {
        json_objects_to_csv(&objects)
    } else if out_media.contains("yaml") {
        json_objects_to_yaml(&objects)
    } else {
        // Default: JSON array
        serde_json::to_vec_pretty(&objects)
            .map_err(|e| anyhow::anyhow!("Failed to serialize JSON array: {}", e))
    }
}

/// Collect records (CSV or YAML) into the target format.
fn collect_records_to(items: &[Vec<u8>], in_media: &str, out_media: &str) -> Result<Vec<u8>> {
    // Parse each item into JSON objects based on source format
    let mut all_objects: Vec<serde_json::Value> = Vec::new();

    for (i, bytes) in items.iter().enumerate() {
        if in_media.contains("csv") {
            // Parse CSV rows as JSON objects
            let mut rdr = csv::Reader::from_reader(&bytes[..]);
            let headers: Vec<String> = rdr.headers()
                .map_err(|e| anyhow::anyhow!("CSV item {} has no headers: {}", i, e))?
                .iter()
                .map(|h| h.to_string())
                .collect();
            for row_result in rdr.records() {
                let row = row_result
                    .map_err(|e| anyhow::anyhow!("CSV item {} row error: {}", i, e))?;
                let mut obj = serde_json::Map::new();
                for (h, v) in headers.iter().zip(row.iter()) {
                    obj.insert(h.clone(), serde_json::Value::String(v.to_string()));
                }
                all_objects.push(serde_json::Value::Object(obj));
            }
        } else if in_media.contains("yaml") {
            // Parse YAML mapping as JSON object
            let value: serde_json::Value = serde_yaml::from_slice(bytes)
                .map_err(|e| anyhow::anyhow!("YAML item {} parse error: {}", i, e))?;
            all_objects.push(value);
        } else {
            // Try JSON
            let value: serde_json::Value = serde_json::from_slice(bytes)
                .map_err(|e| anyhow::anyhow!("JSON item {} parse error: {}", i, e))?;
            all_objects.push(value);
        }
    }

    // Produce output
    if out_media.contains("csv") {
        json_objects_to_csv(&all_objects)
    } else if out_media.contains("yaml") {
        json_objects_to_yaml(&all_objects)
    } else {
        serde_json::to_vec_pretty(&all_objects)
            .map_err(|e| anyhow::anyhow!("Failed to serialize JSON array: {}", e))
    }
}

/// Convert a list of JSON objects to CSV bytes.
fn json_objects_to_csv(objects: &[serde_json::Value]) -> Result<Vec<u8>> {
    if objects.is_empty() {
        return Ok(Vec::new());
    }
    // Collect all unique keys across all objects for headers
    let mut headers: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for obj in objects {
        if let serde_json::Value::Object(map) = obj {
            for key in map.keys() {
                if seen.insert(key.clone()) {
                    headers.push(key.clone());
                }
            }
        }
    }

    let mut wtr = csv::Writer::from_writer(Vec::new());
    wtr.write_record(&headers)
        .map_err(|e| anyhow::anyhow!("Failed to write CSV header: {}", e))?;
    for obj in objects {
        let row: Vec<String> = headers.iter()
            .map(|h| {
                obj.get(h)
                    .map(|v| match v {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    })
                    .unwrap_or_default()
            })
            .collect();
        wtr.write_record(&row)
            .map_err(|e| anyhow::anyhow!("Failed to write CSV row: {}", e))?;
    }
    wtr.into_inner()
        .map_err(|e| anyhow::anyhow!("Failed to finalize CSV: {}", e))
}

/// Convert a list of JSON objects to YAML sequence bytes.
fn json_objects_to_yaml(objects: &[serde_json::Value]) -> Result<Vec<u8>> {
    let yaml_str = serde_yaml::to_string(objects)
        .map_err(|e| anyhow::anyhow!("Failed to serialize YAML: {}", e))?;
    Ok(yaml_str.into_bytes())
}

// =============================================================================
// ADAPTER SELECTION
// =============================================================================

/// Data format content inspection adapter.
/// Detects JSON, NDJSON, CSV, TSV, PSV, YAML, XML, and TOML formats.
struct DataAdapterSelectionOp;

#[async_trait]
impl Op<()> for DataAdapterSelectionOp {
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

        let content = streams
            .first()
            .map(|(_, bytes, _)| bytes.as_slice())
            .unwrap_or(&[]);

        // Extract extension from stream meta if available
        let extension = streams
            .first()
            .and_then(|(_, _, meta)| meta.as_ref())
            .and_then(|m| m.get("file_path"))
            .and_then(|v| {
                if let ciborium::Value::Text(s) = v {
                    std::path::Path::new(s.as_str())
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.to_lowercase())
                } else {
                    None
                }
            })
            .unwrap_or_default();

        let media_urns = adapter::detect_data_media_urns(content, &extension);

        if media_urns.is_empty() {
            return Ok(());
        }

        let response = serde_json::json!({ "media_urns": media_urns });
        let json_bytes = serde_json::to_vec(&response)
            .map_err(|e| OpError::ExecutionFailed(format!("JSON error: {}", e)))?;

        output
            .start(false, None)
            .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;
        output
            .emit_cbor(&ciborium::Value::Bytes(json_bytes))
            .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;

        Ok(())
    }

    fn metadata(&self) -> capdag::OpMetadata {
        capdag::OpMetadata::builder("DataAdapterSelectionOp")
            .description("Data format content inspection adapter")
            .build()
    }
}

// =============================================================================
// MAIN
// =============================================================================

#[tokio::main]
async fn main() -> Result<()> {
    let manifest = build_manifest();
    let mut runtime = CartridgeRuntime::with_manifest(manifest);

    // Register adapter selection handler
    runtime.register_op(CAP_ADAPTER_SELECTION, || Box::new(DataAdapterSelectionOp));

    for path in capdag::all_format_conversion_paths() {
        let urn = capdag::format_conversion_urn(path.in_media, path.out_media);
        let in_media = path.in_media;
        let out_media = path.out_media;
        runtime.register_op(&urn.to_string(), move || {
            Box::new(ConvertFormatOp { in_media, out_media })
        });
    }

    for (source_type, target_type) in capdag::all_coercion_paths() {
        let urn = capdag::coercion_urn(source_type, target_type);
        runtime.register_op(&urn.to_string(), move || {
            Box::new(CoerceOp { source_type, target_type })
        });
    }

    // Collect JSON objects → JSON array / CSV / YAML
    {
        let json_array_urn = capdag::CapUrnBuilder::new()
            .tag("op", "collect_json_objects")
            .in_spec("media:json;record;textable")
            .out_spec("media:json;list;record;textable")
            .build().expect("collect_json_objects → json array URN");
        runtime.register_op(&json_array_urn.to_string(), || {
            Box::new(CollectJsonObjectsOp { out_media: "media:json;list;record;textable" })
        });

        let csv_urn = capdag::CapUrnBuilder::new()
            .tag("op", "collect_json_objects")
            .in_spec("media:json;record;textable")
            .out_spec("media:csv;list;record;textable")
            .build().expect("collect_json_objects → csv URN");
        runtime.register_op(&csv_urn.to_string(), || {
            Box::new(CollectJsonObjectsOp { out_media: "media:csv;list;record;textable" })
        });

        let yaml_urn = capdag::CapUrnBuilder::new()
            .tag("op", "collect_json_objects")
            .in_spec("media:json;record;textable")
            .out_spec("media:list;record;textable;yaml")
            .build().expect("collect_json_objects → yaml URN");
        runtime.register_op(&yaml_urn.to_string(), || {
            Box::new(CollectJsonObjectsOp { out_media: "media:list;record;textable;yaml" })
        });
    }

    // Collect CSV records → merged CSV / JSON array / YAML list
    {
        let csv_csv_urn = capdag::CapUrnBuilder::new()
            .tag("op", "collect_records")
            .in_spec("media:csv;list;record;textable")
            .out_spec("media:csv;list;record;textable")
            .build().expect("collect_records csv→csv URN");
        runtime.register_op(&csv_csv_urn.to_string(), || {
            Box::new(CollectRecordsOp { in_media: "media:csv;list;record;textable", out_media: "media:csv;list;record;textable" })
        });

        let csv_json_urn = capdag::CapUrnBuilder::new()
            .tag("op", "collect_records")
            .in_spec("media:csv;list;record;textable")
            .out_spec("media:json;list;record;textable")
            .build().expect("collect_records csv→json URN");
        runtime.register_op(&csv_json_urn.to_string(), || {
            Box::new(CollectRecordsOp { in_media: "media:csv;list;record;textable", out_media: "media:json;list;record;textable" })
        });

        let csv_yaml_urn = capdag::CapUrnBuilder::new()
            .tag("op", "collect_records")
            .in_spec("media:csv;list;record;textable")
            .out_spec("media:list;record;textable;yaml")
            .build().expect("collect_records csv→yaml URN");
        runtime.register_op(&csv_yaml_urn.to_string(), || {
            Box::new(CollectRecordsOp { in_media: "media:csv;list;record;textable", out_media: "media:list;record;textable;yaml" })
        });
    }

    // Collect YAML mappings → merged YAML list / JSON array / CSV
    {
        let yaml_yaml_urn = capdag::CapUrnBuilder::new()
            .tag("op", "collect_records")
            .in_spec("media:record;textable;yaml")
            .out_spec("media:list;record;textable;yaml")
            .build().expect("collect_records yaml→yaml URN");
        runtime.register_op(&yaml_yaml_urn.to_string(), || {
            Box::new(CollectRecordsOp { in_media: "media:record;textable;yaml", out_media: "media:list;record;textable;yaml" })
        });

        let yaml_json_urn = capdag::CapUrnBuilder::new()
            .tag("op", "collect_records")
            .in_spec("media:record;textable;yaml")
            .out_spec("media:json;list;record;textable")
            .build().expect("collect_records yaml→json URN");
        runtime.register_op(&yaml_json_urn.to_string(), || {
            Box::new(CollectRecordsOp { in_media: "media:record;textable;yaml", out_media: "media:json;list;record;textable" })
        });

        let yaml_csv_urn = capdag::CapUrnBuilder::new()
            .tag("op", "collect_records")
            .in_spec("media:record;textable;yaml")
            .out_spec("media:csv;list;record;textable")
            .build().expect("collect_records yaml→csv URN");
        runtime.register_op(&yaml_csv_urn.to_string(), || {
            Box::new(CollectRecordsOp { in_media: "media:record;textable;yaml", out_media: "media:csv;list;record;textable" })
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
    /// Bare textable list — CBOR sequence of byte strings, no format tag.
    TextableList,
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
    } else if urn.is_list() && !urn.is_json() && !urn.is_yaml() && !urn.is_csv() {
        Ok(Fmt::TextableList)
    } else {
        anyhow::bail!("Media URN '{}' is not a recognized data format (json, yaml, csv, or textable list)", media_urn)
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
        // Textable list conversions
        (Fmt::TextableList, Fmt::Json) => textable_list_to_json(data),
        (Fmt::Json, Fmt::TextableList) => json_to_textable_list(data),
        (Fmt::TextableList, Fmt::Yaml) => textable_list_to_yaml(data),
        (Fmt::Yaml, Fmt::TextableList) => yaml_to_textable_list(data),
        (Fmt::TextableList, Fmt::Csv) => textable_list_to_csv(data),
        (Fmt::Csv, Fmt::TextableList) => csv_to_textable_list(data),
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
// TEXTABLE LIST CONVERSIONS
// =============================================================================

/// Decode a CBOR sequence of byte strings into a Vec of raw UTF-8 strings.
/// Each item in the CBOR sequence is expected to be a Value::Bytes containing UTF-8 text.
/// Decode a textable list: plain text with one value per line.
/// Empty lines are skipped. Trailing newline is tolerated.
fn decode_textable_list(data: &[u8]) -> Result<Vec<String>> {
    let text = std::str::from_utf8(data)
        .map_err(|e| anyhow::anyhow!("Textable list is not valid UTF-8: {}", e))?;
    Ok(text.lines()
        .map(|line| line.to_string())
        .filter(|line| !line.is_empty())
        .collect())
}

/// Encode a Vec of strings into a textable list: one value per line.
fn encode_textable_list(items: &[String]) -> Vec<u8> {
    let mut result = String::new();
    for item in items {
        result.push_str(item);
        result.push('\n');
    }
    result.into_bytes()
}

/// Textable list (CBOR sequence) -> JSON array.
/// Each item is parsed as a JSON value if possible, otherwise kept as a JSON string.
fn textable_list_to_json(data: &[u8]) -> Result<Vec<u8>> {
    let items = decode_textable_list(data)?;
    let json_values: Vec<serde_json::Value> = items.iter()
        .map(|s| serde_json::from_str(s).unwrap_or_else(|_| serde_json::Value::String(s.clone())))
        .collect();
    serde_json::to_vec_pretty(&json_values)
        .map_err(|e| anyhow::anyhow!("Failed to serialize to JSON: {}", e))
}

/// JSON array -> textable list (one value per line).
/// Each JSON value is serialized to its string representation.
fn json_to_textable_list(data: &[u8]) -> Result<Vec<u8>> {
    let values: Vec<serde_json::Value> = serde_json::from_slice(data)
        .map_err(|e| anyhow::anyhow!("Invalid JSON array: {}", e))?;
    let strings: Vec<String> = values.iter()
        .map(|v| match v {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        })
        .collect();
    Ok(encode_textable_list(&strings))
}

/// Textable list (one value per line) -> YAML sequence.
/// Each item is parsed as a YAML value if possible, otherwise kept as a string.
fn textable_list_to_yaml(data: &[u8]) -> Result<Vec<u8>> {
    let items = decode_textable_list(data)?;
    let yaml_values: Vec<serde_yaml::Value> = items.iter()
        .map(|s| serde_yaml::from_str(s).unwrap_or_else(|_| serde_yaml::Value::String(s.clone())))
        .collect();
    let yaml_str = serde_yaml::to_string(&yaml_values)
        .map_err(|e| anyhow::anyhow!("Failed to serialize to YAML: {}", e))?;
    Ok(yaml_str.into_bytes())
}

/// YAML sequence -> textable list (one value per line).
/// Each YAML value is serialized to its string representation.
fn yaml_to_textable_list(data: &[u8]) -> Result<Vec<u8>> {
    let values: Vec<serde_yaml::Value> = serde_yaml::from_slice(data)
        .map_err(|e| anyhow::anyhow!("Invalid YAML sequence: {}", e))?;
    let strings: Vec<String> = values.iter()
        .map(|v| {
            let s = serde_yaml::to_string(v).unwrap_or_default();
            s.trim_start_matches("---\n").trim_end().to_string()
        })
        .collect();
    Ok(encode_textable_list(&strings))
}

/// Textable list (one value per line) -> CSV.
/// Single-column CSV with header "value".
fn textable_list_to_csv(data: &[u8]) -> Result<Vec<u8>> {
    let items = decode_textable_list(data)?;
    let mut wtr = csv::Writer::from_writer(Vec::new());
    wtr.write_record(&["value"])
        .map_err(|e| anyhow::anyhow!("Failed to write CSV header: {}", e))?;
    for item in &items {
        wtr.write_record(&[item])
            .map_err(|e| anyhow::anyhow!("Failed to write CSV row: {}", e))?;
    }
    wtr.into_inner()
        .map_err(|e| anyhow::anyhow!("Failed to finalize CSV: {}", e))
}

/// CSV -> textable list (one value per line).
/// Reads the first column of each row (ignoring headers).
fn csv_to_textable_list(data: &[u8]) -> Result<Vec<u8>> {
    let mut rdr = csv::Reader::from_reader(data);
    let mut items = Vec::new();
    for result in rdr.records() {
        let row = result.map_err(|e| anyhow::anyhow!("Failed to read CSV row: {}", e))?;
        let value = row.get(0).unwrap_or("").to_string();
        items.push(value);
    }
    Ok(encode_textable_list(&items))
}

// =============================================================================
// COERCION FUNCTIONS
// =============================================================================

fn coerce(data: &[u8], _source_type: &str, target_type: &str) -> Result<Vec<u8>> {
    let s = std::str::from_utf8(data)
        .map_err(|e| anyhow::anyhow!("Content is not valid UTF-8: {}", e))?;
    match target_type {
        "string" => coerce_to_string(s),
        "integer" => coerce_to_integer(s),
        "number" => coerce_to_number(s),
        "object" => coerce_to_object(s, _source_type),
        other => anyhow::bail!("Unsupported coercion target type: '{}'", other),
    }
}

fn coerce_to_string(s: &str) -> Result<Vec<u8>> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(s) {
        let result = match value {
            serde_json::Value::String(s) => format!("\"{}\"", s),
            serde_json::Value::Number(n) => format!("\"{}\"", n),
            serde_json::Value::Bool(b) => format!("\"{}\"", b),
            serde_json::Value::Null => "\"null\"".to_string(),
            serde_json::Value::Array(ref arr) => {
                serde_json::to_string(arr)
                    .map(|s| format!("\"{}\"", s.replace('\"', "\\\"")))
                    .unwrap_or_else(|_| format!("\"{}\"", value))
            }
            serde_json::Value::Object(ref _obj) => {
                serde_json::to_string(&value)
                    .map(|s| format!("\"{}\"", s.replace('\"', "\\\"")))
                    .unwrap_or_else(|_| format!("\"{}\"", value))
            }
        };
        return Ok(result.into_bytes());
    }
    Ok(format!("\"{}\"", s).into_bytes())
}

fn coerce_to_integer(s: &str) -> Result<Vec<u8>> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(s) {
        match value {
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    return Ok(i.to_string().into_bytes());
                } else if let Some(f) = n.as_f64() {
                    return Ok((f.round() as i64).to_string().into_bytes());
                }
            }
            serde_json::Value::String(s) => {
                if let Ok(i) = s.parse::<i64>() {
                    return Ok(i.to_string().into_bytes());
                } else if let Ok(f) = s.parse::<f64>() {
                    return Ok((f.round() as i64).to_string().into_bytes());
                }
            }
            serde_json::Value::Bool(b) => {
                return Ok(if b { b"1".to_vec() } else { b"0".to_vec() });
            }
            _ => {}
        }
    }
    let trimmed = s.trim();
    if let Ok(i) = trimmed.parse::<i64>() {
        return Ok(i.to_string().into_bytes());
    }
    if let Ok(f) = trimmed.parse::<f64>() {
        return Ok((f.round() as i64).to_string().into_bytes());
    }
    anyhow::bail!("Cannot coerce content to integer: '{}'", s)
}

fn coerce_to_number(s: &str) -> Result<Vec<u8>> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(s) {
        match value {
            serde_json::Value::Number(n) => {
                if let Some(f) = n.as_f64() {
                    return Ok(f.to_string().into_bytes());
                }
            }
            serde_json::Value::String(s) => {
                if let Ok(f) = s.parse::<f64>() {
                    return Ok(f.to_string().into_bytes());
                }
            }
            serde_json::Value::Bool(b) => {
                return Ok(if b { b"1.0".to_vec() } else { b"0.0".to_vec() });
            }
            _ => {}
        }
    }
    let trimmed = s.trim();
    if let Ok(f) = trimmed.parse::<f64>() {
        return Ok(f.to_string().into_bytes());
    }
    anyhow::bail!("Cannot coerce content to number: '{}'", s)
}

fn coerce_to_object(s: &str, source_type: &str) -> Result<Vec<u8>> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(s) {
        match value {
            serde_json::Value::Object(_) => {
                return Ok(s.as_bytes().to_vec());
            }
            _ => {
                let source_media = capdag::media_urn_for_type(source_type);
                let obj = serde_json::json!({
                    "value": value,
                    "source_type": source_media
                });
                return serde_json::to_vec(&obj)
                    .map_err(|e| anyhow::anyhow!("Failed to serialize to JSON object: {}", e));
            }
        }
    }
    let source_media = capdag::media_urn_for_type(source_type);
    let obj = serde_json::json!({
        "value": s,
        "source_type": source_media
    });
    serde_json::to_vec(&obj)
        .map_err(|e| anyhow::anyhow!("Failed to serialize to JSON object: {}", e))
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

    // =========================================================================
    // COERCION TESTS
    // =========================================================================

    #[test]
    fn test_coerce_integer_to_string() {
        let result = coerce(b"42", "integer", "string").unwrap();
        assert_eq!(std::str::from_utf8(&result).unwrap(), "\"42\"");
    }

    #[test]
    fn test_coerce_number_to_string() {
        let result = coerce(b"3.14", "number", "string").unwrap();
        assert_eq!(std::str::from_utf8(&result).unwrap(), "\"3.14\"");
    }

    #[test]
    fn test_coerce_boolean_to_string() {
        let result = coerce(b"true", "boolean", "string").unwrap();
        assert_eq!(std::str::from_utf8(&result).unwrap(), "\"true\"");
    }

    #[test]
    fn test_coerce_string_to_integer() {
        let result = coerce(b"\"42\"", "string", "integer").unwrap();
        assert_eq!(std::str::from_utf8(&result).unwrap(), "42");
    }

    #[test]
    fn test_coerce_number_to_integer() {
        let result = coerce(b"3.7", "number", "integer").unwrap();
        assert_eq!(std::str::from_utf8(&result).unwrap(), "4");
    }

    #[test]
    fn test_coerce_boolean_to_integer() {
        assert_eq!(coerce(b"true", "boolean", "integer").unwrap(), b"1");
        assert_eq!(coerce(b"false", "boolean", "integer").unwrap(), b"0");
    }

    #[test]
    fn test_coerce_string_to_number() {
        let result = coerce(b"\"3.14\"", "string", "number").unwrap();
        assert_eq!(std::str::from_utf8(&result).unwrap(), "3.14");
    }

    #[test]
    fn test_coerce_integer_to_number() {
        let result = coerce(b"42", "integer", "number").unwrap();
        // 42 as f64 can be "42" or "42.0" depending on serde
        let s = std::str::from_utf8(&result).unwrap();
        let f: f64 = s.parse().unwrap();
        assert!((f - 42.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_coerce_boolean_to_number() {
        assert_eq!(coerce(b"true", "boolean", "number").unwrap(), b"1.0");
        assert_eq!(coerce(b"false", "boolean", "number").unwrap(), b"0.0");
    }

    #[test]
    fn test_coerce_string_to_object() {
        let result = coerce(b"\"hello\"", "string", "object").unwrap();
        let val: serde_json::Value = serde_json::from_slice(&result).unwrap();
        assert_eq!(val["value"], "hello");
        assert!(val["source_type"].as_str().unwrap().contains("textable"));
    }

    #[test]
    fn test_coerce_object_passthrough() {
        let input = br#"{"key": "value"}"#;
        let result = coerce(input, "object", "string").unwrap();
        let s = std::str::from_utf8(&result).unwrap();
        // Object to string wraps as JSON string
        assert!(s.starts_with('"'));
    }

    #[test]
    fn test_coerce_invalid_to_integer_fails() {
        let result = coerce(b"\"not a number\"", "string", "integer");
        assert!(result.is_err());
    }

    #[test]
    fn test_coerce_invalid_to_number_fails() {
        let result = coerce(b"\"not a number\"", "string", "number");
        assert!(result.is_err());
    }

    #[test]
    fn test_coerce_unsupported_target_fails() {
        let result = coerce(b"42", "integer", "array");
        assert!(result.is_err());
    }
}
