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

fn detect_json(text: &str) -> Vec<String> {
    let trimmed = text.trim_start();
    if trimmed.is_empty() {
        return vec!["media:json;textable".to_string()];
    }

    match trimmed.chars().next() {
        Some('{') => vec![
            "media:json;record;textable".to_string(),
            "media:json;textable".to_string(),
        ],
        Some('[') => {
            let after_bracket = trimmed[1..].trim_start();
            if after_bracket.is_empty() || after_bracket.starts_with(']') {
                vec![
                    "media:json;list;textable".to_string(),
                    "media:json;textable".to_string(),
                ]
            } else if after_bracket.starts_with('{') {
                vec![
                    "media:json;list;record;textable".to_string(),
                    "media:json;list;textable".to_string(),
                    "media:json;textable".to_string(),
                ]
            } else {
                vec![
                    "media:json;list;textable".to_string(),
                    "media:json;textable".to_string(),
                ]
            }
        }
        _ => vec!["media:json;textable".to_string()],
    }
}

fn detect_ndjson(text: &str) -> Vec<String> {
    let has_object = text
        .lines()
        .take(10)
        .any(|line| line.trim().starts_with('{'));

    // URN tag order is not semantically significant — the parser
    // canonicalises on parse — but this codebase consistently writes
    // tags in the order `<format>;<list>;<record>;<textable>` for
    // wire-shape clarity, and the catalog accepts that form.
    if has_object {
        vec![
            "media:ndjson;list;record;textable".to_string(),
            "media:ndjson;list;textable".to_string(),
            "media:ndjson;textable".to_string(),
        ]
    } else {
        vec![
            "media:ndjson;list;textable".to_string(),
            "media:ndjson;textable".to_string(),
        ]
    }
}

fn detect_csv(_text: &str) -> Vec<String> {
    // CSV is always list-of-records by the catalog's `_csv-data.toml`
    // anchor — list-marker and record-marker are both required, so the
    // single canonical URN is `media:csv;list;record;textable`. A
    // single-column CSV is still a list of one-field records, not a
    // different shape.
    vec!["media:ext=csv;list;record;textable".to_string()]
}

fn detect_tsv(_text: &str) -> Vec<String> {
    // TSV mirrors CSV: tab-separated values, list of records, single
    // canonical URN. See `fabric/media/_tsv-data.toml`.
    vec!["media:ext=tsv;list;record;textable".to_string()]
}

fn detect_psv(_text: &str) -> Vec<String> {
    // PSV mirrors CSV: pipe-separated values. See
    // `fabric/media/_psv-data.toml`.
    vec!["media:ext=psv;list;record;textable".to_string()]
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
                "media:list;record;textable;yaml".to_string(),
                "media:list;textable;yaml".to_string(),
                "media:textable;yaml".to_string(),
            ];
        } else {
            return vec![
                "media:list;textable;yaml".to_string(),
                "media:textable;yaml".to_string(),
            ];
        }
    }

    let doc = trimmed.strip_prefix("---").unwrap_or(trimmed).trim_start();

    if doc.is_empty() {
        return vec!["media:textable;yaml".to_string()];
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
                "media:list;record;textable;yaml".to_string(),
                "media:list;textable;yaml".to_string(),
                "media:textable;yaml".to_string(),
            ]
        } else {
            vec![
                "media:list;textable;yaml".to_string(),
                "media:textable;yaml".to_string(),
            ]
        }
    } else if doc.starts_with('{') {
        vec![
            "media:record;textable;yaml".to_string(),
            "media:textable;yaml".to_string(),
        ]
    } else if doc.starts_with('[') {
        if doc.contains('{') {
            vec![
                "media:list;record;textable;yaml".to_string(),
                "media:list;textable;yaml".to_string(),
                "media:textable;yaml".to_string(),
            ]
        } else {
            vec![
                "media:list;textable;yaml".to_string(),
                "media:textable;yaml".to_string(),
            ]
        }
    } else if doc.contains(':') {
        vec![
            "media:record;textable;yaml".to_string(),
            "media:textable;yaml".to_string(),
        ]
    } else {
        vec!["media:textable;yaml".to_string()]
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
                    "media:ext=xml;list;record;textable".to_string(),
                    "media:ext=xml;textable".to_string(),
                ];
            }
        }
    }

    if trimmed.contains('=') || (trimmed.matches('<').count() > 2) {
        vec![
            "media:ext=xml;record;textable".to_string(),
            "media:ext=xml;textable".to_string(),
        ]
    } else {
        vec!["media:ext=xml;textable".to_string()]
    }
}

