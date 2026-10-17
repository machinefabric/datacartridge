# datacartridge's proved model

How forecasts are scored and how ways of forecasting are tried on a series' history. The Rust that makes these decisions is generated from these files by lungo:
- Brier scores, tenths of the scale, and means;
- how a statement about a change settles;
- how a forecaster's word is read through its record;
- where a trial's stretches are cut;
- which way is chosen on what came before.

The modules are `Datacartridge.Forecast` and `Datacartridge.Hindcast`, named for this cartridge so a package that requires it beside other packages has no module of its name but these.

`lakefile.lean` is rendered from `lakefile.lean.wstemplate`: in local mode lungo's Lean library is the workspace's working copy, otherwise lungo's release by URL, so the package builds wherever it is required from. `lake-manifest.json` is Lake's record of what it resolved and is not kept.

Regenerate after changing the model: run `lungo generate` at the repository root. `lungo generate --verify` fails when the checked-in `crates/datacartridge-formal/src/generated` is not what the model generates.
