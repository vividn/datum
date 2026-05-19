//! Per-side alias body parsing + global slot-list construction.
//!
//! da-gh0.B foundation. Given the tokens for one side of a multi-entry
//! invocation (post comma split, post recursive alias resolution), parse
//! them into a structured [`AliasBody`]. Then walk an `N`-sided list of
//! bodies through [`build_slot_list`] to produce the global slot list
//! that maps invocation positionals onto per-side bindings.
//!
//! Rules covered (from da-gh0):
//! - **Rule 1** — shared `-k`/`-K` keys propagate their value across every
//!   side that declares them.
//! - **Rule 5** — `-K` (identity/slug) keys share and propagate exactly
//!   like lowercase `-k`.
//! - **Rule 10** — a `.` positional on a shared slot skips on all sides.
//! - **Rule 12** — three `-k` declaration forms:
//!   - `-k key`     = required (error if unfilled)
//!   - `-k key=`    = optional with no default (silently skipped)
//!   - `-k key=val` = default `val` (auto-fills if unfilled)
//!
//! Verb-derived slots (Section L of the bead corpus):
//! - `switch` / `start` with no bound state in body → `[state, dur]`
//! - `switch` / `start` with a bound state in body → `[dur]`
//! - `occur` / `add` / `note` / terminal verbs → `[]`
//!
//! Slot list construction is a 2-pass walk: collect each side's
//! `-k`/`-K` declarations in declaration order, then append that side's
//! verb-derived slots. As keys repeat across sides they're deduplicated
//! at their leftmost position; the slot's `sides` list records every
//! side that declares them (so the walker can propagate the chosen
//! value back to each one).

use crate::error::{ParseError, Result};
use crate::input::consolidate_keys::consolidate_keys;

use super::expand::KNOWN_SUBCOMMANDS;

/// Which trichotomy form a `-k key[=val]` declaration takes. Rule 12.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyForm {
    /// `-k key` — must be filled (positional or `key=val`) or
    /// `MissingRequiredKey` errors at write time.
    Required,
    /// `-k key=` — optional, no default. If unfilled, silently skipped.
    Optional,
    /// `-k key=val` — default value applies when unfilled. A positional
    /// `.` falls back to this default. Explicit `key=.` likewise.
    WithDefault(String),
}

impl KeyForm {
    /// Reconstruct the canonical spec string the `data_args` layer
    /// already understands (`"key"`, `"key="`, `"key=val"`). Lets us
    /// hand a slot's form straight to `consolidate_keys` /
    /// `handle_data_args` without inventing new types downstream.
    pub fn to_spec(&self, key: &str) -> String {
        match self {
            KeyForm::Required => key.to_string(),
            KeyForm::Optional => format!("{key}="),
            KeyForm::WithDefault(v) => format!("{key}={v}"),
        }
    }
}

/// A single `-k key[=val]` or `-K key[=val]` declaration from a side's
/// alias body. `is_identity` is `true` when sourced from `-K` (so the
/// caller can build the slug template). The trichotomy form lives in
/// [`KeyForm`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyDecl {
    pub key: String,
    pub form: KeyForm,
    pub is_identity: bool,
}

/// A single `-F field[=val]` declaration from a side's alias body. The
/// trichotomy form (required / optional / defaulted) mirrors [`KeyDecl`]
/// exactly — see [`KeyForm`]. Unlike `-k`/`-K`, `-F` resolves to the
/// entry's field rather than a data key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDecl {
    pub name: String,
    pub form: KeyForm,
}

/// Parsed view of one side's tokens. Produced by [`parse_alias_body`].
///
/// The bound `field` / `bound_state` are recorded when the verb-derived
/// positional shape predicts them (`switch`/`start` see the second
/// token as field, the third as state; verbless aliases like `vbp` are
/// out of scope for the body parser — they're already substituted by
/// the resolver before this layer sees them).
///
/// `bound_keyed` collects `key=val` pairs embedded in the body itself
/// (e.g. `note word srcLang=pt pos=verb`). They behave like the result
/// of a positional `key=val` in `handleDataArgs`: the key's slot is
/// pre-filled at parse time.
///
/// `bound_positionals` holds extra bare positionals after the verb's
/// state binding. These pre-fill the leftmost unfilled slot for the
/// side at walker time (Rule 12 ordering).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AliasBody {
    pub verb: Option<String>,
    pub field: Option<String>,
    /// Only meaningful for `switch`/`start`. When `Some`, no state slot
    /// is appended for this side.
    pub bound_state: Option<String>,
    pub key_decls: Vec<KeyDecl>,
    /// `-F field[=val]` declarations from the body, in declaration order.
    /// Each becomes a [`SlotKind::Field`] in the global slot list.
    pub field_decls: Vec<FieldDecl>,
    pub bound_keyed: Vec<(String, String)>,
    pub bound_positionals: Vec<String>,
    /// Any tokens we don't classify here (flags other than `-k`/`-K`,
    /// `--`, etc.). The body parser leaves them for later layers. Out
    /// of scope for da-gh0.B but preserved so callers can route them.
    pub remaining: Vec<String>,
}

impl AliasBody {
    /// Does this side carry a bound state in its body? Used by
    /// [`build_slot_list`] to decide whether a `state` verb-derived
    /// slot needs to be appended.
    fn has_bound_state(&self) -> bool {
        self.bound_state.is_some()
    }

    /// Verb-derived slot shape per Section L of the da-gh0 corpus.
    fn verb_slots(&self) -> Vec<VerbSlot> {
        let Some(verb) = self.verb.as_deref() else {
            return Vec::new();
        };
        match verb {
            "switch" | "start" => {
                if self.has_bound_state() {
                    vec![VerbSlot::Duration]
                } else {
                    vec![VerbSlot::State, VerbSlot::Duration]
                }
            }
            _ => Vec::new(),
        }
    }
}

/// Verb-derived slot identity (state vs duration). The slot list uses
/// the richer [`SlotKind`] which also covers `-k`-derived slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VerbSlot {
    State,
    Duration,
}

