# Rust Test Catalog

**Total Tests:** 53

**Numbered Tests:** 53

**Unnumbered Tests:** 0

**Numbered Tests Missing Descriptions:** 0

**Numbering Mismatches:** 0

All numbered test numbers are unique.

This catalog lists all tests in the Rust codebase.

| Test # | Function Name | Description | File |
|--------|---------------|-------------|------|
| test0001 | `test0001_json_object_emits_record_then_bare` | / JSON object → most-specific is `media:ext=json;fmt=json;record`, / followed by the bare `media:ext=json;fmt=json`. Both URNs must / be present in the catalog for the input-resolver's / conformance walk to succeed. | src/adapter.rs:255 |
| test0002 | `test0002_json_array_of_objects_emits_full_tier` | / JSON array of objects → list+record narrowing first, then / list, then bare. Three tiers of specificity, all in the / catalog. | src/adapter.rs:270 |
| test0003 | `test0003_json_array_of_scalars_has_no_record_narrowing` | / JSON array of scalars → list narrowing without record, then / bare. Only two tiers because record doesn't apply. | src/adapter.rs:285 |
| test0004 | `test0004_csv_is_single_canonical_urn` | / CSV is always list-of-records by the catalog anchor. There / is exactly one canonical CSV URN; the previous adapter / returned that URN duplicated (a copy-paste bug producing a / 2-element list of identical strings). | src/adapter.rs:306 |
| test0005 | `test0005_csv_single_column_same_urn_as_multi` | / Single-column CSV is still list-of-records (one record per / row, one field per record). Same canonical URN as / multi-column. The previous adapter branched on column count / — that distinction had no semantic meaning at the URN level / since the catalog only publishes one CSV URN. | src/adapter.rs:318 |
| test0006 | `test0006_tsv_emits_canonical_catalog_urn` | / TSV emits the canonical catalog URN. The previous adapter / returned `media:enc=utf-8;list;tsv` for single-column TSV — / a URN that did not exist in the published catalog because / no `_tsv-data.toml` anchor was declared. | src/adapter.rs:330 |
| test0007 | `test0007_psv_emits_canonical_catalog_urn` | / PSV emits the canonical catalog URN. | src/adapter.rs:337 |
| test0008 | `test0008_yaml_mapping_emits_record_then_bare` | / YAML mapping → record narrowing without list, then bare. / The `looks_like_yaml_mapping` heuristic detects `key: / value` lines. | src/adapter.rs:346 |
| test0009 | `test0009_yaml_sequence_of_mappings_full_tier` | / YAML sequence of mappings → list+record, list, bare. | src/adapter.rs:359 |
| test0010 | `test0010_toml_is_single_canonical_urn` | / A `.toml` file is a UTF-8 text file (no `fmt=` of its own), so the / catalog publishes `media:enc=utf-8;ext=toml`. A single emission, / not the duplicated form the previous adapter returned (also / a copy-paste bug). | src/adapter.rs:376 |
| test0011 | `test0011_binary_returns_empty` | / Non-UTF-8 bytes given to a JSON-extension path return the / empty vec. Text data formats require UTF-8; emitting any / URN for binary content would lie about the bytes. | src/adapter.rs:386 |
| test0012 | `test0012_unknown_extension_returns_empty` | / An extension this adapter doesn't own returns the empty / vec — datacartridge handles only the structured-data / formats (JSON, CSV, etc.); a `.txt` file is not its / concern. Emitting `media:enc=utf-8` here would step on / txtcartridge's adapter and would also be wrong (no JSON / detection has happened). | src/adapter.rs:398 |
| test0013 | `test0013_every_emitted_urn_is_in_catalog_form` | / **Catalog-presence regression guard.** Every URN this / adapter emits for any of its supported extensions must be / a URN the published catalog actually contains. The earlier / adapter emitted `media:enc=utf-8;list;tsv`, / `media:enc=utf-8;list;psv`, and other URN forms that did / not exist in the catalog because no anchor declared them. / This test exhaustively enumerates the URN forms the adapter / is allowed to return; cross-validation at `dx fabric update` / time catches any of these strings drifting away from / catalog truth. | src/adapter.rs:414 |
| test0014 | `test0014_json_to_yaml_object` | TEST0014: Json to yaml object | src/main.rs:1632 |
| test0015 | `test0015_yaml_to_json_object` | TEST0015: Yaml to json object | src/main.rs:1643 |
| test0016 | `test0016_json_to_yaml_array` | TEST0016: Json to yaml array | src/main.rs:1653 |
| test0017 | `test0017_yaml_to_json_list` | TEST0017: Yaml to json list | src/main.rs:1664 |
| test0018 | `test0018_json_to_yaml_scalar` | TEST0018: Json to yaml scalar | src/main.rs:1673 |
| test0019 | `test0019_json_records_to_csv` | TEST0019: Json records to csv | src/main.rs:1682 |
| test0020 | `test0020_csv_to_json_records` | TEST0020: Csv to json records | src/main.rs:1695 |
| test0021 | `test0021_csv_to_yaml_records` | TEST0021: Csv to yaml records | src/main.rs:1708 |
| test0022 | `test0022_yaml_records_to_csv` | TEST0022: Yaml records to csv | src/main.rs:1718 |
| test0023 | `test0023_roundtrip_json_yaml_json` | TEST0023: Roundtrip json yaml json | src/main.rs:1728 |
| test0024 | `test0024_roundtrip_csv_json_csv` | TEST0024: Roundtrip csv json csv | src/main.rs:1739 |
| test0025 | `test0025_empty_json_array_to_csv` | TEST0025: Empty json array to csv | src/main.rs:1751 |
| test0026 | `test0026_malformed_json_fails` | TEST0026: Malformed json fails | src/main.rs:1759 |
| test0027 | `test0027_malformed_yaml_fails` | TEST0027: Malformed yaml fails | src/main.rs:1767 |
| test0028 | `test0028_csv_type_inference` | TEST0028: Csv type inference | src/main.rs:1775 |
| test0029 | `test0029_csv_with_mixed_columns` | TEST0029: Csv with mixed columns | src/main.rs:1786 |
| test0030 | `test0030_yaml_tagged_values_stripped` | TEST0030: Yaml tagged values stripped | src/main.rs:1798 |
| test0031 | `test0031_json_records_superset_headers` | TEST0031: Json records superset headers | src/main.rs:1808 |
| test0032 | `test0032_coerce_integer_to_string` | COERCION TESTS | src/main.rs:1824 |
| test0033 | `test0033_coerce_number_to_string` | TEST0033: Coerce number to string | src/main.rs:1831 |
| test0034 | `test0034_coerce_boolean_to_string` | TEST0034: Coerce boolean to string | src/main.rs:1838 |
| test0035 | `test0035_coerce_string_to_integer` | TEST0035: Coerce string to integer | src/main.rs:1845 |
| test0036 | `test0036_coerce_number_to_integer` | TEST0036: Coerce number to integer | src/main.rs:1852 |
| test0037 | `test0037_coerce_boolean_to_integer` | TEST0037: Coerce boolean to integer | src/main.rs:1859 |
| test0038 | `test0038_coerce_string_to_number` | TEST0038: Coerce string to number | src/main.rs:1866 |
| test0039 | `test0039_coerce_integer_to_number` | TEST0039: Coerce integer to number | src/main.rs:1873 |
| test0040 | `test0040_coerce_boolean_to_number` | TEST0040: Coerce boolean to number | src/main.rs:1883 |
| test0041 | `test0041_coerce_string_to_object` | TEST0041: Coerce string to object | src/main.rs:1890 |
| test0042 | `test0042_coerce_object_passthrough` | TEST0042: Coerce object passthrough | src/main.rs:1901 |
| test0043 | `test0043_coerce_invalid_to_integer_fails` | TEST0043: Coerce invalid to integer fails | src/main.rs:1911 |
| test0044 | `test0044_coerce_invalid_to_number_fails` | TEST0044: Coerce invalid to number fails | src/main.rs:1918 |
| test0045 | `test0045_coerce_unsupported_target_fails` | TEST0045: Coerce unsupported target fails | src/main.rs:1925 |
| test0046 | `test0046_decimate_indices_stride_one_keeps_all` | / Stride 1 keeps every index — this is the passthrough contract / the cap promises when --keep-every is omitted (the Op / substitutes `1` and calls the gate). | src/main.rs:1936 |
| test0047 | `test0047_decimate_indices_starts_at_zero` | / Stride N starts at index 0 and keeps every Nth thereafter, / regardless of count. An off-by-one (e.g. starting at index 1 / instead of 0) shows up here as the first kept index being N / instead of 0 — exactly the failure we want to surface. | src/main.rs:1949 |
| test0048 | `test0048_decimate_indices_every_third_of_ten` | / Specific case spelled out by the user requirement: every Nth. / This pins down N=3 over a small enumerated count where the / expected output is hand-readable, so a regression that / changes "every Nth from 0" to "every Nth except 0" or to / "0-indexed but offset N-1" produces a clearly wrong list. | src/main.rs:1965 |
| test0049 | `test0049_decimate_indices_stride_larger_than_count` | / A stride larger than the input length keeps exactly the / first item (index 0) and nothing else. Catches the "what if / stride > count" edge. | src/main.rs:1974 |
| test0050 | `test0050_decimate_indices_empty_input` | / Empty input yields empty output. The Op layer turns this / into a hard error (an empty input sequence is suspicious), / but the gate itself must be honest about returning [] — / otherwise we'd hide the empty case from the Op. | src/main.rs:1983 |
| test0051 | `test0051_save_as_txt_manifest_and_runtime_urn_agree` | / The cap manifest declares `save-as-txt` with a specific URN / shape; `main()` registers the op's runtime handler under / the URN built from the same parts. If those two strings / diverge, the planner accepts the cap but the runtime has / no dispatch entry — and the cartridge silently fails the / first time a user invokes it. / / This test reconstructs both URNs exactly the way each / site builds them and asserts byte equality. A future / refactor that touches one site without the other surfaces / here at compile/test time rather than at runtime. | src/main.rs:2002 |
| test0052 | `test0052_save_as_txt_cap_present_in_manifest` | / The save-as-txt cap is registered in the manifest builder / (`build_manifest`). Verify it's actually present there with / the right shape — input urn, output urn, command. A / regression that drops the cap from the manifest would / remove it from the cartridge's cap-graph contribution / entirely, and the planner would never reach a `.txt` / target via this cartridge. | src/main.rs:2042 |
| test1859 | `test1859_advertised_caps_resolve_in_catalog` | TEST1859: every cap URN this cartridge advertises must resolve in the pinned fabric catalog. A drifted/bare-marker URN absent from the catalog is silently dropped by LiveCapFab at runtime; this guard turns that into a hard failure naming the exact URN. `get_cap` consults the same canonical manifest map LiveCapFab resolves against. Network-mediated against the env-configured registry (set by `dx test`), pinned at FABRIC_MANIFEST_VERSION. The identity cap is engine-provided, not a catalog entry, so it is excluded. | src/main.rs:1604 |
---

*Generated from Rust source tree*
*Total tests: 53*
*Total numbered tests: 53*
*Total unnumbered tests: 0*
*Total numbered tests missing descriptions: 0*
*Total numbering mismatches: 0*
