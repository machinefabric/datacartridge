//! Semantic-primitive caps: schema-constrained "judgment" operations.
//!
//! These moved out of the engine's former in-process `GenerativeTextCartridge`
//! into this cartridge unchanged: the prompt building, the judgment-envelope
//! schemas, the structured-query registry, and the grounding checks are the
//! same code. Only the plumbing differs — instead of reaching into engine
//! services (`config_service`/`llm_service`), each cap builds the constrained
//! request and peer-calls the constrained-inference cap itself, exactly as
//! this cartridge's `edit` cap already does. The prompt/dynamic-schema
//! registry (`structured_queries`) is shared through `machfab-cartridge-sdk`.
//!
//! Every judgment cap is deterministic (greedy sampling + fixed seed) — the
//! same input, model, and args produce the same judgment. `generate_json` is
//! the one non-deterministic member (free generation constrained to a schema).

use anyhow::Result;
use capdag::{
    async_trait, DryContext, Op, OpError, OpResult, OutputStream, PeerInvoker, Request, StreamMeta,
    WetContext, WET_KEY_REQUEST,
};
use machfab_cartridge_sdk::structured_queries::{
    MakeDecisionResult, MakeMultipleDecisionsResult, StructuredQueryRegistry,
};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{error, info};

use crate::{input_arg_error, invoke_constrained_peer, DEFAULT_LLM_MODEL};

/// The stream triple `collect_streams` yields: (media-urn, bytes, stream meta).
type Streams = [(String, Vec<u8>, Option<StreamMeta>)];

// =============================================================================
// SHARED ARG / OUTPUT PLUMBING
// =============================================================================

/// Read a required arg whose declared URN *conforms to* `pattern`, failing
/// hard (never a silent default) when it is absent. Conformance matching
/// mirrors the former cartridge's `find_arg`: the primary content arg is the
/// first stream conforming to the bare `media:enc=utf-8`, the refined args
/// (`…;question`, `…;rubric`, …) match their own specific patterns.
fn require_conforming(streams: &Streams, pattern: &str, what: &str) -> OpResult<String> {
    let bytes = capdag::find_stream_conforming(streams, pattern).ok_or_else(|| {
        input_arg_error(
            "INPUT_REQUIRED",
            format!("Missing {} argument ({})", what, pattern),
            pattern,
        )
    })?;
    std::str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|error| {
            input_arg_error(
                "INVALID_INPUT",
                format!("{} argument ({}) is not valid UTF-8: {}", what, pattern, error),
                pattern,
            )
        })
}

/// Optional counterpart to [`require_conforming`].
fn optional_conforming(streams: &Streams, pattern: &str) -> OpResult<Option<String>> {
    let Some(bytes) = capdag::find_stream_conforming(streams, pattern) else {
        return Ok(None);
    };
    std::str::from_utf8(bytes)
        .map(|value| Some(value.to_owned()))
        .map_err(|error| {
            input_arg_error(
                "INVALID_INPUT",
                format!("argument ({}) is not valid UTF-8: {}", pattern, error),
                pattern,
            )
        })
}

/// Resolve the model spec from the `…;model-spec` arg, falling back to the
/// offline-travel-kit GGUF LLM the fabric catalog declares as the default for
/// every semantic cap (identical to `edit`'s default).
fn resolve_model_spec(streams: &Streams) -> OpResult<String> {
    match optional_conforming(streams, MODEL_SPEC_URN)? {
        None => Ok(DEFAULT_LLM_MODEL.to_string()),
        Some(value) if value.trim().is_empty() => Err(input_arg_error(
            "INVALID_INPUT",
            "model spec must not be empty",
            MODEL_SPEC_URN,
        )),
        Some(value) => Ok(value),
    }
}

// -----------------------------------------------------------------------------
// INFERENCE PARAMETERS (normal cap args — defaults live in the fabric tomls)
// -----------------------------------------------------------------------------
//
// Every constrained-inference param is an ordinary cap arg with its own URN. On
// cap-add the engine discovers each arg's default (below) into a setting record, then
// resolves the value (default, or a per-cap/per-model/user override) and delivers it
// as an arg stream on every call — so the arg is practically always present. For
// capdag CLI / direct invocation, where no engine is in the loop, the cartridge falls
// back to the SAME default from its own cap definition (the arg's declared default,
// not a fallback masking a bug). A value that IS supplied but unparseable fails hard.
//
// `temperature`/`seed` are special: a judgment must be reproducible, so those are not
// configurable for the judgment caps — they use the deterministic constants below and
// do not declare temperature/seed args at all. Only the one free-generation cap
// (`generate_json`) declares temperature/seed as real args.

// The default values — the SINGLE source of truth shared by the arg builders (which
// advertise them in the cap definition, mirroring the fabric tomls) and the reader
// (which substitutes them for an absent optional arg). These MUST equal the
// fabric-toml defaults.
const DEFAULT_MAX_TOKENS: u64 = 4096;
const DEFAULT_TOP_K: i32 = 40;
const DEFAULT_TOP_P: f32 = 0.95;
const DEFAULT_MIN_P: f32 = 0.05;
const DEFAULT_REPEAT_PENALTY: f32 = 1.1;
const DEFAULT_MAX_CONTEXT: u32 = 8192;
const DEFAULT_BATCH_SIZE: u32 = 2048;
const DEFAULT_ROPE_BASE: f32 = 10000.0;
const DEFAULT_ROPE_SCALE: f32 = 1.0;
/// The free-generation cap's temperature/seed defaults (its only configurable
/// sampling knobs; the judgment caps use the deterministic constants instead).
const DEFAULT_TEMPERATURE: f32 = 0.7;
const DEFAULT_SEED: u32 = 1234;

/// Greedy-decoding temperature for the judgment caps (reproducible judgments). Not a
/// configurable arg — judgment reproducibility is intrinsic, never user-tunable.
const JUDGMENT_TEMPERATURE: f32 = 0.0;
/// Fixed seed for the judgment caps. Inert under greedy decoding, pinned so the
/// request is fully specified.
const JUDGMENT_SEED: u32 = DEFAULT_SEED;

/// URNs of the nine configurable inference-param args every LLM cap declares.
const MAX_TOKENS_URN: &str = "media:max-tokens;inference;limit;user;task;numeric";
const TOP_K_URN: &str = "media:top-k;inference;sampling;user;task;numeric";
const TOP_P_URN: &str = "media:top-p;inference;sampling;user;task;numeric";
const MIN_P_URN: &str = "media:min-p;inference;sampling;user;task;numeric";
const REPEAT_PENALTY_URN: &str = "media:repeat-penalty;inference;sampling;user;task;numeric";
const MAX_CONTEXT_URN: &str = "media:max-context-length;inference;limit;operator;model;numeric";
const BATCH_SIZE_URN: &str = "media:batch-size;inference;limit;operator;task;numeric";
const ROPE_BASE_URN: &str = "media:rope-frequency-base;inference;policy;operator;model;numeric";
const ROPE_SCALE_URN: &str = "media:rope-frequency-scale;inference;policy;operator;model;numeric";
/// URNs of the temperature/seed args, declared only by the free-generation cap.
const TEMPERATURE_URN: &str = "media:temperature;inference;sampling;user;task;numeric";
const SEED_URN: &str = "media:sampling-seed;inference;sampling;user;task;numeric";
const MODEL_SPEC_URN: &str =
    "media:enc=utf-8;gguf;llm;model-spec;tokenizer-embedded-gguf";

/// Read an optional numeric inference-param arg from the delivered streams. Returns
/// `None` when the arg is absent (the caller substitutes its declared default — see
/// the module notes: the engine practically always supplies it, but capdag CLI /
/// direct invocation may not). A value that IS present but unparseable fails hard,
/// exposing bad input rather than silently defaulting over it.
fn optional_num<T>(streams: &Streams, urn: &str, name: &str) -> OpResult<Option<T>>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match optional_conforming(streams, urn)? {
        None => Ok(None),
        Some(raw) => raw.trim().parse::<T>().map(Some).map_err(|e| {
            input_arg_error(
                "INVALID_INPUT",
                format!("inference param '{name}' ({urn}) is not a valid number: '{raw}' ({e})"),
                urn,
            )
        }),
    }
}

/// Resolve the fully-populated [`crate::InferenceParams`] from the delivered arg
/// streams. Each of the nine configurable params is read from the streams, falling
/// back to its declared default when absent. `temperature`/`seed` are supplied by the
/// caller — the judgment constants for judgment caps, or the values the
/// free-generation cap resolved from its own temperature/seed args.
fn resolve_inference_params(
    streams: &Streams,
    temperature: f32,
    seed: u32,
) -> OpResult<crate::InferenceParams> {
    Ok(crate::InferenceParams {
        max_tokens: optional_num(streams, MAX_TOKENS_URN, "max_tokens")?.unwrap_or(DEFAULT_MAX_TOKENS),
        temperature,
        top_k: optional_num(streams, TOP_K_URN, "top_k")?.unwrap_or(DEFAULT_TOP_K),
        top_p: optional_num(streams, TOP_P_URN, "top_p")?.unwrap_or(DEFAULT_TOP_P),
        min_p: optional_num(streams, MIN_P_URN, "min_p")?.unwrap_or(DEFAULT_MIN_P),
        seed,
        repeat_penalty: optional_num(streams, REPEAT_PENALTY_URN, "repeat_penalty")?
            .unwrap_or(DEFAULT_REPEAT_PENALTY),
        max_context_length: optional_num(streams, MAX_CONTEXT_URN, "max_context_length")?
            .unwrap_or(DEFAULT_MAX_CONTEXT),
        batch_size: optional_num(streams, BATCH_SIZE_URN, "batch_size")?.unwrap_or(DEFAULT_BATCH_SIZE),
        rope_freq_base: optional_num(streams, ROPE_BASE_URN, "rope_freq_base")?
            .unwrap_or(DEFAULT_ROPE_BASE),
        rope_freq_scale: optional_num(streams, ROPE_SCALE_URN, "rope_freq_scale")?
            .unwrap_or(DEFAULT_ROPE_SCALE),
    })
}

/// Resolve inference params for a judgment cap (deterministic constants for
/// temperature/seed; the nine configurable params from the streams). Shared with the
/// `edit` cap in `main.rs`, whose transform-program generation is likewise
/// deterministic.
pub(crate) fn resolve_judgment_params(
    streams: &[(String, Vec<u8>, Option<StreamMeta>)],
) -> OpResult<crate::InferenceParams> {
    resolve_inference_params(streams, JUDGMENT_TEMPERATURE, JUDGMENT_SEED)
}

/// The stream meta of the primary content input (first stream conforming to
/// `media:enc=utf-8`), carrying the `title` provenance downstream writers use
/// to derive per-row file names. Returns the meta of that stream even when it
/// is `None`, never skipping ahead to a refined sibling's meta.
fn content_stream_meta(streams: &Streams) -> Option<StreamMeta> {
    let p = capdag::MediaUrn::from_string("media:enc=utf-8").ok()?;
    let (_, _, meta) = streams.iter().find(|(urn_str, _, _)| {
        capdag::MediaUrn::from_string(urn_str)
            .map(|u| u.conforms_to(&p).unwrap_or(false))
            .unwrap_or(false)
    })?;
    meta.clone()
}

