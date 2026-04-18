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

fn detect_csv(text: &str) -> Vec<String> {
    let column_count = count_delimited_columns(text, ',');
    if column_count > 1 {
        vec![
            "media:csv;list;record;textable".to_string(),
            "media:csv;list;textable".to_string(),
        ]
    } else {
        vec!["media:csv;list;textable".to_string()]
    }
}

fn detect_tsv(text: &str) -> Vec<String> {
    let column_count = count_delimited_columns(text, '\t');
    if column_count > 1 {
        vec![
            "media:tsv;list;record;textable".to_string(),
            "media:tsv;list;textable".to_string(),
        ]
    } else {
        vec!["media:tsv;list;textable".to_string()]
    }
}

fn detect_psv(text: &str) -> Vec<String> {
    let column_count = count_delimited_columns(text, '|');
    if column_count > 1 {
        vec![
            "media:psv;list;record;textable".to_string(),
            "media:psv;list;textable".to_string(),
        ]
    } else {
        vec!["media:psv;list;textable".to_string()]
    }
}

fn count_delimited_columns(text: &str, delimiter: char) -> usize {
    let first_line = match text.lines().next() {
        Some(line) => line,
        None => return 1,
    };

    let mut count = 1;
    let mut in_quotes = false;
    for ch in first_line.chars() {
        if ch == '"' {
            in_quotes = !in_quotes;
        } else if ch == delimiter && !in_quotes {
            count += 1;
        }
    }
    count
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
                    "media:xml;list;record;textable".to_string(),
                    "media:xml;textable".to_string(),
                ];
            }
        }
    }

    if trimmed.contains('=') || (trimmed.matches('<').count() > 2) {
        vec![
            "media:xml;record;textable".to_string(),
            "media:xml;textable".to_string(),
        ]
    } else {
        vec!["media:xml;textable".to_string()]
    }
}

fn detect_toml(text: &str) -> Vec<String> {
    let _ = text;
    vec![
        "media:record;textable;toml".to_string(),
        "media:textable;toml".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_object() {
        let urns = detect_data_media_urns(br#"{"key": "value"}"#, "json");
        assert!(urns.iter().any(|u| u.contains("record")));
    }

    #[test]
    fn test_json_array_of_objects() {
        let urns = detect_data_media_urns(br#"[{"a": 1}]"#, "json");
        assert!(urns.iter().any(|u| u.contains("list") && u.contains("record")));
    }

    #[test]
    fn test_csv_multi_column() {
        let urns = detect_data_media_urns(b"a,b,c\n1,2,3", "csv");
        assert!(urns.iter().any(|u| u.contains("list") && u.contains("record")));
    }

    #[test]
    fn test_yaml_mapping() {
        let urns = detect_data_media_urns(b"key: value\nother: data", "yaml");
        assert!(urns.iter().any(|u| u.contains("record")));
    }

    #[test]
    fn test_binary_returns_empty() {
        let urns = detect_data_media_urns(&[0xFF, 0xFE, 0x00], "json");
        assert!(urns.is_empty());
    }

    #[test]
    fn test_unknown_extension_returns_empty() {
        let urns = detect_data_media_urns(b"some text", "xyz");
        assert!(urns.is_empty());
    }
}