/// Parse one side's tokens into an [`AliasBody`]. The input is the
/// per-side argv produced by [`super::expand::split_argv_at_commas`]
/// (i.e. the argv minus the leading binary name and minus the comma
/// separators). Recursive alias resolution must have already
/// substituted the inner expansions — the body parser walks the
/// "leaf" token stream.
///
/// The grammar is intentionally minimal:
/// - Token 0 → verb (any [`KNOWN_SUBCOMMANDS`] entry). Non-verb leading
///   tokens land in `remaining` and produce no field/state binding.
/// - For `switch`/`start`: tokens 1 / 2 → field / bound_state when they
///   aren't flags or `key=val` pairs.
/// - For other verbs (`occur`, `add`, `note`, `end`, `stop`, …) → token 1
///   → field when present.
/// - `-k <spec>` / `-K <spec>` (and `--key=spec` long forms) → [`KeyDecl`].
/// - Bare `key=val` tokens (with the `=` not at the start) → bound_keyed.
/// - Bare `.` is preserved as a positional (so the walker can route it
///   per Rule 10 / da-1n61's use-default-or-skip).
/// - Everything else lands in `bound_positionals` or `remaining` depending
///   on whether it could plausibly bind a slot.
pub fn parse_alias_body(tokens: &[String]) -> Result<AliasBody> {
    let mut body = AliasBody::default();
    let mut i = 0;

    // Verb at position 0 — only when it's a known subcommand.
    if let Some(first) = tokens.first() {
        if KNOWN_SUBCOMMANDS.contains(&first.as_str()) {
            body.verb = Some(first.clone());
            i = 1;
        }
    }

    let is_switch_like = matches!(body.verb.as_deref(), Some("switch") | Some("start"));

    // Field token (verb's positional argument). Must be a bare,
    // non-flag, non-`key=val` token.
    if body.verb.is_some() {
        if let Some(tok) = tokens.get(i) {
            if is_field_token(tok) {
                body.field = Some(tok.clone());
                i += 1;
            }
        }
    }

    // Bound state — only for switch/start, only when the next token
    // looks like a bare positional value. `start field1 -k arg1` has
    // no bound state; `vertical = switch body vertical` does.
    if is_switch_like {
        if let Some(tok) = tokens.get(i) {
            if is_field_token(tok) {
                body.bound_state = Some(tok.clone());
                i += 1;
            }
        }
    }

    // Walk the remaining tokens, classifying each into one of:
    // `-k`/`-K` decl, bound_keyed (`key=val`), bound_positional (bare),
    // or remaining (flag we don't claim).
    while i < tokens.len() {
        let tok = &tokens[i];

        if let Some((is_identity, spec)) = match_key_short(tok) {
            // `-k <spec>` or `-K <spec>` — consume the next token as spec.
            let value = match spec {
                Some(s) => s.to_string(),
                None => {
                    i += 1;
                    tokens.get(i).cloned().ok_or_else(|| {
                        ParseError::InvalidArg(format!(
                            "alias body: `{tok}` requires a key spec but argv ended"
                        ))
                    })?
                }
            };
            body.key_decls.push(parse_key_spec(&value, is_identity));
            i += 1;
            continue;
        }

        if let Some(spec) = match_key_long(tok) {
            // `--key=<spec>` or `--slug-key=<spec>`; long-form-with-equals
            // is the only shape we accept here (separate-token long
            // flags can be added later if needed).
            let (is_identity, value) = spec;
            body.key_decls.push(parse_key_spec(value, is_identity));
            i += 1;
            continue;
        }

        if let Some(spec) = match_field_short(tok) {
            // `-F <spec>` or `-F<spec>` — declares the entry's field as
            // a slot with the trichotomy form. Consumes the next token
            // when the spec wasn't glued.
            let value = match spec {
                Some(s) => s.to_string(),
                None => {
                    i += 1;
                    tokens.get(i).cloned().ok_or_else(|| {
                        ParseError::InvalidArg(format!(
                            "alias body: `{tok}` requires a field spec but argv ended"
                        ))
                    })?
                }
            };
            body.field_decls.push(parse_field_spec(&value));
            i += 1;
            continue;
        }

        if let Some(spec) = match_field_long(tok) {
            body.field_decls.push(parse_field_spec(spec));
            i += 1;
            continue;
        }

        // Bare `key=val` (not at position 0 of the token, i.e. proper
        // assignment). Bound keyed pre-fill of a slot.
        if let Some((k, v)) = bare_key_value(tok) {
            body.bound_keyed.push((k.to_string(), v.to_string()));
            i += 1;
            continue;
        }

        // Recognised flag we don't claim here — preserve for caller.
        if tok.starts_with('-') && tok.as_str() != "." {
            body.remaining.push(tok.clone());
            i += 1;
            continue;
        }

        // Otherwise it's a positional value (including bare `.`).
        body.bound_positionals.push(tok.clone());
        i += 1;
    }

    Ok(body)
}

/// True for a token that could legitimately bind a verb's
/// field/state positional: not a flag, not a `key=val` pair, not
/// empty. We deliberately accept bare `.` so the walker can still see
/// it (it'll route as use-default-or-skip when it lands on a slot).
fn is_field_token(tok: &str) -> bool {
    !tok.is_empty() && !tok.starts_with('-') && !tok.contains('=')
}

/// `-k` / `-K` short-form match. Returns `Some((is_identity, None))`
/// for a separate spec token (`-k key`, `-K key`), `Some((is_identity,
/// Some(s)))` for a glued form (`-kkey`/`-Kkey`), `None` for non-match.
fn match_key_short(tok: &str) -> Option<(bool, Option<&str>)> {
    if tok == "-k" {
        return Some((false, None));
    }
    if tok == "-K" {
        return Some((true, None));
    }
    if let Some(rest) = tok.strip_prefix("-k") {
        if !rest.is_empty() && !rest.starts_with('-') {
            return Some((false, Some(rest)));
        }
    }
    if let Some(rest) = tok.strip_prefix("-K") {
        if !rest.is_empty() && !rest.starts_with('-') {
            return Some((true, Some(rest)));
        }
    }
    None
}

