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
    for out_media in &["media:fmt=json;list;record", "media:fmt=csv;list;record", "media:fmt=yaml;list;record"] {
        let urn = capdag::CapUrnBuilder::new()
            .marker("collect-json-objects")
            .in_spec("media:fmt=json;record")
            .out_spec(out_media)
            .build().expect("collect_json_objects URN");
        let mut cap = Cap::with_description(urn, format!("Collect JSON Objects into {}", out_media), "collect_json_objects".to_string(), format!("Collect individual JSON objects into {}", out_media));
        let mut arg = CapArg::with_description(
            "media:fmt=json;record", true,
            vec![ArgSource::Stdin { stdin: "media:fmt=json;record".to_string() }, ArgSource::Position { position: 0 }],
            "JSON objects to collect",
        );
        arg.is_sequence = true;
        cap.add_arg(arg);
        cap.set_output(capdag::CapOutput::new(*out_media, "Collected output"));
        all_caps.push(cap);
    }

    // Collect CSV records caps
    for out_media in &["media:fmt=csv;list;record", "media:fmt=json;list;record", "media:fmt=yaml;list;record"] {
        let urn = capdag::CapUrnBuilder::new()
            .marker("collect-records")
            .in_spec("media:fmt=csv;list;record")
            .out_spec(out_media)
            .build().expect("collect_records csv URN");
        let mut cap = Cap::with_description(urn, format!("Merge CSV into {}", out_media), "collect_records".to_string(), format!("Merge CSV files into {}", out_media));
        let mut arg = CapArg::with_description(
            "media:fmt=csv;list;record", true,
            vec![ArgSource::Stdin { stdin: "media:fmt=csv;list;record".to_string() }, ArgSource::Position { position: 0 }],
            "CSV files to merge",
        );
        arg.is_sequence = true;
        cap.add_arg(arg);
        cap.set_output(capdag::CapOutput::new(*out_media, "Merged output"));
        all_caps.push(cap);
    }

    // Collect YAML mappings caps
    for out_media in &["media:fmt=yaml;list;record", "media:fmt=json;list;record", "media:fmt=csv;list;record"] {
        let urn = capdag::CapUrnBuilder::new()
            .marker("collect-records")
            .in_spec("media:fmt=yaml;record")
            .out_spec(out_media)
            .build().expect("collect_records yaml URN");
        let mut cap = Cap::with_description(urn, format!("Merge YAML into {}", out_media), "collect_records".to_string(), format!("Merge YAML mapping files into {}", out_media));
        let mut arg = CapArg::with_description(
            "media:fmt=yaml;record", true,
            vec![ArgSource::Stdin { stdin: "media:fmt=yaml;record".to_string() }, ArgSource::Position { position: 0 }],
            "YAML mapping files to merge",
        );
        arg.is_sequence = true;
        cap.add_arg(arg);
        cap.set_output(capdag::CapOutput::new(*out_media, "Merged output"));
        all_caps.push(cap);
    }

    // Save-as-txt cap: media:enc=utf-8;ext=txt;plain-text → same.
    // Pure relabel-and-persist. Input must already carry the
    // `plain-text` marker — that is the explicit opt-in that
    // keeps every text-producing cap (PDF page extractor,
    // JSON-as-text adapter, prompt loader) from becoming a
    // recommended feeder for `.txt` persistence. Producers of
    // genuinely-finalised prose (LLM text-generation, OCR's
    // extracted-text, vision descriptions, transcriptions) carry
    // the marker; coercion-class text values do not.
    //
    // Mirrors fabric/caps/save-as-txt.toml.
    {
        let urn = capdag::CapUrnBuilder::new()
            .marker("save-as-txt")
            .in_spec("media:enc=utf-8;ext=txt;plain-text")
            .out_spec("media:enc=utf-8;ext=txt;plain-text")
            .build()
            .expect("save-as-txt URN");
        let mut cap = Cap::with_description(
            urn,
            "Save as Plain Text".to_string(),
            "save_as_txt".to_string(),
            "Persist a finalised plain-text value as a .txt file. \
             Byte-for-byte passthrough."
                .to_string(),
        );
        cap.add_arg(CapArg::with_description(
            "media:enc=utf-8;ext=txt;plain-text",
            true,
            vec![
                ArgSource::Stdin {
                    stdin: "media:enc=utf-8;ext=txt;plain-text".to_string(),
                },
                ArgSource::Position { position: 0 },
            ],
            "The finalised plain-text value to persist as .txt".to_string(),
        ));
        cap.set_output(capdag::CapOutput::new(
            "media:enc=utf-8;ext=txt;plain-text",
            "The same bytes, persisted as a .txt file",
        ));
        all_caps.push(cap);
    }

    // Sequence decimate cap: generic over media. Takes a sequence of
    // any media URN and emits a sequence of the same media URN with
    // every Nth item kept. The argument `--keep-every` is N; missing
    // ⇒ N=1 ⇒ passthrough. Built once with `media:` (the wildcard
    // media URN) so the planner sees it as applicable to any
    // sequence input.
    {
        let urn = capdag::CapUrnBuilder::new()
            .marker("decimate-sequence")
            .in_spec("media:")
            .out_spec("media:")
            .effect(capdag::CapEffect::None)
            .build()
            .expect("decimate-sequence URN");
        let mut cap = Cap::with_description(
            urn,
            "Decimate Sequence".to_string(),
            "decimate-sequence".to_string(),
            "Keep one item out of every N from an input sequence (drop the rest). \
             With --keep-every N omitted (or N=1), the sequence is passed through unchanged."
                .to_string(),
        );
        let mut input_arg = CapArg::with_description(
            "media:",
            true,
            vec![ArgSource::Stdin {
                stdin: "media:".to_string(),
            }],
            "Input sequence".to_string(),
        );
        input_arg.is_sequence = true;
        cap.add_arg(input_arg);
        cap.add_arg(CapArg::with_description(
            "media:keep-every;numeric",
            false,
            vec![ArgSource::CliFlag {
                cli_flag: "--keep-every".to_string(),
            }],
            "Stride N: keep one item out of every N. Positive integer. Omit for passthrough."
                .to_string(),
        ));
        cap.set_output(capdag::CapOutput::with_full_definition(
            "media:",
            "Decimated sequence (same media as input)",
            true,
            None,
        ));
        all_caps.push(cap);
    }

    // All caps in a single cap group with data format adapter URNs
    let data_group = CapGroup {
        name: "data-formats".to_string(),
        caps: all_caps, // identity_cap() is already first in all_caps
        adapter_urns: vec![
            "media:fmt=json".to_string(),
            "media:fmt=ndjson".to_string(),
            "media:fmt=csv".to_string(),
            "media:ext=tsv".to_string(),
            "media:ext=psv".to_string(),
            "media:fmt=yaml".to_string(),
            "media:ext=xml".to_string(),
            "media:ext=toml".to_string(),
        ],
    };

    CapManifest::new(
        "datacartridge".to_string(),
        env!("CARGO_PKG_VERSION").to_string(),
        capdag::CartridgeChannel::from_build_env(env!("MFR_CARTRIDGE_CHANNEL")),
        // Registry URL is baked at compile time via
        // `MFR_CARTRIDGE_REGISTRY_URL`. Unset (plain `cargo build`) ⇒ dev
        // build, manifest emits null.
        capdag::registry_url_from_build_env(option_env!("MFR_CARTRIDGE_REGISTRY_URL")).map(|s| s.to_string()),
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
            .await
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
            .await
            .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;

        Ok(())
    }

    fn metadata(&self) -> capdag::OpMetadata {
        capdag::OpMetadata::builder("CoerceOp").build()
    }
}

