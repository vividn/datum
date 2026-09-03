//! What ambiguous time/date input is resolved *against*.
//!
//! `sunday`, `apr29` and friends name a recurring point, not a moment.
//! Turning one into a date needs two things the string itself can't
//! supply: a reference moment to measure from, and a direction to search
//! in when the user gave no trailing `+`/`-` marker. [`TimeCtx`] carries
//! both, so `parse_time` / `parse_date` stay pure and callers can
//! anchor them on something other than the wall clock — a second `-t`
//! resolving against the first, or a data key resolving against its
//! entry's occur-time.

use chrono::NaiveDateTime;

/// Which way an ambiguous reference resolves when the user supplies no
/// trailing `+` / `-` marker.
///
/// An explicit marker always wins, and it searches *strictly*: `apr29-`
/// on April 29 is last year. A default is non-strict, so the reference
/// day itself still counts as "the most recent past".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DefaultDirection {
    /// Most recent occurrence at or before the reference. The default for
    /// backward-looking args — `--since`, `-t`, `-d`.
    #[default]
    Past,
    /// Whichever of the past/future occurrence is closer; ties go to the
    /// past. Used for data args, where `finish_time=sunday` means the
    /// Sunday nearest the entry rather than one a week out.
    Nearest,
    /// Next occurrence at or after the reference.
    Future,
}

/// Reference point + default direction for the argument parsers.
#[derive(Debug, Clone, Copy, Default)]
pub struct TimeCtx {
    /// Local wall-clock moment ambiguous input is measured from. `None`
    /// when no clock is available (a wasm build with no explicit anchor);
    /// year-less and word forms then fail to parse rather than guess.
    pub reference: Option<NaiveDateTime>,
    pub default_direction: DefaultDirection,
}

impl TimeCtx {
    /// Anchored on the host wall clock, facing the past. On builds without
    /// the `native` feature there is no clock, so this is unanchored — use
    /// [`TimeCtx::at`] there.
    pub fn now() -> Self {
        TimeCtx {
            reference: host_now(),
            default_direction: DefaultDirection::Past,
        }
    }

    /// Anchored on an explicit local wall-clock moment.
    pub fn at(reference: NaiveDateTime) -> Self {
        TimeCtx {
            reference: Some(reference),
            default_direction: DefaultDirection::Past,
        }
    }

    /// Re-anchor on a more specific moment when the caller has one (an
    /// entry's own occur-time, say). `None` keeps the existing anchor.
    pub fn anchored_at(mut self, reference: Option<NaiveDateTime>) -> Self {
        if reference.is_some() {
            self.reference = reference;
        }
        self
    }

    /// Set the direction ambiguous input resolves in absent a `+`/`-`.
    pub fn facing(mut self, default_direction: DefaultDirection) -> Self {
        self.default_direction = default_direction;
        self
    }
}

#[cfg(feature = "native")]
fn host_now() -> Option<NaiveDateTime> {
    Some(crate::time::clock::now().local_time().naive_local())
}

#[cfg(not(feature = "native"))]
fn host_now() -> Option<NaiveDateTime> {
    None
}
