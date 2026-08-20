# datacartridge

`datacartridge` is MachineFabric's internal structured-data and semantic-primitive cartridge. It converts compatible JSON, YAML, CSV, TSV, PSV, XML, and TOML values; coerces supported scalar types; inspects data formats; and implements the judgment-envelope operations registered by its embedded cap snapshots.

This page is for cartridge maintainers. The embedded `cap-snapshots.json`, checked against the pinned fabric registry, is the authoritative capability reference.

## Build and test

Run the workspace tool from the superproject root:

```bash
sdx cartridge datacartridge
sdx test --edition website cartridges
```

Do not build the crate directly: the build requires registry versions, channel, and environment values exported by `sdx`.

## Implementation map

- `src/main.rs` registers format conversion, type coercion, sequence, and persistence operations.
- `src/adapter.rs` classifies supported structured-data inputs without claiming formats it cannot verify.
- `src/repair.rs` implements repair behavior.
- `src/semantic.rs` implements schema-guided generation, extraction, decisions, comparison, classification, scoring, verification, routing, normalization, grounded question answering, explanation, and summarization.
- `src/transform.rs` contains structured transformations.

Inputs that cannot be parsed or safely coerced fail explicitly. The cartridge does not silently relabel incompatible data.
