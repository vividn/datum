//! `input/` — port of datum's `src/input/` argparse layer.
//!
//! This module owns the bespoke parser that (in R2 step 3/N) will replace
//! clap. Per-submodule mapping to the original TS files:
//!
//! | TS                      | Rust                              | Status |
//! |-------------------------|-----------------------------------|--------|
//! | `consolidateKeys.ts`    | [`consolidate_keys`]              | ported |
//! | `flexiblePositional.ts` | [`flexible_positional`]           | ported |
//! | `dataArgs.ts`           | [`data_args`]                     | stub (step 2b/N) |
//! | `timeArgs.ts`           | [`time_args`]                     | stub (step 2b/N) |
//! | `durationArgs.ts`       | [`duration_args`]                 | stub (step 2b/N) |
//! | `fieldArgs.ts`          | [`field_args`]                    | stub (step 2b/N) |
//! | `outputArgs.ts`         | [`output_args`]                   | stub (step 2b/N) |
//! | `quickIdArg.ts`         | [`quick_id_arg`]                  | stub (step 2b/N) |
//! | `mainArgs.ts`           | [`main_args`]                     | stub (step 3/N)  |
//! | `defaults.ts`           | [`defaults`]                      | stub (intentionally empty upstream) |
//!
//! See `docs/parity-audit/parsing.md` for the full behavior inventory.

pub mod consolidate_keys;
pub mod data_args;
pub mod defaults;
pub mod duration_args;
pub mod field_args;
pub mod flexible_positional;
pub mod main_args;
pub mod output_args;
pub mod quick_id_arg;
pub mod time_args;

pub use consolidate_keys::consolidate_keys;
pub use flexible_positional::{flexible_positional, FlexibleTarget};
