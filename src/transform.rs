//! The `edit` primitive's deterministic transform engine
//! (docs/semantic-primitives.md, Group 2).
//!
//! `edit` is "describe the transformation like you would to a chatbot,
//! get a Unix tool": the MODEL translates the instruction into a
//! program in the closed operation language below (token-level
//! constrained to [`PROGRAM_SCHEMA`], so an ill-formed program is
//! impossible by construction), and THIS module executes that program
//! deterministically against the data. The model never touches the
//! data — fuzzy in, auditable and replayable out: the same program run
//! on tomorrow's file produces tomorrow's answer by the same rules.
//!
//! v1 data model: a JSON ARRAY of OBJECTS ("records"). Fields are
//! top-level keys. Every operation is total and deterministic; the
//! only runtime errors are contract violations (non-array input,
//! non-object records, an unknown op — impossible under the schema —
//! or a transform that cannot apply to a value's type, which is a
//! HARD error naming record index, field, and value, never a silent
//! skip).

use anyhow::{bail, Result};
use serde::Deserialize;
use serde_json::Value;

/// JSON Schema for a whole program — the constraint handed to the
/// model. Every op is a tagged union on `"op"`; adding an op means
/// extending BOTH this schema and [`Op`] (the deserializer rejects
/// anything the executor doesn't implement, so they cannot drift
/// apart silently).
pub const PROGRAM_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "ops": {
      "type": "array",
      "minItems": 1,
      "maxItems": 32,
      "items": {
        "oneOf": [
          {
            "type": "object",
            "properties": {
              "op": { "type": "string", "enum": ["select_fields"] },
              "fields": { "type": "array", "items": { "type": "string" }, "minItems": 1 }
            },
            "required": ["op", "fields"],
            "additionalProperties": false
          },
          {
            "type": "object",
            "properties": {
              "op": { "type": "string", "enum": ["drop_fields"] },
              "fields": { "type": "array", "items": { "type": "string" }, "minItems": 1 }
            },
            "required": ["op", "fields"],
            "additionalProperties": false
          },
          {
            "type": "object",
            "properties": {
              "op": { "type": "string", "enum": ["rename_field"] },
              "from": { "type": "string" },
              "to": { "type": "string" }
            },
            "required": ["op", "from", "to"],
            "additionalProperties": false
          },
          {
            "type": "object",
            "properties": {
              "op": { "type": "string", "enum": ["set_field"] },
              "field": { "type": "string" },
              "value": {}
            },
            "required": ["op", "field", "value"],
            "additionalProperties": false
          },
          {
            "type": "object",
            "properties": {
              "op": { "type": "string", "enum": ["filter"] },
              "field": { "type": "string" },
              "predicate": {
                "type": "string",
                "enum": ["eq", "ne", "gt", "lt", "ge", "le", "contains", "exists", "not_exists"]
              },
              "value": {}
            },
            "required": ["op", "field", "predicate"],
            "additionalProperties": false
          },
          {
            "type": "object",
            "properties": {
              "op": { "type": "string", "enum": ["map_field"] },
              "field": { "type": "string" },
              "transform": {
                "type": "string",
                "enum": ["lowercase", "uppercase", "trim", "to_string", "to_number"]
              }
            },
            "required": ["op", "field", "transform"],
            "additionalProperties": false
          },
          {
            "type": "object",
            "properties": {
              "op": { "type": "string", "enum": ["sort_by"] },
              "field": { "type": "string" },
              "order": { "type": "string", "enum": ["asc", "desc"] }
            },
            "required": ["op", "field", "order"],
            "additionalProperties": false
          },
          {
            "type": "object",
            "properties": {
              "op": { "type": "string", "enum": ["limit"] },
              "n": { "type": "integer", "minimum": 0 }
            },
            "required": ["op", "n"],
            "additionalProperties": false
          },
          {
            "type": "object",
            "properties": {
              "op": { "type": "string", "enum": ["reverse"] }
            },
            "required": ["op"],
            "additionalProperties": false
          }
        ]
      }
    }
  },
  "required": ["ops"],
  "additionalProperties": false
}"#;

