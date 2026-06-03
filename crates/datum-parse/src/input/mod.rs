//! `input/` — port of datum's `src/input/` argparse layer.
//!
//! This module owns the argument-parsing primitives feeding the bespoke
//! CLI parser. Per-submodule mapping to the original TS files:
//!
//! | TS                      | Rust                              | Status |
//! |-------------------------|-----------------------------------|--------|
//! | `consolidateKeys.ts`    | [`consolidate_keys`]              | ported |
//! | `flexiblePositional.ts` | [`flexible_positional`]           | ported |
//! | `dataArgs.ts`           | [`data_args`]                     | ported |
//! | `timeArgs.ts`           | [`time_args`]                     | ported |
//! | `durationArgs.ts`       | [`duration_args`]                 | ported |
//! | `fieldArgs.ts`          | [`field_args`]                    | ported |
//! | `outputArgs.ts`         | [`output_args`]                   | ported |
//! | `quickIdArg.ts`         | [`quick_id_arg`]                  | ported |
//! | —                       | [`flag_registry`]                 | Rust-only |
//! | —                       | [`matcher`]                       | Rust-only |
//!
//! See `docs/parity-audit/parsing.md` for the full behavior inventory.

pub mod consolidate_keys;
pub mod data_args;
pub mod duration_args;
pub mod field_args;
pub mod flag_registry;
pub mod flexible_positional;
pub mod matcher;
pub mod output_args;
pub mod quick_id_arg;
pub mod time_args;

pub use consolidate_keys::consolidate_keys;
pub use data_args::{handle_data_args, infer_type, DataArgs, DatumData};
pub use duration_args::{DurationArgs, DurationIntent};
pub use field_args::FieldArgs;
pub use flag_registry::{
    lift, local_optional, local_switch, local_value, FlagAction, FlagSpec, LiftedSpec, ValueMode,
};
pub use flexible_positional::{flexible_positional, FlexibleTarget};
pub use matcher::{parse_matcher_token, Matcher, MatcherOp};
pub use output_args::{OutputArgs, OutputResolved, Show};
pub use quick_id_arg::{OnAmbiguous, QuickIdArg};
pub use time_args::{TimeArgs, TimeFromArgs};