/// `--key=spec` / `--slug-key=spec` long-form match. Returns
/// `Some((is_identity, spec))` on match.
fn match_key_long(tok: &str) -> Option<(bool, &str)> {
    if let Some(rest) = tok.strip_prefix("--key=") {
        return Some((false, rest));
    }
    if let Some(rest) = tok.strip_prefix("--slug-key=") {
        return Some((true, rest));
    }
    None
}

/// `-F` short-form match. `Some(None)` for a separate spec token
/// (`-F field`), `Some(Some(s))` for a glued form (`-Ffield`), `None`
/// for non-match.
fn match_field_short(tok: &str) -> Option<Option<&str>> {
    if tok == "-F" {
        return Some(None);
    }
    if let Some(rest) = tok.strip_prefix("-F") {
        if !rest.is_empty() && !rest.starts_with('-') {
            return Some(Some(rest));
        }
    }
    None
}

/// `--field=spec` long-form match. Returns `Some(spec)` on match.
fn match_field_long(tok: &str) -> Option<&str> {
    tok.strip_prefix("--field=")
}

/// Parse one `-F` spec string into a [`FieldDecl`]. Mirrors
/// [`parse_key_spec`] — empty default (`name=`) is Optional, present
/// default (`name=val`) is WithDefault, no `=` is Required.
fn parse_field_spec(spec: &str) -> FieldDecl {
    match spec.split_once('=') {
        None => FieldDecl {
            name: spec.to_string(),
            form: KeyForm::Required,
        },
        Some((name, "")) => FieldDecl {
            name: name.to_string(),
            form: KeyForm::Optional,
        },
        Some((name, default)) => FieldDecl {
            name: name.to_string(),
            form: KeyForm::WithDefault(default.to_string()),
        },
    }
}

/// Parse one `-k`/`-K` spec string (`"key"`, `"key="`, `"key=val"`)
/// into a [`KeyDecl`]. Out-of-band `=`s in the value side stay intact:
/// `equation=1+2=3` declares `equation` with default `1+2=3`.
fn parse_key_spec(spec: &str, is_identity: bool) -> KeyDecl {
    match spec.split_once('=') {
        None => KeyDecl {
            key: spec.to_string(),
            form: KeyForm::Required,
            is_identity,
        },
        Some((key, "")) => KeyDecl {
            key: key.to_string(),
            form: KeyForm::Optional,
            is_identity,
        },
        Some((key, default)) => KeyDecl {
            key: key.to_string(),
            form: KeyForm::WithDefault(default.to_string()),
            is_identity,
        },
    }
}

/// Bare `key=value` predicate. Must have an `=`, and the key portion
/// must be non-empty and not start with `-`. Empty values (`"k="`) are
/// allowed and treated as the assignment delete in `handleDataArgs`.
fn bare_key_value(tok: &str) -> Option<(&str, &str)> {
    let (k, v) = tok.split_once('=')?;
    if k.is_empty() || k.starts_with('-') {
        return None;
    }
    Some((k, v))
}

// ─────────────────────────────────────────────────────────────────────
// Global slot list construction
// ─────────────────────────────────────────────────────────────────────

/// One slot in the global slot list. Holds enough info to (a) drive the
/// walker (`SlotKind::form` for the `.` fallback / required check) and
/// (b) tell the writer which sides receive the value once chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalSlot {
    pub kind: SlotKind,
    /// Side indices (0-based) that declared this slot. For shared keys
    /// the same slot appears on multiple sides; Rule 1 says the value
    /// propagates to all of them.
    pub sides: Vec<usize>,
}

/// What the slot represents. `Key` carries the form trichotomy so the
/// walker can apply defaults / detect required-but-unfilled. `State`
/// and `Duration` are the verb-derived positionals from Section L.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotKind {
    Key {
        name: String,
        form: KeyForm,
        is_identity: bool,
    },
    /// `-F`-derived slot. `name` is the placeholder identifier the user
    /// wrote (e.g. `-F field`). Same trichotomy as a key slot.
    Field {
        name: String,
        form: KeyForm,
    },
    State,
    Duration,
}

impl SlotKind {
    /// Display key — useful for error messages / debugging.
    pub fn name(&self) -> &str {
        match self {
            SlotKind::Key { name, .. } => name.as_str(),
            SlotKind::Field { name, .. } => name.as_str(),
            SlotKind::State => "<state>",
            SlotKind::Duration => "<dur>",
        }
    }
}

