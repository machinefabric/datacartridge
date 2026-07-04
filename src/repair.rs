//! The `repair` primitive: explicit forgiveness for broken structured
//! data (docs/semantic-primitives.md, law P9).
//!
//! The strict caps stay strict — when real-world input is broken
//! (single quotes, trailing commas, truncated documents, BOM-prefixed
//! ragged CSV), `repair` fixes it VISIBLY: every fix is recorded with
//! its byte position and logged by the op, so a pipeline that needs
//! leniency writes `repair | convert-…` and the leniency is on the
//! record.
//!
//! Contract: repairs are performed only where the intended structure
//! is unambiguous. Anything that would require *guessing at data*
//! (trailing junk after a complete JSON document, CSV rows with MORE
//! fields than the header) is a hard error naming the position — a
//! wrong guess silently propagated is worse than a loud stop.

use anyhow::{bail, Result};
use serde_json::Value;

/// One repair action, in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepairAction {
    /// Byte offset in the original input where the fix applied.
    pub position: usize,
    /// What was fixed, in one sentence.
    pub what: String,
}

// =============================================================================
// JSON repair
// =============================================================================

/// Tolerant JSON reader: parses `input` into a [`Value`], recording a
/// [`RepairAction`] for every deviation from strict JSON it fixed.
/// Valid JSON round-trips with zero actions.
pub fn repair_json(input: &str) -> Result<(Value, Vec<RepairAction>)> {
    let mut p = JsonRepairer {
        bytes: input.as_bytes(),
        pos: 0,
        repairs: Vec::new(),
    };
    if p.bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        p.pos = 3;
        p.note(0, "removed UTF-8 byte-order mark");
    }
    p.skip_ws_and_comments();
    if p.pos >= p.bytes.len() {
        bail!("repair-json: input is empty — nothing to repair");
    }
    let value = p.parse_value(0)?;
    p.skip_ws_and_comments();
    if p.pos < p.bytes.len() {
        bail!(
            "repair-json: trailing content at byte {} after a complete JSON document — \
             refusing to guess whether it is data (starts with {:?})",
            p.pos,
            preview(&p.bytes[p.pos..])
        );
    }
    Ok((value, p.repairs))
}

fn preview(bytes: &[u8]) -> String {
    String::from_utf8_lossy(&bytes[..bytes.len().min(20)]).into_owned()
}

struct JsonRepairer<'a> {
    bytes: &'a [u8],
    pos: usize,
    repairs: Vec<RepairAction>,
}

const MAX_DEPTH: usize = 128;

impl<'a> JsonRepairer<'a> {
    fn note(&mut self, position: usize, what: &str) {
        self.repairs.push(RepairAction {
            position,
            what: what.to_string(),
        });
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn skip_ws_and_comments(&mut self) {
        loop {
            while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
                self.pos += 1;
            }
            match (self.peek(), self.bytes.get(self.pos + 1).copied()) {
                (Some(b'/'), Some(b'/')) => {
                    let start = self.pos;
                    while !matches!(self.peek(), None | Some(b'\n')) {
                        self.pos += 1;
                    }
                    self.note(start, "removed // comment (not JSON)");
                }
                (Some(b'/'), Some(b'*')) => {
                    let start = self.pos;
                    self.pos += 2;
                    while self.pos + 1 < self.bytes.len()
                        && !(self.bytes[self.pos] == b'*' && self.bytes[self.pos + 1] == b'/')
                    {
                        self.pos += 1;
                    }
                    self.pos = (self.pos + 2).min(self.bytes.len());
                    self.note(start, "removed /* */ comment (not JSON)");
                }
                _ => return,
            }
        }
    }

    fn parse_value(&mut self, depth: usize) -> Result<Value> {
        if depth > MAX_DEPTH {
            bail!("repair-json: nesting deeper than {} levels", MAX_DEPTH);
        }
        self.skip_ws_and_comments();
        match self.peek() {
            None => bail!("repair-json: unexpected end of input while expecting a value"),
            Some(b'{') => self.parse_object(depth),
            Some(b'[') => self.parse_array(depth),
            Some(b'"') | Some(b'\'') => self.parse_string().map(Value::String),
            Some(c) if c == b'-' || c == b'+' || c.is_ascii_digit() => self.parse_number(),
            Some(_) => self.parse_word(),
        }
    }

