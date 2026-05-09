# Rust Test Catalog

**Total Tests:** 45

**Numbered Tests:** 0

**Unnumbered Tests:** 45

**Numbered Tests Missing Descriptions:** 0

**Numbering Mismatches:** 0

All numbered test numbers are unique.

This catalog lists all tests in the Rust codebase.

| Test # | Function Name | Description | File |
|--------|---------------|-------------|------|
| | | | |
| unnumbered | `test_binary_returns_empty` |  | src/adapter.rs:305 |
| unnumbered | `test_coerce_boolean_to_integer` |  | src/main.rs:1787 |
| unnumbered | `test_coerce_boolean_to_number` |  | src/main.rs:1808 |
| unnumbered | `test_coerce_boolean_to_string` |  | src/main.rs:1769 |
| unnumbered | `test_coerce_integer_to_number` |  | src/main.rs:1799 |
| unnumbered | `test_coerce_integer_to_string` | COERCION TESTS | src/main.rs:1757 |
| unnumbered | `test_coerce_invalid_to_integer_fails` |  | src/main.rs:1831 |
| unnumbered | `test_coerce_invalid_to_number_fails` |  | src/main.rs:1837 |
| unnumbered | `test_coerce_number_to_integer` |  | src/main.rs:1781 |
| unnumbered | `test_coerce_number_to_string` |  | src/main.rs:1763 |
| unnumbered | `test_coerce_object_passthrough` |  | src/main.rs:1822 |
| unnumbered | `test_coerce_string_to_integer` |  | src/main.rs:1775 |
| unnumbered | `test_coerce_string_to_number` |  | src/main.rs:1793 |
| unnumbered | `test_coerce_string_to_object` |  | src/main.rs:1814 |
| unnumbered | `test_coerce_unsupported_target_fails` |  | src/main.rs:1843 |
| unnumbered | `test_csv_multi_column` |  | src/adapter.rs:293 |
| unnumbered | `test_csv_to_json_records` |  | src/main.rs:1639 |
| unnumbered | `test_csv_to_yaml_records` |  | src/main.rs:1651 |
| unnumbered | `test_csv_type_inference` |  | src/main.rs:1711 |
| unnumbered | `test_csv_with_mixed_columns` |  | src/main.rs:1721 |
| unnumbered | `test_decimate_indices_empty_input` | / Empty input yields empty output. The Op layer turns this / into a hard error (an empty input sequence is suspicious), / but the gate itself must be honest about returning [] — / otherwise we'd hide the empty case from the Op. | src/main.rs:1901 |
| unnumbered | `test_decimate_indices_every_third_of_ten` | / Specific case spelled out by the user requirement: every Nth. / This pins down N=3 over a small enumerated count where the / expected output is hand-readable, so a regression that / changes "every Nth from 0" to "every Nth except 0" or to / "0-indexed but offset N-1" produces a clearly wrong list. | src/main.rs:1883 |
| unnumbered | `test_decimate_indices_starts_at_zero` | / Stride N starts at index 0 and keeps every Nth thereafter, / regardless of count. An off-by-one (e.g. starting at index 1 / instead of 0) shows up here as the first kept index being N / instead of 0 — exactly the failure we want to surface. | src/main.rs:1867 |
| unnumbered | `test_decimate_indices_stride_larger_than_count` | / A stride larger than the input length keeps exactly the / first item (index 0) and nothing else. Catches the "what if / stride > count" edge. | src/main.rs:1892 |
| unnumbered | `test_decimate_indices_stride_one_keeps_all` | / Stride 1 keeps every index — this is the passthrough contract / the cap promises when --keep-every is omitted (the Op / substitutes `1` and calls the gate). | src/main.rs:1854 |
| unnumbered | `test_empty_json_array_to_csv` |  | src/main.rs:1690 |
| unnumbered | `test_json_array_of_objects` |  | src/adapter.rs:287 |
| unnumbered | `test_json_object` |  | src/adapter.rs:281 |
| unnumbered | `test_json_records_superset_headers` |  | src/main.rs:1741 |
| unnumbered | `test_json_records_to_csv` |  | src/main.rs:1627 |
| unnumbered | `test_json_to_yaml_array` |  | src/main.rs:1601 |
| unnumbered | `test_json_to_yaml_object` |  | src/main.rs:1582 |
| unnumbered | `test_json_to_yaml_scalar` |  | src/main.rs:1619 |
| unnumbered | `test_malformed_json_fails` |  | src/main.rs:1697 |
| unnumbered | `test_malformed_yaml_fails` |  | src/main.rs:1704 |
| unnumbered | `test_roundtrip_csv_json_csv` |  | src/main.rs:1679 |
| unnumbered | `test_roundtrip_json_yaml_json` |  | src/main.rs:1669 |
| unnumbered | `test_save_as_txt_cap_present_in_manifest` | / The save-as-txt cap is registered in the manifest builder / (`build_manifest`). Verify it's actually present there with / the right shape — input urn, output urn, command. A / regression that drops the cap from the manifest would / remove it from the cartridge's cap-graph contribution / entirely, and the planner would never reach a `.txt` / target via this cartridge. | src/main.rs:1960 |
| unnumbered | `test_save_as_txt_manifest_and_runtime_urn_agree` | / The cap manifest declares `save-as-txt` with a specific URN / shape; `main()` registers the op's runtime handler under / the URN built from the same parts. If those two strings / diverge, the planner accepts the cap but the runtime has / no dispatch entry — and the cartridge silently fails the / first time a user invokes it. / / This test reconstructs both URNs exactly the way each / site builds them and asserts byte equality. A future / refactor that touches one site without the other surfaces / here at compile/test time rather than at runtime. | src/main.rs:1920 |
| unnumbered | `test_unknown_extension_returns_empty` |  | src/adapter.rs:311 |
| unnumbered | `test_yaml_mapping` |  | src/adapter.rs:299 |
| unnumbered | `test_yaml_records_to_csv` |  | src/main.rs:1660 |
| unnumbered | `test_yaml_tagged_values_stripped` |  | src/main.rs:1732 |
| unnumbered | `test_yaml_to_json_list` |  | src/main.rs:1611 |
| unnumbered | `test_yaml_to_json_object` |  | src/main.rs:1592 |
---