/// The four lines every Op opens with: take the request + its input, collect
/// all arg streams, and hand back the request (for `peer()`/`output()`) and
/// the streams.
async fn take_request_streams(
    wet: &mut WetContext,
) -> OpResult<(Arc<Request>, Vec<(String, Vec<u8>, Option<StreamMeta>)>)> {
    let req: Arc<Request> = wet
        .get_required(WET_KEY_REQUEST)
        .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;
    let input = req
        .take_input()
        .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;
    let streams = input
        .collect_streams()
        .await
        .map_err(|e| OpError::ExecutionFailed(format!("Stream error: {}", e)))?;
    Ok((req, streams))
}

// =============================================================================
// PROMPT + SCHEMA SUFFIX
// =============================================================================

/// The mandatory JSON-shape suffix appended to every registry-rendered prompt,
/// pinning the exact schema the model must emit. Identical wording to the
/// former cartridge so the constrained decode behaves the same.
fn with_schema_suffix(base_prompt: &str, schema: &serde_json::Value) -> String {
    let schema_json =
        serde_json::to_string_pretty(schema).unwrap_or_else(|_| schema.to_string());
    format!(
        "{}\n\nYou MUST respond with valid JSON matching this exact schema:\n```json\n{}\n```\n\nRespond with ONLY the JSON object, no other text.",
        base_prompt, schema_json
    )
}

// =============================================================================
// EXECUTE FUNCTIONS (copied verbatim from the engine cartridge; only the
// registry access and inference invocation are re-plumbed for the cartridge)
// =============================================================================

/// `generate_json`: schema-constrained free generation over the caller's
/// content. No registry template — the caller supplies both content and the
/// output JSON Schema directly. This is the one non-deterministic cap.
async fn execute_generate_json(
    content: &str,
    output_schema: serde_json::Value,
    model_spec: &str,
    params: &crate::InferenceParams,
    peer: &dyn PeerInvoker,
    output: &OutputStream,
) -> Result<serde_json::Value> {
    let schema_json = serde_json::to_string_pretty(&output_schema)
        .unwrap_or_else(|_| output_schema.to_string());
    let prompt = format!(
        "Analyze the content below and produce a JSON object matching the given schema exactly.\n\n\
         Content:\n{}\n\n\
         You MUST respond with valid JSON matching this exact schema:\n```json\n{}\n```\n\n\
         Respond with ONLY the JSON object, no other text.",
        content, schema_json
    );

    let result_json =
        invoke_constrained_peer(peer, output, 0.0, 0.95, &prompt, output_schema, model_spec, params)
            .await?;

    serde_json::from_str::<serde_json::Value>(&result_json).map_err(|e| {
        error!(
            "Failed to parse generate_json output as JSON: {} (len={}) raw first 200 chars: {}",
            e,
            result_json.len(),
            result_json.chars().take(200).collect::<String>()
        );
        anyhow::anyhow!(
            "generate_json: model output failed JSON parse despite schema constraint: {}",
            e
        )
    })
}

/// `extract`: schema-guided extraction wrapped in the judgment envelope — the
/// caller's schema becomes the type of the envelope's `result` field so the
/// whole response (instance + confidence/reason) is token-level constrained.
async fn execute_extract(
    content: &str,
    user_schema: serde_json::Value,
    model_spec: &str,
    params: &crate::InferenceParams,
    peer: &dyn PeerInvoker,
    output: &OutputStream,
) -> Result<serde_json::Value> {
    let envelope_schema = serde_json::json!({
        "type": "object",
        "properties": {
            "result": user_schema,
            "confidence": { "type": "number", "minimum": 0, "maximum": 1 },
            "reason": { "type": "string", "maxLength": 300 }
        },
        "required": ["result", "confidence", "reason"],
        "additionalProperties": false
    });

    let schema_json = serde_json::to_string_pretty(&envelope_schema)
        .unwrap_or_else(|_| envelope_schema.to_string());
    let prompt = format!(
        "Extract the structured fields described by the schema from the content below.\n\n\
         Content:\n{}\n\n\
         Fill \"result\" with the extracted instance. Extract only what the content \
         actually states — never invent values; where the schema permits, omit or \
         null what is absent. \"confidence\" is your confidence between 0.0 and 1.0 \
         that every extracted field is faithful to the content, and \"reason\" is one \
         short sentence noting anything ambiguous or missing.\n\n\
         You MUST respond with valid JSON matching this exact schema:\n```json\n{}\n```\n\n\
         Respond with ONLY the JSON object, no other text.",
        content, schema_json
    );

    let result_json =
        invoke_constrained_peer(peer, output, 0.0, 0.95, &prompt, envelope_schema, model_spec, params)
            .await?;

    let judgment: serde_json::Value = serde_json::from_str(&result_json).map_err(|e| {
        error!(
            "Failed to parse extract judgment: {} (len={}) raw first 200 chars: {}",
            e,
            result_json.len(),
            result_json.chars().take(200).collect::<String>()
        );
        anyhow::anyhow!(
            "extract: model output failed JSON parse despite schema constraint: {}",
            e
        )
    })?;
    if judgment.get("result").is_none() {
        return Err(anyhow::anyhow!(
            "extract: judgment record is missing the 'result' field"
        ));
    }
    Ok(judgment)
}

/// `make_decision`: one binary yes/no judgment carrying the full envelope
/// (decision + confidence + one-sentence reason).
async fn execute_make_decision(
    content: &str,
    question: &str,
    max_content_length: Option<usize>,
    model_spec: &str,
    params: &crate::InferenceParams,
    peer: &dyn PeerInvoker,
    output: &OutputStream,
) -> Result<(bool, f32, String)> {
    let registry = StructuredQueryRegistry::new();

    // 0/absent = whole content: the gguf backend auto-sizes its context to the
    // prompt and prefills in batch windows, so there is no fixed window to
    // "fit". A positive value is an explicit operator bound — applied VISIBLY.
    let content_for_prompt = bound_content(content, max_content_length);

    let mut substitutions = HashMap::new();
    substitutions.insert(
        "content".to_string(),
        serde_json::Value::String(content_for_prompt),
    );
    substitutions.insert(
        "question".to_string(),
        serde_json::Value::String(question.to_string()),
    );

    let query = registry.query("make_decision").map_err(|e| {
        error!("CRITICAL: 'make_decision' structured query not found in registry");
        anyhow::anyhow!(
            "Bit choice structured query 'make_decision' not found in registry: {}",
            e
        )
    })?;

    let base_prompt = query.generate_prompt(&substitutions)?;
    let prompt_with_schema = with_schema_suffix(&base_prompt, &query.output_schema);

    let result_json = invoke_constrained_peer(
        peer,
        output,
        0.0,
        0.95,
        &prompt_with_schema,
        query.output_schema.clone(),
        model_spec,
        params,
    )
    .await?;

    match serde_json::from_str::<MakeDecisionResult>(&result_json) {
        Ok(make_decision) => match make_decision.res {
            1 => Ok((true, make_decision.confidence, make_decision.reason)),
            0 => Ok((false, make_decision.confidence, make_decision.reason)),
            other => Err(anyhow::anyhow!(
                "Bit choice value must be 0 or 1, got: {}",
                other
            )),
        },
        Err(e) => {
            error!(
                "Failed to parse Make Decision JSON: {}. Raw result (len={}): '{}'",
                e,
                result_json.len(),
                result_json
            );
            Err(anyhow::anyhow!("Failed to parse Make Decision JSON: {}", e))
        }
    }
}

/// `make_multiple_decisions`: N binary judgments over one shared content in a
/// single constrained decode, each carrying its own envelope.
async fn execute_make_multiple_decisions(
    content: &str,
    questions: &[String],
    max_content_length: Option<usize>,
    model_spec: &str,
    params: &crate::InferenceParams,
    peer: &dyn PeerInvoker,
    output: &OutputStream,
) -> Result<Vec<(bool, f32, String)>> {
    let registry = StructuredQueryRegistry::new();

    // Same content-bounding contract as execute_make_decision.
    let content_for_prompt = bound_content(content, max_content_length);

    let mut prompt_substitutions = HashMap::new();
    prompt_substitutions.insert(
        "content".to_string(),
        serde_json::Value::String(content_for_prompt),
    );
    prompt_substitutions.insert(
        "questions".to_string(),
        serde_json::Value::Array(
            questions
                .iter()
                .map(|q| serde_json::Value::String(q.clone()))
                .collect(),
        ),
    );

    let mut schema_variables = HashMap::new();
    schema_variables.insert(
        "expected_length".to_string(),
        serde_json::Value::Number(serde_json::Number::from(questions.len())),
    );

    let query = registry.query("make_multiple_decisions").map_err(|e| {
        error!("CRITICAL: 'make_multiple_decisions' structured query not found in registry");
        anyhow::anyhow!(
            "Bit choices structured query 'make_multiple_decisions' not found in registry: {}",
            e
        )
    })?;

    let base_prompt = query.generate_prompt(&prompt_substitutions)?;

    let output_schema = if query.has_schema_template() {
        query.generate_schema(&schema_variables)?
    } else {
        query.output_schema.clone()
    };

    let prompt_with_schema = with_schema_suffix(&base_prompt, &output_schema);

    let result_json = invoke_constrained_peer(
        peer,
        output,
        0.0,
        0.95,
        &prompt_with_schema,
        output_schema,
        model_spec,
        params,
    )
    .await?;

    match serde_json::from_str::<MakeMultipleDecisionsResult>(&result_json) {
        Ok(make_multiple_decisions) => {
            if make_multiple_decisions.res.len() != questions.len() {
                return Err(anyhow::anyhow!(
                    "Expected {} Make Decision results but got {}",
                    questions.len(),
                    make_multiple_decisions.res.len()
                ));
            }

            if make_multiple_decisions
                .res
                .iter()
                .all(|item| item.res == 0 || item.res == 1)
            {
                let results: Vec<(bool, f32, String)> = make_multiple_decisions
                    .res
                    .into_iter()
                    .map(|item| (item.res == 1, item.confidence, item.reason))
                    .collect();
                Ok(results)
            } else {
                Err(anyhow::anyhow!("Bit choices values must be 0 or 1"))
            }
        }
        Err(e) => {
            error!(
                "Failed to parse Make Multiple Decisions JSON: {}. Raw result (len={}): '{}'",
                e,
                result_json.len(),
                result_json
            );
            Err(anyhow::anyhow!(
                "Failed to parse Make Multiple Decisions JSON: {}",
                e
            ))
        }
    }
}