fn detect_toml(_text: &str) -> Vec<String> {
    // TOML is a record-shaped configuration format. The catalog
    // publishes `media:textable;toml` (no list/record narrowing) as
    // the canonical form — adding a record narrowing would be honest,
    // but the existing anchor `_data-format-bare.toml` doesn't, so we
    // emit what's published. If a stricter narrowing is wanted later,
    // extend the TOML anchor and update this single emission point.
    vec!["media:ext=toml;textable".to_string()]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// JSON object → most-specific is `media:json;record;textable`,
    /// followed by the bare `media:json;textable`. Both URNs must
    /// be present in the catalog for the input-resolver's
    /// conformance walk to succeed.
    #[test]
    fn test0001_json_object_emits_record_then_bare() {
        let urns = detect_data_media_urns(br#"{"key": "value"}"#, "json");
        assert_eq!(
            urns,
            vec![
                "media:json;record;textable".to_string(),
                "media:json;textable".to_string(),
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
                "media:json;list;record;textable".to_string(),
                "media:json;list;textable".to_string(),
                "media:json;textable".to_string(),
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
                "media:json;list;textable".to_string(),
                "media:json;textable".to_string(),
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
        assert_eq!(urns, vec!["media:ext=csv;list;record;textable".to_string()]);
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
        assert_eq!(single, vec!["media:ext=csv;list;record;textable".to_string()]);
    }

    /// TSV emits the canonical catalog URN. The previous adapter
    /// returned `media:tsv;list;textable` for single-column TSV —
    /// a URN that did not exist in the published catalog because
    /// no `_tsv-data.toml` anchor was declared.
    #[test]
    fn test0006_tsv_emits_canonical_catalog_urn() {
        let urns = detect_data_media_urns(b"a\tb\n1\t2", "tsv");
        assert_eq!(urns, vec!["media:ext=tsv;list;record;textable".to_string()]);
    }

    /// PSV emits the canonical catalog URN.
    #[test]
    fn test0007_psv_emits_canonical_catalog_urn() {
        let urns = detect_data_media_urns(b"a|b\n1|2", "psv");
        assert_eq!(urns, vec!["media:ext=psv;list;record;textable".to_string()]);
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
                "media:record;textable;yaml".to_string(),
                "media:textable;yaml".to_string(),
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
                "media:list;record;textable;yaml".to_string(),
                "media:list;textable;yaml".to_string(),
                "media:textable;yaml".to_string(),
            ]
        );
    }

    /// TOML is currently published as the bare `media:textable;toml`
    /// — no list/record narrowing on the anchor. A single emission,
    /// not the duplicated form the previous adapter returned (also
    /// a copy-paste bug).
    #[test]
    fn test0010_toml_is_single_canonical_urn() {
        let urns = detect_data_media_urns(b"key = \"value\"", "toml");
        assert_eq!(urns, vec!["media:ext=toml;textable".to_string()]);
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
    /// concern. Emitting `media:textable` here would step on
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
            "media:json;textable",
            "media:json;list;textable",
            "media:json;record;textable",
            "media:json;list;record;textable",
            // NDJSON variants
            "media:ndjson;textable",
            "media:ndjson;list;textable",
            "media:ndjson;list;record;textable",
            // CSV/TSV/PSV — single canonical each
            "media:ext=csv;list;record;textable",
            "media:ext=tsv;list;record;textable",
            "media:ext=psv;list;record;textable",
            // YAML variants
            "media:textable;yaml",
            "media:list;textable;yaml",
            "media:record;textable;yaml",
            "media:list;record;textable;yaml",
            // XML variants
            "media:ext=xml;textable",
            "media:ext=xml;record;textable",
            "media:ext=xml;list;record;textable",
            // TOML
            "media:ext=toml;textable",
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
