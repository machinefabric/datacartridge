# datacartridge's proved model

How forecasts are scored and how ways of forecasting are tried on a series' history, as MarketOS decides them. The Rust that makes these decisions is generated from these files by lungo:
- Brier scores, tenths of the scale, and means;
- how a statement about a change settles;
- how a forecaster's word is read through its record;
- where a trial's stretches are cut;
- which way is chosen on what came before.

The modules are MarketOS's (`crates/mos-formal/lean/Mos` in marketos): `Mos.Forecast` and `Mos.Hindcast`, copied unchanged and still named so, so the two stay diffable. A change to one is made to both.

Regenerate after changing the model: run `lungo generate` at the repository root. `lungo generate --verify` fails when the checked-in `crates/mos-formal/src/generated` is not what the model generates.