/// Run one registered semantic-judgment structured query end to end: render
/// the prompt, resolve the (possibly templated) schema, run deterministic
/// constrained inference, parse the judgment, and assert the load-bearing
/// fields are present so a constraint regression fails HERE with a named field
/// rather than downstream with a missing key.
async fn execute_judgment_query(
    query_name: &str,
    prompt_substitutions: HashMap<String, serde_json::Value>,
    schema_variables: Option<HashMap<String, serde_json::Value>>,
    required_fields: &[&str],
    model_spec: &str,
    params: &crate::InferenceParams,
    peer: &dyn PeerInvoker,
    output: &OutputStream,
) -> Result<serde_json::Value> {
    let registry = StructuredQueryRegistry::new();

    let query = registry.query(query_name).map_err(|e| {
        error!("CRITICAL: '{}' structured query not found in registry", query_name);
        anyhow::anyhow!("Structured query '{}' not found in registry: {}", query_name, e)
    })?;

    let base_prompt = query.generate_prompt(&prompt_substitutions)?;
    let output_schema = match (&schema_variables, query.has_schema_template()) {
        (Some(vars), true) => query.generate_schema(vars)?,
        (None, false) => query.output_schema.clone(),
        (Some(_), false) => {
            return Err(anyhow::anyhow!(
                "'{}' was given schema variables but has no schema template",
                query_name
            ));
        }
        (None, true) => {
            return Err(anyhow::anyhow!(
                "'{}' has a schema template but was given no schema variables",
                query_name
            ));
        }
    };

    let prompt_with_schema = with_schema_suffix(&base_prompt, &output_schema);

    let result_json = invoke_constrained_peer(
        peer,
        output,
        0.0,
        0.95,
        &prompt_with_schema,
        output_schema,
        model_spec,
        params,
    )
    .await?;

    let judgment: serde_json::Value = serde_json::from_str(&result_json).map_err(|e| {
        error!(
            "Failed to parse '{}' judgment: {}. Raw (len={}): '{}'",
            query_name,
            e,
            result_json.len(),
            result_json.chars().take(200).collect::<String>()
        );
        anyhow::anyhow!(
            "{}: model output failed JSON parse despite schema constraint: {}",
            query_name,
            e
        )
    })?;
    for field in required_fields {
        if judgment.get(field).is_none() {
            return Err(anyhow::anyhow!(
                "{}: judgment record is missing the required '{}' field",
                query_name,
                field
            ));
        }
    }
    Ok(judgment)
}

/// `same`: judge whether two items refer to the same thing, under an optional
/// context that frames what "the same" means. Returns the full judgment record
/// `{same, confidence, reason}`.
async fn execute_same(
    left: &str,
    right: &str,
    context: Option<&str>,
    model_spec: &str,
    params: &crate::InferenceParams,
    peer: &dyn PeerInvoker,
    output: &OutputStream,
) -> Result<serde_json::Value> {
    let registry = StructuredQueryRegistry::new();

    let mut substitutions = HashMap::new();
    substitutions.insert("left".to_string(), serde_json::Value::String(left.to_string()));
    substitutions.insert(
        "right".to_string(),
        serde_json::Value::String(right.to_string()),
    );
    substitutions.insert(
        "context".to_string(),
        match context.map(str::trim).filter(|c| !c.is_empty()) {
            Some(c) => serde_json::Value::String(c.to_string()),
            None => serde_json::Value::Null,
        },
    );

    let query = registry.query("semantic_same").map_err(|e| {
        error!("CRITICAL: 'semantic_same' structured query not found in registry");
        anyhow::anyhow!("Semantic-same structured query not found in registry: {}", e)
    })?;

    let base_prompt = query.generate_prompt(&substitutions)?;
    let prompt_with_schema = with_schema_suffix(&base_prompt, &query.output_schema);

    let result_json = invoke_constrained_peer(
        peer,
        output,
        0.0,
        0.95,
        &prompt_with_schema,
        query.output_schema.clone(),
        model_spec,
        params,
    )
    .await?;

    let judgment: serde_json::Value = serde_json::from_str(&result_json).map_err(|e| {
        error!(
            "Failed to parse semantic-same judgment: {}. Raw (len={}): '{}'",
            e,
            result_json.len(),
            result_json.chars().take(200).collect::<String>()
        );
        anyhow::anyhow!(
            "same: model output failed JSON parse despite schema constraint: {}",
            e
        )
    })?;
    // The schema constrains the shape; assert the load-bearing field is present
    // so a constraint regression fails here with a named field.
    if judgment.get("same").and_then(|v| v.as_bool()).is_none() {
        return Err(anyhow::anyhow!(
            "same: judgment record is missing the boolean 'same' field"
        ));
    }
    Ok(judgment)
}

// =============================================================================
// HELPERS (copied verbatim from the engine cartridge)
// =============================================================================

/// Apply the optional operator content bound. `None`/`Some(0)` = whole content
/// (the inference backend auto-sizes its context to the prompt and prefills in
/// batch-sized windows, so there is no fixed window to "fit"); a positive bound
/// truncates WITH a warning naming the counts — never silently.
fn bound_content(content: &str, max_content_length: Option<usize>) -> String {
    match max_content_length {
        None | Some(0) => content.to_string(),
        Some(max) => {
            let total = content.chars().count();
            if total > max {
                tracing::warn!(
                    total_chars = total,
                    max_content_length = max,
                    "content exceeds the explicit --max-content-length bound; \
                     truncating to the bound (pass 0 to use the whole content)"
                );
                content.chars().take(max).collect()
            } else {
                content.to_string()
            }
        }
    }
}

/// Parse a `--labels` value: comma- or newline-separated label names, trimmed.
/// Strict: 2..=64 labels, no empties, no duplicates — a malformed label set
/// silently narrowed would corrupt every classification downstream.
fn parse_label_set(raw: &str) -> std::result::Result<Vec<String>, String> {
    let labels: Vec<String> = raw
        .split(|c| c == ',' || c == '\n')
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    if labels.len() < 2 || labels.len() > 64 {
        return Err(format!(
            "--labels must list 2..=64 labels (comma- or newline-separated), got {}",
            labels.len()
        ));
    }
    let mut seen = std::collections::HashSet::new();
    for l in &labels {
        if !seen.insert(l.as_str()) {
            return Err(format!("--labels contains duplicate label '{}'", l));
        }
    }
    Ok(labels)
}

/// Parse a `--scale` value of the form `min..max` (integers, min < max).
fn parse_scale(raw: &str) -> std::result::Result<(i64, i64), String> {
    let (a, b) = raw
        .trim()
        .split_once("..")
        .ok_or_else(|| format!("--scale must be 'min..max' (e.g. 0..10), got '{}'", raw))?;
    let min: i64 = a
        .trim()
        .parse()
        .map_err(|_| format!("--scale min '{}' is not an integer", a.trim()))?;
    let max: i64 = b
        .trim()
        .parse()
        .map_err(|_| format!("--scale max '{}' is not an integer", b.trim()))?;
    if min >= max {
        return Err(format!("--scale must satisfy min < max, got {}..{}", min, max));
    }
    Ok((min, max))
}

/// Parse a `--targets` value: a JSON object mapping target name → contract
/// description. Strict: 2..=64 targets, non-empty names and descriptions (a
/// target without a contract cannot be routed to honestly). Returned
/// alphabetically by name (serde_json objects are order-normalized), which
/// keeps the constrained target enum deterministic regardless of how the caller
/// ordered the JSON.
fn parse_target_set(raw: &str) -> std::result::Result<Vec<(String, String)>, String> {
    let value: serde_json::Value = serde_json::from_str(raw)
        .map_err(|e| format!("--targets must be a JSON object of name: description — {}", e))?;
    let obj = value
        .as_object()
        .ok_or_else(|| "--targets must be a JSON object of name: description".to_string())?;
    let mut targets = Vec::with_capacity(obj.len());
    for (name, desc) in obj {
        let name = name.trim();
        let desc = desc.as_str().map(str::trim).unwrap_or_default();
        if name.is_empty() || desc.is_empty() {
            return Err(format!(
                "--targets entries need a non-empty name and description (offending name: '{}')",
                name
            ));
        }
        targets.push((name.to_string(), desc.to_string()));
    }
    if targets.len() < 2 || targets.len() > 64 {
        return Err(format!(
            "--targets must define 2..=64 targets, got {}",
            targets.len()
        ));
    }
    Ok(targets)
}

/// Enforce grounding on quote-carrying judgments (`ask`/`explain`): every
/// quoted excerpt must occur VERBATIM in the source content (compared with
/// whitespace runs collapsed, so line-wrapping differences don't produce false
/// rejections). A quote that does not occur is fabricated evidence — a hard
/// error, never forwarded.
fn verify_quotes_grounded(
    content: &str,
    quotes: &serde_json::Value,
    cap_name: &str,
) -> Result<()> {
    let quotes = quotes
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("{}: quotes field is not an array", cap_name))?;
    let normalized_content = collapse_ws(content);
    for (i, q) in quotes.iter().enumerate() {
        let text = q
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("{}: quote {} is not a string", cap_name, i))?;
        let needle = collapse_ws(text);
        if needle.is_empty() {
            return Err(anyhow::anyhow!("{}: quote {} is empty", cap_name, i));
        }
        if !normalized_content.contains(&needle) {
            return Err(anyhow::anyhow!(
                "{}: quote {} does not occur in the content — the model fabricated its evidence (quote: '{}')",
                cap_name,
                i,
                text.chars().take(120).collect::<String>()
            ));
        }
    }
    Ok(())
}

/// Collapse whitespace runs to single spaces (for quote matching).
fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Read the `title` provenance value out of a stream's `StreamMeta`. `None`
/// when the meta is absent or the `title` key is missing or non-text; the call
/// site maps that to an empty-string body field (honest absence).
fn stream_meta_title(meta: Option<&StreamMeta>) -> Option<String> {
    let meta = meta?;
    match meta.get("title")? {
        ciborium::Value::Text(s) => Some(s.clone()),
        _ => None,
    }
}

/// Build the wire-shape body for a single `make-decision` row: `{ title,
/// identifier, value, confidence, reason, confidence_below_threshold }`,
/// mirroring the `media:decision;fmt=json;record` schema so a downstream writer
/// can rely on every field being present. Below-threshold confidence is flagged
/// on the record (and warned).
fn build_decision_record(
    title: &str,
    question: &str,
    value: bool,
    confidence: f32,
    reason: &str,
    confidence_threshold: f32,
) -> serde_json::Value {
    let below = confidence < confidence_threshold;
    if below {
        tracing::warn!(
            confidence,
            confidence_threshold,
            question,
            "decision confidence is below the configured threshold"
        );
    }
    serde_json::json!({
        "title": title,
        "identifier": question,
        "value": value,
        "confidence": confidence,
        "reason": reason,
        "confidence_below_threshold": below,
    })
}

// =============================================================================
// OPS
// =============================================================================

/// Emit a scalar (single-item) output carrying the content input's provenance
/// meta. `value` is already the finished payload (a JSON string or plain text).
async fn emit_scalar(req: &Request, meta: Option<StreamMeta>, value: ciborium::Value) -> OpResult<()> {
    let output = req.output();
    output
        .start(false, meta)
        .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;
    output
        .emit_cbor(&value)
        .await
        .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;
    Ok(())
}

/// `cap:generate-json` — schema-constrained generation.
struct GenerateJsonOp;

#[async_trait]
impl Op<()> for GenerateJsonOp {
    async fn perform(&self, _dry: &mut DryContext, wet: &mut WetContext) -> OpResult<()> {
        let (req, streams) = take_request_streams(wet).await?;

        let content = require_conforming(&streams, "media:enc=utf-8", "content")?;
        let schema_str = require_conforming(&streams, "media:fmt=json;json-schema;record", "schema")?;
        let output_schema: serde_json::Value = serde_json::from_str(&schema_str)
            .map_err(|e| input_arg_error("INVALID_INPUT", format!("Failed to parse schema argument as JSON: {}", e), "media:fmt=json;json-schema;record"))?;
        let model_spec = resolve_model_spec(&streams)?;
        // generate_json is the one non-deterministic cap: temperature/seed are real,
        // configurable args here, each backstopped to its declared default.
        let temperature: f32 = optional_num(&streams, TEMPERATURE_URN, "temperature")?
            .unwrap_or(DEFAULT_TEMPERATURE);
        let seed: u32 = optional_num(&streams, SEED_URN, "seed")?
            .unwrap_or(DEFAULT_SEED);
        let inference_params = resolve_inference_params(&streams, temperature, seed)?;

        let result = execute_generate_json(
            &content,
            output_schema,
            &model_spec,
            &inference_params,
            req.peer(),
            req.output(),
        )
            .await
            .map_err(|e| OpError::ExecutionFailed(format!("Failed to execute generate_json: {}", e)))?;

        let body = serde_json::to_string(&result)
            .map_err(|e| OpError::ExecutionFailed(format!("Serialize failed: {}", e)))?;
        emit_scalar(&req, content_stream_meta(&streams), ciborium::Value::Text(body)).await
    }

