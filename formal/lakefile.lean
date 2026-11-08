import Lake
open Lake DSL

package datacartridgeFormal where

-- lungo's Lean library: the specifications and claims lungo carries into the generated
-- package, at the lungo the generated crate's runtime is.
-- MFR_LOCAL_LIBS was set when this was rendered: the working copy in this workspace rather
-- than a published release. Re-render without it before committing or releasing — a path
-- resolves on this machine only.
require lungo from "../../../lungo/lungo-rs/lean"

-- datacartridge's model: forecasts and how they are scored (Datacartridge.Forecast), and
-- trying ways of forecasting on a series' history (Datacartridge.Hindcast).
@[default_target]
lean_lib Datacartridge where
