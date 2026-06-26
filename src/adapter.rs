//! Content inspection adapter for structured data formats.
//!
//! Inspects file bytes to identify which data media URNs match the content.
//! Handles JSON, NDJSON, CSV, TSV, PSV, YAML, XML, and TOML.

/// Inspect file bytes and return the media URNs for data formats.
///
/// Returns a list of media URN strings, most specific first.
/// Returns empty vec if content is not valid UTF-8 or doesn't match any data format.
pub fn detect_data_media_urns(content: &[u8], extension: &str) -> Vec<String> {
    let text = match std::str::from_utf8(content) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    match extension {
        "json" => detect_json(text),
        "ndjson" | "jsonl" => detect_ndjson(text),
        "csv" => detect_csv(text),
        "tsv" => detect_tsv(text),
        "psv" => detect_psv(text),
        "yaml" | "yml" => detect_yaml(text),
        "xml" => detect_xml(text),
        "toml" => detect_toml(text),
        _ => Vec::new(),
    }
}

// Each detector emits FILE-shape media URNs — `ext=<fmt>;fmt=<fmt>;<markers>` —
// because this adapter discriminates the type of a file READ from disk: it
// carries both the on-disk file-type (`ext=`) and the content serialization
// format (`fmt=`). The catalog's `_<fmt>-file.toml` anchors publish exactly
// these shapes. Pure inter-cap content (no file) uses `fmt=` alone, but that is
// produced by the converter caps, not by this file adapter.

fn detect_json(text: &str) -> Vec<String> {
    let trimmed = text.trim_start();
    if trimmed.is_empty() {
        return vec!["media:ext=json;fmt=json".to_string()];
    }

    match trimmed.chars().next() {
        Some('{') => vec![
            "media:ext=json;fmt=json;record".to_string(),
            "media:ext=json;fmt=json".to_string(),
        ],
        Some('[') => {
            let after_bracket = trimmed[1..].trim_start();
            if after_bracket.is_empty() || after_bracket.starts_with(']') {
                vec![
                    "media:ext=json;fmt=json;list".to_string(),
                    "media:ext=json;fmt=json".to_string(),
                ]
            } else if after_bracket.starts_with('{') {
                vec![
                    "media:ext=json;fmt=json;list;record".to_string(),
                    "media:ext=json;fmt=json;list".to_string(),
                    "media:ext=json;fmt=json".to_string(),
                ]
            } else {
                vec![
                    "media:ext=json;fmt=json;list".to_string(),
                    "media:ext=json;fmt=json".to_string(),
                ]
            }
        }
        _ => vec!["media:ext=json;fmt=json".to_string()],
    }
}

fn detect_ndjson(text: &str) -> Vec<String> {
    let has_object = text
        .lines()
        .take(10)
        .any(|line| line.trim().starts_with('{'));

    if has_object {
        vec![
            "media:ext=ndjson;fmt=ndjson;list;record".to_string(),
            "media:ext=ndjson;fmt=ndjson;list".to_string(),
            "media:ext=ndjson;fmt=ndjson".to_string(),
        ]
    } else {
        vec![
            "media:ext=ndjson;fmt=ndjson;list".to_string(),
            "media:ext=ndjson;fmt=ndjson".to_string(),
        ]
    }
}

fn detect_csv(_text: &str) -> Vec<String> {
    // CSV is always list-of-records by the catalog's `_csv-file.toml` anchor —
    // list-marker and record-marker are both required. A `.csv` file read from
    // disk carries both the `ext=csv` file-type and the `fmt=csv` content tag.
    vec!["media:ext=csv;fmt=csv;list;record".to_string()]
}

fn detect_tsv(_text: &str) -> Vec<String> {
    // TSV mirrors CSV: list of records. See `fabric/media/_tsv-file.toml`.
    vec!["media:ext=tsv;fmt=tsv;list;record".to_string()]
}

fn detect_psv(_text: &str) -> Vec<String> {
    // PSV mirrors CSV: pipe-separated. See `fabric/media/_psv-file.toml`.
    vec!["media:ext=psv;fmt=psv;list;record".to_string()]
}