    fn metadata(&self) -> capdag::OpMetadata {
        capdag::OpMetadata::builder("GenerateJsonOp")
            .description("Schema-constrained JSON generation over the caller's content")
            .build()
    }
}

/// `cap:extract` — schema-guided extraction inside the judgment envelope.
struct ExtractOp;

#[async_trait]
impl Op<()> for ExtractOp {
    async fn perform(&self, _dry: &mut DryContext, wet: &mut WetContext) -> OpResult<()> {
        let (req, streams) = take_request_streams(wet).await?;

        let content = require_conforming(&streams, "media:enc=utf-8", "content")?;
        let schema_str = require_conforming(&streams, "media:fmt=json;json-schema;record", "schema")?;
        let user_schema: serde_json::Value = serde_json::from_str(&schema_str)
            .map_err(|e| input_arg_error("INVALID_INPUT", format!("Failed to parse schema argument as JSON: {}", e), "media:fmt=json;json-schema;record"))?;
        let model_spec = resolve_model_spec(&streams)?;
        let inference_params = resolve_judgment_params(&streams)?;

        let judgment = execute_extract(
            &content,
            user_schema,
            &model_spec,
            &inference_params,
            req.peer(),
            req.output(),
        )
            .await
            .map_err(|e| OpError::ExecutionFailed(format!("Failed to execute extract: {}", e)))?;

        let body = serde_json::to_string_pretty(&judgment)
            .map_err(|e| OpError::ExecutionFailed(format!("Serialize failed: {}", e)))?;
        emit_scalar(&req, content_stream_meta(&streams), ciborium::Value::Text(body)).await
    }

    fn metadata(&self) -> capdag::OpMetadata {
        capdag::OpMetadata::builder("ExtractOp")
            .description("Schema-guided extraction wrapped in the semantic-judgment envelope")
            .build()
    }
}

/// `cap:make-decision` — one binary judgment.
struct MakeDecisionOp;

#[async_trait]
impl Op<()> for MakeDecisionOp {
    async fn perform(&self, _dry: &mut DryContext, wet: &mut WetContext) -> OpResult<()> {
        let (req, streams) = take_request_streams(wet).await?;

        let content = require_conforming(&streams, "media:enc=utf-8", "content")?;
        let question = require_conforming(&streams, "media:enc=utf-8;question", "question")?;
        let max_content_length = optional_num(&streams, "media:max-content-length;numeric", "max_content_length")?;
        // The threshold arg is REAL: below-threshold judgments are flagged on
        // the record (and warned), exactly as the cap doc promises.
        let confidence_threshold: f32 = optional_num(&streams, "media:confidence-threshold;numeric", "confidence_threshold")?
            .unwrap_or(0.7);
        let model_spec = resolve_model_spec(&streams)?;
        let inference_params = resolve_judgment_params(&streams)?;

        let (value, confidence, reason) =
            execute_make_decision(
                &content,
                &question,
                max_content_length,
                &model_spec,
                &inference_params,
                req.peer(),
                req.output(),
            )
                .await
                .map_err(|e| OpError::ExecutionFailed(format!("Failed to execute Make Decision: {}", e)))?;

        // The input stream's `title` provenance becomes a first-class column on
        // the decision record so a downstream writer can derive a per-row file
        // name from the body without inspecting StreamMeta separately.
        let meta = content_stream_meta(&streams);
        let title = stream_meta_title(meta.as_ref()).unwrap_or_default();
        let decision = build_decision_record(
            &title,
            &question,
            value,
            confidence,
            &reason,
            confidence_threshold,
        );
        let body = serde_json::to_string_pretty(&decision)
            .map_err(|e| OpError::ExecutionFailed(format!("Failed to serialize decision: {}", e)))?;
        emit_scalar(&req, meta, ciborium::Value::Text(body)).await
    }

    fn metadata(&self) -> capdag::OpMetadata {
        capdag::OpMetadata::builder("MakeDecisionOp")
            .description("One binary yes/no judgment with confidence and reason")
            .build()
    }
}

/// `cap:make-multiple-decisions` — N binary judgments over one content, emitted
/// as a sequence (one record per question).
struct MakeMultipleDecisionsOp;

#[async_trait]
impl Op<()> for MakeMultipleDecisionsOp {
    async fn perform(&self, _dry: &mut DryContext, wet: &mut WetContext) -> OpResult<()> {
        let (req, streams) = take_request_streams(wet).await?;

        let content = require_conforming(&streams, "media:enc=utf-8", "content")?;
        let questions_str = require_conforming(&streams, "media:enc=utf-8;question", "questions")?;
        let questions: Vec<String> = questions_str
            .lines()
            .filter(|line| !line.is_empty())
            .map(|line| line.to_string())
            .collect();
        let max_content_length = optional_num(&streams, "media:max-content-length;numeric", "max_content_length")?;
        let confidence_threshold: f32 = optional_num(&streams, "media:confidence-threshold;numeric", "confidence_threshold")?
            .unwrap_or(0.7);
        let model_spec = resolve_model_spec(&streams)?;
        let inference_params = resolve_judgment_params(&streams)?;

        let results = execute_make_multiple_decisions(
            &content,
            &questions,
            max_content_length,
            &model_spec,
            &inference_params,
            req.peer(),
            req.output(),
        )
        .await
        .map_err(|e| OpError::ExecutionFailed(format!("Failed to execute Make Multiple Decisions: {}", e)))?;

        // Each per-row decision carries the question both as its stream meta
        // `title` (so downstream sequence consumers can route on it) AND as a
        // `title` column on the JSON body. The two must remain in lockstep.
        let output = req.output();
        output
            .start(true, None)
            .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;
        for ((value, confidence, reason), question) in results.iter().zip(questions.iter()) {
            let decision = build_decision_record(
                question,
                question,
                *value,
                *confidence,
                reason,
                confidence_threshold,
            );
            let json_bytes = serde_json::to_vec_pretty(&decision)
                .map_err(|e| OpError::ExecutionFailed(format!("Failed to serialize decision JSON: {}", e)))?;
            let mut meta = std::collections::BTreeMap::new();
            meta.insert("title".to_string(), ciborium::Value::Text(question.clone()));
            output
                .emit_list_item(&ciborium::Value::Bytes(json_bytes), Some(meta))
                .await
                .map_err(|e| OpError::ExecutionFailed(format!("emit_list_item failed: {}", e)))?;
        }
        Ok(())
    }

    fn metadata(&self) -> capdag::OpMetadata {
        capdag::OpMetadata::builder("MakeMultipleDecisionsOp")
            .description("One binary judgment per question over shared content, emitted as a sequence")
            .build()
    }
}

/// `cap:same` — semantic-equivalence judgment over two items.
struct SameOp;

#[async_trait]
impl Op<()> for SameOp {
    async fn perform(&self, _dry: &mut DryContext, wet: &mut WetContext) -> OpResult<()> {
        let (req, streams) = take_request_streams(wet).await?;

        let left = require_conforming(&streams, "media:enc=utf-8", "first item")?;
        let right = require_conforming(&streams, "media:enc=utf-8;second-item", "second item")?;
        let context = optional_conforming(&streams, "media:criterion;enc=utf-8")?;
        let model_spec = resolve_model_spec(&streams)?;
        let inference_params = resolve_judgment_params(&streams)?;

        let judgment = execute_same(
            &left,
            &right,
            context.as_deref(),
            &model_spec,
            &inference_params,
            req.peer(),
            req.output(),
        )
            .await
            .map_err(|e| OpError::ExecutionFailed(format!("Failed to execute same: {}", e)))?;

        let body = serde_json::to_string_pretty(&judgment)
            .map_err(|e| OpError::ExecutionFailed(format!("Failed to serialize judgment: {}", e)))?;
        emit_scalar(&req, content_stream_meta(&streams), ciborium::Value::Text(body)).await
    }

    fn metadata(&self) -> capdag::OpMetadata {
        capdag::OpMetadata::builder("SameOp")
            .description("Judge whether two items refer to the same thing, under an optional context")
            .build()
    }
}

/// `cap:classify` — single-label classification against a caller-supplied set.
struct ClassifyOp;

#[async_trait]
impl Op<()> for ClassifyOp {
    async fn perform(&self, _dry: &mut DryContext, wet: &mut WetContext) -> OpResult<()> {
        let (req, streams) = take_request_streams(wet).await?;

        let content = require_conforming(&streams, "media:enc=utf-8", "content")?;
        let labels_raw = require_conforming(&streams, "media:enc=utf-8;label-set", "labels")?;
        let labels = parse_label_set(&labels_raw).map_err(|message| {
            input_arg_error("INVALID_INPUT", message, "media:enc=utf-8;label-set")
        })?;
        let context = optional_conforming(&streams, "media:criterion;enc=utf-8")?;
        let model_spec = resolve_model_spec(&streams)?;

        let mut subs = HashMap::new();
        subs.insert("content".to_string(), serde_json::Value::String(content));
        subs.insert(
            "labels_display".to_string(),
            serde_json::Value::String(labels.join(", ")),
        );
        subs.insert(
            "context".to_string(),
            match context.map(|c| c.trim().to_string()).filter(|c| !c.is_empty()) {
                Some(c) => serde_json::Value::String(c),
                None => serde_json::Value::Null,
            },
        );
        let mut schema_vars = HashMap::new();
        schema_vars.insert(
            "labels_json".to_string(),
            serde_json::Value::String(serde_json::to_string(&labels).expect("label list serializes")),
        );

        let inference_params = resolve_judgment_params(&streams)?;
        let judgment = execute_judgment_query(
            "semantic_classify",
            subs,
            Some(schema_vars),
            &["label", "confidence", "reason"],
            &model_spec,
            &inference_params,
            req.peer(),
            req.output(),
        )
        .await
        .map_err(|e| OpError::ExecutionFailed(format!("Failed to execute classify: {}", e)))?;

        // The enum constraint makes an out-of-set label impossible; assert it
        // anyway so a constraint regression is caught here.
        let label = judgment["label"].as_str().unwrap_or_default();
        if !labels.iter().any(|l| l == label) {
            return Err(OpError::ExecutionFailed(format!(
                "classify produced label '{}' outside the allowed set {:?} — constraint regression",
                label, labels
            )));
        }
        let body = serde_json::to_string_pretty(&judgment)
            .map_err(|e| OpError::ExecutionFailed(format!("Serialize failed: {}", e)))?;
        emit_scalar(&req, content_stream_meta(&streams), ciborium::Value::Text(body)).await
    }

    fn metadata(&self) -> capdag::OpMetadata {
        capdag::OpMetadata::builder("ClassifyOp")
            .description("Single-label classification against a caller-supplied label set")
            .build()
    }
}

/// `cap:score` — integer score on a caller-supplied scale against a rubric.
struct ScoreOp;