// =============================================================================
// SAVE-AS-TXT OP — relabel any text value as a `.txt` file
// =============================================================================
//
// The cap signature is `media:enc=utf-8 → media:enc=utf-8;ext=txt`. The
// op is byte-passthrough: input bytes flow to the output stream
// unchanged; only the URN-side type narrows from the abstract
// enc=utf-8 wildcard to the concrete `.txt` file format.
//
// Why this op exists: the Finder transmute dialog filters target
// candidates to URNs that own a file extension. `media:enc=utf-8`
// is an abstract coercion class with no extension and is suppressed
// from the target list (see `LiveCapFab::get_reachable_targets`).
// `media:enc=utf-8;ext=txt` carries the `txt` extension and is offered
// as a target. This op closes the gap so any chain that ends in a
// text value can be saved as `.txt` without an additional
// per-cap output declaration.
//
// `media:enc=utf-8` PROMISES UTF-8 representability; bytes that
// fail UTF-8 validation here are a contract violation upstream and
// surface as a hard error rather than being silently corrupted.

struct SaveAsTxtOp;

#[async_trait]
impl Op<()> for SaveAsTxtOp {
    async fn perform(&self, _dry: &mut DryContext, wet: &mut WetContext) -> OpResult<()> {
        let req: Arc<Request> = wet
            .get_required(WET_KEY_REQUEST)
            .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;
        let mut input = req
            .take_input()
            .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;
        let output = req.output();

        // The input arrives as a single stream whose URN is whatever
        // the upstream cap emitted (any URN that conforms to
        // `media:enc=utf-8`). We don't filter by URN here — the
        // planner already verified conformance — we just take the
        // stream that arrives and copy its bytes through.
        let mut received: Option<(Vec<u8>, Option<capdag::StreamMeta>)> = None;
        while let Some(stream_result) = input.recv().await {
            let stream = stream_result
                .map_err(|e| OpError::ExecutionFailed(format!("Stream error: {}", e)))?;
            let meta = stream.stream_meta().cloned();
            let bytes = stream
                .collect_bytes()
                .await
                .map_err(|e| {
                    OpError::ExecutionFailed(format!("collect_bytes for save-as-txt: {}", e))
                })?;
            if received.is_some() {
                return Err(OpError::ExecutionFailed(
                    "save-as-txt received more than one input stream — \
                     the cap takes a single textable scalar"
                        .to_string(),
                ));
            }
            received = Some((bytes, meta));
        }

        let (bytes, input_meta) = received.ok_or_else(|| {
            OpError::ExecutionFailed(
                "save-as-txt received no input stream — missing required textable input"
                    .to_string(),
            )
        })?;

        // The cap-arg URN (`media:enc=utf-8;ext=txt;plain-text`)
        // promises UTF-8 representability via the `enc=` dim.
        // Validate here so a contract violation upstream surfaces
        // immediately rather than producing a garbled `.txt` file
        // the user has to debug later. Failing hard is the
        // no-fallback policy.
        let text = String::from_utf8(bytes).map_err(|e| {
            OpError::ExecutionFailed(format!(
                "save-as-txt: input bytes are not valid UTF-8 — \
                 the cap-arg URN `media:enc=utf-8;ext=txt;plain-text` \
                 requires UTF-8 representability. Upstream cap or \
                 the producer is violating the textable contract: {}",
                e
            ))
        })?;

        // Scalar → scalar; propagate the input stream's meta
        // (e.g. provenance `title`) so a downstream writer can
        // derive the `.txt` filename from upstream context.
        output
            .start(false, input_meta)
            .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;
        output
            .emit_cbor(&ciborium::Value::Text(text))
            .await
            .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;

        Ok(())
    }

    fn metadata(&self) -> capdag::OpMetadata {
        capdag::OpMetadata::builder("SaveAsTxtOp").build()
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
            .await
            .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;

        Ok(())
    }

    fn metadata(&self) -> capdag::OpMetadata {
        capdag::OpMetadata::builder("CollectJsonObjectsOp").build()
    }
}

/// Indices the decimate gate emits given a sequence length and a
/// stride. Hoisted out so it can be unit-tested without spinning up
/// the whole cartridge runtime — the gate is the only behaviour the
/// Op contributes beyond stream plumbing, so a regression in stride
/// math has to surface here.
///
/// Contract:
///   - `keep_every == 1` ⇒ every index in `0..count`.
///   - `keep_every > 1`  ⇒ indices `0, keep_every, 2*keep_every, ...`
///     while `< count`.
///   - `keep_every == 0` is impossible at this point (the Op
///     rejects it as a parse error before calling the gate); we
///     defend against it here with a debug_assert so a future
///     refactor can't silently introduce a divide-by-zero.
fn decimate_indices(count: usize, keep_every: usize) -> Vec<usize> {
    debug_assert!(keep_every >= 1, "decimate_indices requires keep_every >= 1");
    let n = keep_every.max(1);
    (0..count).step_by(n).collect()
}

/// Op behind `cap:decimate-sequence;effect=none`.
///
/// Keeps every N-th item from the input sequence (positions 0, N,
/// 2N, ... of the original sequence are emitted). With `--keep-every`
/// missing or N=1, the sequence passes through unchanged.
///
/// Per-item metadata is preserved: the kept item's original meta
/// flows verbatim to the output. The output's media URN matches the
/// input sequence's media URN — we never change the type of the
/// items themselves.
struct DecimateSequenceOp;

