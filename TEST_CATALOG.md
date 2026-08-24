# Test catalogue — cartridges/datacartridge

Generated from the catalogue table by `sdx catalog export`. Edit the tests, not this file.

83 tests: 80 numbered, 3 unnumbered.

## Numbered

| Number | Repository | Language | Test | Location | Description |
|---|---|---|---|---|---|
| TEST1 | cartridges/datacartridge | rust | `test0001_json_object_emits_record_then_bare` | src/adapter.rs:255 | JSON object → most-specific is `media:ext=json;fmt=json;record`, followed by the bare `media:ext=json;fmt=json`. Both URNs must be present in the catalog for the input-resolver's conformance walk to succeed. |
| TEST2 | cartridges/datacartridge | rust | `test0002_json_array_of_objects_emits_full_tier` | src/adapter.rs:270 | JSON array of objects → list+record narrowing first, then list, then bare. Three tiers of specificity, all in the catalog. |
| TEST3 | cartridges/datacartridge | rust | `test0003_json_array_of_scalars_has_no_record_narrowing` | src/adapter.rs:285 | JSON array of scalars → list narrowing without record, then bare. Only two tiers because record doesn't apply. |
| TEST4 | cartridges/datacartridge | rust | `test0004_csv_is_single_canonical_urn` | src/adapter.rs:306 | CSV is always list-of-records by the catalog anchor. There is exactly one canonical CSV URN; the previous adapter returned that URN duplicated (a copy-paste bug producing a 2-element list of identical strings). |
| TEST5 | cartridges/datacartridge | rust | `test0005_csv_single_column_same_urn_as_multi` | src/adapter.rs:318 | Single-column CSV is still list-of-records (one record per row, one field per record). Same canonical URN as multi-column. The previous adapter branched on column count — that distinction had no semantic meaning at the URN level since the catalog only publishes one CSV URN. |
| TEST6 | cartridges/datacartridge | rust | `test0006_tsv_emits_canonical_catalog_urn` | src/adapter.rs:330 | TSV emits the canonical catalog URN. The previous adapter returned `media:enc=utf-8;list;tsv` for single-column TSV — a URN that did not exist in the published catalog because no `_tsv-data.toml` anchor was declared. |
| TEST7 | cartridges/datacartridge | rust | `test0007_psv_emits_canonical_catalog_urn` | src/adapter.rs:337 | PSV emits the canonical catalog URN. |
| TEST8 | cartridges/datacartridge | rust | `test0008_yaml_mapping_emits_record_then_bare` | src/adapter.rs:346 | YAML mapping → record narrowing without list, then bare. The `looks_like_yaml_mapping` heuristic detects `key: value` lines. |
| TEST9 | cartridges/datacartridge | rust | `test0009_yaml_sequence_of_mappings_full_tier` | src/adapter.rs:359 | YAML sequence of mappings → list+record, list, bare. |
| TEST10 | cartridges/datacartridge | rust | `test0010_toml_is_single_canonical_urn` | src/adapter.rs:376 | A `.toml` file is a UTF-8 text file (no `fmt=` of its own), so the catalog publishes `media:enc=utf-8;ext=toml`. A single emission, not the duplicated form the previous adapter returned (also a copy-paste bug). |
| TEST11 | cartridges/datacartridge | rust | `test0011_binary_returns_empty` | src/adapter.rs:386 | Non-UTF-8 bytes given to a JSON-extension path return the empty vec. Text data formats require UTF-8; emitting any URN for binary content would lie about the bytes. |
| TEST12 | cartridges/datacartridge | rust | `test0012_unknown_extension_returns_empty` | src/adapter.rs:398 | An extension this adapter doesn't own returns the empty vec — datacartridge handles only the structured-data formats (JSON, CSV, etc.); a `.txt` file is not its concern. Emitting `media:enc=utf-8` here would step on txtcartridge's adapter and would also be wrong (no JSON detection has happened). |
| TEST13 | cartridges/datacartridge | rust | `test0013_every_emitted_urn_is_in_catalog_form` | src/adapter.rs:414 | **Catalog-presence regression guard.** Every URN this adapter emits for any of its supported extensions must be a URN the published catalog actually contains. The earlier adapter emitted `media:enc=utf-8;list;tsv`, `media:enc=utf-8;list;psv`, and other URN forms that did not exist in the catalog because no anchor declared them. This test exhaustively enumerates the URN forms the adapter is allowed to return; cross-validation at `sdx publish fabric` time catches any of these strings drifting away from catalog truth. |
| TEST14 | cartridges/datacartridge | rust | `test0014_json_to_yaml_object` | src/main.rs:2199 | TEST0014: Json to yaml object |
| TEST15 | cartridges/datacartridge | rust | `test0015_yaml_to_json_object` | src/main.rs:2210 | TEST0015: Yaml to json object |
| TEST16 | cartridges/datacartridge | rust | `test0016_json_to_yaml_array` | src/main.rs:2220 | TEST0016: Json to yaml array |
| TEST17 | cartridges/datacartridge | rust | `test0017_yaml_to_json_list` | src/main.rs:2231 | TEST0017: Yaml to json list |
| TEST18 | cartridges/datacartridge | rust | `test0018_json_to_yaml_scalar` | src/main.rs:2240 | TEST0018: Json to yaml scalar |
| TEST19 | cartridges/datacartridge | rust | `test0019_json_records_to_csv` | src/main.rs:2249 | TEST0019: Json records to csv |
| TEST20 | cartridges/datacartridge | rust | `test0020_csv_to_json_records` | src/main.rs:2262 | TEST0020: Csv to json records |
| TEST21 | cartridges/datacartridge | rust | `test0021_csv_to_yaml_records` | src/main.rs:2275 | TEST0021: Csv to yaml records |
| TEST22 | cartridges/datacartridge | rust | `test0022_yaml_records_to_csv` | src/main.rs:2285 | TEST0022: Yaml records to csv |
| TEST23 | cartridges/datacartridge | rust | `test0023_roundtrip_json_yaml_json` | src/main.rs:2295 | TEST0023: Roundtrip json yaml json |
| TEST24 | cartridges/datacartridge | rust | `test0024_roundtrip_csv_json_csv` | src/main.rs:2306 | TEST0024: Roundtrip csv json csv |
| TEST25 | cartridges/datacartridge | rust | `test0025_empty_json_array_to_csv` | src/main.rs:2318 | TEST0025: Empty json array to csv |
| TEST26 | cartridges/datacartridge | rust | `test0026_malformed_json_fails` | src/main.rs:2326 | TEST0026: Malformed json fails |
| TEST27 | cartridges/datacartridge | rust | `test0027_malformed_yaml_fails` | src/main.rs:2334 | TEST0027: Malformed yaml fails |
| TEST28 | cartridges/datacartridge | rust | `test0028_csv_type_inference` | src/main.rs:2342 | TEST0028: Csv type inference |
| TEST29 | cartridges/datacartridge | rust | `test0029_csv_with_mixed_columns` | src/main.rs:2379 | TEST0029: Csv with mixed columns |
| TEST30 | cartridges/datacartridge | rust | `test0030_yaml_tagged_values_stripped` | src/main.rs:2391 | TEST0030: Yaml tagged values stripped |
| TEST31 | cartridges/datacartridge | rust | `test0031_json_records_superset_headers` | src/main.rs:2401 | TEST0031: Json records superset headers |
| TEST32 | cartridges/datacartridge | rust | `test0032_coerce_integer_to_string` | src/main.rs:2417 | COERCION TESTS |
| TEST33 | cartridges/datacartridge | rust | `test0033_coerce_number_to_string` | src/main.rs:2424 | TEST0033: Coerce number to string |
| TEST34 | cartridges/datacartridge | rust | `test0034_coerce_boolean_to_string` | src/main.rs:2431 | TEST0034: Coerce boolean to string |
| TEST35 | cartridges/datacartridge | rust | `test0035_coerce_string_to_integer` | src/main.rs:2438 | TEST0035: Coerce string to integer |
| TEST36 | cartridges/datacartridge | rust | `test0036_coerce_number_to_integer` | src/main.rs:2445 | TEST0036: Coerce number to integer |
| TEST37 | cartridges/datacartridge | rust | `test0037_coerce_boolean_to_integer` | src/main.rs:2452 | TEST0037: Coerce boolean to integer |
| TEST38 | cartridges/datacartridge | rust | `test0038_coerce_string_to_number` | src/main.rs:2459 | TEST0038: Coerce string to number |
| TEST39 | cartridges/datacartridge | rust | `test0039_coerce_integer_to_number` | src/main.rs:2466 | TEST0039: Coerce integer to number |
| TEST40 | cartridges/datacartridge | rust | `test0040_coerce_boolean_to_number` | src/main.rs:2478 | TEST0040: Coerce boolean to number — canonical f64 rendering, consistent with every other number-producing coercion (integral values render without a decimal point; "42" not "42.0"). |
| TEST41 | cartridges/datacartridge | rust | `test0041_coerce_string_to_object` | src/main.rs:2519 | TEST0041: Coerce string to object |
| TEST42 | cartridges/datacartridge | rust | `test0042_coerce_object_passthrough` | src/main.rs:2530 | TEST0042: Coerce object passthrough |
| TEST44 | cartridges/datacartridge | rust | `test0044_coerce_invalid_to_number_fails` | src/main.rs:2547 | TEST0044: Coerce invalid to number fails |
| TEST45 | cartridges/datacartridge | rust | `test0045_coerce_unsupported_target_fails` | src/main.rs:2554 | TEST0045: Coerce unsupported target fails |
| TEST46 | cartridges/datacartridge | rust | `test0046_decimate_indices_stride_one_keeps_all` | src/main.rs:2565 | Stride 1 keeps every index — this is the passthrough contract the cap promises when --keep-every is omitted (the Op substitutes `1` and calls the gate). |
| TEST47 | cartridges/datacartridge | rust | `test0047_decimate_indices_starts_at_zero` | src/main.rs:2578 | Stride N starts at index 0 and keeps every Nth thereafter, regardless of count. An off-by-one (e.g. starting at index 1 instead of 0) shows up here as the first kept index being N instead of 0 — exactly the failure we want to surface. |
| TEST48 | cartridges/datacartridge | rust | `test0048_decimate_indices_every_third_of_ten` | src/main.rs:2594 | Specific case spelled out by the user requirement: every Nth. This pins down N=3 over a small enumerated count where the expected output is hand-readable, so a regression that changes "every Nth from 0" to "every Nth except 0" or to "0-indexed but offset N-1" produces a clearly wrong list. |
| TEST49 | cartridges/datacartridge | rust | `test0049_decimate_indices_stride_larger_than_count` | src/main.rs:2603 | A stride larger than the input length keeps exactly the first item (index 0) and nothing else. Catches the "what if stride > count" edge. |
| TEST50 | cartridges/datacartridge | rust | `test0050_decimate_indices_empty_input` | src/main.rs:2612 | Empty input yields empty output. The Op layer turns this into a hard error (an empty input sequence is suspicious), but the gate itself must be honest about returning [] — otherwise we'd hide the empty case from the Op. |
| TEST51 | cartridges/datacartridge | rust | `test0051_save_as_txt_manifest_and_runtime_urn_agree` | src/main.rs:2631 | The cap manifest declares `save-as-txt` with a specific URN shape; `main()` registers the op's runtime handler under the URN built from the same parts. If those two strings diverge, the planner accepts the cap but the runtime has no dispatch entry — and the cartridge silently fails the first time a user invokes it. This test reconstructs both URNs exactly the way each site builds them and asserts byte equality. A future refactor that touches one site without the other surfaces here at compile/test time rather than at runtime. |
| TEST52 | cartridges/datacartridge | rust | `test0052_save_as_txt_cap_present_in_manifest` | src/main.rs:2671 | The save-as-txt cap is registered in the manifest builder (`build_manifest`). Verify it's actually present there with the right shape — input urn, output urn, command. A regression that drops the cap from the manifest would remove it from the cartridge's cap-graph contribution entirely, and the planner would never reach a `.txt` target via this cartridge. |
| TEST53 | cartridges/datacartridge | rust | `test0053_coerce_to_boolean_strict` | src/main.rs:2488 | TEST0053: Coerce to boolean — strict spellings both ways, hard error on everything else (the missing inverse of boolean→*). |
| TEST54 | cartridges/datacartridge | rust | `test0054_csv_bom_stripped` | src/main.rs:2365 | TEST0054: UTF-8 BOM never leaks into CSV header keys. |
| TEST60 | cartridges/datacartridge | rust | `test0060_program_schema_and_ops_in_lockstep` | src/transform.rs:559 | TEST0060: the schema constant itself is valid JSON and every op kind it names deserializes — schema and executor cannot drift. |
| TEST61 | cartridges/datacartridge | rust | `test0061_program_end_to_end` | src/transform.rs:636 | TEST0061: an end-to-end program — the "describe it like a chatbot" example made concrete: filter active users, lowercase emails, keep two fields, sort by name, take the top 2. |
| TEST62 | cartridges/datacartridge | rust | `test0062_hard_errors_name_the_problem` | src/transform.rs:662 | TEST0062: contract violations are hard errors naming the exact problem — never silent skips. |
| TEST63 | cartridges/datacartridge | rust | `test0063_filter_semantics` | src/transform.rs:697 | TEST0063: filter predicate semantics — absent fields fail comparisons but satisfy not_exists; ordering works on numbers and strings; contains covers substrings and array membership. |
| TEST64 | cartridges/datacartridge | rust | `test0064_sort_total_order` | src/transform.rs:723 | TEST0064: sort is total and deterministic across mixed and absent values (absent < null < bool < number < string). |
| TEST65 | cartridges/datacartridge | rust | `test0065_schema_constrains_read_op_fields_to_input` | src/transform.rs:594 | TEST0065: the dynamic schema constrains every existing-field reference to the input's field names, so a read op over a non-existent field is undecodable; `set_field` stays open so a NEW field can still be added. |
| TEST70 | cartridges/datacartridge | rust | `test0070_valid_json_zero_repairs` | src/repair.rs:567 | TEST0070: strict JSON round-trips untouched — repair is idempotent on valid input (zero actions, identical value). |
| TEST71 | cartridges/datacartridge | rust | `test0071_common_breakages_repaired_and_recorded` | src/repair.rs:583 | TEST0071: the common breakages LLM/hand-written JSON exhibits, each repaired AND recorded. |
| TEST72 | cartridges/datacartridge | rust | `test0072_truncated_documents_close` | src/repair.rs:613 | TEST0072: truncated documents close cleanly with the truncation recorded — the exact shape a cut-off model response produces. |
| TEST73 | cartridges/datacartridge | rust | `test0073_unguessable_is_a_hard_error` | src/repair.rs:623 | TEST0073: what cannot be repaired without guessing is a HARD error naming the position — never a silent guess. |
| TEST74 | cartridges/datacartridge | rust | `test0074_csv_repair` | src/repair.rs:637 | TEST0074: CSV — BOM stripped, short rows padded (recorded), long rows fatal with the row number. |
| TEST291 | cartridges/datacartridge | rust | `test0291_stream_meta_title_extracts_text_value` | src/semantic.rs:1988 | `stream_meta_title` returns the inner string when meta carries a `title` Text value — the empty-vs-non-empty distinction the writer's filename derivation relies on. |
| TEST292 | cartridges/datacartridge | rust | `test0292_stream_meta_title_none_when_meta_absent` | src/semantic.rs:1996 | Absent meta returns `None`, not `Some("")`. |
| TEST293 | cartridges/datacartridge | rust | `test0293_stream_meta_title_none_when_key_missing` | src/semantic.rs:2002 | Meta present but no `title` key: also `None`. |
| TEST294 | cartridges/datacartridge | rust | `test0294_stream_meta_title_none_when_not_text` | src/semantic.rs:2010 | A non-text `title` returns `None` rather than coercing. |
| TEST295 | cartridges/datacartridge | rust | `test0295_decision_record_carries_declared_fields_exactly` | src/semantic.rs:2019 | `build_decision_record` emits all six declared fields (the schema's `additionalProperties: false` forbids more). |
| TEST296 | cartridges/datacartridge | rust | `test0296_decision_record_with_empty_title_keeps_field_present` | src/semantic.rs:2033 | Empty title is a legitimate value (input meta absent — honest absence); the wire shape still carries the field. |
| TEST297 | cartridges/datacartridge | rust | `test0297_parse_scale_strict` | src/semantic.rs:2072 | Scale parsing accepts min..max integers only. |
| TEST298 | cartridges/datacartridge | rust | `test0298_parse_target_set_strict` | src/semantic.rs:2084 | Target-set parsing requires a JSON object with non-empty names AND descriptions, returned alphabetically by name. |
| TEST299 | cartridges/datacartridge | rust | `test0299_verify_quotes_grounded` | src/semantic.rs:2101 | Grounding enforcement — verbatim quotes pass (including across line-wrap differences); fabricated or empty quotes are hard errors. |
| TEST300 | cartridges/datacartridge | rust | `test0300_inference_params_backstop_to_declared_defaults` | src/semantic.rs:2124 | With no arg streams supplied (the capdag-CLI / direct-invocation path), every param backstops to its declared cap-definition default. Concrete values are asserted so this guards the ACTUAL defaults — notably that `max_tokens` is the fabric `4096`, never the old hardcoded `2048` that truncated the JSON mid-string. |
| TEST301 | cartridges/datacartridge | rust | `test0301_inference_params_read_supplied_values` | src/semantic.rs:2145 | A supplied arg stream (the normal engine-delivered path) is read and parsed; params not supplied still backstop to their defaults. |
| TEST302 | cartridges/datacartridge | rust | `test0302_inference_params_malformed_value_fails_hard` | src/semantic.rs:2159 | A supplied-but-unparseable value fails hard — it exposes bad input rather than silently substituting the default over a malformed override. |
| TEST303 | cartridges/datacartridge | rust | `test0303_llm_caps_declare_inference_args` | src/semantic.rs:2186 | Every semantic cap declares all nine configurable inference args, so the engine surfaces and delivers them. A cap that forgot `add_llm_args` would silently lose its tunable inference params (and, with the reader's backstop, run on defaults with no way to override) — this catches that. |
| TEST304 | cartridges/datacartridge | rust | `test0304_optional_argument_invalid_utf8_is_attributed` | src/semantic.rs:2172 | A present but non-UTF-8 optional argument is invalid input, never indistinguishable from an absent argument that legitimately defaults. |
| TEST1859 | cartridges/datacartridge | rust | `test1859_advertised_caps_resolve_in_catalog` | src/main.rs:2171 | TEST1859: every cap URN this cartridge advertises must resolve in the pinned fabric catalog. A drifted/bare-marker URN absent from the catalog is silently dropped by LiveCapFab at runtime; this guard turns that into a hard failure naming the exact URN. `get_cap` consults the same canonical manifest map LiveCapFab resolves against. Network-mediated against the env-configured registry (set by `sdx test`), pinned at FABRIC_MANIFEST_VERSION. The identity cap is engine-provided, not a catalog entry, so it is excluded. |
| TEST11014 | cartridges/datacartridge | rust | `test11014_temperature_seed_only_on_generate_json` | src/semantic.rs:2208 | Only the free-generation cap exposes temperature/seed as configurable args; judgment caps keep determinism intrinsic and must NOT expose them. |

## Unnumbered

| Repository | Language | Test | Location |
|---|---|---|---|
| cartridges/datacartridge | rust | `test_guard_schema_complexity` | src/main.rs:2130 |
| cartridges/datacartridge | rust | `test0292b_decision_record_envelope_and_threshold` | src/semantic.rs:2043 |
| cartridges/datacartridge | rust | `test0293b_parse_label_set_strict` | src/semantic.rs:2057 |