/// Build the program schema with existing-field references constrained to the
/// input's actual field names. Ops that READ an existing field
/// (`select_fields`, `drop_fields`, `rename_field.from`, `filter`, `map_field`,
/// `sort_by`) get an `enum` of `field_names` on their field reference, so under
/// constrained decoding the model literally cannot name a field that isn't in
/// the data — a whole class of wrong programs (e.g. `map_field` on a field that
/// doesn't exist) becomes *undecodable* rather than merely rejected afterward.
///
/// `set_field.field` and `rename_field.to` stay OPEN on purpose: they
/// legitimately introduce a NEW field (that is how you'd add a column). With no
/// field names (empty or object-free input) the base schema is returned
/// unchanged — an empty `enum` would make every read op undecodable.
pub fn program_schema_with_fields(field_names: &[String]) -> Value {
    let mut schema: Value =
        serde_json::from_str(PROGRAM_SCHEMA).expect("PROGRAM_SCHEMA is valid JSON");
    if field_names.is_empty() {
        return schema;
    }
    let field_enum = Value::Array(field_names.iter().cloned().map(Value::String).collect());
    if let Some(branches) = schema["properties"]["ops"]["items"]["oneOf"].as_array_mut() {
        for branch in branches {
            let op = branch["properties"]["op"]["enum"][0].as_str().unwrap_or("");
            match op {
                "select_fields" | "drop_fields" => {
                    branch["properties"]["fields"]["items"]["enum"] = field_enum.clone();
                }
                "rename_field" => {
                    branch["properties"]["from"]["enum"] = field_enum.clone();
                }
                "filter" | "map_field" | "sort_by" => {
                    branch["properties"]["field"]["enum"] = field_enum.clone();
                }
                // set_field (may add a field), limit, reverse: no existing-field
                // reference to constrain.
                _ => {}
            }
        }
    }
    schema
}

/// The union of every object key across `records`, sorted — the set of fields a
/// read op may reference. Deterministic (BTreeSet) so the generated schema, and
/// thus constrained decoding, is reproducible.
pub fn input_field_names(records: &[Value]) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    for rec in records {
        if let Some(obj) = rec.as_object() {
            for k in obj.keys() {
                seen.insert(k.clone());
            }
        }
    }
    seen.into_iter().collect()
}

/// A transform program: an ordered list of operations.
#[derive(Debug, Clone, Deserialize)]
pub struct Program {
    pub ops: Vec<Op>,
}

/// One operation. Tagged on `op`, mirroring [`PROGRAM_SCHEMA`].
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    SelectFields { fields: Vec<String> },
    DropFields { fields: Vec<String> },
    RenameField { from: String, to: String },
    SetField { field: String, value: Value },
    Filter {
        field: String,
        predicate: Predicate,
        #[serde(default)]
        value: Option<Value>,
    },
    MapField { field: String, transform: Transform },
    SortBy { field: String, order: SortOrder },
    Limit { n: u64 },
    Reverse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Predicate {
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
    Contains,
    Exists,
    NotExists,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transform {
    Lowercase,
    Uppercase,
    Trim,
    ToString,
    ToNumber,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    Asc,
    Desc,
}

/// Parse a program from the model's constrained output. The schema
/// constraint makes malformed programs impossible in production; the
/// parse is still strict so a constraint regression fails HERE with a
/// serde error naming the offending op.
pub fn parse_program(json: &str) -> Result<Program> {
    let program: Program = serde_json::from_str(json)
        .map_err(|e| anyhow::anyhow!("transform program failed to parse: {}", e))?;
    if program.ops.is_empty() {
        bail!("transform program has no operations");
    }
    Ok(program)
}

/// Execute a program against a JSON array of objects. Returns the
/// transformed array.
pub fn apply_program(program: &Program, data: &Value) -> Result<Value> {
    let records = data
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("edit input must be a JSON array of objects"))?;
    let mut rows: Vec<serde_json::Map<String, Value>> = Vec::with_capacity(records.len());
    for (i, rec) in records.iter().enumerate() {
        match rec.as_object() {
            Some(obj) => rows.push(obj.clone()),
            None => bail!(
                "edit input record {} is not a JSON object (got {})",
                i,
                type_name(rec)
            ),
        }
    }

    for op in &program.ops {
        rows = apply_op(op, rows)?;
    }

    Ok(Value::Array(rows.into_iter().map(Value::Object).collect()))
}

