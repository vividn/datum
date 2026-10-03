#![doc = include_str!("../README.md")]

//! Pure layout for daydumb's dayview and nowview.
//!
//! [`dayview::compute_dayview`] takes typed day/field inputs plus a
//! [`dayview::DayviewConfig`] and computes a [`dayview::DayviewLayout`]:
//! pixel-positioned blocks, points, and axis ticks. No I/O, no datum-core.
//!
//! Renderers (the CLI's SVG and terminal output) consume the
//! layout struct without re-doing math; datum.day runs that same
//! CLI as WASI, draws its SVG, and reads `--show json`.

pub mod dayview;
pub mod dayview_svg;
pub mod nowview;
pub mod nowview_svg;
pub mod nowview_terminal;