fn detect_yaml(text: &str) -> Vec<String> {
    let trimmed = text.trim_start();

    let doc_count = text.matches("\n---").count()
        + if trimmed.starts_with("---") { 1 } else { 0 };

    if doc_count > 1 {
        let first_doc = trimmed.split("\n---").next().unwrap_or("");
        let first_doc = first_doc
            .strip_prefix("---")
            .unwrap_or(first_doc)
            .trim_start();
        if looks_like_yaml_mapping(first_doc) {
            return vec![
                "media:ext=yaml;fmt=yaml;list;record".to_string(),
                "media:ext=yaml;fmt=yaml;list".to_string(),
                "media:ext=yaml;fmt=yaml".to_string(),
            ];
        } else {
            return vec![
                "media:ext=yaml;fmt=yaml;list".to_string(),
                "media:ext=yaml;fmt=yaml".to_string(),
            ];
        }
    }

    let doc = trimmed.strip_prefix("---").unwrap_or(trimmed).trim_start();

    if doc.is_empty() {
        return vec!["media:ext=yaml;fmt=yaml".to_string()];
    }

    if doc.starts_with('-') {
        let first_item = doc
            .lines()
            .find(|l| l.trim_start().starts_with('-'))
            .map(|l| l.trim_start().strip_prefix('-').unwrap_or("").trim_start())
            .unwrap_or("");

        let is_record = looks_like_yaml_mapping(first_item) || first_item.contains(':');
        if is_record {
            vec![
                "media:ext=yaml;fmt=yaml;list;record".to_string(),
                "media:ext=yaml;fmt=yaml;list".to_string(),
                "media:ext=yaml;fmt=yaml".to_string(),
            ]
        } else {
            vec![
                "media:ext=yaml;fmt=yaml;list".to_string(),
                "media:ext=yaml;fmt=yaml".to_string(),
            ]
        }
    } else if doc.starts_with('{') {
        vec![
            "media:ext=yaml;fmt=yaml;record".to_string(),
            "media:ext=yaml;fmt=yaml".to_string(),
        ]
    } else if doc.starts_with('[') {
        if doc.contains('{') {
            vec![
                "media:ext=yaml;fmt=yaml;list;record".to_string(),
                "media:ext=yaml;fmt=yaml;list".to_string(),
                "media:ext=yaml;fmt=yaml".to_string(),
            ]
        } else {
            vec![
                "media:ext=yaml;fmt=yaml;list".to_string(),
                "media:ext=yaml;fmt=yaml".to_string(),
            ]
        }
    } else if doc.contains(':') {
        vec![
            "media:ext=yaml;fmt=yaml;record".to_string(),
            "media:ext=yaml;fmt=yaml".to_string(),
        ]
    } else {
        vec!["media:ext=yaml;fmt=yaml".to_string()]
    }
}

fn looks_like_yaml_mapping(content: &str) -> bool {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(colon_pos) = trimmed.find(':') {
            let before_colon = &trimmed[..colon_pos];
            if !before_colon.is_empty() && !before_colon.contains(' ') {
                return true;
            }
        }
    }
    false
}

fn detect_xml(text: &str) -> Vec<String> {
    let body = if let Some(pos) = text.find("?>") {
        &text[pos + 2..]
    } else {
        text
    };

    let trimmed = body.trim();

    if let Some(start) = trimmed.find('<') {
        if let Some(end) = trimmed[start..].find(|c| c == '>' || c == ' ' || c == '/') {
            let tag_name = &trimmed[start + 1..start + end];
            let child_pattern = format!("<{}", tag_name.chars().take(1).collect::<String>());
            let child_count = trimmed.matches(&child_pattern).count();

            if child_count > 2 {
                return vec![
                    "media:ext=xml;fmt=xml;list;record".to_string(),
                    "media:ext=xml;fmt=xml".to_string(),
                ];
            }
        }
    }

    if trimmed.contains('=') || (trimmed.matches('<').count() > 2) {
        vec![
            "media:ext=xml;fmt=xml;record".to_string(),
            "media:ext=xml;fmt=xml".to_string(),
        ]
    } else {
        vec!["media:ext=xml;fmt=xml".to_string()]
    }
}

