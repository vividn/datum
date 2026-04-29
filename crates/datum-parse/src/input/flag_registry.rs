//! Central flag registry — per-module `FLAG_SPECS` arrays that the bespoke
//! parser unions into a per-command lookup table. Compound-short handling
//! (e.g. `-yyt 3`) and value-form parsing (`--long VALUE` / `--long=VALUE`)
//! live in the parser; modules just declare what they own.
//!
//! # Why generic `T`?
//!
//! Each module's specs target its own struct (e.g. `FlagSpec<TimeArgs>`).
//! `parse.rs` lifts those specs onto a per-command target struct (e.g.
//! `LiftedSpec<LogParsing>`) via [`lift`], using a field-accessor closure
//! that walks from the parent down to the module. The lift step is paid
//! once per command at parse-startup; the heap allocation is negligible
//! against argv-walking cost.
//!
//! See `da-26e` for the architectural rationale.
//!
//! # Behavior
//!
//! * [`FlagAction::Switch`] — bare bool / count flag. Setter fires once per
//!   occurrence; stackable shorts (e.g. `-yy` → `-y -y`) are pre-expanded
//!   by `alias::expand_stacked_flags`.
//! * [`FlagAction::Value`] — value-taking flag. The parser handles the
//!   long/short attached/detached forms and hands the resolved value to
//!   the setter.
//!
//! # Hyphen-prefixed values
//!
//! Some flags accept values that start with `-` (e.g. `--time=-5`). The
//! parser uses `=`-attached form to disambiguate; setters never see the
//! raw token, so they don't need to care.

use crate::error::Result;

/// What to do when the parser matches a flag from this spec.
pub enum FlagAction<T> {
    /// Switch / count flag (no value). Setter fires once per occurrence.
    Switch(fn(&mut T) -> Result<()>),
    /// Value-taking flag. Setter receives the resolved value (from the
    /// attached `=VALUE` form or the next argv token).
    Value(fn(&mut T, &str) -> Result<()>),
    /// Optional-value flag — accepts the bare form (no value) and the
    /// attached `=VALUE` form, but does NOT consume a detached next
    /// token. Mirrors `-L`/`--last-state` and other "presence sets one
    /// thing, presence+value sets another" flags.
    OptionalValue(fn(&mut T, Option<&str>) -> Result<()>),
}

/// One flag declaration: what the user types (`short` / `long`) and what
/// happens when the parser sees it.
pub struct FlagSpec<T: 'static> {
    pub short: Option<char>,
    pub long: Option<&'static str>,
    pub action: FlagAction<T>,
}

impl<T: 'static> FlagSpec<T> {
    pub const fn new(
        short: Option<char>,
        long: Option<&'static str>,
        action: FlagAction<T>,
    ) -> Self {
        Self { short, long, action }
    }
}

/// Argv-side value form for a [`LiftedSpec`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueMode {
    /// No value; bare presence only. Attached `=VALUE` is silently
    /// ignored (matching pre-registry behaviour).
    None,
    /// Required value; consumed from the attached form or the next argv
    /// token if detached.
    Required,
    /// Optional attached value; bare → setter sees `None`, attached
    /// `=VALUE` → setter sees `Some(value)`. A detached next token is
    /// NOT consumed.
    OptionalAttached,
}

/// Type alias for the heap-allocated `apply` closure on [`LiftedSpec`].
pub type ApplyFn<P> = Box<dyn Fn(&mut P, Option<&str>) -> Result<()>>;

/// A spec lifted from a module-local target (`M`) to a parent target (`P`).
/// The `apply` closure threads through the field accessor.
pub struct LiftedSpec<P: 'static> {
    pub short: Option<char>,
    pub long: Option<&'static str>,
    pub value_mode: ValueMode,
    pub apply: ApplyFn<P>,
}

/// Lift a module-local [`FlagSpec`] onto a parent target via a field
/// accessor. The accessor must be `Copy` (a function pointer is fine) so
/// the resulting closure stays `'static`.
pub fn lift<P, M>(accessor: fn(&mut P) -> &mut M, spec: &FlagSpec<M>) -> LiftedSpec<P>
where
    P: 'static,
    M: 'static,
{
    let (value_mode, apply): (ValueMode, ApplyFn<P>) = match spec.action {
        FlagAction::Switch(setter) => (
            ValueMode::None,
            Box::new(move |parent, _val| setter(accessor(parent))),
        ),
        FlagAction::Value(setter) => (
            ValueMode::Required,
            Box::new(move |parent, val| {
                let v = val.unwrap_or("");
                setter(accessor(parent), v)
            }),
        ),
        FlagAction::OptionalValue(setter) => (
            ValueMode::OptionalAttached,
            Box::new(move |parent, val| setter(accessor(parent), val)),
        ),
    };
    LiftedSpec {
        short: spec.short,
        long: spec.long,
        value_mode,
        apply,
    }
}

/// Build a one-off [`LiftedSpec`] in-place — handy for command-specific
/// extras that don't have a module-level spec table to draw from.
pub fn local_switch<P: 'static>(
    short: Option<char>,
    long: Option<&'static str>,
    setter: fn(&mut P) -> Result<()>,
) -> LiftedSpec<P> {
    LiftedSpec {
        short,
        long,
        value_mode: ValueMode::None,
        apply: Box::new(move |parent, _val| setter(parent)),
    }
}

pub fn local_value<P: 'static>(
    short: Option<char>,
    long: Option<&'static str>,
    setter: fn(&mut P, &str) -> Result<()>,
) -> LiftedSpec<P> {
    LiftedSpec {
        short,
        long,
        value_mode: ValueMode::Required,
        apply: Box::new(move |parent, val| {
            let v = val.unwrap_or("");
            setter(parent, v)
        }),
    }
}

pub fn local_optional<P: 'static>(
    short: Option<char>,
    long: Option<&'static str>,
    setter: fn(&mut P, Option<&str>) -> Result<()>,
) -> LiftedSpec<P> {
    LiftedSpec {
        short,
        long,
        value_mode: ValueMode::OptionalAttached,
        apply: Box::new(setter),
    }
}
