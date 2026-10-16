# datacartridge

`datacartridge` is MachineFabric's internal structured-data and semantic-primitive cartridge. It converts compatible JSON, YAML, CSV, TSV, PSV, XML, and TOML values; coerces supported scalar types; inspects data formats; runs steps over time series; and implements the judgment-envelope operations registered by its embedded cap snapshots.

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
- `src/transform.rs` contains structured transformations: the record program language, which edit-records has the constrained-generation peer write and transform-records (`src/records.rs`) runs as given.
- `src/series.rs` implements the steps over time series: resample, change, lag, rolling statistics, align, combine, filter (a router: what passes, and what does not when anything does not), summarize, describe, regress (with its residuals as a second output) and correlate. They are built on datacartridge's own crates in `crates/`: `datacartridge-core` (errors and instants) and `datacartridge-quant` (statistics, linear algebra, option maths and the steps). marketcartridge takes both from here, and MarketOS is to rather than keep its own copies.

- `src/forecasts.rs` scores settled forecasts (score-forecasts) and tries ways of forecasting a change in a series on its history (try-forecasting-ways): MarketOS's scoring and hindcast, their decisions made by datacartridge's proved model (`formal/`: `Mos.Forecast`, `Mos.Hindcast`), generated into `crates/datacartridge-formal` by lungo and checked in (`lungo generate --verify`).

The caps that generate with the constrained-generation peer (the semantic judgment caps and edit-records) share one pool of capacity 1. Every other cap is deterministic and runs unpooled, so a conversion or a series step never waits behind generation.

Inputs that cannot be parsed or safely coerced fail explicitly. The cartridge does not silently relabel incompatible data.