/// Walk each side's parsed body in order, producing the global slot
/// list. Shared `-k`/`-K` keys collapse to a single slot at their
/// leftmost position, with `sides` recording every contributor.
/// Verb-derived slots (Section L) append per-side after that side's
/// `-k`/`-K` slots — they're never shared across sides (each side
/// gets its own state/dur).
///
/// Conflicting forms on the same key across sides: Rule 1 says values
/// propagate, but the trichotomy form needs to be deterministic. We
/// pick the *strongest* form across declarations:
/// - any `Required` → slot is `Required`
/// - else any `WithDefault(v)` → slot is `WithDefault(v)` (leftmost
///   side's default wins for determinism)
/// - else (`Optional` everywhere) → `Optional`
pub fn build_slot_list(sides: &[AliasBody]) -> Result<Vec<GlobalSlot>> {
    let mut slots: Vec<GlobalSlot> = Vec::new();

    for (side_idx, body) in sides.iter().enumerate() {
        // Field slots first — `-F` declarations bind the entry's field,
        // which sits structurally before any data key. Shared by name
        // across sides (mirrors `-k`/`-K` Rule 1).
        for decl in &body.field_decls {
            if let Some(existing) = slots
                .iter_mut()
                .find(|s| matches!(&s.kind, SlotKind::Field { name, .. } if name == &decl.name))
            {
                if let SlotKind::Field {
                    form: existing_form,
                    ..
                } = &mut existing.kind
                {
                    *existing_form = merge_forms(existing_form.clone(), decl.form.clone());
                }
                if !existing.sides.contains(&side_idx) {
                    existing.sides.push(side_idx);
                }
                continue;
            }
            slots.push(GlobalSlot {
                kind: SlotKind::Field {
                    name: decl.name.clone(),
                    form: decl.form.clone(),
                },
                sides: vec![side_idx],
            });
        }

        // Consolidate this side's repeated `-k` declarations the same
        // way `handleDataArgs` already does (last-seen form wins in
        // first-seen position). This keeps `-k foo -k foo=10` legal
        // within a single side.
        let specs: Vec<String> = body
            .key_decls
            .iter()
            .map(|d| d.form.to_spec(&d.key))
            .collect();
        let consolidated = consolidate_keys(&specs);

        // Rebuild the per-side decl list from the consolidated specs,
        // preserving the `-K` identity bit from the original
        // declarations.
        let identity_map: std::collections::HashMap<&str, bool> = body
            .key_decls
            .iter()
            .map(|d| (d.key.as_str(), d.is_identity))
            .collect();

        for spec in &consolidated {
            let key_name = spec
                .split_once('=')
                .map(|(k, _)| k)
                .unwrap_or(spec.as_str());
            let form = match spec.split_once('=') {
                None => KeyForm::Required,
                Some((_, "")) => KeyForm::Optional,
                Some((_, v)) => KeyForm::WithDefault(v.to_string()),
            };
            let is_identity = identity_map.get(key_name).copied().unwrap_or(false);

            if let Some(existing) = slots
                .iter_mut()
                .find(|s| matches!(&s.kind, SlotKind::Key { name, .. } if name == key_name))
            {
                // Shared key — merge: strengthen form, record side.
                if let SlotKind::Key {
                    form: existing_form,
                    is_identity: existing_id,
                    ..
                } = &mut existing.kind
                {
                    *existing_form = merge_forms(existing_form.clone(), form);
                    *existing_id |= is_identity;
                }
                if !existing.sides.contains(&side_idx) {
                    existing.sides.push(side_idx);
                }
                continue;
            }

            slots.push(GlobalSlot {
                kind: SlotKind::Key {
                    name: key_name.to_string(),
                    form,
                    is_identity,
                },
                sides: vec![side_idx],
            });
        }

        // Append verb-derived slots for this side. Never shared.
        for verb_slot in body.verb_slots() {
            let kind = match verb_slot {
                VerbSlot::State => SlotKind::State,
                VerbSlot::Duration => SlotKind::Duration,
            };
            slots.push(GlobalSlot {
                kind,
                sides: vec![side_idx],
            });
        }
    }

    Ok(slots)
}

/// Combine two [`KeyForm`]s into the stronger one (see
/// [`build_slot_list`] doc).
fn merge_forms(a: KeyForm, b: KeyForm) -> KeyForm {
    match (&a, &b) {
        (KeyForm::Required, _) | (_, KeyForm::Required) => KeyForm::Required,
        (KeyForm::WithDefault(_), _) => a,
        (_, KeyForm::WithDefault(_)) => b,
        (KeyForm::Optional, KeyForm::Optional) => KeyForm::Optional,
    }
}

// ─────────────────────────────────────────────────────────────────────
// Slot walking
// ─────────────────────────────────────────────────────────────────────

/// Per-side resolved binding after [`walk_slots`]. Verb-derived
/// state/dur and `-k`/`-K` keyed values are surfaced separately so
/// the write layer can route them to the right entry columns.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SideBinding {
    pub state: Option<String>,
    pub dur: Option<String>,
    /// `-F`-resolved field for this side. `None` when no `-F` slot was
    /// filled and no default applied; callers fall back to the literal
    /// [`AliasBody::field`] (the body-bound positional field).
    pub field: Option<String>,
    /// `(key, value)` pairs in declaration order on this side. Includes
    /// values originally bound in the alias body (`bound_keyed`),
    /// values propagated from shared slots, defaults from `-k key=val`,
    /// and explicit `key=val` overrides walked from positionals.
    pub data: Vec<(String, String)>,
}