fn apply_op(
    op: &Op,
    mut rows: Vec<serde_json::Map<String, Value>>,
) -> Result<Vec<serde_json::Map<String, Value>>> {
    match op {
        Op::SelectFields { fields } => {
            for row in &mut rows {
                let kept: serde_json::Map<String, Value> = fields
                    .iter()
                    .filter_map(|f| row.get(f).map(|v| (f.clone(), v.clone())))
                    .collect();
                *row = kept;
            }
            Ok(rows)
        }
        Op::DropFields { fields } => {
            for row in &mut rows {
                for f in fields {
                    row.remove(f);
                }
            }
            Ok(rows)
        }
        Op::RenameField { from, to } => {
            if from == to {
                bail!("rename_field: 'from' and 'to' are both '{}'", from);
            }
            for row in &mut rows {
                if let Some(v) = row.remove(from) {
                    row.insert(to.clone(), v);
                }
            }
            Ok(rows)
        }
        Op::SetField { field, value } => {
            for row in &mut rows {
                row.insert(field.clone(), value.clone());
            }
            Ok(rows)
        }
        Op::Filter {
            field,
            predicate,
            value,
        } => {
            // Comparison predicates need a comparand; existence checks
            // must not carry one. Enforced here (the schema cannot
            // express the cross-field rule).
            let needs_value = !matches!(predicate, Predicate::Exists | Predicate::NotExists);
            match (needs_value, value) {
                (true, None) => bail!(
                    "filter on '{}': predicate {:?} requires a comparison value",
                    field,
                    predicate
                ),
                (false, Some(_)) => bail!(
                    "filter on '{}': predicate {:?} takes no comparison value",
                    field,
                    predicate
                ),
                _ => {}
            }
            let mut kept = Vec::with_capacity(rows.len());
            for row in rows {
                let field_value = row.get(field);
                let keep = match predicate {
                    Predicate::Exists => field_value.is_some(),
                    Predicate::NotExists => field_value.is_none(),
                    _ => {
                        let comparand = value.as_ref().expect("checked above");
                        match field_value {
                            // Absent fields never satisfy a comparison —
                            // that is a fact about the record, not an error.
                            None => false,
                            Some(v) => compare(v, comparand, *predicate)?,
                        }
                    }
                };
                if keep {
                    kept.push(row);
                }
            }
            Ok(kept)
        }
        Op::MapField { field, transform } => {
            for (i, row) in rows.iter_mut().enumerate() {
                let Some(v) = row.get(field) else {
                    // Absent field: nothing to map on this record.
                    continue;
                };
                let mapped = apply_transform(v, *transform).map_err(|e| {
                    anyhow::anyhow!("map_field '{}' on record {}: {}", field, i, e)
                })?;
                row.insert(field.clone(), mapped);
            }
            Ok(rows)
        }
        Op::SortBy { field, order } => {
            // Total, deterministic order across mixed types: sort by
            // type class first (null < bool < number < string <
            // array < object), then by value within the class.
            rows.sort_by(|a, b| {
                let cmp = value_cmp(a.get(field), b.get(field));
                match order {
                    SortOrder::Asc => cmp,
                    SortOrder::Desc => cmp.reverse(),
                }
            });
            Ok(rows)
        }
        Op::Limit { n } => {
            rows.truncate(*n as usize);
            Ok(rows)
        }
        Op::Reverse => {
            rows.reverse();
            Ok(rows)
        }
    }
}

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn compare(v: &Value, comparand: &Value, predicate: Predicate) -> Result<bool> {
    match predicate {
        Predicate::Eq => Ok(v == comparand),
        Predicate::Ne => Ok(v != comparand),
        Predicate::Contains => match (v, comparand) {
            (Value::String(s), Value::String(needle)) => Ok(s.contains(needle.as_str())),
            (Value::Array(items), needle) => Ok(items.contains(needle)),
            _ => bail!(
                "contains requires a string field with a string value, or an array field \
                 (got field {} vs value {})",
                type_name(v),
                type_name(comparand)
            ),
        },
        Predicate::Gt | Predicate::Lt | Predicate::Ge | Predicate::Le => {
            let ord = match (v, comparand) {
                (Value::Number(a), Value::Number(b)) => {
                    let (a, b) = (
                        a.as_f64().ok_or_else(|| anyhow::anyhow!("non-finite number"))?,
                        b.as_f64().ok_or_else(|| anyhow::anyhow!("non-finite number"))?,
                    );
                    a.partial_cmp(&b)
                        .ok_or_else(|| anyhow::anyhow!("numbers are not comparable"))?
                }
                (Value::String(a), Value::String(b)) => a.cmp(b),
                _ => bail!(
                    "ordering comparison requires two numbers or two strings \
                     (got field {} vs value {})",
                    type_name(v),
                    type_name(comparand)
                ),
            };
            Ok(match predicate {
                Predicate::Gt => ord.is_gt(),
                Predicate::Lt => ord.is_lt(),
                Predicate::Ge => ord.is_ge(),
                Predicate::Le => ord.is_le(),
                _ => unreachable!(),
            })
        }
        Predicate::Exists | Predicate::NotExists => unreachable!("handled by caller"),
    }
}

