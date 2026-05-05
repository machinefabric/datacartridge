# Rust Test Catalog

**Total Tests:** 43

**Numbered Tests:** 0

**Unnumbered Tests:** 43

**Numbered Tests Missing Descriptions:** 0

**Numbering Mismatches:** 0

All numbered test numbers are unique.

This catalog lists all tests in the Rust codebase.

| Test # | Function Name | Description | File |
|--------|---------------|-------------|------|
| | | | |
| unnumbered | `test_binary_returns_empty` |  | src/adapter.rs:305 |
| unnumbered | `test_coerce_boolean_to_integer` |  | src/main.rs:1629 |
| unnumbered | `test_coerce_boolean_to_number` |  | src/main.rs:1650 |
| unnumbered | `test_coerce_boolean_to_string` |  | src/main.rs:1611 |
| unnumbered | `test_coerce_integer_to_number` |  | src/main.rs:1641 |
| unnumbered | `test_coerce_integer_to_string` | COERCION TESTS | src/main.rs:1599 |
| unnumbered | `test_coerce_invalid_to_integer_fails` |  | src/main.rs:1673 |
| unnumbered | `test_coerce_invalid_to_number_fails` |  | src/main.rs:1679 |
| unnumbered | `test_coerce_number_to_integer` |  | src/main.rs:1623 |
| unnumbered | `test_coerce_number_to_string` |  | src/main.rs:1605 |
| unnumbered | `test_coerce_object_passthrough` |  | src/main.rs:1664 |
| unnumbered | `test_coerce_string_to_integer` |  | src/main.rs:1617 |
| unnumbered | `test_coerce_string_to_number` |  | src/main.rs:1635 |
| unnumbered | `test_coerce_string_to_object` |  | src/main.rs:1656 |
| unnumbered | `test_coerce_unsupported_target_fails` |  | src/main.rs:1685 |
| unnumbered | `test_csv_multi_column` |  | src/adapter.rs:293 |
| unnumbered | `test_csv_to_json_records` |  | src/main.rs:1481 |
| unnumbered | `test_csv_to_yaml_records` |  | src/main.rs:1493 |
| unnumbered | `test_csv_type_inference` |  | src/main.rs:1553 |
| unnumbered | `test_csv_with_mixed_columns` |  | src/main.rs:1563 |
| unnumbered | `test_decimate_indices_empty_input` | / Empty input yields empty output. The Op layer turns this / into a hard error (an empty input sequence is suspicious), / but the gate itself must be honest about returning [] — / otherwise we'd hide the empty case from the Op. | src/main.rs:1743 |
| unnumbered | `test_decimate_indices_every_third_of_ten` | / Specific case spelled out by the user requirement: every Nth. / This pins down N=3 over a small enumerated count where the / expected output is hand-readable, so a regression that / changes "every Nth from 0" to "every Nth except 0" or to / "0-indexed but offset N-1" produces a clearly wrong list. | src/main.rs:1725 |
| unnumbered | `test_decimate_indices_starts_at_zero` | / Stride N starts at index 0 and keeps every Nth thereafter, / regardless of count. An off-by-one (e.g. starting at index 1 / instead of 0) shows up here as the first kept index being N / instead of 0 — exactly the failure we want to surface. | src/main.rs:1709 |
| unnumbered | `test_decimate_indices_stride_larger_than_count` | / A stride larger than the input length keeps exactly the / first item (index 0) and nothing else. Catches the "what if / stride > count" edge. | src/main.rs:1734 |
| unnumbered | `test_decimate_indices_stride_one_keeps_all` | / Stride 1 keeps every index — this is the passthrough contract / the cap promises when --keep-every is omitted (the Op / substitutes `1` and calls the gate). | src/main.rs:1696 |
| unnumbered | `test_empty_json_array_to_csv` |  | src/main.rs:1532 |
| unnumbered | `test_json_array_of_objects` |  | src/adapter.rs:287 |
| unnumbered | `test_json_object` |  | src/adapter.rs:281 |
| unnumbered | `test_json_records_superset_headers` |  | src/main.rs:1583 |
| unnumbered | `test_json_records_to_csv` |  | src/main.rs:1469 |
| unnumbered | `test_json_to_yaml_array` |  | src/main.rs:1443 |
| unnumbered | `test_json_to_yaml_object` |  | src/main.rs:1424 |
| unnumbered | `test_json_to_yaml_scalar` |  | src/main.rs:1461 |
| unnumbered | `test_malformed_json_fails` |  | src/main.rs:1539 |
| unnumbered | `test_malformed_yaml_fails` |  | src/main.rs:1546 |
| unnumbered | `test_roundtrip_csv_json_csv` |  | src/main.rs:1521 |
| unnumbered | `test_roundtrip_json_yaml_json` |  | src/main.rs:1511 |
| unnumbered | `test_unknown_extension_returns_empty` |  | src/adapter.rs:311 |
| unnumbered | `test_yaml_mapping` |  | src/adapter.rs:299 |
| unnumbered | `test_yaml_records_to_csv` |  | src/main.rs:1502 |
| unnumbered | `test_yaml_tagged_values_stripped` |  | src/main.rs:1574 |
| unnumbered | `test_yaml_to_json_list` |  | src/main.rs:1453 |
| unnumbered | `test_yaml_to_json_object` |  | src/main.rs:1434 |
---