#[async_trait]
impl Op<()> for DecimateSequenceOp {
    async fn perform(&self, _dry: &mut DryContext, wet: &mut WetContext) -> OpResult<()> {
        let req: Arc<Request> = wet
            .get_required(WET_KEY_REQUEST)
            .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;
        let mut input = req
            .take_input()
            .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;
        let output = req.output();

        // Drain all incoming streams. We don't know the arrival
        // order of the sequence input vs the CLI flag, so we
        // classify by media URN as each stream arrives and store
        // them separately. The keep-every stream is a small scalar;
        // the sequence stream's items are buffered into a Vec
        // because we don't start emitting until we know N.
        let keep_every_urn = "media:keep-every;numeric";

        let mut keep_every_raw: Option<String> = None;
        let mut sequence_items: Vec<(Vec<u8>, Option<capdag::StreamMeta>)> = Vec::new();
        let mut sequence_input_urn: Option<String> = None;
        let mut sequence_input_meta: Option<capdag::StreamMeta> = None;

        while let Some(stream_result) = input.recv().await {
            let stream = stream_result
                .map_err(|e| OpError::ExecutionFailed(format!("Stream error: {}", e)))?;
            let urn = stream.media_urn().to_string();
            if urn == keep_every_urn {
                let bytes = stream
                    .collect_bytes()
                    .await
                    .map_err(|e| OpError::ExecutionFailed(format!("collect_bytes for keep-every: {}", e)))?;
                let raw = String::from_utf8(bytes).map_err(|e| {
                    OpError::ExecutionFailed(format!(
                        "--keep-every value is not valid UTF-8: {}",
                        e
                    ))
                })?;
                keep_every_raw = Some(raw);
            } else {
                if sequence_input_urn.is_some() {
                    return Err(OpError::ExecutionFailed(format!(
                        "decimate-sequence received two non-keep-every streams: {} and {}",
                        sequence_input_urn.as_deref().unwrap(),
                        urn
                    )));
                }
                sequence_input_urn = Some(urn);
                sequence_input_meta = stream.stream_meta().cloned();
                let items = stream
                    .collect_items()
                    .await
                    .map_err(|e| OpError::ExecutionFailed(format!("collect_items: {}", e)))?;
                sequence_items = items;
            }
        }

        let sequence_input_urn = sequence_input_urn.ok_or_else(|| {
            OpError::ExecutionFailed(
                "decimate-sequence received no input stream — missing required sequence input"
                    .to_string(),
            )
        })?;

        // Parse and validate --keep-every. Missing ⇒ N=1
        // (passthrough). N must be a positive integer; negative,
        // zero, fractional, or unparseable values are programmer
        // errors, not silent passthrough — fail loud.
        let keep_every: usize = match keep_every_raw {
            None => 1,
            Some(raw) => {
                let trimmed = raw.trim();
                let n: i64 = trimmed.parse().map_err(|e| {
                    OpError::ExecutionFailed(format!(
                        "--keep-every must be a positive integer, got {:?}: {}",
                        trimmed, e
                    ))
                })?;
                if n < 1 {
                    return Err(OpError::ExecutionFailed(format!(
                        "--keep-every must be ≥ 1 (got {})",
                        n
                    )));
                }
                n as usize
            }
        };

        // Output as a sequence of the same media URN as the input
        // sequence; the items themselves pass through unchanged.
        output
            .start(true, sequence_input_meta.clone())
            .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;

        let total = sequence_items.len();
        let keep = decimate_indices(total, keep_every);
        let mut emitted = 0usize;
        // Walk the indices to keep in order; index into the
        // pre-buffered items vector. We move the items into an
        // Option<...> array so we can `take` each kept slot without
        // cloning the byte vectors.
        let mut slots: Vec<Option<(Vec<u8>, Option<capdag::StreamMeta>)>> =
            sequence_items.into_iter().map(Some).collect();
        for i in keep {
            let (item_bytes, item_meta) = slots[i]
                .take()
                .expect("decimate_indices yielded a duplicate or out-of-range index");
            output
                .emit_list_item(
                    &ciborium::Value::Bytes(item_bytes),
                    item_meta,
                )
                .await
                .map_err(|e| OpError::ExecutionFailed(format!(
                    "emit_list_item at index {} (input sequence {}): {}",
                    i, sequence_input_urn, e
                )))?;
            emitted += 1;
        }

        // No items in: that's a real input error to surface, not a
        // "successfully decimated zero items" — silently emitting an
        // empty sequence would let downstream caps run on nothing
        // without anyone noticing.
        if total == 0 {
            return Err(OpError::ExecutionFailed(format!(
                "decimate-sequence: input sequence {} was empty",
                sequence_input_urn
            )));
        }

        output.progress(
            1.0,
            &format!(
                "decimate-sequence: kept {} of {} items (every {})",
                emitted, total, keep_every
            ),
        );
        Ok(())
    }

