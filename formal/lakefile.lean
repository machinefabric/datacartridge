import Lake
open Lake DSL

package datacartridgeFormal where

-- lungo's Lean library: the specifications and claims lungo carries into the generated
-- package. The workspace's lungo, at the release capdag pins.
require lungo from "../../../lungo/lungo-rs/lean"

@[default_target]
lean_lib Mos where