fn detect_toml(_text: &str) -> Vec<String> {
    // TOML has no `fmt=` serialization value of its own; a `.toml` file is a
    // UTF-8 text file, so the catalog publishes `media:ext=toml;enc=utf-8`
    // (the file-type dim narrows `toml` to a UTF-8 text file). Emit that single
    // file shape.
    vec!["media:enc=utf-8;ext=toml".to_string()]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// JSON object → most-specific is `media:ext=json;fmt=json;record`,
    /// followed by the bare `media:ext=json;fmt=json`. Both URNs must
    /// be present in the catalog for the input-resolver's
    /// conformance walk to succeed.
    #[test]
    fn test0001_json_object_emits_record_then_bare() {
        let urns = detect_data_media_urns(br#"{"key": "value"}"#, "json");
        assert_eq!(
            urns,
            vec![
                "media:ext=json;fmt=json;record".to_string(),
                "media:ext=json;fmt=json".to_string(),
            ]
        );
    }

    /// JSON array of objects → list+record narrowing first, then
    /// list, then bare. Three tiers of specificity, all in the
    /// catalog.
    #[test]
    fn test0002_json_array_of_objects_emits_full_tier() {
        let urns = detect_data_media_urns(br#"[{"a": 1}]"#, "json");
        assert_eq!(
            urns,
            vec![
                "media:ext=json;fmt=json;list;record".to_string(),
                "media:ext=json;fmt=json;list".to_string(),
                "media:ext=json;fmt=json".to_string(),
            ]
        );
    }

    /// JSON array of scalars → list narrowing without record, then
    /// bare. Only two tiers because record doesn't apply.
    #[test]
    fn test0003_json_array_of_scalars_has_no_record_narrowing() {
        let urns = detect_data_media_urns(b"[1, 2, 3]", "json");
        assert_eq!(
            urns,
            vec![
                "media:ext=json;fmt=json;list".to_string(),
                "media:ext=json;fmt=json".to_string(),
            ]
        );
        assert!(
            !urns.iter().any(|u| u.contains(";record")),
            "list-of-scalars JSON has no record marker — emitting one \
             would claim the items are record-shaped when they aren't"
        );
    }

    /// CSV is always list-of-records by the catalog anchor. There
    /// is exactly one canonical CSV URN; the previous adapter
    /// returned that URN duplicated (a copy-paste bug producing a
    /// 2-element list of identical strings).
    #[test]
    fn test0004_csv_is_single_canonical_urn() {
        let urns = detect_data_media_urns(b"a,b,c\n1,2,3", "csv");
        assert_eq!(urns, vec!["media:ext=csv;fmt=csv;list;record".to_string()]);
        assert_eq!(urns.len(), 1, "CSV emits a single URN, not duplicates");
    }

    /// Single-column CSV is still list-of-records (one record per
    /// row, one field per record). Same canonical URN as
    /// multi-column. The previous adapter branched on column count
    /// — that distinction had no semantic meaning at the URN level
    /// since the catalog only publishes one CSV URN.
    #[test]
    fn test0005_csv_single_column_same_urn_as_multi() {
        let single = detect_data_media_urns(b"a\n1\n2", "csv");
        let multi = detect_data_media_urns(b"a,b\n1,2\n3,4", "csv");
        assert_eq!(single, multi);
        assert_eq!(single, vec!["media:ext=csv;fmt=csv;list;record".to_string()]);
    }

    /// TSV emits the canonical catalog URN. The previous adapter
    /// returned `media:tsv;list;textable` for single-column TSV —
    /// a URN that did not exist in the published catalog because
    /// no `_tsv-data.toml` anchor was declared.
    #[test]
    fn test0006_tsv_emits_canonical_catalog_urn() {
        let urns = detect_data_media_urns(b"a\tb\n1\t2", "tsv");
        assert_eq!(urns, vec!["media:ext=tsv;fmt=tsv;list;record".to_string()]);
    }

    /// PSV emits the canonical catalog URN.
    #[test]
    fn test0007_psv_emits_canonical_catalog_urn() {
        let urns = detect_data_media_urns(b"a|b\n1|2", "psv");
        assert_eq!(urns, vec!["media:ext=psv;fmt=psv;list;record".to_string()]);
    }

    /// YAML mapping → record narrowing without list, then bare.
    /// The `looks_like_yaml_mapping` heuristic detects `key:
    /// value` lines.
    #[test]
    fn test0008_yaml_mapping_emits_record_then_bare() {
        let urns = detect_data_media_urns(b"key: value\nother: data", "yaml");
        assert_eq!(
            urns,
            vec![
                "media:ext=yaml;fmt=yaml;record".to_string(),
                "media:ext=yaml;fmt=yaml".to_string(),
            ]
        );
    }

    /// YAML sequence of mappings → list+record, list, bare.
    #[test]
    fn test0009_yaml_sequence_of_mappings_full_tier() {
        let urns = detect_data_media_urns(b"- name: a\n  v: 1\n- name: b\n  v: 2", "yaml");
        assert_eq!(
            urns,
            vec![
                "media:ext=yaml;fmt=yaml;list;record".to_string(),
                "media:ext=yaml;fmt=yaml;list".to_string(),
                "media:ext=yaml;fmt=yaml".to_string(),
            ]
        );
    }

    /// A `.toml` file is a UTF-8 text file (no `fmt=` of its own), so the
    /// catalog publishes `media:enc=utf-8;ext=toml`. A single emission,
    /// not the duplicated form the previous adapter returned (also
    /// a copy-paste bug).
    #[test]
    fn test0010_toml_is_single_canonical_urn() {
        let urns = detect_data_media_urns(b"key = \"value\"", "toml");
        assert_eq!(urns, vec!["media:enc=utf-8;ext=toml".to_string()]);
        assert_eq!(urns.len(), 1, "TOML emits a single URN, not duplicates");
    }

    /// Non-UTF-8 bytes given to a JSON-extension path return the
    /// empty vec. Text data formats require UTF-8; emitting any
    /// URN for binary content would lie about the bytes.
    #[test]
    fn test0011_binary_returns_empty() {
        let urns = detect_data_media_urns(&[0xFF, 0xFE, 0x00], "json");
        assert!(urns.is_empty());
    }

    /// An extension this adapter doesn't own returns the empty
    /// vec — datacartridge handles only the structured-data
    /// formats (JSON, CSV, etc.); a `.txt` file is not its
    /// concern. Emitting `media:enc=utf-8` here would step on
    /// txtcartridge's adapter and would also be wrong (no JSON
    /// detection has happened).
    #[test]
    fn test0012_unknown_extension_returns_empty() {
        let urns = detect_data_media_urns(b"some text", "xyz");
        assert!(urns.is_empty());
    }

    /// **Catalog-presence regression guard.** Every URN this
    /// adapter emits for any of its supported extensions must be
    /// a URN the published catalog actually contains. The earlier
    /// adapter emitted `media:tsv;list;textable`,
    /// `media:psv;list;textable`, and other URN forms that did
    /// not exist in the catalog because no anchor declared them.
    /// This test exhaustively enumerates the URN forms the adapter
    /// is allowed to return; cross-validation at `dx fabric update`
    /// time catches any of these strings drifting away from
    /// catalog truth.
    #[test]
    fn test0013_every_emitted_urn_is_in_catalog_form() {
        // Trigger every detect_* branch with carefully-chosen
        // inputs and union the URN strings. Any string emitted
        // outside this allow-list must be added to the dim
        // catalogue or removed from the adapter.
        let mut emitted = std::collections::BTreeSet::new();
        for (bytes, ext) in [
            (br#"{"a":1}"# as &[u8], "json"),
            (br#"[{"a":1}]"# as &[u8], "json"),
            (br#"[1,2,3]"# as &[u8], "json"),
            (br#"42"# as &[u8], "json"),
            (br#"{"a":1}"# as &[u8], "ndjson"),
            (br#"42"# as &[u8], "ndjson"),
            (b"a,b\n1,2" as &[u8], "csv"),
            (b"a\tb\n1\t2" as &[u8], "tsv"),
            (b"a|b\n1|2" as &[u8], "psv"),
            (b"key: v\nother: d" as &[u8], "yaml"),
            (b"- a\n- b" as &[u8], "yaml"),
            (b"- name: a\n- name: b" as &[u8], "yaml"),
            (b"---\nkey: v\n---\nkey: v2" as &[u8], "yaml"),
            (b"<root><a/><b/><c/></root>" as &[u8], "xml"),
            (b"<root attr=\"v\"/>" as &[u8], "xml"),
            (b"<x/>" as &[u8], "xml"),
            (b"key = \"v\"" as &[u8], "toml"),
        ] {
            for u in detect_data_media_urns(bytes, ext) {
                emitted.insert(u);
            }
        }

        let allowed: std::collections::BTreeSet<String> = [
            // JSON variants
            "media:ext=json;fmt=json",
            "media:ext=json;fmt=json;list",
            "media:ext=json;fmt=json;record",
            "media:ext=json;fmt=json;list;record",
            // NDJSON variants
            "media:ext=ndjson;fmt=ndjson",
            "media:ext=ndjson;fmt=ndjson;list",
            "media:ext=ndjson;fmt=ndjson;list;record",
            // CSV/TSV/PSV — single canonical each
            "media:ext=csv;fmt=csv;list;record",
            "media:ext=tsv;fmt=tsv;list;record",
            "media:ext=psv;fmt=psv;list;record",
            // YAML variants
            "media:ext=yaml;fmt=yaml",
            "media:ext=yaml;fmt=yaml;list",
            "media:ext=yaml;fmt=yaml;record",
            "media:ext=yaml;fmt=yaml;list;record",
            // XML variants
            "media:ext=xml;fmt=xml",
            "media:ext=xml;fmt=xml;record",
            "media:ext=xml;fmt=xml;list;record",
            // TOML
            "media:enc=utf-8;ext=toml",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let stray: Vec<&String> = emitted.difference(&allowed).collect();
        assert!(
            stray.is_empty(),
            "datacartridge adapter emitted URNs not in the catalog allow-list: \
             {:?}\n— either add them to the dim catalogue (and verify they \
             publish) or fix the adapter to emit only catalog-published forms",
            stray
        );
    }
}