impl SideBinding {
    /// Convenience lookup for tests.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.data
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

/// Walk positional invocation values onto the global slot list,
/// producing one [`SideBinding`] per side.
///
/// Algorithm:
/// 1. Seed each side from its [`AliasBody`] (bound_state, bound_keyed).
/// 2. Drain `positionals` left-to-right. The next unfilled slot
///    consumes the value:
///    - Slot `Key { form, .. }`:
///      - `"."` → leave unfilled (Rule 10 — defaults apply at step 3,
///        or skip if `Optional`, error if `Required`). For shared keys
///        this means *all* declaring sides skip.
///      - other value → assign to every side in `slot.sides`.
///    - Slot `State` → assign to that side's `state`.
///    - Slot `Duration` → assign to that side's `dur`.
/// 3. After the walk, apply defaults for unfilled `WithDefault` slots
///    and error on unfilled `Required` slots.
///
/// Extra positionals past the end of the slot list raise
/// [`ParseError::ExtraData`] (Rule 3). Bound positionals in a side's
/// body pre-fill the leftmost unfilled slot for that side and aren't
/// counted as invocation positionals.
pub fn walk_slots(
    sides: &[AliasBody],
    slots: &[GlobalSlot],
    positionals: &[String],
) -> Result<Vec<SideBinding>> {
    let mut bindings: Vec<SideBinding> = sides
        .iter()
        .map(|body| SideBinding {
            state: body.bound_state.clone(),
            dur: None,
            field: None,
            data: body
                .bound_keyed
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        })
        .collect();

    // Track which slots are already filled (for shared keys, "filled"
    // means at least one side received the value). For unshared
    // verb-derived slots, "filled" is per-slot.
    let mut filled: Vec<bool> = vec![false; slots.len()];

    // Pre-fill from each side's bound positionals — they bind the
    // leftmost unfilled slot that names this side.
    for (side_idx, body) in sides.iter().enumerate() {
        for value in &body.bound_positionals {
            let Some(slot_idx) = next_unfilled_for_side(slots, &filled, side_idx) else {
                return Err(ParseError::ExtraData(vec![value.clone()]));
            };
            if value == "." {
                // Bound `.` defers to default / skip at finalization.
                filled[slot_idx] = true;
                continue;
            }
            assign_to_slot(&mut bindings, &slots[slot_idx], value)?;
            filled[slot_idx] = true;
        }
    }

    // Drain invocation positionals.
    let mut slot_cursor = 0usize;
    for value in positionals {
        // Advance to the next unfilled slot.
        while slot_cursor < slots.len() && filled[slot_cursor] {
            slot_cursor += 1;
        }
        if slot_cursor >= slots.len() {
            return Err(ParseError::ExtraData(vec![value.clone()]));
        }
        let slot = &slots[slot_cursor];
        if value == "." {
            // Rule 10: defer to default / skip for all declaring sides.
            filled[slot_cursor] = true;
            slot_cursor += 1;
            continue;
        }
        assign_to_slot(&mut bindings, slot, value)?;
        filled[slot_cursor] = true;
        slot_cursor += 1;
    }

    // Finalize: defaults for unfilled `WithDefault`, error on
    // unfilled `Required`.
    for (slot_idx, slot) in slots.iter().enumerate() {
        if filled[slot_idx] {
            continue;
        }
        match &slot.kind {
            SlotKind::Key { name, form, .. } => match form {
                KeyForm::Required => {
                    return Err(ParseError::MissingRequiredKey(name.clone()));
                }
                KeyForm::Optional => continue,
                KeyForm::WithDefault(default) => {
                    let value = default.clone();
                    for &side_idx in &slot.sides {
                        upsert_key(&mut bindings[side_idx].data, name, &value);
                    }
                }
            },
            SlotKind::Field { name, form } => match form {
                KeyForm::Required => {
                    return Err(ParseError::MissingRequiredKey(name.clone()));
                }
                KeyForm::Optional => continue,
                KeyForm::WithDefault(default) => {
                    for &side_idx in &slot.sides {
                        bindings[side_idx].field = Some(default.clone());
                    }
                }
            },
            SlotKind::State | SlotKind::Duration => {
                // No default for verb-derived slots; leaving unset is
                // legal — the write layer decides what `state=null` /
                // no-dur means downstream.
            }
        }
    }

    Ok(bindings)
}

fn next_unfilled_for_side(slots: &[GlobalSlot], filled: &[bool], side_idx: usize) -> Option<usize> {
    slots
        .iter()
        .enumerate()
        .find(|(i, s)| !filled[*i] && s.sides.contains(&side_idx))
        .map(|(i, _)| i)
}

fn assign_to_slot(bindings: &mut [SideBinding], slot: &GlobalSlot, value: &str) -> Result<()> {
    match &slot.kind {
        SlotKind::Key { name, .. } => {
            for &side_idx in &slot.sides {
                upsert_key(&mut bindings[side_idx].data, name, value);
            }
        }
        SlotKind::Field { .. } => {
            for &side_idx in &slot.sides {
                bindings[side_idx].field = Some(value.to_string());
            }
        }
        SlotKind::State => {
            for &side_idx in &slot.sides {
                bindings[side_idx].state = Some(value.to_string());
            }
        }
        SlotKind::Duration => {
            for &side_idx in &slot.sides {
                bindings[side_idx].dur = Some(value.to_string());
            }
        }
    }
    Ok(())
}

fn upsert_key(data: &mut Vec<(String, String)>, key: &str, value: &str) {
    if let Some(existing) = data.iter_mut().find(|(k, _)| k == key) {
        existing.1 = value.to_string();
    } else {
        data.push((key.to_string(), value.to_string()));
    }
}

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn t(ss: &[&str]) -> Vec<String> {
        ss.iter().map(|s| s.to_string()).collect()
    }

    fn body(ss: &[&str]) -> AliasBody {
        parse_alias_body(&t(ss)).expect("parse_alias_body")
    }

    fn slots(sides: &[AliasBody]) -> Vec<GlobalSlot> {
        build_slot_list(sides).expect("build_slot_list")
    }

    fn walk(sides: &[AliasBody], positionals: &[&str]) -> Vec<SideBinding> {
        let s = slots(sides);
        walk_slots(sides, &s, &t(positionals)).expect("walk_slots")
    }

    // ── Section E: -k declaration trichotomy ─────────────────────────────

    #[test]
    fn k_bare_key_is_required_errors_if_unfilled() {
        // `-k foo` with no positional and no default → MissingRequiredKey.
        let b = body(&["occur", "f", "-k", "foo"]);
        assert_eq!(
            b.key_decls,
            vec![KeyDecl {
                key: "foo".into(),
                form: KeyForm::Required,
                is_identity: false,
            }]
        );
        let s = slots(std::slice::from_ref(&b));
        let err = walk_slots(&[b], &s, &[]).unwrap_err();
        assert!(matches!(err, ParseError::MissingRequiredKey(k) if k == "foo"));
    }

    #[test]
    fn k_key_with_equals_no_default_silently_skipped() {
        // `-k foo=` unfilled → silently skipped, no error, no data key.
        let b = body(&["occur", "f", "-k", "foo="]);
        assert_eq!(b.key_decls[0].form, KeyForm::Optional);
        let bind = walk(&[b], &[]);
        assert_eq!(bind[0].data, Vec::<(String, String)>::new());
    }

    #[test]
    fn k_key_with_equals_value_uses_default_when_unfilled() {
        let b = body(&["occur", "f", "-k", "foo=bar"]);
        assert_eq!(b.key_decls[0].form, KeyForm::WithDefault("bar".to_string()));
        let bind = walk(&[b], &[]);
        assert_eq!(bind[0].get("foo"), Some("bar"));
    }

    #[test]
    fn k_key_with_equals_value_overrideable_by_positional() {
        let b = body(&["occur", "f", "-k", "foo=bar"]);
        let bind = walk(&[b], &["override"]);
        assert_eq!(bind[0].get("foo"), Some("override"));
    }

