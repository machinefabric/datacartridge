//! The proved decisions datacartridge's forecasts make.
//!
//! The Lean modules in `formal/` are datacartridge's; lungo generated the Rust
//! in [`spec`] from them (`lungo generate`, checked by `lungo generate --verify`). The other
//! crates call these functions; they do not re-implement them.

/// The generated module: Lean namespace `Mos.X` is `spec::x`.
#[allow(warnings, clippy::all, non_snake_case, non_camel_case_types, non_upper_case_globals)]
pub mod spec {
    include!("generated/datacartridgeFormal/datacartridgeFormal.rs");
}

/// The runtime types generated signatures use (`List`, `Nat`, `Int`).
pub use lungo;
pub use lungo::{Int, List, Nat};
