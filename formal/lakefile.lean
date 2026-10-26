import Lake
open Lake DSL

package datacartridgeFormal where

-- lungo's Lean library: the specifications and claims lungo carries into the generated
-- package, at the lungo the generated crate's runtime is.
-- The release, by URL, so the package builds wherever it is required from, not only inside
-- this workspace.
require lungo from git "https://github.com/machinefabric/lungo-rs" @ "v1.117.0" / "lean"

-- datacartridge's model: forecasts and how they are scored (Datacartridge.Forecast), and
-- trying ways of forecasting on a series' history (Datacartridge.Hindcast).
@[default_target]
lean_lib Datacartridge where