    fn parse_object(&mut self, depth: usize) -> Result<Value> {
        debug_assert_eq!(self.peek(), Some(b'{'));
        self.pos += 1;
        let mut map = serde_json::Map::new();
        loop {
            self.skip_ws_and_comments();
            match self.peek() {
                None => {
                    self.note(self.pos, "closed unterminated object at end of input");
                    return Ok(Value::Object(map));
                }
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Value::Object(map));
                }
                Some(b',') => {
                    // Stray comma (leading, doubled, or trailing).
                    let at = self.pos;
                    self.pos += 1;
                    self.skip_ws_and_comments();
                    if matches!(self.peek(), Some(b'}') | None) {
                        self.note(at, "removed trailing comma in object");
                    } else if map.is_empty() {
                        self.note(at, "removed leading comma in object");
                    } else {
                        // Separator between pairs: normal, no repair —
                        // unless the NEXT char is another comma, which
                        // the loop handles on the next pass.
                    }
                    continue;
                }
                _ => {}
            }

            // Key.
            let key_at = self.pos;
            let key = match self.peek() {
                Some(b'"') | Some(b'\'') => self.parse_string()?,
                Some(c) if is_ident_byte(c) => {
                    let word = self.take_ident();
                    self.note(key_at, "quoted an unquoted object key");
                    word
                }
                Some(c) => bail!(
                    "repair-json: expected an object key at byte {} but found {:?}",
                    key_at,
                    c as char
                ),
                None => unreachable!("handled above"),
            };

            // Colon.
            self.skip_ws_and_comments();
            match self.peek() {
                Some(b':') => {
                    self.pos += 1;
                }
                Some(b'=') => {
                    self.note(self.pos, "replaced '=' with ':' after object key");
                    self.pos += 1;
                }
                _ => {
                    self.note(self.pos, "inserted missing ':' after object key");
                }
            }

            let value = self.parse_value(depth + 1)?;
            if map.insert(key.clone(), value).is_some() {
                self.note(key_at, "later duplicate object key overwrote the earlier value");
            }