#[async_trait]
impl Op<()> for ScoreOp {
    async fn perform(&self, _dry: &mut DryContext, wet: &mut WetContext) -> OpResult<()> {
        let (req, streams) = take_request_streams(wet).await?;

        let content = require_conforming(&streams, "media:enc=utf-8", "content")?;
        let rubric = require_conforming(&streams, "media:enc=utf-8;rubric", "rubric")?;
        let scale_raw = optional_conforming(&streams, "media:enc=utf-8;scale")?
            .unwrap_or_else(|| "0..10".to_string());
        let (scale_min, scale_max) = parse_scale(&scale_raw).map_err(|message| {
            input_arg_error("INVALID_INPUT", message, "media:enc=utf-8;scale")
        })?;
        let model_spec = resolve_model_spec(&streams)?;

        let mut subs = HashMap::new();
        subs.insert("content".to_string(), serde_json::Value::String(content));
        subs.insert("rubric".to_string(), serde_json::Value::String(rubric));
        subs.insert("scale_min".to_string(), serde_json::json!(scale_min));
        subs.insert("scale_max".to_string(), serde_json::json!(scale_max));
        let mut schema_vars = HashMap::new();
        schema_vars.insert("scale_min".to_string(), serde_json::json!(scale_min));
        schema_vars.insert("scale_max".to_string(), serde_json::json!(scale_max));

        let inference_params = resolve_judgment_params(&streams)?;
        let judgment = execute_judgment_query(
            "semantic_score",
            subs,
            Some(schema_vars),
            &["score", "confidence", "reason"],
            &model_spec,
            &inference_params,
            req.peer(),
            req.output(),
        )
        .await
        .map_err(|e| OpError::ExecutionFailed(format!("Failed to execute score: {}", e)))?;

        let body = serde_json::to_string_pretty(&judgment)
            .map_err(|e| OpError::ExecutionFailed(format!("Serialize failed: {}", e)))?;
        emit_scalar(&req, content_stream_meta(&streams), ciborium::Value::Text(body)).await
    }

    fn metadata(&self) -> capdag::OpMetadata {
        capdag::OpMetadata::builder("ScoreOp")
            .description("Integer score on a caller-supplied scale against a rubric")
            .build()
    }
}

/// `cap:verify` — pass/fail an artifact against requirements, with violations.
struct VerifyOp;

#[async_trait]
impl Op<()> for VerifyOp {
    async fn perform(&self, _dry: &mut DryContext, wet: &mut WetContext) -> OpResult<()> {
        let (req, streams) = take_request_streams(wet).await?;

        let content = require_conforming(&streams, "media:enc=utf-8", "artifact")?;
        let requirements = require_conforming(&streams, "media:enc=utf-8;rubric", "requirements")?;
        let model_spec = resolve_model_spec(&streams)?;

        let mut subs = HashMap::new();
        subs.insert("content".to_string(), serde_json::Value::String(content));
        subs.insert(
            "requirements".to_string(),
            serde_json::Value::String(requirements),
        );

        let inference_params = resolve_judgment_params(&streams)?;
        let judgment = execute_judgment_query(
            "semantic_verify",
            subs,
            None,
            &["pass", "violations", "confidence", "reason"],
            &model_spec,
            &inference_params,
            req.peer(),
            req.output(),
        )
        .await
        .map_err(|e| OpError::ExecutionFailed(format!("Failed to execute verify: {}", e)))?;

        // Internal consistency: pass=true with violations (or pass=false with
        // none) is a broken judgment — expose it.
        let pass = judgment["pass"].as_bool().unwrap_or(false);
        let n_violations = judgment["violations"].as_array().map(|v| v.len()).unwrap_or(0);
        if pass == (n_violations > 0) {
            return Err(OpError::ExecutionFailed(format!(
                "verify produced an inconsistent judgment: pass={} with {} violation(s)",
                pass, n_violations
            )));
        }
        let body = serde_json::to_string_pretty(&judgment)
            .map_err(|e| OpError::ExecutionFailed(format!("Serialize failed: {}", e)))?;
        emit_scalar(&req, content_stream_meta(&streams), ciborium::Value::Text(body)).await
    }

    fn metadata(&self) -> capdag::OpMetadata {
        capdag::OpMetadata::builder("VerifyOp")
            .description("Pass/fail an artifact against requirements, listing violations")
            .build()
    }
}

/// `cap:route` — pick one target from a caller-supplied contract set.
struct RouteOp;

#[async_trait]
impl Op<()> for RouteOp {
    async fn perform(&self, _dry: &mut DryContext, wet: &mut WetContext) -> OpResult<()> {
        let (req, streams) = take_request_streams(wet).await?;

        let content = require_conforming(&streams, "media:enc=utf-8", "content")?;
        let targets_raw = require_conforming(&streams, "media:fmt=json;record;target-set", "targets")?;
        let targets = parse_target_set(&targets_raw).map_err(|message| {
            input_arg_error("INVALID_INPUT", message, "media:fmt=json;record;target-set")
        })?;
        let model_spec = resolve_model_spec(&streams)?;

        let names: Vec<String> = targets.iter().map(|(n, _)| n.clone()).collect();
        let display = targets
            .iter()
            .map(|(n, d)| format!("- {}: {}", n, d))
            .collect::<Vec<_>>()
            .join("\n");

        let mut subs = HashMap::new();
        subs.insert("content".to_string(), serde_json::Value::String(content));
        subs.insert("targets_display".to_string(), serde_json::Value::String(display));
        let mut schema_vars = HashMap::new();
        schema_vars.insert(
            "targets_json".to_string(),
            serde_json::Value::String(serde_json::to_string(&names).expect("target names serialize")),
        );

        let inference_params = resolve_judgment_params(&streams)?;
        let judgment = execute_judgment_query(
            "semantic_route",
            subs,
            Some(schema_vars),
            &["target", "confidence", "reason"],
            &model_spec,
            &inference_params,
            req.peer(),
            req.output(),
        )
        .await
        .map_err(|e| OpError::ExecutionFailed(format!("Failed to execute route: {}", e)))?;

        let target = judgment["target"].as_str().unwrap_or_default();
        if !names.iter().any(|n| n == target) {
            return Err(OpError::ExecutionFailed(format!(
                "route produced target '{}' outside the allowed set {:?} — constraint regression",
                target, names
            )));
        }
        let body = serde_json::to_string_pretty(&judgment)
            .map_err(|e| OpError::ExecutionFailed(format!("Serialize failed: {}", e)))?;
        emit_scalar(&req, content_stream_meta(&streams), ciborium::Value::Text(body)).await
    }

    fn metadata(&self) -> capdag::OpMetadata {
        capdag::OpMetadata::builder("RouteOp")
            .description("Pick one target from a caller-supplied contract set")
            .build()
    }
}

/// `cap:normalize` — canonicalize a value to its standard form for an entity type.
struct NormalizeOp;

#[async_trait]
impl Op<()> for NormalizeOp {
    async fn perform(&self, _dry: &mut DryContext, wet: &mut WetContext) -> OpResult<()> {
        let (req, streams) = take_request_streams(wet).await?;

        let content = require_conforming(&streams, "media:enc=utf-8", "value")?;
        let entity_type = require_conforming(&streams, "media:enc=utf-8;entity-type", "entity type")?;
        let locale = optional_conforming(&streams, "media:enc=utf-8;locale")?;
        let model_spec = resolve_model_spec(&streams)?;

        let mut subs = HashMap::new();
        subs.insert("content".to_string(), serde_json::Value::String(content));
        subs.insert("entity_type".to_string(), serde_json::Value::String(entity_type));
        subs.insert(
            "locale".to_string(),
            match locale.map(|l| l.trim().to_string()).filter(|l| !l.is_empty()) {
                Some(l) => serde_json::Value::String(l),
                None => serde_json::Value::Null,
            },
        );

        let inference_params = resolve_judgment_params(&streams)?;
        let judgment = execute_judgment_query(
            "semantic_normalize",
            subs,
            None,
            &["canonical", "confidence", "reason"],
            &model_spec,
            &inference_params,
            req.peer(),
            req.output(),
        )
        .await
        .map_err(|e| OpError::ExecutionFailed(format!("Failed to execute normalize: {}", e)))?;

        let body = serde_json::to_string_pretty(&judgment)
            .map_err(|e| OpError::ExecutionFailed(format!("Serialize failed: {}", e)))?;
        emit_scalar(&req, content_stream_meta(&streams), ciborium::Value::Text(body)).await
    }

    fn metadata(&self) -> capdag::OpMetadata {
        capdag::OpMetadata::builder("NormalizeOp")
            .description("Canonicalize a value to its standard form for an entity type")
            .build()
    }
}

/// `cap:ask` — grounded question answering: every quote must occur in the content.
struct AskOp;

#[async_trait]
impl Op<()> for AskOp {
    async fn perform(&self, _dry: &mut DryContext, wet: &mut WetContext) -> OpResult<()> {
        let (req, streams) = take_request_streams(wet).await?;

        let content = require_conforming(&streams, "media:enc=utf-8", "content")?;
        let question = require_conforming(&streams, "media:enc=utf-8;question", "question")?;
        let model_spec = resolve_model_spec(&streams)?;

        let mut subs = HashMap::new();
        subs.insert("content".to_string(), serde_json::Value::String(content.clone()));
        subs.insert("question".to_string(), serde_json::Value::String(question));

        let inference_params = resolve_judgment_params(&streams)?;
        let judgment = execute_judgment_query(
            "semantic_ask",
            subs,
            None,
            &["answer", "quotes", "confidence", "reason"],
            &model_spec,
            &inference_params,
            req.peer(),
            req.output(),
        )
        .await
        .map_err(|e| OpError::ExecutionFailed(format!("Failed to execute ask: {}", e)))?;

        // Grounding enforcement (the whole point of `ask`): every quote must
        // actually occur in the content. A fabricated quote is an answer
        // resting on invented evidence — hard error, never forwarded.
        verify_quotes_grounded(&content, &judgment["quotes"], "ask")
            .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;
        let body = serde_json::to_string_pretty(&judgment)
            .map_err(|e| OpError::ExecutionFailed(format!("Serialize failed: {}", e)))?;
        emit_scalar(&req, content_stream_meta(&streams), ciborium::Value::Text(body)).await
    }

    fn metadata(&self) -> capdag::OpMetadata {
        capdag::OpMetadata::builder("AskOp")
            .description("Grounded question answering with verbatim-quote evidence")
            .build()
    }
}

/// `cap:explain` — hypothesis + grounded evidence + next checks for a goal.
struct ExplainOp;

#[async_trait]
impl Op<()> for ExplainOp {
    async fn perform(&self, _dry: &mut DryContext, wet: &mut WetContext) -> OpResult<()> {
        let (req, streams) = take_request_streams(wet).await?;

        let content = require_conforming(&streams, "media:enc=utf-8", "data")?;
        let goal = require_conforming(&streams, "media:criterion;enc=utf-8", "goal")?;
        let model_spec = resolve_model_spec(&streams)?;

        let mut subs = HashMap::new();
        subs.insert("content".to_string(), serde_json::Value::String(content.clone()));
        subs.insert("goal".to_string(), serde_json::Value::String(goal));

        let inference_params = resolve_judgment_params(&streams)?;
        let judgment = execute_judgment_query(
            "semantic_explain",
            subs,
            None,
            &["hypothesis", "evidence", "next_checks", "confidence", "reason"],
            &model_spec,
            &inference_params,
            req.peer(),
            req.output(),
        )
        .await
        .map_err(|e| OpError::ExecutionFailed(format!("Failed to execute explain: {}", e)))?;

        // Same grounding enforcement as `ask`: evidence must occur in the data.
        verify_quotes_grounded(&content, &judgment["evidence"], "explain")
            .map_err(|e| OpError::ExecutionFailed(e.to_string()))?;
        let body = serde_json::to_string_pretty(&judgment)
            .map_err(|e| OpError::ExecutionFailed(format!("Serialize failed: {}", e)))?;
        emit_scalar(&req, content_stream_meta(&streams), ciborium::Value::Text(body)).await
    }