    #[test]
    fn k_capital_required_identity_errors_if_unfilled() {
        let b = body(&["occur", "f", "-K", "id"]);
        assert!(b.key_decls[0].is_identity);
        assert_eq!(b.key_decls[0].form, KeyForm::Required);
        let s = slots(std::slice::from_ref(&b));
        let err = walk_slots(&[b], &s, &[]).unwrap_err();
        assert!(matches!(err, ParseError::MissingRequiredKey(_)));
    }

    #[test]
    fn k_capital_with_default_uses_it_for_slug() {
        let b = body(&["occur", "f", "-K", "id=default-slug"]);
        assert!(b.key_decls[0].is_identity);
        let bind = walk(&[b], &[]);
        assert_eq!(bind[0].get("id"), Some("default-slug"));
    }

    #[test]
    fn k_three_forms_distinguishable_in_alias_body_parser() {
        let b = body(&["occur", "f", "-k", "req", "-k", "opt=", "-k", "def=value"]);
        assert_eq!(b.key_decls[0].form, KeyForm::Required);
        assert_eq!(b.key_decls[1].form, KeyForm::Optional);
        assert_eq!(
            b.key_decls[2].form,
            KeyForm::WithDefault("value".to_string())
        );
    }

    // ── -F trichotomy (mirrors `-k`/`-K` semantics for the entry's field) ──

    #[test]
    fn f_bare_field_is_required_errors_if_unfilled() {
        let b = body(&["occur", "-F", "field"]);
        assert_eq!(b.field_decls.len(), 1);
        assert_eq!(b.field_decls[0].name, "field");
        assert_eq!(b.field_decls[0].form, KeyForm::Required);
        let s = slots(std::slice::from_ref(&b));
        let err = walk_slots(&[b], &s, &[]).unwrap_err();
        assert!(matches!(err, ParseError::MissingRequiredKey(k) if k == "field"));
    }

    #[test]
    fn f_field_with_equals_no_default_silently_skipped() {
        let b = body(&["occur", "-F", "field="]);
        assert_eq!(b.field_decls[0].form, KeyForm::Optional);
        let bind = walk(&[b], &[]);
        assert_eq!(bind[0].field, None);
    }

    #[test]
    fn f_field_with_default_uses_it_when_unfilled() {
        let b = body(&["occur", "-F", "field=coffee"]);
        assert_eq!(
            b.field_decls[0].form,
            KeyForm::WithDefault("coffee".to_string())
        );
        let bind = walk(&[b], &[]);
        assert_eq!(bind[0].field.as_deref(), Some("coffee"));
    }

    #[test]
    fn f_field_with_default_overrideable_by_positional() {
        let b = body(&["occur", "-F", "field=coffee"]);
        let bind = walk(&[b], &["tea"]);
        assert_eq!(bind[0].field.as_deref(), Some("tea"));
    }

    #[test]
    fn f_three_forms_distinguishable_in_alias_body_parser() {
        let b = body(&["occur", "-F", "req", "-F", "opt=", "-F", "def=value"]);
        assert_eq!(b.field_decls[0].form, KeyForm::Required);
        assert_eq!(b.field_decls[1].form, KeyForm::Optional);
        assert_eq!(
            b.field_decls[2].form,
            KeyForm::WithDefault("value".to_string())
        );
    }

    #[test]
    fn f_glued_short_form_parses() {
        let b = body(&["occur", "-Ffoo=bar"]);
        assert_eq!(b.field_decls[0].name, "foo");
        assert_eq!(
            b.field_decls[0].form,
            KeyForm::WithDefault("bar".to_string())
        );
    }

    #[test]
    fn f_long_form_field_equals_spec_parses() {
        let b = body(&["occur", "--field=foo=bar"]);
        assert_eq!(b.field_decls[0].name, "foo");
        assert_eq!(
            b.field_decls[0].form,
            KeyForm::WithDefault("bar".to_string())
        );
    }

    #[test]
    fn f_without_spec_errors() {
        let err = parse_alias_body(&t(&["occur", "-F"])).unwrap_err();
        assert!(matches!(err, ParseError::InvalidArg(_)));
    }