## Unnumbered Tests

The following tests are cataloged but do not currently participate in numeric test indexing.

- `test_binary_returns_empty` — src/adapter.rs:305
- `test_coerce_boolean_to_integer` — src/main.rs:1787
- `test_coerce_boolean_to_number` — src/main.rs:1808
- `test_coerce_boolean_to_string` — src/main.rs:1769
- `test_coerce_integer_to_number` — src/main.rs:1799
- `test_coerce_integer_to_string` — src/main.rs:1757
- `test_coerce_invalid_to_integer_fails` — src/main.rs:1831
- `test_coerce_invalid_to_number_fails` — src/main.rs:1837
- `test_coerce_number_to_integer` — src/main.rs:1781
- `test_coerce_number_to_string` — src/main.rs:1763
- `test_coerce_object_passthrough` — src/main.rs:1822
- `test_coerce_string_to_integer` — src/main.rs:1775
- `test_coerce_string_to_number` — src/main.rs:1793
- `test_coerce_string_to_object` — src/main.rs:1814
- `test_coerce_unsupported_target_fails` — src/main.rs:1843
- `test_csv_multi_column` — src/adapter.rs:293
- `test_csv_to_json_records` — src/main.rs:1639
- `test_csv_to_yaml_records` — src/main.rs:1651
- `test_csv_type_inference` — src/main.rs:1711
- `test_csv_with_mixed_columns` — src/main.rs:1721
- `test_decimate_indices_empty_input` — src/main.rs:1901
- `test_decimate_indices_every_third_of_ten` — src/main.rs:1883
- `test_decimate_indices_starts_at_zero` — src/main.rs:1867
- `test_decimate_indices_stride_larger_than_count` — src/main.rs:1892
- `test_decimate_indices_stride_one_keeps_all` — src/main.rs:1854
- `test_empty_json_array_to_csv` — src/main.rs:1690
- `test_json_array_of_objects` — src/adapter.rs:287
- `test_json_object` — src/adapter.rs:281
- `test_json_records_superset_headers` — src/main.rs:1741
- `test_json_records_to_csv` — src/main.rs:1627
- `test_json_to_yaml_array` — src/main.rs:1601
- `test_json_to_yaml_object` — src/main.rs:1582
- `test_json_to_yaml_scalar` — src/main.rs:1619
- `test_malformed_json_fails` — src/main.rs:1697
- `test_malformed_yaml_fails` — src/main.rs:1704
- `test_roundtrip_csv_json_csv` — src/main.rs:1679
- `test_roundtrip_json_yaml_json` — src/main.rs:1669
- `test_save_as_txt_cap_present_in_manifest` — src/main.rs:1960
- `test_save_as_txt_manifest_and_runtime_urn_agree` — src/main.rs:1920
- `test_unknown_extension_returns_empty` — src/adapter.rs:311
- `test_yaml_mapping` — src/adapter.rs:299
- `test_yaml_records_to_csv` — src/main.rs:1660
- `test_yaml_tagged_values_stripped` — src/main.rs:1732
- `test_yaml_to_json_list` — src/main.rs:1611
- `test_yaml_to_json_object` — src/main.rs:1592

---

*Generated from Rust source tree*
*Total tests: 45*
*Total numbered tests: 0*
*Total unnumbered tests: 45*
*Total numbered tests missing descriptions: 0*
*Total numbering mismatches: 0*