fn apply_transform(v: &Value, transform: Transform) -> Result<Value> {
    match transform {
        Transform::Lowercase => match v {
            Value::String(s) => Ok(Value::String(s.to_lowercase())),
            other => bail!("lowercase requires a string, got {}", type_name(other)),
        },
        Transform::Uppercase => match v {
            Value::String(s) => Ok(Value::String(s.to_uppercase())),
            other => bail!("uppercase requires a string, got {}", type_name(other)),
        },
        Transform::Trim => match v {
            Value::String(s) => Ok(Value::String(s.trim().to_string())),
            other => bail!("trim requires a string, got {}", type_name(other)),
        },
        Transform::ToString => Ok(match v {
            Value::String(_) => v.clone(),
            other => Value::String(match other {
                Value::Null => "null".to_string(),
                Value::Bool(b) => b.to_string(),
                Value::Number(n) => n.to_string(),
                _ => serde_json::to_string(other).expect("JSON value serializes"),
            }),
        }),
        Transform::ToNumber => match v {
            Value::Number(_) => Ok(v.clone()),
            Value::String(s) => {
                let f: f64 = s.trim().parse().map_err(|_| {
                    anyhow::anyhow!("to_number: '{}' is not a number", s)
                })?;
                serde_json::Number::from_f64(f)
                    .map(Value::Number)
                    .ok_or_else(|| anyhow::anyhow!("to_number: '{}' is not finite", s))
            }
            other => bail!("to_number requires a string or number, got {}", type_name(other)),
        },
    }
}

