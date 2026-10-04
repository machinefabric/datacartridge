//! A JSON schema described for a model to read.
//!
//! Constrained generation enforces the schema token by token, so the prompt
//! does not have to; what the prompt has to do is tell the model what each
//! field is for. Pasted raw, a schema is mostly JSON-Schema vocabulary the
//! model then imitates — its strongest first key under constraint was
//! `"type"`. The outline names each field, its kind, the values it may take
//! and what it means, and nothing else.

use serde_json::Value;

/// The outline of `schema`, one line per field, nested by indentation. Fields
/// are listed in key order: this crate's `serde_json` keeps objects sorted, so
/// the order a schema declared is not known here — and the model may write the
/// keys in any order (the constraint does not fix one).
pub(crate) fn outline(schema: &Value) -> String {
    let mut lines = Vec::new();
    describe(schema, 0, &(String::new(), ""), &mut lines);
    lines.join("\n")
}

/// How a part is introduced: what comes before its phrase and after it. A
/// property's phrase sits inside parentheses after its key — `- "name"
/// (required, a string)` — never where its value would go: written
/// `"name": text`, the outline was copied, and an extraction answered
/// `{"name": "text"}`.
type Label = (String, &'static str);

/// Push `label` around what `schema` is, and under it whatever it is made of,
/// at `depth`.
fn describe(schema: &Value, depth: usize, label: &Label, lines: &mut Vec<String>) {
    let (summary, children) = summarize(schema);
    lines.push(format!("{}{}{}{}", "  ".repeat(depth), label.0, summary, label.1));
    for (child_label, child) in children {
        describe(child, depth + 1, &child_label, lines);
    }
}

/// What `schema` is, in a phrase, and the parts listed under it.
fn summarize(schema: &Value) -> (String, Vec<(Label, &Value)>) {
    let Some(map) = schema.as_object() else {
        // `true` (or anything that is not a schema object) allows any value.
        return ("any JSON value".to_string(), Vec::new());
    };
    let note = |phrase: String| match map.get("description").and_then(Value::as_str) {
        Some(description) if !description.trim().is_empty() => {
            format!("{phrase} — {}", description.trim())
        }
        _ => phrase,
    };

    if let Some(value) = map.get("const") {
        return (note(format!("exactly {value}")), Vec::new());
    }
    if let Some(values) = map.get("enum").and_then(Value::as_array) {
        let phrase = match values.as_slice() {
            [only] => format!("exactly {only}"),
            _ => format!(
                "one of {}",
                values.iter().map(Value::to_string).collect::<Vec<_>>().join(", ")
            ),
        };
        return (note(phrase), Vec::new());
    }
    for key in ["oneOf", "anyOf"] {
        if let Some(alternatives) = map.get(key).and_then(Value::as_array) {
            let children = alternatives.iter().map(|a| (("- ".to_string(), ""), a)).collect();
            return (note("one of:".to_string()), children);
        }
    }

    let types: Vec<&str> = match map.get("type") {
        Some(Value::String(t)) => vec![t.as_str()],
        Some(Value::Array(ts)) => ts.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    if types == ["object"] || (types.is_empty() && map.contains_key("properties")) {
        let required: Vec<&str> = map
            .get("required")
            .and_then(Value::as_array)
            .map(|r| r.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        let mut children: Vec<(Label, &Value)> = map
            .get("properties")
            .and_then(Value::as_object)
            .map(|properties| {
                properties
                    .iter()
                    .map(|(name, property)| {
                        let presence = if required.contains(&name.as_str()) {
                            "required"
                        } else {
                            "optional"
                        };
                        ((format!("- {} ({presence}, ", Value::String(name.clone())), ")"), property)
                    })
                    .collect()
            })
            .unwrap_or_default();
        match map.get("additionalProperties") {
            Some(Value::Bool(false)) => {}
            Some(other) if other.is_object() => {
                children.push((("- any other key (".to_string(), ")"), other));
            }
            _ if children.is_empty() => {
                return (note("an object with any keys".to_string()), Vec::new());
            }
            _ => {}
        }
        return (note("an object with:".to_string()), children);
    }
    if types == ["array"] {
        let count = match (
            map.get("minItems").and_then(Value::as_u64),
            map.get("maxItems").and_then(Value::as_u64),
        ) {
            (Some(min), Some(max)) if min == max => format!("{min} items"),
            (Some(min), Some(max)) => format!("{min} to {max} items"),
            (Some(min), None) => format!("at least {min} items"),
            (None, Some(max)) => format!("at most {max} items"),
            (None, None) => "items".to_string(),
        };
        let Some(items) = map.get("items") else {
            return (note(format!("a list of {count} of any kind")), Vec::new());
        };
        let (item_summary, item_children) = summarize(items);
        if item_children.is_empty() {
            return (note(format!("a list of {count}, each {item_summary}")), Vec::new());
        }
        return (note(format!("a list of {count}, each:")), vec![((String::new(), ""), items)]);
    }

    let mut phrase = if types.is_empty() {
        "any JSON value".to_string()
    } else {
        types.iter().map(|t| kind(t)).collect::<Vec<_>>().join(" or ")
    };
    let mut bounds = Vec::new();
    if let Some(min) = map.get("minimum") {
        bounds.push(format!("at least {min}"));
    }
    if let Some(max) = map.get("maximum") {
        bounds.push(format!("at most {max}"));
    }
    if let Some(max) = map.get("maxLength") {
        bounds.push(format!("at most {max} characters"));
    }
    if let Some(format) = map.get("format").and_then(Value::as_str) {
        bounds.push(format!("format {format}"));
    }
    if !bounds.is_empty() {
        phrase = format!("{phrase}, {}", bounds.join(", "));
    }
    (note(phrase), Vec::new())
}

fn kind(json_type: &str) -> &str {
    match json_type {
        "string" => "a string",
        "integer" => "an integer",
        "number" => "a number",
        "boolean" => "true or false",
        "null" => "null",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // TEST12474: an object schema reads as its fields — names, whether each
    // is required, kinds, allowed values, bounds and descriptions — with
    // none of the JSON-Schema vocabulary the model would copy.
    #[test]
    fn test12474_an_object_reads_as_its_fields() {
        let schema = json!({
            "type": "object",
            "properties": {
                "same": { "type": "boolean" },
                "confidence": { "type": "number", "minimum": 0, "maximum": 1 },
                "reason": { "type": "string", "maxLength": 300, "description": "the decisive evidence" },
                "tier": { "type": "string", "enum": ["low", "high"] }
            },
            "required": ["same", "confidence", "reason"],
            "additionalProperties": false
        });
        assert_eq!(
            outline(&schema),
            [
                "an object with:",
                "  - \"confidence\" (required, a number, at least 0, at most 1)",
                "  - \"reason\" (required, a string, at most 300 characters — the decisive evidence)",
                "  - \"same\" (required, true or false)",
                "  - \"tier\" (optional, one of \"low\", \"high\")",
            ]
            .join("\n")
        );
    }

    // TEST12475: alternatives and lists nest — a list of one-of objects is
    // how the transform program's operations read.
    #[test]
    fn test12475_alternatives_and_lists_nest() {
        let schema = json!({
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
                                    "op": { "type": "string", "enum": ["drop_fields"] },
                                    "fields": { "type": "array", "items": { "type": "string", "enum": ["a", "b"] }, "minItems": 1 }
                                },
                                "required": ["op", "fields"],
                                "additionalProperties": false
                            },
                            {
                                "type": "object",
                                "properties": {
                                    "op": { "type": "string", "enum": ["set_field"] },
                                    "value": {}
                                },
                                "required": ["op", "value"],
                                "additionalProperties": false
                            }
                        ]
                    }
                }
            },
            "required": ["ops"],
            "additionalProperties": false
        });
        assert_eq!(
            outline(&schema),
            [
                "an object with:",
                "  - \"ops\" (required, a list of 1 to 32 items, each:)",
                "    one of:",
                "      - an object with:",
                "        - \"fields\" (required, a list of at least 1 items, each one of \"a\", \"b\")",
                "        - \"op\" (required, exactly \"drop_fields\")",
                "      - an object with:",
                "        - \"op\" (required, exactly \"set_field\")",
                "        - \"value\" (required, any JSON value)",
            ]
            .join("\n")
        );
    }
}
