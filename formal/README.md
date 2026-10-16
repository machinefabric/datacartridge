# datacartridge's proved model

How forecasts are scored and how ways of forecasting are tried on a series' history. The Rust that makes these decisions is generated from these files by lungo:
- Brier scores, tenths of the scale, and means;
- how a statement about a change settles;
- how a forecaster's word is read through its record;
- where a trial's stretches are cut;
- which way is chosen on what came before.

The modules are datacartridge's: `Mos.Forecast` and `Mos.Hindcast`, still named as in MarketOS, which is to take them from here rather than keep its own.

Regenerate after changing the model: run `lungo generate` at the repository root. `lungo generate --verify` fails when the checked-in `crates/datacartridge-formal/src/generated` is not what the model generates.