            // Separator: comma, or the next pair/end directly (missing
            // comma is repaired).
            self.skip_ws_and_comments();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b'}') | None => {}
                Some(_) => {
                    self.note(self.pos, "inserted missing ',' between object entries");
                }
            }
        }
    }

    fn parse_array(&mut self, depth: usize) -> Result<Value> {
        debug_assert_eq!(self.peek(), Some(b'['));
        self.pos += 1;
        let mut items = Vec::new();
        loop {
            self.skip_ws_and_comments();
            match self.peek() {
                None => {
                    self.note(self.pos, "closed unterminated array at end of input");
                    return Ok(Value::Array(items));
                }
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Value::Array(items));
                }
                Some(b',') => {
                    let at = self.pos;
                    self.pos += 1;
                    self.skip_ws_and_comments();
                    if matches!(self.peek(), Some(b']') | None) {
                        self.note(at, "removed trailing comma in array");
                    } else if items.is_empty() {
                        self.note(at, "removed leading comma in array");
                    }
                    continue;
                }
                _ => {}
            }

            items.push(self.parse_value(depth + 1)?);

            self.skip_ws_and_comments();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b']') | None => {}
                Some(_) => {
                    self.note(self.pos, "inserted missing ',' between array items");
                }
            }
        }
    }

    fn parse_string(&mut self) -> Result<String> {
        let quote = self.peek().expect("caller checked");
        let start = self.pos;
        if quote == b'\'' {
            self.note(start, "converted single-quoted string to double-quoted");
        }
        self.pos += 1;
        let mut out = String::new();
        loop {
            match self.peek() {
                None => {
                    self.note(self.pos, "closed unterminated string at end of input");
                    return Ok(out);
                }
                Some(b'\n') => {
                    self.note(self.pos, "closed unterminated string at end of line");
                    return Ok(out);
                }
                Some(c) if c == quote => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.pos += 1;
                    match self.peek() {
                        None => {
                            self.note(self.pos, "dropped dangling backslash at end of input");
                            return Ok(out);
                        }
                        Some(esc) => {
                            self.pos += 1;
                            match esc {
                                b'"' => out.push('"'),
                                b'\'' => out.push('\''),
                                b'\\' => out.push('\\'),
                                b'/' => out.push('/'),
                                b'b' => out.push('\u{0008}'),
                                b'f' => out.push('\u{000C}'),
                                b'n' => out.push('\n'),
                                b'r' => out.push('\r'),
                                b't' => out.push('\t'),
                                b'u' => {
                                    let hex_start = self.pos;
                                    let hex: String = self.bytes
                                        [self.pos..self.bytes.len().min(self.pos + 4)]
                                        .iter()
                                        .map(|&b| b as char)
                                        .collect();
                                    match (hex.len() == 4)
                                        .then(|| u32::from_str_radix(&hex, 16).ok())
                                        .flatten()
                                        .and_then(char::from_u32)
                                    {
                                        Some(ch) => {
                                            out.push(ch);
                                            self.pos += 4;
                                        }
                                        None => bail!(
                                            "repair-json: invalid \\u escape at byte {}",
                                            hex_start
                                        ),
                                    }
                                }
                                other => {
                                    self.note(
                                        self.pos - 1,
                                        "kept character after an invalid escape sequence",
                                    );
                                    out.push(other as char);
                                }
                            }
                        }
                    }
                }
                Some(c) if c < 0x20 => {
                    // Raw control character inside a string: keep the
                    // character, note the fix (strict JSON requires
                    // escaping).
                    self.note(self.pos, "escaped raw control character inside string");
                    out.push(c as char);
                    self.pos += 1;
                }
                Some(c) if c < 0x80 => {
                    out.push(c as char);
                    self.pos += 1;
                }
                Some(_) => {
                    // Multi-byte UTF-8: copy the whole sequence.
                    let rest = &self.bytes[self.pos..];
                    let s = std::str::from_utf8(rest).map_err(|_| {
                        anyhow::anyhow!(
                            "repair-json: invalid UTF-8 inside string at byte {}",
                            self.pos
                        )
                    })?;
                    let ch = s.chars().next().expect("non-empty");
                    out.push(ch);
                    self.pos += ch.len_utf8();
                }
            }
        }
    }

    fn parse_number(&mut self) -> Result<Value> {
        let start = self.pos;
        if self.peek() == Some(b'+') {
            self.note(start, "removed leading '+' from number");
            self.pos += 1;
        }
        let num_start = self.pos;
        while matches!(
            self.peek(),
            Some(b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E')
        ) {
            self.pos += 1;
        }
        let text = std::str::from_utf8(&self.bytes[num_start..self.pos])
            .expect("ASCII number bytes");
        // `-Infinity` arrives here via the leading '-'.
        if text == "-" && self.bytes[self.pos..].starts_with(b"Infinity") {
            self.pos += "Infinity".len();
            self.note(start, "replaced -Infinity with null (not representable in JSON)");
            return Ok(Value::Null);
        }
        match serde_json::from_str::<serde_json::Number>(text) {
            Ok(n) => Ok(Value::Number(n)),
            Err(_) => {
                // Common shapes strict JSON rejects but whose intent is
                // unambiguous: leading '.', trailing '.', leading zeros.
                let f: f64 = text.parse().map_err(|_| {
                    anyhow::anyhow!(
                        "repair-json: '{}' at byte {} is not a number",
                        text,
                        num_start
                    )
                })?;
                self.note(num_start, "normalized non-standard number literal");
                serde_json::Number::from_f64(f)
                    .map(Value::Number)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "repair-json: '{}' at byte {} is not finite",
                            text,
                            num_start
                        )
                    })
            }
        }
    }

    fn take_ident(&mut self) -> String {
        let start = self.pos;
        while matches!(self.peek(), Some(c) if is_ident_byte(c)) {
            self.pos += 1;
        }
        String::from_utf8_lossy(&self.bytes[start..self.pos]).into_owned()
    }

    fn parse_word(&mut self) -> Result<Value> {
        let start = self.pos;
        let word = self.take_ident();
        match word.as_str() {
            "true" | "false" => Ok(Value::Bool(word == "true")),
            "null" => Ok(Value::Null),
            "True" | "False" => {
                self.note(start, "lowercased Python-style boolean literal");
                Ok(Value::Bool(word == "True"))
            }
            "None" | "undefined" => {
                self.note(start, "replaced non-JSON null literal with null");
                Ok(Value::Null)
            }
            "NaN" | "Infinity" => {
                self.note(start, "replaced non-finite number literal with null");
                Ok(Value::Null)
            }
            "" => bail!(
                "repair-json: unexpected character {:?} at byte {}",
                self.peek().map(|c| c as char),
                self.pos
            ),
            other => bail!(
                "repair-json: unknown literal '{}' at byte {} — refusing to guess what it means",
                other,
                start
            ),
        }
    }
}

