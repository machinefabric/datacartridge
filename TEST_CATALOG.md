# Rust Test Catalog

**Total Tests:** 52

**Numbered Tests:** 0

**Unnumbered Tests:** 52

**Numbered Tests Missing Descriptions:** 0

**Numbering Mismatches:** 0

All numbered test numbers are unique.

This catalog lists all tests in the Rust codebase.

| Test # | Function Name | Description | File |
|--------|---------------|-------------|------|
| | | | |
| unnumbered | `test_binary_returns_empty` | / Non-UTF-8 bytes given to a JSON-extension path return the / empty vec. Text data formats require UTF-8; emitting any / URN for binary content would lie about the bytes. | src/adapter.rs:389 |
| unnumbered | `test_coerce_boolean_to_integer` |  | src/main.rs:1791 |
| unnumbered | `test_coerce_boolean_to_number` |  | src/main.rs:1812 |
| unnumbered | `test_coerce_boolean_to_string` |  | src/main.rs:1773 |
| unnumbered | `test_coerce_integer_to_number` |  | src/main.rs:1803 |
| unnumbered | `test_coerce_integer_to_string` | COERCION TESTS | src/main.rs:1761 |
| unnumbered | `test_coerce_invalid_to_integer_fails` |  | src/main.rs:1835 |
| unnumbered | `test_coerce_invalid_to_number_fails` |  | src/main.rs:1841 |
| unnumbered | `test_coerce_number_to_integer` |  | src/main.rs:1785 |
| unnumbered | `test_coerce_number_to_string` |  | src/main.rs:1767 |
| unnumbered | `test_coerce_object_passthrough` |  | src/main.rs:1826 |
| unnumbered | `test_coerce_string_to_integer` |  | src/main.rs:1779 |
| unnumbered | `test_coerce_string_to_number` |  | src/main.rs:1797 |
| unnumbered | `test_coerce_string_to_object` |  | src/main.rs:1818 |
| unnumbered | `test_coerce_unsupported_target_fails` |  | src/main.rs:1847 |
| unnumbered | `test_csv_is_single_canonical_urn` | / CSV is always list-of-records by the catalog anchor. There / is exactly one canonical CSV URN; the previous adapter / returned that URN duplicated (a copy-paste bug producing a / 2-element list of identical strings). | src/adapter.rs:309 |
| unnumbered | `test_csv_single_column_same_urn_as_multi` | / Single-column CSV is still list-of-records (one record per / row, one field per record). Same canonical URN as / multi-column. The previous adapter branched on column count / — that distinction had no semantic meaning at the URN level / since the catalog only publishes one CSV URN. | src/adapter.rs:321 |
| unnumbered | `test_csv_to_json_records` |  | src/main.rs:1643 |
| unnumbered | `test_csv_to_yaml_records` |  | src/main.rs:1655 |
| unnumbered | `test_csv_type_inference` |  | src/main.rs:1715 |
| unnumbered | `test_csv_with_mixed_columns` |  | src/main.rs:1725 |
| unnumbered | `test_decimate_indices_empty_input` | / Empty input yields empty output. The Op layer turns this / into a hard error (an empty input sequence is suspicious), / but the gate itself must be honest about returning [] — / otherwise we'd hide the empty case from the Op. | src/main.rs:1905 |
| unnumbered | `test_decimate_indices_every_third_of_ten` | / Specific case spelled out by the user requirement: every Nth. / This pins down N=3 over a small enumerated count where the / expected output is hand-readable, so a regression that / changes "every Nth from 0" to "every Nth except 0" or to / "0-indexed but offset N-1" produces a clearly wrong list. | src/main.rs:1887 |
| unnumbered | `test_decimate_indices_starts_at_zero` | / Stride N starts at index 0 and keeps every Nth thereafter, / regardless of count. An off-by-one (e.g. starting at index 1 / instead of 0) shows up here as the first kept index being N / instead of 0 — exactly the failure we want to surface. | src/main.rs:1871 |
| unnumbered | `test_decimate_indices_stride_larger_than_count` | / A stride larger than the input length keeps exactly the / first item (index 0) and nothing else. Catches the "what if / stride > count" edge. | src/main.rs:1896 |
| unnumbered | `test_decimate_indices_stride_one_keeps_all` | / Stride 1 keeps every index — this is the passthrough contract / the cap promises when --keep-every is omitted (the Op / substitutes `1` and calls the gate). | src/main.rs:1858 |
| unnumbered | `test_empty_json_array_to_csv` |  | src/main.rs:1694 |
| unnumbered | `test_every_emitted_urn_is_in_catalog_form` | / **Catalog-presence regression guard.** Every URN this / adapter emits for any of its supported extensions must be / a URN the published catalog actually contains. The earlier / adapter emitted `media:tsv;list;textable`, / `media:psv;list;textable`, and other URN forms that did / not exist in the catalog because no anchor declared them. / This test exhaustively enumerates the URN forms the adapter / is allowed to return; cross-validation at `dx fabric update` / time catches any of these strings drifting away from / catalog truth. | src/adapter.rs:417 |
| unnumbered | `test_json_array_of_objects_emits_full_tier` | / JSON array of objects → list+record narrowing first, then / list, then bare. Three tiers of specificity, all in the / catalog. | src/adapter.rs:273 |
| unnumbered | `test_json_array_of_scalars_has_no_record_narrowing` | / JSON array of scalars → list narrowing without record, then / bare. Only two tiers because record doesn't apply. | src/adapter.rs:288 |
| unnumbered | `test_json_object_emits_record_then_bare` | / JSON object → most-specific is `media:json;record;textable`, / followed by the bare `media:json;textable`. Both URNs must / be present in the catalog for the input-resolver's / conformance walk to succeed. | src/adapter.rs:258 |
| unnumbered | `test_json_records_superset_headers` |  | src/main.rs:1745 |
| unnumbered | `test_json_records_to_csv` |  | src/main.rs:1631 |
| unnumbered | `test_json_to_yaml_array` |  | src/main.rs:1605 |
| unnumbered | `test_json_to_yaml_object` |  | src/main.rs:1586 |
| unnumbered | `test_json_to_yaml_scalar` |  | src/main.rs:1623 |
| unnumbered | `test_malformed_json_fails` |  | src/main.rs:1701 |
| unnumbered | `test_malformed_yaml_fails` |  | src/main.rs:1708 |
| unnumbered | `test_psv_emits_canonical_catalog_urn` | / PSV emits the canonical catalog URN. | src/adapter.rs:340 |
| unnumbered | `test_roundtrip_csv_json_csv` |  | src/main.rs:1683 |
| unnumbered | `test_roundtrip_json_yaml_json` |  | src/main.rs:1673 |
| unnumbered | `test_save_as_txt_cap_present_in_manifest` | / The save-as-txt cap is registered in the manifest builder / (`build_manifest`). Verify it's actually present there with / the right shape — input urn, output urn, command. A / regression that drops the cap from the manifest would / remove it from the cartridge's cap-graph contribution / entirely, and the planner would never reach a `.txt` / target via this cartridge. | src/main.rs:1964 |
| unnumbered | `test_save_as_txt_manifest_and_runtime_urn_agree` | / The cap manifest declares `save-as-txt` with a specific URN / shape; `main()` registers the op's runtime handler under / the URN built from the same parts. If those two strings / diverge, the planner accepts the cap but the runtime has / no dispatch entry — and the cartridge silently fails the / first time a user invokes it. / / This test reconstructs both URNs exactly the way each / site builds them and asserts byte equality. A future / refactor that touches one site without the other surfaces / here at compile/test time rather than at runtime. | src/main.rs:1924 |
| unnumbered | `test_toml_is_single_canonical_urn` | / TOML is currently published as the bare `media:textable;toml` / — no list/record narrowing on the anchor. A single emission, / not the duplicated form the previous adapter returned (also / a copy-paste bug). | src/adapter.rs:379 |
| unnumbered | `test_tsv_emits_canonical_catalog_urn` | / TSV emits the canonical catalog URN. The previous adapter / returned `media:tsv;list;textable` for single-column TSV — / a URN that did not exist in the published catalog because / no `_tsv-data.toml` anchor was declared. | src/adapter.rs:333 |
| unnumbered | `test_unknown_extension_returns_empty` | / An extension this adapter doesn't own returns the empty / vec — datacartridge handles only the structured-data / formats (JSON, CSV, etc.); a `.txt` file is not its / concern. Emitting `media:textable` here would step on / txtcartridge's adapter and would also be wrong (no JSON / detection has happened). | src/adapter.rs:401 |
| unnumbered | `test_yaml_mapping_emits_record_then_bare` | / YAML mapping → record narrowing without list, then bare. / The `looks_like_yaml_mapping` heuristic detects `key: / value` lines. | src/adapter.rs:349 |
| unnumbered | `test_yaml_records_to_csv` |  | src/main.rs:1664 |
| unnumbered | `test_yaml_sequence_of_mappings_full_tier` | / YAML sequence of mappings → list+record, list, bare. | src/adapter.rs:362 |
| unnumbered | `test_yaml_tagged_values_stripped` |  | src/main.rs:1736 |
| unnumbered | `test_yaml_to_json_list` |  | src/main.rs:1615 |
| unnumbered | `test_yaml_to_json_object` |  | src/main.rs:1596 |
---