    fn metadata(&self) -> capdag::OpMetadata {
        capdag::OpMetadata::builder("ExplainOp")
            .description("Hypothesis + grounded evidence + next checks for a goal")
            .build()
    }
}

/// `cap:summarize` — purpose-driven plain-text summary (the summary text is the
/// output; confidence/dropped-detail go to the log, not the artifact).
struct SummarizeOp;

#[async_trait]
impl Op<()> for SummarizeOp {
    async fn perform(&self, _dry: &mut DryContext, wet: &mut WetContext) -> OpResult<()> {
        let (req, streams) = take_request_streams(wet).await?;

        let content = require_conforming(&streams, "media:enc=utf-8", "content")?;
        let purpose = require_conforming(&streams, "media:criterion;enc=utf-8", "purpose")?;
        let budget_words = match optional_conforming(&streams, "media:numeric;word-budget")? {
            None => None,
            Some(raw) => {
                let n: u64 = raw.trim().parse().map_err(|_| {
                    input_arg_error(
                        "INVALID_INPUT",
                        format!("--budget must be a positive word count, got '{}'", raw),
                        "media:numeric;word-budget",
                    )
                })?;
                if n == 0 {
                    return Err(input_arg_error(
                        "INVALID_INPUT",
                        "--budget must be a positive word count, got 0",
                        "media:numeric;word-budget",
                    ));
                }
                Some(n)
            }
        };
        let model_spec = resolve_model_spec(&streams)?;

        let mut subs = HashMap::new();
        subs.insert("content".to_string(), serde_json::Value::String(content));
        subs.insert("purpose".to_string(), serde_json::Value::String(purpose));
        subs.insert(
            "budget_words".to_string(),
            match budget_words {
                Some(n) => serde_json::json!(n),
                None => serde_json::Value::Null,
            },
        );

        let inference_params = resolve_judgment_params(&streams)?;
        let judgment = execute_judgment_query(
            "semantic_summarize",
            subs,
            None,
            &["summary", "confidence", "reason"],
            &model_spec,
            &inference_params,
            req.peer(),
            req.output(),
        )
        .await
        .map_err(|e| OpError::ExecutionFailed(format!("Failed to execute summarize: {}", e)))?;

        // The cap's output is the summary TEXT itself; confidence and the
        // dropped-detail note go to the caller's log, not the artifact.
        let summary = judgment["summary"].as_str().unwrap_or_default().to_string();
        if summary.trim().is_empty() {
            return Err(OpError::ExecutionFailed(format!(
                "summarize produced an empty summary — reason: {}",
                judgment["reason"].as_str().unwrap_or("<none>")
            )));
        }
        info!(
            confidence = judgment["confidence"].as_f64().unwrap_or(0.0),
            dropped = judgment["reason"].as_str().unwrap_or(""),
            "summarize judgment"
        );
        emit_scalar(&req, content_stream_meta(&streams), ciborium::Value::Text(summary)).await
    }

    fn metadata(&self) -> capdag::OpMetadata {
        capdag::OpMetadata::builder("SummarizeOp")
            .description("Purpose-driven plain-text summary")
            .build()
    }
}

// =============================================================================
// REGISTRATION
// =============================================================================

/// Register every semantic-cap Op with the cartridge runtime under its
/// canonical cap URN — the same URNs `semantic_caps()` advertises, from the
/// canonical `capdag::standard` builders, so advertised and dispatched URNs are
/// byte-identical (the catalog-drift guard dispatches by URN).
pub fn register_semantic_ops(runtime: &mut capdag::CartridgeRuntime) {
    use capdag::standard;
    let en = "en";
    runtime.register_op(&standard::generate_json_urn(en).to_string(), || Box::new(GenerateJsonOp));
    runtime.register_op(&standard::extract_urn(en).to_string(), || Box::new(ExtractOp));
    runtime.register_op(&standard::make_decision_urn(en).to_string(), || Box::new(MakeDecisionOp));
    runtime.register_op(&standard::make_multiple_decisions_urn(en).to_string(), || Box::new(MakeMultipleDecisionsOp));
    runtime.register_op(&standard::same_urn(en).to_string(), || Box::new(SameOp));
    runtime.register_op(&standard::classify_urn(en).to_string(), || Box::new(ClassifyOp));
    runtime.register_op(&standard::score_urn(en).to_string(), || Box::new(ScoreOp));
    runtime.register_op(&standard::verify_urn(en).to_string(), || Box::new(VerifyOp));
    runtime.register_op(&standard::route_urn(en).to_string(), || Box::new(RouteOp));
    runtime.register_op(&standard::normalize_urn(en).to_string(), || Box::new(NormalizeOp));
    runtime.register_op(&standard::ask_urn(en).to_string(), || Box::new(AskOp));
    runtime.register_op(&standard::explain_urn(en).to_string(), || Box::new(ExplainOp));
    runtime.register_op(&standard::summarize_urn(en).to_string(), || Box::new(SummarizeOp));
}

// -----------------------------------------------------------------------------
// MANIFEST CAP DEFINITIONS
// -----------------------------------------------------------------------------
// These mirror fabric/caps/{cap}-en.toml exactly (the fabric catalog these caps
// were served from while they lived in the engine). URNs come from the same
// `capdag::standard` builders `semantic_ops()` registers, so advertised and
// registered URNs are identical.

/// The `--model-spec` arg every semantic cap shares (default = the fabric
/// catalog's declared default GGUF LLM).
fn model_spec_arg() -> capdag::CapArg {
    let mut a = capdag::CapArg::with_description(
        "media:enc=utf-8;gguf;llm;model-spec;tokenizer-embedded-gguf",
        false,
        vec![capdag::ArgSource::CliFlag { cli_flag: "--model-spec".to_string() }],
        "Model spec used for the constrained inference".to_string(),
    );
    a.default_value = Some(serde_json::json!(DEFAULT_LLM_MODEL));
    a
}

/// A numeric CLI-flag inference-param arg with its default. The URN is a config
/// setting URN, so the engine's settings stack can override the fabric-toml default
/// per cap/model/user; the value the cartridge receives is whatever was resolved.
fn num_arg(media: &str, flag: &str, desc: &str, default: serde_json::Value) -> capdag::CapArg {
    let mut a = capdag::CapArg::with_description(
        media,
        false,
        vec![capdag::ArgSource::CliFlag { cli_flag: flag.to_string() }],
        desc.to_string(),
    );
    a.default_value = Some(default);
    a
}

/// The nine configurable inference-param args every LLM cap shares. Temperature and
/// seed are deliberately excluded (see the module's INFERENCE PARAMETERS notes):
/// judgment caps are deterministic and do not expose them. Defaults mirror the
/// fabric tomls exactly. Shared with the `edit` cap in `main.rs`.
pub(crate) fn inference_args() -> Vec<capdag::CapArg> {
    vec![
        num_arg(MAX_TOKENS_URN, "--max-tokens", "Maximum tokens to generate", serde_json::json!(DEFAULT_MAX_TOKENS)),
        num_arg(TOP_K_URN, "--top-k", "Top-k sampling parameter", serde_json::json!(DEFAULT_TOP_K)),
        num_arg(TOP_P_URN, "--top-p", "Top-p (nucleus) sampling parameter", serde_json::json!(DEFAULT_TOP_P)),
        num_arg(MIN_P_URN, "--min-p", "Minimum-probability sampling parameter", serde_json::json!(DEFAULT_MIN_P)),
        num_arg(REPEAT_PENALTY_URN, "--repeat-penalty", "Repetition penalty for token sampling", serde_json::json!(DEFAULT_REPEAT_PENALTY)),
        num_arg(MAX_CONTEXT_URN, "--max-context-length", "Maximum context length", serde_json::json!(DEFAULT_MAX_CONTEXT)),
        num_arg(BATCH_SIZE_URN, "--batch-size", "Batch size for processing", serde_json::json!(DEFAULT_BATCH_SIZE)),
        num_arg(ROPE_BASE_URN, "--rope-freq-base", "RoPE frequency base", serde_json::json!(DEFAULT_ROPE_BASE)),
        num_arg(ROPE_SCALE_URN, "--rope-freq-scale", "RoPE frequency scale parameter", serde_json::json!(DEFAULT_ROPE_SCALE)),
    ]
}

/// The temperature + seed args — declared only by the one free-generation cap
/// (`generate_json`), whose output is not required to be reproducible.
fn sampling_temperature_args() -> Vec<capdag::CapArg> {
    vec![
        num_arg(TEMPERATURE_URN, "--temperature", "Sampling temperature", serde_json::json!(DEFAULT_TEMPERATURE)),
        num_arg(SEED_URN, "--seed", "Random seed for reproducible sampling", serde_json::json!(DEFAULT_SEED)),
    ]
}

/// Add the shared LLM control args (`--model-spec` + the nine configurable inference
/// params) to a cap — the set every constrained-inference cap carries.
fn add_llm_args(cap: &mut capdag::Cap) {
    cap.add_arg(model_spec_arg());
    for a in inference_args() {
        cap.add_arg(a);
    }
}

/// The primary content arg (stdin / positional 0) every semantic cap shares.
fn content_arg(desc: &str) -> capdag::CapArg {
    capdag::CapArg::with_description(
        "media:enc=utf-8",
        true,
        vec![
            capdag::ArgSource::Stdin { stdin: "media:enc=utf-8".to_string() },
            capdag::ArgSource::Position { position: 0 },
        ],
        desc.to_string(),
    )
}

/// The optional `--language` arg (present only on the caps whose toml declares
/// it: generate-json, make-decision, make-multiple-decisions).
fn language_arg() -> capdag::CapArg {
    let mut a = capdag::CapArg::with_description(
        "media:enc=utf-8;language-code",
        false,
        vec![capdag::ArgSource::CliFlag { cli_flag: "--language".to_string() }],
        "Language code for processing".to_string(),
    );
    a.default_value = Some(serde_json::json!("en"));
    a
}

/// A required CLI-flag arg.
fn flag_arg(media: &str, flag: &str, desc: &str) -> capdag::CapArg {
    capdag::CapArg::with_description(
        media,
        true,
        vec![capdag::ArgSource::CliFlag { cli_flag: flag.to_string() }],
        desc.to_string(),
    )
}

/// An optional CLI-flag arg, optionally carrying a default value.
fn opt_flag_arg(media: &str, flag: &str, desc: &str, default: Option<serde_json::Value>) -> capdag::CapArg {
    let mut a = capdag::CapArg::with_description(
        media,
        false,
        vec![capdag::ArgSource::CliFlag { cli_flag: flag.to_string() }],
        desc.to_string(),
    );
    a.default_value = default;
    a
}