fn is_ident_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'_' | b'$' | b'-' | b'.')
}

// =============================================================================
// CSV repair
// =============================================================================

/// Repair a CSV document: strip a UTF-8 BOM, and pad rows SHORTER than
/// the header out to the header width (each padding recorded). Rows
/// LONGER than the header are a hard error naming the row — extra
/// fields are data, and merging or dropping them would be a guess.
/// The output is re-serialized through a strict CSV writer, so
/// quoting is normalized as a side effect.
pub fn repair_csv(input: &[u8]) -> Result<(Vec<u8>, Vec<RepairAction>)> {
    let mut repairs = Vec::new();
    let body = if input.starts_with(&[0xEF, 0xBB, 0xBF]) {
        repairs.push(RepairAction {
            position: 0,
            what: "removed UTF-8 byte-order mark".to_string(),
        });
        &input[3..]
    } else {
        input
    };

    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(body);
    let headers = reader
        .headers()
        .map_err(|e| anyhow::anyhow!("repair-csv: header row is unreadable: {}", e))?
        .clone();
    let width = headers.len();
    if width == 0 {
        bail!("repair-csv: input has no header row");
    }

    let mut writer = csv::Writer::from_writer(Vec::new());
    writer
        .write_record(&headers)
        .map_err(|e| anyhow::anyhow!("repair-csv: failed to write header: {}", e))?;

    for (row_index, record) in reader.records().enumerate() {
        let record = record.map_err(|e| {
            anyhow::anyhow!(
                "repair-csv: row {} is unreadable ({}) — this shape cannot be repaired \
                 without guessing",
                row_index + 2,
                e
            )
        })?;
        let position = record.position().map(|p| p.byte() as usize).unwrap_or(0);
        match record.len().cmp(&width) {
            std::cmp::Ordering::Equal => {
                writer
                    .write_record(&record)
                    .map_err(|e| anyhow::anyhow!("repair-csv: write failed: {}", e))?;
            }
            std::cmp::Ordering::Less => {
                let mut padded: Vec<&str> = record.iter().collect();
                let missing = width - record.len();
                padded.resize(width, "");
                repairs.push(RepairAction {
                    position,
                    what: format!(
                        "padded row {} from {} to {} fields ({} empty field{} appended)",
                        row_index + 2,
                        record.len(),
                        width,
                        missing,
                        if missing == 1 { "" } else { "s" }
                    ),
                });
                writer
                    .write_record(&padded)
                    .map_err(|e| anyhow::anyhow!("repair-csv: write failed: {}", e))?;
            }
            std::cmp::Ordering::Greater => {
                bail!(
                    "repair-csv: row {} has {} fields but the header has {} — extra fields \
                     are data; merging or dropping them would be a guess. Fix the source \
                     (usually an unquoted delimiter)",
                    row_index + 2,
                    record.len(),
                    width
                );
            }
        }
    }

    let out = writer
        .into_inner()
        .map_err(|e| anyhow::anyhow!("repair-csv: writer flush failed: {}", e))?;
    Ok((out, repairs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // TEST0070: strict JSON round-trips untouched — repair is idempotent
    // on valid input (zero actions, identical value).
    #[test]
    fn test0070_valid_json_zero_repairs() {
        for text in [
            r#"{"a": 1, "b": [true, null, "x"], "c": {"d": 2.5}}"#,
            r#"[1, 2, 3]"#,
            r#""just a string""#,
            "42",
        ] {
            let (value, repairs) = repair_json(text).unwrap();
            assert_eq!(value, serde_json::from_str::<Value>(text).unwrap());
            assert!(repairs.is_empty(), "{text} needed no repairs, got {repairs:?}");
        }
    }

    // TEST0071: the common breakages LLM/hand-written JSON exhibits,
    // each repaired AND recorded.
    #[test]
    fn test0071_common_breakages_repaired_and_recorded() {
        let cases: &[(&str, Value, usize)] = &[
            // (input, expected value, expected number of repair actions)
            (r#"{'a': 'x'}"#, json!({"a": "x"}), 2),
            (r#"{a: 1}"#, json!({"a": 1}), 1),
            (r#"{"a": 1,}"#, json!({"a": 1}), 1),
            (r#"[1, 2,]"#, json!([1, 2]), 1),
            (r#"{"a": True, "b": None}"#, json!({"a": true, "b": null}), 2),
            (r#"{"a": NaN}"#, json!({"a": null}), 1),
            (r#"// note
{"a": 1}"#, json!({"a": 1}), 1),
            (r#"{"a" 1}"#, json!({"a": 1}), 1),          // missing colon
            (r#"{"a": 1 "b": 2}"#, json!({"a": 1, "b": 2}), 1), // missing comma
            (r#"[1 2]"#, json!([1, 2]), 1),
            (r#"{"a": +5}"#, json!({"a": 5}), 1),
        ];
        for (input, expected, n) in cases {
            let (value, repairs) = repair_json(input).unwrap();
            assert_eq!(&value, expected, "input: {input}");
            assert_eq!(
                repairs.len(),
                *n,
                "input: {input} — repairs: {repairs:?}"
            );
        }
    }

    // TEST0072: truncated documents close cleanly with the truncation
    // recorded — the exact shape a cut-off model response produces.
    #[test]
    fn test0072_truncated_documents_close() {
        let (value, repairs) = repair_json(r#"{"a": [1, 2, {"b": "unfinished"#).unwrap();
        assert_eq!(value, json!({"a": [1, 2, {"b": "unfinished"}]}));
        // one closed string + three closed containers
        assert_eq!(repairs.len(), 4, "repairs: {repairs:?}");
    }

    // TEST0073: what cannot be repaired without guessing is a HARD
    // error naming the position — never a silent guess.
    #[test]
    fn test0073_unguessable_is_a_hard_error() {
        // Trailing content after a complete document.
        let err = repair_json(r#"{"a": 1} {"b": 2}"#).unwrap_err();
        assert!(err.to_string().contains("trailing content"), "got: {err}");
        // An unknown bare word.
        let err = repair_json(r#"{"a": banana}"#).unwrap_err();
        assert!(err.to_string().contains("banana"), "got: {err}");
        // Empty input.
        assert!(repair_json("   ").is_err());
    }

    // TEST0074: CSV — BOM stripped, short rows padded (recorded), long
    // rows fatal with the row number.
    #[test]
    fn test0074_csv_repair() {
        let mut input = vec![0xEF, 0xBB, 0xBF];
        input.extend_from_slice(b"name,age,city\nalice,31,rome\nbob,22\n");
        let (out, repairs) = repair_csv(&input).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with("name,age,city\n"), "BOM must be gone: {text}");
        assert!(text.contains("bob,22,\n"), "short row padded: {text}");
        assert_eq!(repairs.len(), 2, "BOM + one padding: {repairs:?}");
        assert!(repairs[1].what.contains("row 3"), "{repairs:?}");

        // A row with MORE fields than the header cannot be repaired.
        let err = repair_csv(b"a,b\n1,2,3\n").unwrap_err();
        assert!(err.to_string().contains("row 2"), "got: {err}");
        assert!(err.to_string().contains("guess"), "got: {err}");
    }
}