## Unnumbered Tests

The following tests are cataloged but do not currently participate in numeric test indexing.

- `test_binary_returns_empty` — src/adapter.rs:389
- `test_coerce_boolean_to_integer` — src/main.rs:1791
- `test_coerce_boolean_to_number` — src/main.rs:1812
- `test_coerce_boolean_to_string` — src/main.rs:1773
- `test_coerce_integer_to_number` — src/main.rs:1803
- `test_coerce_integer_to_string` — src/main.rs:1761
- `test_coerce_invalid_to_integer_fails` — src/main.rs:1835
- `test_coerce_invalid_to_number_fails` — src/main.rs:1841
- `test_coerce_number_to_integer` — src/main.rs:1785
- `test_coerce_number_to_string` — src/main.rs:1767
- `test_coerce_object_passthrough` — src/main.rs:1826
- `test_coerce_string_to_integer` — src/main.rs:1779
- `test_coerce_string_to_number` — src/main.rs:1797
- `test_coerce_string_to_object` — src/main.rs:1818
- `test_coerce_unsupported_target_fails` — src/main.rs:1847
- `test_csv_is_single_canonical_urn` — src/adapter.rs:309
- `test_csv_single_column_same_urn_as_multi` — src/adapter.rs:321
- `test_csv_to_json_records` — src/main.rs:1643
- `test_csv_to_yaml_records` — src/main.rs:1655
- `test_csv_type_inference` — src/main.rs:1715
- `test_csv_with_mixed_columns` — src/main.rs:1725
- `test_decimate_indices_empty_input` — src/main.rs:1905
- `test_decimate_indices_every_third_of_ten` — src/main.rs:1887
- `test_decimate_indices_starts_at_zero` — src/main.rs:1871
- `test_decimate_indices_stride_larger_than_count` — src/main.rs:1896
- `test_decimate_indices_stride_one_keeps_all` — src/main.rs:1858
- `test_empty_json_array_to_csv` — src/main.rs:1694
- `test_every_emitted_urn_is_in_catalog_form` — src/adapter.rs:417
- `test_json_array_of_objects_emits_full_tier` — src/adapter.rs:273
- `test_json_array_of_scalars_has_no_record_narrowing` — src/adapter.rs:288
- `test_json_object_emits_record_then_bare` — src/adapter.rs:258
- `test_json_records_superset_headers` — src/main.rs:1745
- `test_json_records_to_csv` — src/main.rs:1631
- `test_json_to_yaml_array` — src/main.rs:1605
- `test_json_to_yaml_object` — src/main.rs:1586
- `test_json_to_yaml_scalar` — src/main.rs:1623
- `test_malformed_json_fails` — src/main.rs:1701
- `test_malformed_yaml_fails` — src/main.rs:1708
- `test_psv_emits_canonical_catalog_urn` — src/adapter.rs:340
- `test_roundtrip_csv_json_csv` — src/main.rs:1683
- `test_roundtrip_json_yaml_json` — src/main.rs:1673
- `test_save_as_txt_cap_present_in_manifest` — src/main.rs:1964
- `test_save_as_txt_manifest_and_runtime_urn_agree` — src/main.rs:1924
- `test_toml_is_single_canonical_urn` — src/adapter.rs:379
- `test_tsv_emits_canonical_catalog_urn` — src/adapter.rs:333
- `test_unknown_extension_returns_empty` — src/adapter.rs:401
- `test_yaml_mapping_emits_record_then_bare` — src/adapter.rs:349
- `test_yaml_records_to_csv` — src/main.rs:1664
- `test_yaml_sequence_of_mappings_full_tier` — src/adapter.rs:362
- `test_yaml_tagged_values_stripped` — src/main.rs:1736
- `test_yaml_to_json_list` — src/main.rs:1615
- `test_yaml_to_json_object` — src/main.rs:1596

---

*Generated from Rust source tree*
*Total tests: 52*
*Total numbered tests: 0*
*Total unnumbered tests: 52*
*Total numbered tests missing descriptions: 0*
*Total numbering mismatches: 0*