    #[test]
    fn f_shared_field_propagates_across_sides() {
        // Both sides declare `-F field` → shared slot, value goes to both.
        let sides = vec![
            body(&["occur", "-F", "field"]),
            body(&["occur", "-F", "field"]),
        ];
        let s = slots(&sides);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].sides, vec![0, 1]);
        let bind = walk_slots(&sides, &s, &t(&["coffee"])).unwrap();
        assert_eq!(bind[0].field.as_deref(), Some("coffee"));
        assert_eq!(bind[1].field.as_deref(), Some("coffee"));
    }

    // ── Section L: verb-derived slots ────────────────────────────────────

    #[test]
    fn unbound_switch_state_slot_appended() {
        // `switch body` (unbound) → state then dur slot.
        let b = body(&["switch", "body"]);
        assert_eq!(b.bound_state, None);
        let s = slots(&[b]);
        assert_eq!(s.len(), 2);
        assert!(matches!(s[0].kind, SlotKind::State));
        assert!(matches!(s[1].kind, SlotKind::Duration));
    }

    #[test]
    fn switch_with_bound_state_first_positional_is_dur() {
        // `switch body sit` (bound) → only dur slot.
        let b = body(&["switch", "body", "sit"]);
        assert_eq!(b.bound_state, Some("sit".to_string()));
        let s = slots(std::slice::from_ref(&b));
        assert_eq!(s.len(), 1);
        assert!(matches!(s[0].kind, SlotKind::Duration));

        // Walking `5` → bound state preserved, dur=5.
        let bind = walk(&[b], &["5"]);
        assert_eq!(bind[0].state, Some("sit".into()));
        assert_eq!(bind[0].dur, Some("5".into()));
    }

    #[test]
    fn occur_with_no_k_no_positionals_accepted_for_that_side() {
        let b = body(&["occur", "f"]);
        let s = slots(&[b]);
        assert!(s.is_empty());
    }

    #[test]
    fn verb_specific_positional_order_within_side_state_then_dur() {
        // Unbound switch — slot order is [state, dur], not [dur, state].
        let b = body(&["switch", "body"]);
        let s = slots(&[b]);
        assert!(matches!(s[0].kind, SlotKind::State));
        assert!(matches!(s[1].kind, SlotKind::Duration));
    }

    #[test]
    fn vx_yields_merged_body_row_state_and_dur() {
        // vx = vertical , x  where vertical = switch body vertical (bound)
        // and x = switch body (unbound). Global slots:
        //   side 0 vertical (bound) → [dur]
        //   side 1 x        (unbound) → [state, dur]
        // → slot list = [v_dur, x_state, x_dur]
        let v = body(&["switch", "body", "vertical"]);
        let x = body(&["switch", "body"]);
        let s = slots(&[v, x]);
        assert_eq!(s.len(), 3);
        assert!(matches!(&s[0].kind, SlotKind::Duration) && s[0].sides == vec![0]);
        assert!(matches!(&s[1].kind, SlotKind::State) && s[1].sides == vec![1]);
        assert!(matches!(&s[2].kind, SlotKind::Duration) && s[2].sides == vec![1]);
    }

    #[test]
    fn cx_yields_separate_caffeine_and_body_rows() {
        // cx = caffeine , x  where caffeine = occur caffeine -k doseMg=
        // and x = switch body. Slots:
        //   side 0 caffeine → [doseMg]
        //   side 1 x        → [state, dur]
        let c = body(&["occur", "caffeine", "-k", "doseMg="]);
        let x = body(&["switch", "body"]);
        let s = slots(&[c, x]);
        assert_eq!(s.len(), 3);
        assert!(matches!(&s[0].kind, SlotKind::Key { name, .. } if name == "doseMg"));
        assert_eq!(s[0].sides, vec![0]);
        assert!(matches!(&s[1].kind, SlotKind::State));
        assert_eq!(s[1].sides, vec![1]);
        assert!(matches!(&s[2].kind, SlotKind::Duration));
        assert_eq!(s[2].sides, vec![1]);
    }

    // ── Section C: non-shared positionals (scaff = sit , caffeine) ───────

    fn scaff_sides() -> Vec<AliasBody> {
        // sit = switch body sit (bound state) → [dur]
        // caffeine = occur caffeine -k doseMg= → [doseMg]
        // scaff slot list = [sit.dur, caffeine.doseMg]
        vec![
            body(&["switch", "body", "sit"]),
            body(&["occur", "caffeine", "-k", "doseMg="]),
        ]
    }

    #[test]
    fn scaff_no_args_yields_sit_no_dur_and_caffeine_no_dosemg() {
        let sides = scaff_sides();
        let bind = walk(&sides, &[]);
        assert_eq!(bind[0].state, Some("sit".into()));
        assert_eq!(bind[0].dur, None);
        assert_eq!(bind[1].get("doseMg"), None);
    }

    #[test]
    fn scaff_one_positional_fills_first_slot_only() {
        let sides = scaff_sides();
        let bind = walk(&sides, &["5"]);
        assert_eq!(bind[0].dur, Some("5".into()));
        assert_eq!(bind[1].get("doseMg"), None);
    }

    #[test]
    fn scaff_two_positionals_fill_both_slots_in_order() {
        let sides = scaff_sides();
        let bind = walk(&sides, &["5", "100"]);
        assert_eq!(bind[0].dur, Some("5".into()));
        assert_eq!(bind[1].get("doseMg"), Some("100"));
    }

    #[test]
    fn scaff_dot_skips_first_slot_only() {
        let sides = scaff_sides();
        let bind = walk(&sides, &[".", "100"]);
        assert_eq!(bind[0].dur, None);
        assert_eq!(bind[1].get("doseMg"), Some("100"));
    }

    #[test]
    fn slot_walk_left_to_right_across_sides() {
        // Two sides each with one slot. Positionals fill left side
        // first, then right side.
        let sides = vec![
            body(&["occur", "a", "-k", "x="]),
            body(&["occur", "b", "-k", "y="]),
        ];
        let bind = walk(&sides, &["1", "2"]);
        assert_eq!(bind[0].get("x"), Some("1"));
        assert_eq!(bind[1].get("y"), Some("2"));
    }

    #[test]
    fn positional_after_keyed_still_fills_slot() {
        // Bound `key=val` in body pre-fills its slot for that side; an
        // invocation positional still flows to the next unfilled slot.
        let sides = vec![
            body(&["occur", "a", "-k", "x="]),
            body(&["occur", "b", "extra=data", "-k", "y="]),
        ];
        let bind = walk(&sides, &["5"]);
        assert_eq!(bind[0].get("x"), Some("5"));
        assert_eq!(bind[1].get("extra"), Some("data"));
        // y was not filled by an invocation positional (none left).
        assert_eq!(bind[1].get("y"), None);
    }

    // ── Section D: shared positional slots ───────────────────────────────

    fn shared_sides() -> Vec<AliasBody> {
        // Both sides declare `-k arg` → shared slot at side 0's position.
        vec![
            body(&["occur", "a", "-k", "arg"]),
            body(&["occur", "b", "-k", "arg"]),
        ]
    }

    #[test]
    fn shared_key_appears_once_at_leftmost_position() {
        let sides = shared_sides();
        let s = slots(&sides);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].sides, vec![0, 1]);
    }

    #[test]
    fn shared_key_value_propagates_to_all_declaring_sides() {
        let sides = shared_sides();
        let bind = walk(&sides, &["v"]);
        assert_eq!(bind[0].get("arg"), Some("v"));
        assert_eq!(bind[1].get("arg"), Some("v"));
    }

    #[test]
    fn shared_key_default_propagates_when_unfilled() {
        // Both sides declare `-k arg=def` → default propagates.
        let sides = vec![
            body(&["occur", "a", "-k", "arg=def"]),
            body(&["occur", "b", "-k", "arg=def"]),
        ];
        let bind = walk(&sides, &[]);
        assert_eq!(bind[0].get("arg"), Some("def"));
        assert_eq!(bind[1].get("arg"), Some("def"));
    }

    #[test]
    fn shared_key_explicit_value_overrides_default_on_all_sides() {
        let sides = vec![
            body(&["occur", "a", "-k", "arg=def"]),
            body(&["occur", "b", "-k", "arg=def"]),
        ];
        let bind = walk(&sides, &["override"]);
        assert_eq!(bind[0].get("arg"), Some("override"));
        assert_eq!(bind[1].get("arg"), Some("override"));
    }

    #[test]
    fn shared_key_dot_skips_all_sides() {
        // Rule 10: `.` on a shared slot skips on all sides — neither
        // side gets a value (the default is applied at finalize, but
        // we use `Optional` here to assert pure skip).
        let sides = vec![
            body(&["occur", "a", "-k", "arg="]),
            body(&["occur", "b", "-k", "arg="]),
        ];
        let bind = walk(&sides, &["."]);
        assert_eq!(bind[0].get("arg"), None);
        assert_eq!(bind[1].get("arg"), None);
    }

    #[test]
    fn shared_capital_k_identity_keys_propagate_like_lowercase_k() {
        // Rule 5.
        let sides = vec![
            body(&["occur", "a", "-K", "id"]),
            body(&["occur", "b", "-K", "id"]),
        ];
        let s = slots(&sides);
        assert_eq!(s.len(), 1);
        match &s[0].kind {
            SlotKind::Key { is_identity, .. } => assert!(*is_identity),
            _ => panic!("expected identity key slot"),
        }
        let bind = walk_slots(&sides, &s, &t(&["X"])).unwrap();
        assert_eq!(bind[0].get("id"), Some("X"));
        assert_eq!(bind[1].get("id"), Some("X"));
    }

    #[test]
    fn shared_key_required_on_either_side_errors_if_unfilled() {
        // One side: Required, other side: WithDefault. Merge picks
        // Required (strongest). Unfilled → MissingRequiredKey.
        let sides = vec![
            body(&["occur", "a", "-k", "arg"]),
            body(&["occur", "b", "-k", "arg=def"]),
        ];
        let s = slots(&sides);
        let err = walk_slots(&sides, &s, &[]).unwrap_err();
        assert!(matches!(err, ParseError::MissingRequiredKey(k) if k == "arg"));
    }

    #[test]
    fn shared_key_three_way_propagation() {
        let sides = vec![
            body(&["occur", "a", "-k", "arg"]),
            body(&["occur", "b", "-k", "arg"]),
            body(&["occur", "c", "-k", "arg"]),
        ];
        let s = slots(&sides);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].sides, vec![0, 1, 2]);
        let bind = walk_slots(&sides, &s, &t(&["X"])).unwrap();
        assert_eq!(bind[0].get("arg"), Some("X"));
        assert_eq!(bind[1].get("arg"), Some("X"));
        assert_eq!(bind[2].get("arg"), Some("X"));
    }

    // ── miscellaneous parser sanity ──────────────────────────────────────

    #[test]
    fn glued_short_form_dash_k_key_parses() {
        let b = body(&["occur", "f", "-kfoo=bar"]);
        assert_eq!(b.key_decls[0].key, "foo");
        assert_eq!(b.key_decls[0].form, KeyForm::WithDefault("bar".to_string()));
    }

    #[test]
    fn long_form_key_equals_spec_parses() {
        let b = body(&["occur", "f", "--key=foo=bar"]);
        assert_eq!(b.key_decls[0].key, "foo");
        assert_eq!(b.key_decls[0].form, KeyForm::WithDefault("bar".to_string()));
    }

    #[test]
    fn long_form_slug_key_is_identity() {
        let b = body(&["occur", "f", "--slug-key=id"]);
        assert!(b.key_decls[0].is_identity);
    }

    #[test]
    fn bare_key_value_token_records_bound_keyed() {
        let b = body(&["note", "word", "srcLang=pt", "pos=verb"]);
        assert_eq!(
            b.bound_keyed,
            vec![
                ("srcLang".to_string(), "pt".to_string()),
                ("pos".to_string(), "verb".to_string())
            ]
        );
    }

    #[test]
    fn unknown_flag_preserved_in_remaining() {
        let b = body(&["occur", "f", "-t", "+5"]);
        assert!(b.remaining.iter().any(|t| t == "-t"));
        assert!(b.bound_positionals.iter().any(|t| t == "+5"));
    }

    #[test]
    fn dash_k_without_spec_errors() {
        let err = parse_alias_body(&t(&["occur", "f", "-k"])).unwrap_err();
        assert!(matches!(err, ParseError::InvalidArg(_)));
    }

    #[test]
    fn dash_capital_k_glued_spec_parses() {
        let b = body(&["occur", "f", "-Kid"]);
        assert!(b.key_decls[0].is_identity);
        assert_eq!(b.key_decls[0].key, "id");
        assert_eq!(b.key_decls[0].form, KeyForm::Required);
    }

    #[test]
    fn consolidate_repeated_keys_within_side() {
        // `-k foo -k foo=10`: last-seen form wins in first-seen position.
        let b = body(&["occur", "f", "-k", "foo", "-k", "foo=10"]);
        let s = slots(&[b]);
        assert_eq!(s.len(), 1);
        match &s[0].kind {
            SlotKind::Key { name, form, .. } => {
                assert_eq!(name, "foo");
                assert_eq!(*form, KeyForm::WithDefault("10".to_string()));
            }
            _ => panic!("expected key slot"),
        }
    }

    #[test]
    fn extra_positional_past_slot_list_errors() {
        // One slot, two positionals.
        let b = body(&["occur", "f", "-k", "x="]);
        let s = slots(std::slice::from_ref(&b));
        let err = walk_slots(&[b], &s, &t(&["1", "2"])).unwrap_err();
        assert!(matches!(err, ParseError::ExtraData(_)));
    }
}