## Unnumbered Tests

The following tests are cataloged but do not currently participate in numeric test indexing.

- `test_binary_returns_empty` — src/adapter.rs:305
- `test_coerce_boolean_to_integer` — src/main.rs:1629
- `test_coerce_boolean_to_number` — src/main.rs:1650
- `test_coerce_boolean_to_string` — src/main.rs:1611
- `test_coerce_integer_to_number` — src/main.rs:1641
- `test_coerce_integer_to_string` — src/main.rs:1599
- `test_coerce_invalid_to_integer_fails` — src/main.rs:1673
- `test_coerce_invalid_to_number_fails` — src/main.rs:1679
- `test_coerce_number_to_integer` — src/main.rs:1623
- `test_coerce_number_to_string` — src/main.rs:1605
- `test_coerce_object_passthrough` — src/main.rs:1664
- `test_coerce_string_to_integer` — src/main.rs:1617
- `test_coerce_string_to_number` — src/main.rs:1635
- `test_coerce_string_to_object` — src/main.rs:1656
- `test_coerce_unsupported_target_fails` — src/main.rs:1685
- `test_csv_multi_column` — src/adapter.rs:293
- `test_csv_to_json_records` — src/main.rs:1481
- `test_csv_to_yaml_records` — src/main.rs:1493
- `test_csv_type_inference` — src/main.rs:1553
- `test_csv_with_mixed_columns` — src/main.rs:1563
- `test_decimate_indices_empty_input` — src/main.rs:1743
- `test_decimate_indices_every_third_of_ten` — src/main.rs:1725
- `test_decimate_indices_starts_at_zero` — src/main.rs:1709
- `test_decimate_indices_stride_larger_than_count` — src/main.rs:1734
- `test_decimate_indices_stride_one_keeps_all` — src/main.rs:1696
- `test_empty_json_array_to_csv` — src/main.rs:1532
- `test_json_array_of_objects` — src/adapter.rs:287
- `test_json_object` — src/adapter.rs:281
- `test_json_records_superset_headers` — src/main.rs:1583
- `test_json_records_to_csv` — src/main.rs:1469
- `test_json_to_yaml_array` — src/main.rs:1443
- `test_json_to_yaml_object` — src/main.rs:1424
- `test_json_to_yaml_scalar` — src/main.rs:1461
- `test_malformed_json_fails` — src/main.rs:1539
- `test_malformed_yaml_fails` — src/main.rs:1546
- `test_roundtrip_csv_json_csv` — src/main.rs:1521
- `test_roundtrip_json_yaml_json` — src/main.rs:1511
- `test_unknown_extension_returns_empty` — src/adapter.rs:311
- `test_yaml_mapping` — src/adapter.rs:299
- `test_yaml_records_to_csv` — src/main.rs:1502
- `test_yaml_tagged_values_stripped` — src/main.rs:1574
- `test_yaml_to_json_list` — src/main.rs:1453
- `test_yaml_to_json_object` — src/main.rs:1434

---

*Generated from Rust source tree*
*Total tests: 43*
*Total numbered tests: 0*
*Total unnumbered tests: 43*
*Total numbered tests missing descriptions: 0*
*Total numbering mismatches: 0*