/// Total order over optional JSON values for sort_by: absent < null <
/// bool < number < string < array < object; within a class, natural
/// order (arrays/objects by their canonical JSON text — deterministic,
/// if arbitrary).
fn value_cmp(a: Option<&Value>, b: Option<&Value>) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    fn class(v: &Value) -> u8 {
        match v {
            Value::Null => 0,
            Value::Bool(_) => 1,
            Value::Number(_) => 2,
            Value::String(_) => 3,
            Value::Array(_) => 4,
            Value::Object(_) => 5,
        }
    }
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(a), Some(b)) => {
            let by_class = class(a).cmp(&class(b));
            if by_class != Ordering::Equal {
                return by_class;
            }
            match (a, b) {
                (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
                (Value::Number(x), Value::Number(y)) => x
                    .as_f64()
                    .partial_cmp(&y.as_f64())
                    .unwrap_or(Ordering::Equal),
                (Value::String(x), Value::String(y)) => x.cmp(y),
                _ => {
                    let xs = serde_json::to_string(a).expect("JSON value serializes");
                    let ys = serde_json::to_string(b).expect("JSON value serializes");
                    xs.cmp(&ys)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn run(program_json: &str, data: serde_json::Value) -> Result<Value> {
        let program = parse_program(program_json)?;
        apply_program(&program, &data)
    }

    // TEST0060: the schema constant itself is valid JSON and every op
    // kind it names deserializes — schema and executor cannot drift.
    #[test]
    fn test0060_program_schema_and_ops_in_lockstep() {
        let schema: Value = serde_json::from_str(PROGRAM_SCHEMA).expect("schema is valid JSON");
        let one_of = schema["properties"]["ops"]["items"]["oneOf"]
            .as_array()
            .expect("oneOf branch list");
        // One example instance per schema branch must parse into an Op.
        let examples = [
            json!({"op": "select_fields", "fields": ["a"]}),
            json!({"op": "drop_fields", "fields": ["a"]}),
            json!({"op": "rename_field", "from": "a", "to": "b"}),
            json!({"op": "set_field", "field": "a", "value": 1}),
            json!({"op": "filter", "field": "a", "predicate": "eq", "value": 1}),
            json!({"op": "map_field", "field": "a", "transform": "trim"}),
            json!({"op": "sort_by", "field": "a", "order": "asc"}),
            json!({"op": "limit", "n": 3}),
            json!({"op": "reverse"}),
        ];
        assert_eq!(
            one_of.len(),
            examples.len(),
            "schema branches and executor op kinds must match 1:1"
        );
        for ex in &examples {
            let program = json!({"ops": [ex]});
            parse_program(&program.to_string())
                .unwrap_or_else(|e| panic!("op {} failed to parse: {}", ex, e));
        }
        // Unknown ops are rejected (a schema/executor drift would land here).
        assert!(parse_program(r#"{"ops": [{"op": "explode"}]}"#).is_err());
    }

    // TEST0065: the dynamic schema constrains every existing-field reference to
    // the input's field names, so a read op over a non-existent field is
    // undecodable; `set_field` stays open so a NEW field can still be added.
    #[test]
    fn test0065_schema_constrains_read_op_fields_to_input() {
        let schema = program_schema_with_fields(&["name".to_string(), "age".to_string()]);
        let branches = schema["properties"]["ops"]["items"]["oneOf"]
            .as_array()
            .expect("oneOf");
        let get = |op: &str| {
            branches
                .iter()
                .find(|b| b["properties"]["op"]["enum"][0] == op)
                .unwrap_or_else(|| panic!("branch {op}"))
        };
        let want = json!(["name", "age"]);
        assert_eq!(get("map_field")["properties"]["field"]["enum"], want);
        assert_eq!(get("filter")["properties"]["field"]["enum"], want);
        assert_eq!(get("sort_by")["properties"]["field"]["enum"], want);
        assert_eq!(get("select_fields")["properties"]["fields"]["items"]["enum"], want);
        assert_eq!(get("drop_fields")["properties"]["fields"]["items"]["enum"], want);
        assert_eq!(get("rename_field")["properties"]["from"]["enum"], want);
        // set_field.field and rename_field.to stay OPEN (they may add a new field).
        assert!(get("set_field")["properties"]["field"].get("enum").is_none());
        assert!(get("rename_field")["properties"]["to"].get("enum").is_none());
        // Empty field set -> base schema unchanged (no empty enum on any read op).
        let base = program_schema_with_fields(&[]);
        let mf = base["properties"]["ops"]["items"]["oneOf"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["properties"]["op"]["enum"][0] == "map_field")
            .unwrap();
        assert!(mf["properties"]["field"].get("enum").is_none());
        // The union of record keys, sorted, is what feeds the enum.
        let records = json!([{"b": 1, "a": 2}, {"a": 3, "c": 4}]);
        assert_eq!(
            input_field_names(records.as_array().unwrap()),
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
    }

    // TEST0061: an end-to-end program — the "describe it like a chatbot"
    // example made concrete: filter active users, lowercase emails,
    // keep two fields, sort by name, take the top 2.
    #[test]
    fn test0061_program_end_to_end() {
        let data = json!([
            {"name": "Carol", "email": "CAROL@X.COM", "active": true,  "age": 31},
            {"name": "alice", "email": "Alice@X.com", "active": true,  "age": 44},
            {"name": "Bob",   "email": "BOB@X.COM",   "active": false, "age": 22}
        ]);
        let program = r#"{"ops": [
            {"op": "filter", "field": "active", "predicate": "eq", "value": true},
            {"op": "map_field", "field": "email", "transform": "lowercase"},
            {"op": "select_fields", "fields": ["name", "email"]},
            {"op": "sort_by", "field": "name", "order": "asc"},
            {"op": "limit", "n": 2}
        ]}"#;
        let out = run(program, data).unwrap();
        assert_eq!(
            out,
            json!([
                {"name": "Carol", "email": "carol@x.com"},
                {"name": "alice", "email": "alice@x.com"}
            ])
        );
    }

    // TEST0062: contract violations are hard errors naming the exact
    // problem — never silent skips.
    #[test]
    fn test0062_hard_errors_name_the_problem() {
        // Non-array input.
        let err = run(r#"{"ops": [{"op": "reverse"}]}"#, json!({"a": 1})).unwrap_err();
        assert!(err.to_string().contains("array"), "got: {err}");
        // Non-object record, with its index.
        let err = run(r#"{"ops": [{"op": "reverse"}]}"#, json!([{"a": 1}, 42])).unwrap_err();
        assert!(err.to_string().contains("record 1"), "got: {err}");
        // Type-incompatible transform names record, field, and rule.
        let err = run(
            r#"{"ops": [{"op": "map_field", "field": "n", "transform": "lowercase"}]}"#,
            json!([{"n": 5}]),
        )
        .unwrap_err();
        assert!(err.to_string().contains("record 0"), "got: {err}");
        assert!(err.to_string().contains("lowercase"), "got: {err}");
        // Comparison without a comparand.
        let err = run(
            r#"{"ops": [{"op": "filter", "field": "a", "predicate": "gt"}]}"#,
            json!([{"a": 1}]),
        )
        .unwrap_err();
        assert!(err.to_string().contains("requires a comparison value"), "got: {err}");
        // Existence check WITH a comparand is equally malformed.
        let err = run(
            r#"{"ops": [{"op": "filter", "field": "a", "predicate": "exists", "value": 1}]}"#,
            json!([{"a": 1}]),
        )
        .unwrap_err();
        assert!(err.to_string().contains("takes no comparison value"), "got: {err}");
    }

    // TEST0063: filter predicate semantics — absent fields fail
    // comparisons but satisfy not_exists; ordering works on numbers
    // and strings; contains covers substrings and array membership.
    #[test]
    fn test0063_filter_semantics() {
        let data = json!([
            {"n": 1, "tags": ["a", "b"], "s": "hello world"},
            {"n": 5, "tags": ["c"],      "s": "goodbye"},
            {"tags": [],                 "s": "no n field"}
        ]);
        let keep = |prog: &str| -> usize {
            run(prog, data.clone()).unwrap().as_array().unwrap().len()
        };
        assert_eq!(keep(r#"{"ops":[{"op":"filter","field":"n","predicate":"gt","value":2}]}"#), 1);
        assert_eq!(keep(r#"{"ops":[{"op":"filter","field":"n","predicate":"le","value":5}]}"#), 2);
        assert_eq!(keep(r#"{"ops":[{"op":"filter","field":"n","predicate":"exists"}]}"#), 2);
        assert_eq!(keep(r#"{"ops":[{"op":"filter","field":"n","predicate":"not_exists"}]}"#), 1);
        assert_eq!(
            keep(r#"{"ops":[{"op":"filter","field":"s","predicate":"contains","value":"o w"}]}"#),
            1
        );
        assert_eq!(
            keep(r#"{"ops":[{"op":"filter","field":"tags","predicate":"contains","value":"c"}]}"#),
            1
        );
    }

    // TEST0064: sort is total and deterministic across mixed and absent
    // values (absent < null < bool < number < string).
    #[test]
    fn test0064_sort_total_order() {
        let data = json!([
            {"k": "zeta"},
            {"k": 10},
            {},
            {"k": null},
            {"k": true},
            {"k": 2}
        ]);
        let out = run(
            r#"{"ops": [{"op": "sort_by", "field": "k", "order": "asc"}]}"#,
            data,
        )
        .unwrap();
        let keys: Vec<Value> = out
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r.get("k").cloned().unwrap_or(json!("<absent>")))
            .collect();
        assert_eq!(
            keys,
            vec![
                json!("<absent>"),
                json!(null),
                json!(true),
                json!(2),
                json!(10),
                json!("zeta")
            ]
        );
    }
}