/// The full manifest definitions for the 13 semantic caps.
pub fn semantic_caps() -> Vec<capdag::Cap> {
    use capdag::{standard, Cap, CapOutput};
    let en = "en";
    let judgment_out = "media:fmt=json;record;semantic-judgment";
    let mut caps: Vec<Cap> = Vec::new();

    // generate_json
    {
        let mut cap = Cap::with_description(
            standard::generate_json_urn(en),
            "Generate JSON".to_string(),
            vec!["generate_json".to_string()],
            "Produce a JSON object matching a caller-supplied schema from content, guaranteed valid via constrained generation.".to_string());
        cap.add_arg(content_arg("Content to analyze"));
        cap.add_arg(flag_arg("media:fmt=json;json-schema;record", "--schema", "The JSON Schema the output must match"));
        cap.add_arg(language_arg());
        add_llm_args(&mut cap);
        // generate_json is the one non-deterministic cap: temperature/seed are real,
        // configurable args here (they are intrinsic constants on the judgment caps).
        for a in sampling_temperature_args() {
            cap.add_arg(a);
        }
        cap.set_output(CapOutput::new("media:fmt=json;record", "JSON object conforming to the supplied schema."));
        caps.push(cap);
    }

    // extract
    {
        let mut cap = Cap::with_description(
            standard::extract_urn(en),
            "Extract".to_string(),
            vec!["extract".to_string()],
            "Extract the fields described by a schema from content, wrapped in the semantic-judgment envelope.".to_string());
        cap.add_arg(content_arg("Content to extract from"));
        cap.add_arg(flag_arg("media:fmt=json;json-schema;record", "--schema", "The JSON Schema describing the fields to extract"));
        add_llm_args(&mut cap);
        cap.set_output(CapOutput::new(judgment_out, "Semantic-judgment record: result (instance of the supplied schema), confidence, reason"));
        caps.push(cap);
    }

    // make_decision
    {
        let mut cap = Cap::with_description(
            standard::make_decision_urn(en),
            "Make a Decision".to_string(),
            vec!["make_decision".to_string()],
            "Make a single binary (yes/no, true/false) decision based on content and a question".to_string());
        cap.add_arg(content_arg("Content to analyze for the binary decision"));
        cap.add_arg(flag_arg("media:enc=utf-8;question", "--question", "The binary question to answer about the content"));
        cap.add_arg(language_arg());
        cap.add_arg(opt_flag_arg("media:max-content-length;numeric", "--max-content-length", "Optional bound on content length in characters. 0 (the default) uses the whole content.", Some(serde_json::json!(0))));
        cap.add_arg(opt_flag_arg("media:confidence-threshold;numeric", "--confidence-threshold", "Minimum confidence threshold for decisions", Some(serde_json::json!(0.7))));
        add_llm_args(&mut cap);
        cap.set_output(CapOutput::new("media:decision;fmt=json;record", "Binary decision result (true/false)"));
        caps.push(cap);
    }

    // make_multiple_decisions
    {
        let mut cap = Cap::with_description(
            standard::make_multiple_decisions_urn(en),
            "Make Multiple Decisions".to_string(),
            vec!["make_multiple_decisions".to_string()],
            "Make several binary decisions about the same content in one constrained decode, one record per question.".to_string());
        cap.add_arg(content_arg("Content to analyze for the binary decisions"));
        cap.add_arg(flag_arg("media:enc=utf-8;question", "--questions", "The binary questions (one per line) to answer about the content"));
        cap.add_arg(language_arg());
        cap.add_arg(opt_flag_arg("media:max-content-length;numeric", "--max-content-length", "Optional bound on content length in characters. 0 (the default) uses the whole content.", Some(serde_json::json!(0))));
        cap.add_arg(opt_flag_arg("media:confidence-threshold;numeric", "--confidence-threshold", "Minimum confidence threshold for decisions", Some(serde_json::json!(0.7))));
        add_llm_args(&mut cap);
        // is_sequence=true: this is the one semantic cap that emits a *sequence*
        // (one decision record per question). It must match the fabric TOML's
        // `[output] is_sequence = true`, else the cartridge runtime sets up a
        // scalar output channel while the Op emits a sequence — the mismatch
        // prepends a phantom empty item to the terminal.
        cap.set_output(CapOutput::with_full_definition(
            "media:decision;fmt=json;record",
            "One decision per question, each as a JSON object with identifier and value",
            true,
            None,
        ));
        caps.push(cap);
    }

    // same
    {
        let mut cap = Cap::with_description(
            standard::same_urn(en),
            "Same".to_string(),
            vec!["same".to_string()],
            "Judge whether two items refer to the same thing, under an optional context.".to_string());
        cap.add_arg(content_arg("The first item"));
        cap.add_arg(capdag::CapArg::with_description(
            "media:enc=utf-8;second-item",
            true,
            vec![capdag::ArgSource::Position { position: 1 }],
            "The second item".to_string(),
        ));
        cap.add_arg(opt_flag_arg("media:criterion;enc=utf-8", "--context", "Optional context framing what \"the same\" means", None));
        add_llm_args(&mut cap);
        cap.set_output(CapOutput::new(judgment_out, "Semantic-judgment record: same (bool), confidence (0-1), reason (one sentence)"));
        caps.push(cap);
    }

    // classify
    {
        let mut cap = Cap::with_description(
            standard::classify_urn(en),
            "Classify".to_string(),
            vec!["classify".to_string()],
            "Assign content exactly one label from a caller-supplied set.".to_string());
        cap.add_arg(content_arg("Content to classify"));
        cap.add_arg(flag_arg("media:enc=utf-8;label-set", "--labels", "The allowed labels, comma- or newline-separated (2..=64)"));
        cap.add_arg(opt_flag_arg("media:criterion;enc=utf-8", "--context", "Optional context framing the classification", None));
        add_llm_args(&mut cap);
        cap.set_output(CapOutput::new(judgment_out, "Semantic-judgment record: label (from the allowed set), confidence (0-1), reason (one sentence)"));
        caps.push(cap);
    }

    // score
    {
        let mut cap = Cap::with_description(
            standard::score_urn(en),
            "Score".to_string(),
            vec!["score".to_string()],
            "Score content on a caller-supplied integer scale against a rubric.".to_string());
        cap.add_arg(content_arg("Content to score"));
        cap.add_arg(flag_arg("media:enc=utf-8;rubric", "--rubric", "The rubric describing what to score"));
        cap.add_arg(opt_flag_arg("media:enc=utf-8;scale", "--scale", "The integer scale as min..max (default 0..10)", Some(serde_json::json!("0..10"))));
        add_llm_args(&mut cap);
        cap.set_output(CapOutput::new(judgment_out, "Semantic-judgment record: score (integer on the scale), confidence (0-1), reason (one sentence)"));
        caps.push(cap);
    }

    // verify
    {
        let mut cap = Cap::with_description(
            standard::verify_urn(en),
            "Verify".to_string(),
            vec!["verify".to_string()],
            "Verify an artifact against requirements, returning pass/fail with violations.".to_string());
        cap.add_arg(content_arg("The artifact to verify"));
        cap.add_arg(flag_arg("media:enc=utf-8;rubric", "--against", "The requirements to verify against"));
        add_llm_args(&mut cap);
        cap.set_output(CapOutput::new(judgment_out, "Semantic-judgment record: pass (bool), violations (requirement/evidence/severity), confidence, reason"));
        caps.push(cap);
    }

    // route
    {
        let mut cap = Cap::with_description(
            standard::route_urn(en),
            "Route".to_string(),
            vec!["route".to_string()],
            "Pick exactly one target from a caller-supplied contract set for the content.".to_string());
        cap.add_arg(content_arg("Content to route"));
        cap.add_arg(flag_arg("media:fmt=json;record;target-set", "--targets", "A JSON object mapping target name to its contract description (2..=64)"));
        add_llm_args(&mut cap);
        cap.set_output(CapOutput::new(judgment_out, "Semantic-judgment record: target (from the given set), confidence (0-1), reason (one sentence)"));
        caps.push(cap);
    }

    // normalize
    {
        let mut cap = Cap::with_description(
            standard::normalize_urn(en),
            "Normalize".to_string(),
            vec!["normalize".to_string()],
            "Canonicalize a value to its standard form for a given entity type.".to_string());
        cap.add_arg(content_arg("The value to normalize"));
        cap.add_arg(flag_arg("media:enc=utf-8;entity-type", "--type", "The entity type (e.g. date, currency, phone)"));
        cap.add_arg(opt_flag_arg("media:enc=utf-8;locale", "--locale", "Optional locale hint for normalization", None));
        add_llm_args(&mut cap);
        cap.set_output(CapOutput::new(judgment_out, "Semantic-judgment record: canonical (string; empty with confidence 0.0 when unreadable), confidence, reason"));
        caps.push(cap);
    }

    // ask
    {
        let mut cap = Cap::with_description(
            standard::ask_urn(en),
            "Ask".to_string(),
            vec!["ask".to_string()],
            "Answer a question about content with verbatim-quote evidence that is verified to occur in the content.".to_string());
        cap.add_arg(content_arg("Content to answer against"));
        cap.add_arg(flag_arg("media:enc=utf-8;question", "--question", "The question to answer about the content"));
        add_llm_args(&mut cap);
        cap.set_output(CapOutput::new(judgment_out, "Semantic-judgment record: answer, verified verbatim quotes, confidence, reason"));
        caps.push(cap);
    }

    // explain
    {
        let mut cap = Cap::with_description(
            standard::explain_urn(en),
            "Explain".to_string(),
            vec!["explain".to_string()],
            "Propose a hypothesis for a goal, backed by verified evidence from the data plus next checks.".to_string());
        cap.add_arg(content_arg("The data to explain"));
        cap.add_arg(flag_arg("media:criterion;enc=utf-8", "--goal", "The goal the explanation should serve"));
        add_llm_args(&mut cap);
        cap.set_output(CapOutput::new(judgment_out, "Semantic-judgment record: hypothesis, verified evidence, next checks, confidence, reason"));
        caps.push(cap);
    }

    // summarize
    {
        let mut cap = Cap::with_description(
            standard::summarize_urn(en),
            "Summarize".to_string(),
            vec!["summarize".to_string()],
            "Summarize content for a stated purpose, as plain text.".to_string());
        cap.add_arg(content_arg("Content to summarize"));
        cap.add_arg(flag_arg("media:criterion;enc=utf-8", "--for", "The purpose the summary should serve"));
        cap.add_arg(opt_flag_arg("media:numeric;word-budget", "--budget", "Optional positive word budget for the summary", None));
        add_llm_args(&mut cap);
        cap.set_output(CapOutput::new("media:enc=utf-8;ext=txt;plain-text", "The purpose-driven summary as plain text"));
        caps.push(cap);
    }

    caps
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `stream_meta_title` returns the inner string when meta carries a `title`
    /// Text value — the empty-vs-non-empty distinction the writer's filename
    /// derivation relies on.
    #[test]
    fn test0291_stream_meta_title_extracts_text_value() {
        let mut meta = std::collections::BTreeMap::new();
        meta.insert("title".to_string(), ciborium::Value::Text("page_3".to_string()));
        assert_eq!(stream_meta_title(Some(&meta)), Some("page_3".to_string()));
    }

    /// Absent meta returns `None`, not `Some("")`.
    #[test]
    fn test0292_stream_meta_title_none_when_meta_absent() {
        assert_eq!(stream_meta_title(None), None);
    }

    /// Meta present but no `title` key: also `None`.
    #[test]
    fn test0293_stream_meta_title_none_when_key_missing() {
        let mut meta = std::collections::BTreeMap::new();
        meta.insert("subject".to_string(), ciborium::Value::Text("foo".to_string()));
        assert_eq!(stream_meta_title(Some(&meta)), None);
    }

    /// A non-text `title` returns `None` rather than coercing.
    #[test]
    fn test0294_stream_meta_title_none_when_not_text() {
        let mut meta = std::collections::BTreeMap::new();
        meta.insert("title".to_string(), ciborium::Value::Integer(42_i64.into()));
        assert_eq!(stream_meta_title(Some(&meta)), None);
    }

    /// `build_decision_record` emits all six declared fields (the schema's
    /// `additionalProperties: false` forbids more).
    #[test]
    fn test0295_decision_record_carries_declared_fields_exactly() {
        let body = build_decision_record("page_3", "Does X hold?", true, 0.9, "evidence", 0.7);
        assert_eq!(body["title"], serde_json::json!("page_3"));
        assert_eq!(body["identifier"], serde_json::json!("Does X hold?"));
        assert_eq!(body["value"], serde_json::json!(true));
        assert!(body.get("confidence").is_some());
        assert!(body.get("reason").is_some());
        assert!(body.get("confidence_below_threshold").is_some());
        assert_eq!(body.as_object().expect("object").len(), 6);
    }

    /// Empty title is a legitimate value (input meta absent — honest absence);
    /// the wire shape still carries the field.
    #[test]
    fn test0296_decision_record_with_empty_title_keeps_field_present() {
        let body = build_decision_record("", "Q?", false, 0.5, "r", 0.7);
        assert!(body.get("title").is_some(), "title must be present even when empty");
        assert_eq!(body["title"], serde_json::json!(""));
        assert_eq!(body["value"], serde_json::json!(false));
    }

    /// The decision record carries the full judgment envelope and the threshold
    /// flag is computed from the REAL confidence.
    #[test]
    fn test0292b_decision_record_envelope_and_threshold() {
        let rec = build_decision_record("t", "q?", true, 0.9, "clear evidence", 0.7);
        assert_eq!(rec["value"], serde_json::json!(true));
        assert!((rec["confidence"].as_f64().unwrap() - 0.9).abs() < 1e-6);
        assert_eq!(rec["reason"], serde_json::json!("clear evidence"));
        assert_eq!(rec["confidence_below_threshold"], serde_json::json!(false));

        let low = build_decision_record("t", "q?", false, 0.4, "weak evidence", 0.7);
        assert_eq!(low["confidence_below_threshold"], serde_json::json!(true));
    }

    /// Label-set parsing is strict — count bounds, no empties, no duplicates;
    /// both comma and newline separators work.
    #[test]
    fn test0293b_parse_label_set_strict() {
        assert_eq!(
            parse_label_set("urgent, billing,spam").unwrap(),
            vec!["urgent", "billing", "spam"]
        );
        assert_eq!(parse_label_set("a\nb\nc").unwrap(), vec!["a", "b", "c"]);
        assert!(parse_label_set("only-one").is_err(), "one label is not a classification");
        assert!(parse_label_set("a, a, b").is_err(), "duplicates rejected");
        assert!(parse_label_set("  ,  ,  ").is_err(), "empties rejected");
        let many = (0..65).map(|i| format!("l{i}")).collect::<Vec<_>>().join(",");
        assert!(parse_label_set(&many).is_err(), "more than 64 labels rejected");
    }

    /// Scale parsing accepts min..max integers only.
    #[test]
    fn test0297_parse_scale_strict() {
        assert_eq!(parse_scale("0..10").unwrap(), (0, 10));
        assert_eq!(parse_scale(" -5 .. 5 ").unwrap(), (-5, 5));
        assert!(parse_scale("10..0").is_err(), "backwards scale rejected");
        assert!(parse_scale("5..5").is_err(), "empty scale rejected");
        assert!(parse_scale("0-10").is_err(), "wrong separator rejected");
        assert!(parse_scale("a..b").is_err());
    }

    /// Target-set parsing requires a JSON object with non-empty names AND
    /// descriptions, returned alphabetically by name.
    #[test]
    fn test0298_parse_target_set_strict() {
        let targets = parse_target_set(
            r#"{"triage": "bug reports needing owner assignment", "archive": "resolved threads"}"#,
        )
        .unwrap();
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].0, "archive");
        assert_eq!(targets[1].0, "triage");
        assert!(parse_target_set(r#"{"only": "one target"}"#).is_err());
        assert!(parse_target_set(r#"{"a": "", "b": "x"}"#).is_err(), "empty description rejected");
        assert!(parse_target_set(r#"["a", "b"]"#).is_err(), "arrays rejected");
        assert!(parse_target_set("not json").is_err());
    }

    /// Grounding enforcement — verbatim quotes pass (including across line-wrap
    /// differences); fabricated or empty quotes are hard errors.
    #[test]
    fn test0299_verify_quotes_grounded() {
        let content = "The deploy failed at 12:03.\nRoot cause: expired TLS certificate\non the registry mirror.";
        let ok = serde_json::json!(["expired TLS certificate on the registry mirror"]);
        verify_quotes_grounded(content, &ok, "ask").expect("wrapped quote must ground");

        let fabricated = serde_json::json!(["the database was down"]);
        let err = verify_quotes_grounded(content, &fabricated, "ask").unwrap_err();
        assert!(err.to_string().contains("fabricated"), "got: {err}");

        let empty = serde_json::json!([""]);
        assert!(verify_quotes_grounded(content, &empty, "ask").is_err());

        let not_array = serde_json::json!("quote");
        assert!(verify_quotes_grounded(content, &not_array, "ask").is_err());
    }

    // --- Inference parameters (fabric-toml-defaulted cap args) -----------------

    /// With no arg streams supplied (the capdag-CLI / direct-invocation path), every
    /// param backstops to its declared cap-definition default. Concrete values are
    /// asserted so this guards the ACTUAL defaults — notably that `max_tokens` is the
    /// fabric `4096`, never the old hardcoded `2048` that truncated the JSON mid-string.
    #[test]
    fn test0300_inference_params_backstop_to_declared_defaults() {
        let streams: Vec<(String, Vec<u8>, Option<StreamMeta>)> = Vec::new();
        let p = resolve_inference_params(&streams, JUDGMENT_TEMPERATURE, JUDGMENT_SEED)
            .expect("backstop resolution never fails");
        assert_eq!(p.max_tokens, 4096, "max_tokens must be the fabric default, not the old 2048");
        assert_eq!(p.top_k, 40);
        assert_eq!(p.top_p, 0.95_f32);
        assert_eq!(p.min_p, 0.05_f32);
        assert_eq!(p.repeat_penalty, 1.1_f32);
        assert_eq!(p.max_context_length, 8192);
        assert_eq!(p.batch_size, 2048);
        assert_eq!(p.rope_freq_base, 10000.0_f32);
        assert_eq!(p.rope_freq_scale, 1.0_f32);
        // Judgment temperature/seed are the deterministic constants, not configurable.
        assert_eq!(p.temperature, 0.0_f32);
        assert_eq!(p.seed, 1234);
    }

    /// A supplied arg stream (the normal engine-delivered path) is read and parsed;
    /// params not supplied still backstop to their defaults.
    #[test]
    fn test0301_inference_params_read_supplied_values() {
        let streams = vec![
            (MAX_TOKENS_URN.to_string(), b"9001".to_vec(), None),
            (TOP_K_URN.to_string(), b"7".to_vec(), None),
        ];
        let p = resolve_inference_params(&streams, JUDGMENT_TEMPERATURE, JUDGMENT_SEED).unwrap();
        assert_eq!(p.max_tokens, 9001);
        assert_eq!(p.top_k, 7);
        assert_eq!(p.max_context_length, 8192, "unsupplied params still backstop");
    }

    /// A supplied-but-unparseable value fails hard — it exposes bad input rather than
    /// silently substituting the default over a malformed override.
    #[test]
    fn test0302_inference_params_malformed_value_fails_hard() {
        let streams = vec![(MAX_TOKENS_URN.to_string(), b"not-a-number".to_vec(), None)];
        let err = resolve_inference_params(&streams, JUDGMENT_TEMPERATURE, JUDGMENT_SEED)
            .expect_err("a malformed numeric arg must fail, not default over it");
        assert!(err.to_string().contains("max_tokens"), "error must name the param: {err}");
        assert_eq!(err.attribution_class(), capdag::AttributionClass::Input);
        assert_eq!(err.failure_code(), Some("INVALID_INPUT"));
        assert_eq!(err.failure_arg_urn(), Some(MAX_TOKENS_URN));
    }

    /// A present but non-UTF-8 optional argument is invalid input, never
    /// indistinguishable from an absent argument that legitimately defaults.
    #[test]
    fn test0304_optional_argument_invalid_utf8_is_attributed() {
        let streams = vec![(TOP_P_URN.to_string(), vec![0xff], None)];
        let err = resolve_inference_params(&streams, JUDGMENT_TEMPERATURE, JUDGMENT_SEED)
            .expect_err("invalid UTF-8 must not fall back to the TOML default");
        assert_eq!(err.attribution_class(), capdag::AttributionClass::Input);
        assert_eq!(err.failure_code(), Some("INVALID_INPUT"));
        assert_eq!(err.failure_arg_urn(), Some(TOP_P_URN));
    }

    /// Every semantic cap declares all nine configurable inference args, so the engine
    /// surfaces and delivers them. A cap that forgot `add_llm_args` would silently lose
    /// its tunable inference params (and, with the reader's backstop, run on defaults
    /// with no way to override) — this catches that.
    #[test]
    fn test0303_llm_caps_declare_inference_args() {
        let required = [
            MAX_TOKENS_URN, TOP_K_URN, TOP_P_URN, MIN_P_URN, REPEAT_PENALTY_URN,
            MAX_CONTEXT_URN, BATCH_SIZE_URN, ROPE_BASE_URN, ROPE_SCALE_URN,
        ];
        let caps = semantic_caps();
        for cap in &caps {
            for urn in required {
                assert!(
                    cap.get_args().iter().any(|a| a.media_urn == urn),
                    "cap '{}' is missing inference arg '{}'",
                    cap.urn.to_string(),
                    urn,
                );
            }
        }
        assert_eq!(caps.len(), 13, "expected the 13 semantic LLM caps, found {}", caps.len());
    }

    /// Only the free-generation cap exposes temperature/seed as configurable args;
    /// judgment caps keep determinism intrinsic and must NOT expose them.
    #[test]
    fn test0304_temperature_seed_only_on_generate_json() {
        let caps = semantic_caps();
        let gj_urn = capdag::standard::generate_json_urn("en").to_string();
        let gj = caps.iter().find(|c| c.urn.to_string() == gj_urn).expect("generate_json cap present");
        assert!(gj.get_args().iter().any(|a| a.media_urn == TEMPERATURE_URN), "generate_json exposes temperature");
        assert!(gj.get_args().iter().any(|a| a.media_urn == SEED_URN), "generate_json exposes seed");

        let summ_urn = capdag::standard::summarize_urn("en").to_string();
        let summ = caps.iter().find(|c| c.urn.to_string() == summ_urn).expect("summarize cap present");
        assert!(
            !summ.get_args().iter().any(|a| a.media_urn == TEMPERATURE_URN),
            "judgment cap 'summarize' must not expose a configurable temperature",
        );
        assert!(
            !summ.get_args().iter().any(|a| a.media_urn == SEED_URN),
            "judgment cap 'summarize' must not expose a configurable seed",
        );
    }
}