    fn metadata(&self) -> capdag::OpMetadata {
        capdag::OpMetadata::builder("DecimateSequenceOp")
            .description("Keep every N-th item from a sequence (passthrough when N omitted or 1)")
            .build()
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
            .await
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
            let mut rdr = csv::Reader::from_reader(strip_utf8_bom(&bytes[..]));
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
            .await
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

    // Save-as-txt cap: media:enc=utf-8;ext=txt;plain-text → same.
    // The cap declaration in `build_manifest()` builds the URN
    // with the same markers; mirror it here so the runtime can
    // dispatch the cap to its op.
    {
        let urn = capdag::CapUrnBuilder::new()
            .marker("save-as-txt")
            .in_spec("media:enc=utf-8;ext=txt;plain-text")
            .out_spec("media:enc=utf-8;ext=txt;plain-text")
            .build()
            .expect("save-as-txt URN");
        runtime.register_op(&urn.to_string(), || Box::new(SaveAsTxtOp));
    }

    // Collect JSON objects → JSON array / CSV / YAML
    {
        let json_array_urn = capdag::CapUrnBuilder::new()
            .marker("collect-json-objects")
            .in_spec("media:fmt=json;record")
            .out_spec("media:fmt=json;list;record")
            .build().expect("collect_json_objects → json array URN");
        runtime.register_op(&json_array_urn.to_string(), || {
            Box::new(CollectJsonObjectsOp { out_media: "media:fmt=json;list;record" })
        });

        let csv_urn = capdag::CapUrnBuilder::new()
            .marker("collect-json-objects")
            .in_spec("media:fmt=json;record")
            .out_spec("media:fmt=csv;list;record")
            .build().expect("collect_json_objects → csv URN");
        runtime.register_op(&csv_urn.to_string(), || {
            Box::new(CollectJsonObjectsOp { out_media: "media:fmt=csv;list;record" })
        });

        let yaml_urn = capdag::CapUrnBuilder::new()
            .marker("collect-json-objects")
            .in_spec("media:fmt=json;record")
            .out_spec("media:fmt=yaml;list;record")
            .build().expect("collect_json_objects → yaml URN");
        runtime.register_op(&yaml_urn.to_string(), || {
            Box::new(CollectJsonObjectsOp { out_media: "media:fmt=yaml;list;record" })
        });
    }

    // Collect CSV records → merged CSV / JSON array / YAML list
    {
        let csv_csv_urn = capdag::CapUrnBuilder::new()
            .marker("collect-records")
            .in_spec("media:fmt=csv;list;record")
            .out_spec("media:fmt=csv;list;record")
            .build().expect("collect_records csv→csv URN");
        runtime.register_op(&csv_csv_urn.to_string(), || {
            Box::new(CollectRecordsOp { in_media: "media:fmt=csv;list;record", out_media: "media:fmt=csv;list;record" })
        });

        let csv_json_urn = capdag::CapUrnBuilder::new()
            .marker("collect-records")
            .in_spec("media:fmt=csv;list;record")
            .out_spec("media:fmt=json;list;record")
            .build().expect("collect_records csv→json URN");
        runtime.register_op(&csv_json_urn.to_string(), || {
            Box::new(CollectRecordsOp { in_media: "media:fmt=csv;list;record", out_media: "media:fmt=json;list;record" })
        });

        let csv_yaml_urn = capdag::CapUrnBuilder::new()
            .marker("collect-records")
            .in_spec("media:fmt=csv;list;record")
            .out_spec("media:fmt=yaml;list;record")
            .build().expect("collect_records csv→yaml URN");
        runtime.register_op(&csv_yaml_urn.to_string(), || {
            Box::new(CollectRecordsOp { in_media: "media:fmt=csv;list;record", out_media: "media:fmt=yaml;list;record" })
        });
    }

    // Collect YAML mappings → merged YAML list / JSON array / CSV
    {
        let yaml_yaml_urn = capdag::CapUrnBuilder::new()
            .marker("collect-records")
            .in_spec("media:fmt=yaml;record")
            .out_spec("media:fmt=yaml;list;record")
            .build().expect("collect_records yaml→yaml URN");
        runtime.register_op(&yaml_yaml_urn.to_string(), || {
            Box::new(CollectRecordsOp { in_media: "media:fmt=yaml;record", out_media: "media:fmt=yaml;list;record" })
        });

        let yaml_json_urn = capdag::CapUrnBuilder::new()
            .marker("collect-records")
            .in_spec("media:fmt=yaml;record")
            .out_spec("media:fmt=json;list;record")
            .build().expect("collect_records yaml→json URN");
        runtime.register_op(&yaml_json_urn.to_string(), || {
            Box::new(CollectRecordsOp { in_media: "media:fmt=yaml;record", out_media: "media:fmt=json;list;record" })
        });

        let yaml_csv_urn = capdag::CapUrnBuilder::new()
            .marker("collect-records")
            .in_spec("media:fmt=yaml;record")
            .out_spec("media:fmt=csv;list;record")
            .build().expect("collect_records yaml→csv URN");
        runtime.register_op(&yaml_csv_urn.to_string(), || {
            Box::new(CollectRecordsOp { in_media: "media:fmt=yaml;record", out_media: "media:fmt=csv;list;record" })
        });
    }

    // decimate-sequence: drop items from a sequence at stride N.
    // Single op handles every input/output media because the cap
    // URN's in/out are both `media:` (the wildcard).
    {
        let urn = capdag::CapUrnBuilder::new()
            .marker("decimate-sequence")
            .in_spec("media:")
            .out_spec("media:")
            .effect(capdag::CapEffect::None)
            .build()
            .expect("decimate-sequence URN");
        runtime.register_op(&urn.to_string(), || Box::new(DecimateSequenceOp));
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
    /// Bare UTF-8 text list (`media:enc=utf-8;list`) — CBOR sequence of byte
    /// strings, carrying an encoding but no serialization-format (`fmt=`) tag.
    TextList,
}

/// Identify the data format from a media URN string.
/// Parses the URN properly and checks tags — never inspects the string directly.
/// Serialization formats are read off the single `fmt=` axis (`is_json`/`is_yaml`/
/// `is_csv`); a value with a `list` marker but no `fmt=` is a bare UTF-8 text list.
fn format_of_str(media_urn: &str) -> Result<Fmt> {
    let urn = capdag::MediaUrn::from_string(media_urn)
        .map_err(|e| anyhow::anyhow!("Invalid media URN '{}': {}", media_urn, e))?;
    if urn.is_json() {
        Ok(Fmt::Json)
    } else if urn.is_yaml() {
        Ok(Fmt::Yaml)
    } else if urn.is_csv() {
        Ok(Fmt::Csv)
    } else if urn.is_list() {
        // list marker, no fmt= serialization tag → bare UTF-8 text list
        Ok(Fmt::TextList)
    } else {
        anyhow::bail!("Media URN '{}' is not a recognized data format (json, yaml, csv, or text list)", media_urn)
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
        (Fmt::TextList, Fmt::Json) => text_list_to_json(data),
        (Fmt::Json, Fmt::TextList) => json_to_text_list(data),
        (Fmt::TextList, Fmt::Yaml) => text_list_to_yaml(data),
        (Fmt::Yaml, Fmt::TextList) => yaml_to_text_list(data),
        (Fmt::TextList, Fmt::Csv) => text_list_to_csv(data),
        (Fmt::Csv, Fmt::TextList) => csv_to_text_list(data),
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
    let mut rdr = csv::Reader::from_reader(strip_utf8_bom(data));
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
/// Infer a CSV field's JSON type LOSSLESSLY: a field converts to a
/// number only when the number's canonical rendering round-trips to
/// the original text. This is what keeps identifier-like fields
/// intact — `"007"` (leading zero), `"1e5"` (notation), `"+42"`
/// (sign) all stay strings because converting them would destroy
/// information; `"42"` and `"-3.5"` convert because nothing is lost.
fn infer_csv_value(field: &str) -> serde_json::Value {
    if field.is_empty() {
        return serde_json::Value::Null;
    }
    if let Ok(n) = field.parse::<i64>() {
        if n.to_string() == field {
            return serde_json::Value::Number(n.into());
        }
    }
    if let Ok(f) = field.parse::<f64>() {
        if let Some(n) = serde_json::Number::from_f64(f) {
            if n.to_string() == field {
                return serde_json::Value::Number(n);
            }
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

/// Strip a UTF-8 BOM before CSV parsing. Windows tools emit
/// BOM-prefixed CSVs routinely; without stripping, the first header
/// key silently becomes `"\u{feff}name"` and every record key derived
/// from it is corrupted.
fn strip_utf8_bom(data: &[u8]) -> &[u8] {
    data.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(data)
}

// =============================================================================
// TEXTABLE LIST CONVERSIONS
// =============================================================================

/// Decode a CBOR sequence of byte strings into a Vec of raw UTF-8 strings.
/// Each item in the CBOR sequence is expected to be a Value::Bytes containing UTF-8 text.
/// Decode a text list: plain text with one value per line.
/// Empty lines are skipped. Trailing newline is tolerated.
fn decode_textable_list(data: &[u8]) -> Result<Vec<String>> {
    let text = std::str::from_utf8(data)
        .map_err(|e| anyhow::anyhow!("Textable list is not valid UTF-8: {}", e))?;
    Ok(text.lines()
        .map(|line| line.to_string())
        .filter(|line| !line.is_empty())
        .collect())
}

/// Encode a Vec of strings into a text list: one value per line.
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
fn text_list_to_json(data: &[u8]) -> Result<Vec<u8>> {
    let items = decode_textable_list(data)?;
    let json_values: Vec<serde_json::Value> = items.iter()
        .map(|s| serde_json::from_str(s).unwrap_or_else(|_| serde_json::Value::String(s.clone())))
        .collect();
    serde_json::to_vec_pretty(&json_values)
        .map_err(|e| anyhow::anyhow!("Failed to serialize to JSON: {}", e))
}

/// JSON array -> text list (one value per line).
/// Each JSON value is serialized to its string representation.
fn json_to_text_list(data: &[u8]) -> Result<Vec<u8>> {
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
fn text_list_to_yaml(data: &[u8]) -> Result<Vec<u8>> {
    let items = decode_textable_list(data)?;
    let yaml_values: Vec<serde_yaml::Value> = items.iter()
        .map(|s| serde_yaml::from_str(s).unwrap_or_else(|_| serde_yaml::Value::String(s.clone())))
        .collect();
    let yaml_str = serde_yaml::to_string(&yaml_values)
        .map_err(|e| anyhow::anyhow!("Failed to serialize to YAML: {}", e))?;
    Ok(yaml_str.into_bytes())
}

/// YAML sequence -> text list (one value per line).
/// Each YAML value is serialized to its string representation.
fn yaml_to_text_list(data: &[u8]) -> Result<Vec<u8>> {
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
fn text_list_to_csv(data: &[u8]) -> Result<Vec<u8>> {
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

/// CSV -> text list (one value per line).
/// Reads the first column of each row (ignoring headers).
fn csv_to_text_list(data: &[u8]) -> Result<Vec<u8>> {
    let mut rdr = csv::Reader::from_reader(strip_utf8_bom(data));
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
        "boolean" => coerce_to_boolean(s),
        "object" => coerce_to_object(s, _source_type),
        other => anyhow::bail!("Unsupported coercion target type: '{}'", other),
    }
}

/// Coerce to boolean. Strict, documented spellings only (the inverse
/// of the boolean→* family): strings accept exactly
/// true/false/1/0/yes/no/on/off case-insensitively after trim;
/// numbers accept exactly 1/1.0 → true and 0/0.0 → false. Anything
/// else is a hard error naming the accepted set — "truthiness" of
/// arbitrary values would be a silent guess.
fn coerce_to_boolean(s: &str) -> Result<Vec<u8>> {
    const ACCEPTED: &str =
        "true/false, 1/0, yes/no, on/off (strings, case-insensitive) or the numbers 1/0";

    fn bool_from_number(f: f64) -> Option<bool> {
        if f == 1.0 {
            Some(true)
        } else if f == 0.0 {
            Some(false)
        } else {
            None
        }
    }
    fn bool_from_str(s: &str) -> Option<bool> {
        match s.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Some(true),
            "false" | "0" | "no" | "off" => Some(false),
            _ => None,
        }
    }

    let value = if let Ok(value) = serde_json::from_str::<serde_json::Value>(s) {
        match value {
            serde_json::Value::Bool(b) => Some(b),
            serde_json::Value::Number(n) => n.as_f64().and_then(bool_from_number),
            serde_json::Value::String(inner) => bool_from_str(&inner),
            _ => None,
        }
    } else {
        bool_from_str(s)
    };

    match value {
        Some(b) => Ok(if b { b"true".to_vec() } else { b"false".to_vec() }),
        None => anyhow::bail!(
            "Cannot coerce '{}' to boolean — accepted values: {}",
            s.trim(),
            ACCEPTED
        ),
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
                // Canonical f64 rendering, matching every other
                // number-producing coercion (integral values render
                // without a decimal point).
                let f: f64 = if b { 1.0 } else { 0.0 };
                return Ok(f.to_string().into_bytes());
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

    // TEST1859: every cap URN this cartridge advertises must resolve in the
    // pinned fabric catalog. A drifted/bare-marker URN absent from the catalog
    // is silently dropped by LiveCapFab at runtime; this guard turns that into a
    // hard failure naming the exact URN. `get_cap` consults the same canonical
    // manifest map LiveCapFab resolves against. Network-mediated against the
    // env-configured registry (set by `dx test`), pinned at FABRIC_MANIFEST_VERSION.
    // The identity cap is engine-provided, not a catalog entry, so it is excluded.
    #[tokio::test]
    async fn test1859_advertised_caps_resolve_in_catalog() {
        let registry = capdag::FabricRegistry::new().await.expect(
            "datacartridge catalog guard: FabricRegistry::new() must succeed (set CDG_FABRIC_REGISTRY_URL; run via dx test)",
        );
        let manifest = build_manifest();
        let identity = capdag::identity_cap().urn_string();

        let mut unresolved: Vec<String> = Vec::new();
        for cap in manifest.all_caps() {
            let urn = cap.urn_string();
            if urn == identity {
                continue;
            }
            if registry.get_cap(&urn).await.is_err() {
                unresolved.push(urn);
            }
        }
        assert!(
            unresolved.is_empty(),
            "datacartridge advertises cap URN(s) not present in fabric catalog v{} \
             (LiveCapFab would drop these at runtime): {:?}",
            capdag::FABRIC_MANIFEST_VERSION,
            unresolved
        );
    }

    // TEST0014: Json to yaml object
    #[test]
    fn test0014_json_to_yaml_object() {
        let json = br#"{"name": "Alice", "age": 30}"#;
        let yaml = json_to_yaml(json).unwrap();
        let yaml_str = std::str::from_utf8(&yaml).unwrap();
        assert!(yaml_str.contains("name:"));
        assert!(yaml_str.contains("Alice"));
        assert!(yaml_str.contains("age:"));
    }

    // TEST0015: Yaml to json object
    #[test]
    fn test0015_yaml_to_json_object() {
        let yaml = b"name: Alice\nage: 30";
        let json = yaml_to_json(yaml).unwrap();
        let val: serde_json::Value = serde_json::from_slice(&json).unwrap();
        assert_eq!(val["name"], "Alice");
        assert_eq!(val["age"], 30);
    }

    // TEST0016: Json to yaml array
    #[test]
    fn test0016_json_to_yaml_array() {
        let json = br#"[1, 2, 3]"#;
        let yaml = json_to_yaml(json).unwrap();
        let yaml_str = std::str::from_utf8(&yaml).unwrap();
        assert!(yaml_str.contains("- 1"));
        assert!(yaml_str.contains("- 2"));
        assert!(yaml_str.contains("- 3"));
    }

    // TEST0017: Yaml to json list
    #[test]
    fn test0017_yaml_to_json_list() {
        let yaml = b"- 1\n- 2\n- 3";
        let json = yaml_to_json(yaml).unwrap();
        let val: serde_json::Value = serde_json::from_slice(&json).unwrap();
        assert_eq!(val, serde_json::json!([1, 2, 3]));
    }

    // TEST0018: Json to yaml scalar
    #[test]
    fn test0018_json_to_yaml_scalar() {
        let json = br#""hello world""#;
        let yaml = json_to_yaml(json).unwrap();
        let yaml_str = std::str::from_utf8(&yaml).unwrap();
        assert!(yaml_str.contains("hello world"));
    }

    // TEST0019: Json records to csv
    #[test]
    fn test0019_json_records_to_csv() {
        let json = br#"[{"a": 1, "b": "x"}, {"a": 2, "b": "y"}]"#;
        let csv_bytes = json_records_to_csv(json).unwrap();
        let csv_str = std::str::from_utf8(&csv_bytes).unwrap();
        // Headers present
        assert!(csv_str.starts_with("a,b") || csv_str.starts_with("b,a"));
        // Data present
        assert!(csv_str.contains('1'));
        assert!(csv_str.contains('x'));
    }

    // TEST0020: Csv to json records
    #[test]
    fn test0020_csv_to_json_records() {
        let csv = b"name,age\nAlice,30\nBob,25";
        let json = csv_to_json_records(csv).unwrap();
        let records: Vec<serde_json::Value> = serde_json::from_slice(&json).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0]["name"], "Alice");
        assert_eq!(records[0]["age"], 30);
        assert_eq!(records[1]["name"], "Bob");
        assert_eq!(records[1]["age"], 25);
    }

    // TEST0021: Csv to yaml records
    #[test]
    fn test0021_csv_to_yaml_records() {
        let csv = b"name,age\nAlice,30";
        let yaml = csv_to_yaml_records(csv).unwrap();
        let yaml_str = std::str::from_utf8(&yaml).unwrap();
        assert!(yaml_str.contains("name:"));
        assert!(yaml_str.contains("Alice"));
    }

    // TEST0022: Yaml records to csv
    #[test]
    fn test0022_yaml_records_to_csv() {
        let yaml = b"- name: Alice\n  age: 30\n- name: Bob\n  age: 25";
        let csv_bytes = yaml_records_to_csv(yaml).unwrap();
        let csv_str = std::str::from_utf8(&csv_bytes).unwrap();
        assert!(csv_str.contains("Alice"));
        assert!(csv_str.contains("Bob"));
    }

    // TEST0023: Roundtrip json yaml json
    #[test]
    fn test0023_roundtrip_json_yaml_json() {
        let original = br#"{"key": "value", "num": 42, "flag": true}"#;
        let yaml = json_to_yaml(original).unwrap();
        let json = yaml_to_json(&yaml).unwrap();
        let orig_val: serde_json::Value = serde_json::from_slice(original).unwrap();
        let round_val: serde_json::Value = serde_json::from_slice(&json).unwrap();
        assert_eq!(orig_val, round_val);
    }

    // TEST0024: Roundtrip csv json csv
    #[test]
    fn test0024_roundtrip_csv_json_csv() {
        let csv = b"name,age\nAlice,30\nBob,25";
        let json = csv_to_json_records(csv).unwrap();
        let csv2 = json_records_to_csv(&json).unwrap();
        // Re-parse both CSVs to compare structurally
        let records1 = csv_to_json_records(csv).unwrap();
        let records2 = csv_to_json_records(&csv2).unwrap();
        assert_eq!(records1, records2);
    }

    // TEST0025: Empty json array to csv
    #[test]
    fn test0025_empty_json_array_to_csv() {
        let json = b"[]";
        let csv_bytes = json_records_to_csv(json).unwrap();
        assert!(csv_bytes.is_empty());
    }

    // TEST0026: Malformed json fails
    #[test]
    fn test0026_malformed_json_fails() {
        let bad = b"not json at all";
        let result = json_to_yaml(bad);
        assert!(result.is_err());
    }

    // TEST0027: Malformed yaml fails
    #[test]
    fn test0027_malformed_yaml_fails() {
        let bad = b"{{{{invalid yaml";
        let result = yaml_to_json(bad);
        assert!(result.is_err());
    }

    // TEST0028: Csv type inference
    #[test]
    fn test0028_csv_type_inference() {
        assert_eq!(infer_csv_value(""), serde_json::Value::Null);
        assert_eq!(infer_csv_value("42"), serde_json::json!(42));
        assert_eq!(infer_csv_value("3.14"), serde_json::json!(3.14));
        assert_eq!(infer_csv_value("true"), serde_json::json!(true));
        assert_eq!(infer_csv_value("false"), serde_json::json!(false));
        assert_eq!(infer_csv_value("hello"), serde_json::json!("hello"));
        assert_eq!(infer_csv_value("-7"), serde_json::json!(-7));

        // Lossless-only inference: identifier-like fields whose numeric
        // parse would NOT round-trip stay strings (the old inference
        // corrupted zip codes and IDs: "007" became 7).
        for keep in ["007", "01234", "1e5", "+42", "1_000", " 42", "42 ", "NaN", "inf", "1.10"] {
            assert_eq!(
                infer_csv_value(keep),
                serde_json::json!(keep),
                "'{keep}' must stay a string — numeric conversion is lossy"
            );
        }
    }

    // TEST0054: UTF-8 BOM never leaks into CSV header keys.
    #[test]
    fn test0054_csv_bom_stripped() {
        let mut data = vec![0xEF, 0xBB, 0xBF];
        data.extend_from_slice(b"name,age\nalice,3\n");
        let out = csv_to_json_records(&data).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(
            text.contains("\"name\""),
            "header must parse clean of the BOM, got: {text}"
        );
        assert!(!text.contains('\u{feff}'), "BOM leaked into output: {text}");
    }

    // TEST0029: Csv with mixed columns
    #[test]
    fn test0029_csv_with_mixed_columns() {
        let csv = b"id,name,active,score\n1,Alice,true,95.5\n2,Bob,false,";
        let json = csv_to_json_records(csv).unwrap();
        let records: Vec<serde_json::Value> = serde_json::from_slice(&json).unwrap();
        assert_eq!(records[0]["id"], 1);
        assert_eq!(records[0]["active"], true);
        assert_eq!(records[0]["score"], 95.5);
        assert_eq!(records[1]["score"], serde_json::Value::Null);
    }

    // TEST0030: Yaml tagged values stripped
    #[test]
    fn test0030_yaml_tagged_values_stripped() {
        let yaml = b"!!str 42";
        let json = yaml_to_json(yaml).unwrap();
        let val: serde_json::Value = serde_json::from_slice(&json).unwrap();
        // Tagged !!str should strip the tag and give us the inner value
        assert!(val.is_string() || val.is_number());
    }

    // TEST0031: Json records superset headers
    #[test]
    fn test0031_json_records_superset_headers() {
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
    fn test0032_coerce_integer_to_string() {
        let result = coerce(b"42", "integer", "string").unwrap();
        assert_eq!(std::str::from_utf8(&result).unwrap(), "\"42\"");
    }

    // TEST0033: Coerce number to string
    #[test]
    fn test0033_coerce_number_to_string() {
        let result = coerce(b"3.14", "number", "string").unwrap();
        assert_eq!(std::str::from_utf8(&result).unwrap(), "\"3.14\"");
    }

    // TEST0034: Coerce boolean to string
    #[test]
    fn test0034_coerce_boolean_to_string() {
        let result = coerce(b"true", "boolean", "string").unwrap();
        assert_eq!(std::str::from_utf8(&result).unwrap(), "\"true\"");
    }

    // TEST0035: Coerce string to integer
    #[test]
    fn test0035_coerce_string_to_integer() {
        let result = coerce(b"\"42\"", "string", "integer").unwrap();
        assert_eq!(std::str::from_utf8(&result).unwrap(), "42");
    }

    // TEST0036: Coerce number to integer
    #[test]
    fn test0036_coerce_number_to_integer() {
        let result = coerce(b"3.7", "number", "integer").unwrap();
        assert_eq!(std::str::from_utf8(&result).unwrap(), "4");
    }

    // TEST0037: Coerce boolean to integer
    #[test]
    fn test0037_coerce_boolean_to_integer() {
        assert_eq!(coerce(b"true", "boolean", "integer").unwrap(), b"1");
        assert_eq!(coerce(b"false", "boolean", "integer").unwrap(), b"0");
    }

    // TEST0038: Coerce string to number
    #[test]
    fn test0038_coerce_string_to_number() {
        let result = coerce(b"\"3.14\"", "string", "number").unwrap();
        assert_eq!(std::str::from_utf8(&result).unwrap(), "3.14");
    }

    // TEST0039: Coerce integer to number
    #[test]
    fn test0039_coerce_integer_to_number() {
        let result = coerce(b"42", "integer", "number").unwrap();
        // 42 as f64 can be "42" or "42.0" depending on serde
        let s = std::str::from_utf8(&result).unwrap();
        let f: f64 = s.parse().unwrap();
        assert!((f - 42.0).abs() < f64::EPSILON);
    }

    // TEST0040: Coerce boolean to number — canonical f64 rendering,
    // consistent with every other number-producing coercion (integral
    // values render without a decimal point; "42" not "42.0").
    #[test]
    fn test0040_coerce_boolean_to_number() {
        assert_eq!(coerce(b"true", "boolean", "number").unwrap(), b"1");
        assert_eq!(coerce(b"false", "boolean", "number").unwrap(), b"0");
        // The consistency this pins: integer input renders identically.
        assert_eq!(coerce(b"1", "integer", "number").unwrap(), b"1");
    }

    // TEST0053: Coerce to boolean — strict spellings both ways, hard
    // error on everything else (the missing inverse of boolean→*).
    #[test]
    fn test0053_coerce_to_boolean_strict() {
        for (input, expected) in [
            (&b"true"[..], &b"true"[..]),
            (b"\"YES\"", b"true"),
            (b"\"on\"", b"true"),
            (b"1", b"true"),
            (b"1.0", b"true"),
            (b"false", b"false"),
            (b"\"No\"", b"false"),
            (b"\"off\"", b"false"),
            (b"0", b"false"),
            (b"0.0", b"false"),
        ] {
            assert_eq!(
                coerce(input, "string", "boolean").unwrap(),
                expected,
                "input {:?}",
                std::str::from_utf8(input).unwrap()
            );
        }
        for bad in [&b"2"[..], b"\"truthy\"", b"0.5", b"\"\"", b"null", b"[true]"] {
            assert!(
                coerce(bad, "string", "boolean").is_err(),
                "{:?} must be rejected",
                std::str::from_utf8(bad).unwrap()
            );
        }
    }

    // TEST0041: Coerce string to object
    #[test]
    fn test0041_coerce_string_to_object() {
        let result = coerce(b"\"hello\"", "string", "object").unwrap();
        let val: serde_json::Value = serde_json::from_slice(&result).unwrap();
        assert_eq!(val["value"], "hello");
        // The string source media URN now declares its UTF-8 encoding via enc=utf-8
        // (the old `textable` marker is gone).
        assert!(val["source_type"].as_str().unwrap().contains("enc=utf-8"));
    }

    // TEST0042: Coerce object passthrough
    #[test]
    fn test0042_coerce_object_passthrough() {
        let input = br#"{"key": "value"}"#;
        let result = coerce(input, "object", "string").unwrap();
        let s = std::str::from_utf8(&result).unwrap();
        // Object to string wraps as JSON string
        assert!(s.starts_with('"'));
    }

    // TEST0043: Coerce invalid to integer fails
    #[test]
    fn test0043_coerce_invalid_to_integer_fails() {
        let result = coerce(b"\"not a number\"", "string", "integer");
        assert!(result.is_err());
    }

    // TEST0044: Coerce invalid to number fails
    #[test]
    fn test0044_coerce_invalid_to_number_fails() {
        let result = coerce(b"\"not a number\"", "string", "number");
        assert!(result.is_err());
    }

    // TEST0045: Coerce unsupported target fails
    #[test]
    fn test0045_coerce_unsupported_target_fails() {
        let result = coerce(b"42", "integer", "array");
        assert!(result.is_err());
    }

    // ----- decimate-sequence ---------------------------------------

    /// Stride 1 keeps every index — this is the passthrough contract
    /// the cap promises when --keep-every is omitted (the Op
    /// substitutes `1` and calls the gate).
    #[test]
    fn test0046_decimate_indices_stride_one_keeps_all() {
        for n in [0usize, 1, 5, 100] {
            let got = decimate_indices(n, 1);
            let want: Vec<usize> = (0..n).collect();
            assert_eq!(got, want, "stride=1 over count={} must keep every index", n);
        }
    }

    /// Stride N starts at index 0 and keeps every Nth thereafter,
    /// regardless of count. An off-by-one (e.g. starting at index 1
    /// instead of 0) shows up here as the first kept index being N
    /// instead of 0 — exactly the failure we want to surface.
    #[test]
    fn test0047_decimate_indices_starts_at_zero() {
        for stride in [2usize, 3, 7, 13] {
            let got = decimate_indices(100, stride);
            assert_eq!(got.first().copied(), Some(0),
                "stride={} must start at 0; got {:?}", stride, got.first());
            assert!(got.windows(2).all(|w| w[1] - w[0] == stride),
                "stride={} produced non-uniform spacing: {:?}", stride, got);
        }
    }

    /// Specific case spelled out by the user requirement: every Nth.
    /// This pins down N=3 over a small enumerated count where the
    /// expected output is hand-readable, so a regression that
    /// changes "every Nth from 0" to "every Nth except 0" or to
    /// "0-indexed but offset N-1" produces a clearly wrong list.
    #[test]
    fn test0048_decimate_indices_every_third_of_ten() {
        let got = decimate_indices(10, 3);
        assert_eq!(got, vec![0, 3, 6, 9]);
    }

    /// A stride larger than the input length keeps exactly the
    /// first item (index 0) and nothing else. Catches the "what if
    /// stride > count" edge.
    #[test]
    fn test0049_decimate_indices_stride_larger_than_count() {
        assert_eq!(decimate_indices(5, 100), vec![0]);
    }

    /// Empty input yields empty output. The Op layer turns this
    /// into a hard error (an empty input sequence is suspicious),
    /// but the gate itself must be honest about returning [] —
    /// otherwise we'd hide the empty case from the Op.
    #[test]
    fn test0050_decimate_indices_empty_input() {
        let got = decimate_indices(0, 1);
        assert!(got.is_empty());
        let got = decimate_indices(0, 5);
        assert!(got.is_empty());
    }

    /// The cap manifest declares `save-as-txt` with a specific URN
    /// shape; `main()` registers the op's runtime handler under
    /// the URN built from the same parts. If those two strings
    /// diverge, the planner accepts the cap but the runtime has
    /// no dispatch entry — and the cartridge silently fails the
    /// first time a user invokes it.
    ///
    /// This test reconstructs both URNs exactly the way each
    /// site builds them and asserts byte equality. A future
    /// refactor that touches one site without the other surfaces
    /// here at compile/test time rather than at runtime.
    #[test]
    fn test0051_save_as_txt_manifest_and_runtime_urn_agree() {
        let manifest_urn = capdag::CapUrnBuilder::new()
            .marker("save-as-txt")
            .in_spec("media:enc=utf-8;ext=txt;plain-text")
            .out_spec("media:enc=utf-8;ext=txt;plain-text")
            .build()
            .expect("save-as-txt manifest URN")
            .to_string();
        let runtime_urn = capdag::CapUrnBuilder::new()
            .marker("save-as-txt")
            .in_spec("media:enc=utf-8;ext=txt;plain-text")
            .out_spec("media:enc=utf-8;ext=txt;plain-text")
            .build()
            .expect("save-as-txt runtime URN")
            .to_string();
        assert_eq!(
            manifest_urn, runtime_urn,
            "save-as-txt cap manifest URN must match the runtime registration URN \
             — divergence means cartridge silently fails dispatch on first invocation"
        );
        // Also assert the canonical form so a tag-order
        // regression in CapUrnBuilder surfaces here. This is the
        // string the engine's LiveCapFab will look up against the
        // catalog; the catalog's `save-as-txt` entry is hashed on
        // exactly this string.
        assert_eq!(
            manifest_urn,
            r#"cap:in="media:enc=utf-8;ext=txt;plain-text";out="media:enc=utf-8;ext=txt;plain-text";save-as-txt"#,
            "canonical save-as-txt URN drifted — catalog lookups will 404"
        );
    }

    /// The save-as-txt cap is registered in the manifest builder
    /// (`build_manifest`). Verify it's actually present there with
    /// the right shape — input urn, output urn, command. A
    /// regression that drops the cap from the manifest would
    /// remove it from the cartridge's cap-graph contribution
    /// entirely, and the planner would never reach a `.txt`
    /// target via this cartridge.
    #[test]
    fn test0052_save_as_txt_cap_present_in_manifest() {
        let manifest = build_manifest();
        let group = manifest
            .cap_groups
            .iter()
            .find(|g| g.name == "data-formats")
            .expect("data-formats cap group must exist");
        let cap = group
            .caps
            .iter()
            .find(|c| {
                let urn = c.urn.to_string();
                urn.contains("save-as-txt")
            })
            .expect("save-as-txt cap must be present in data-formats group");
        assert_eq!(cap.urn.in_spec(), "media:enc=utf-8;ext=txt;plain-text");
        assert_eq!(cap.urn.out_spec(), "media:enc=utf-8;ext=txt;plain-text");
        assert_eq!(cap.command, "save_as_txt");
    }
}
