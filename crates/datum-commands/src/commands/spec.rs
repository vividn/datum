//! `datum spec` — field-spec management (set, show, list, aliases).

use anyhow::{Context, Result};
use datum_core::{
    action::Action,
    db::{Db, IGNORED_STATE_VALUE},
    entry::{Entry, Verb},
    field_spec::{parse_key_decl, FieldSpec, GlyphBy, KeyKind},
    time::DatumTime,
};
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;
use unicode_width::UnicodeWidthChar;

use crate::args::{SpecCommand, SpecSetArgs};
use crate::config::client_id;

pub fn spec(sub: SpecCommand, db: &Db) -> Result<()> {
    // A spec is an entry (da-lc26c): a write is stamped with this client.
    if matches!(sub, SpecCommand::Set(_) | SpecCommand::AddState { .. }) {
        db.set_client_id(&client_id()?);
    }
    match sub {
        SpecCommand::Show { field, text } => match db.load_spec(&field)? {
            Some(spec) if text => render_spec_text(&spec),
            Some(spec) => println!("{}", serde_json::to_string_pretty(&spec_json(&spec))?),
            None => println!("No spec defined for field '{field}'"),
        },
        SpecCommand::AddState {
            field,
            name,
            color,
            description,
            glyph,
            keys,
            assign,
        } => spec_add_state(
            &field,
            &name,
            color.as_deref(),
            description.as_deref(),
            glyph.as_deref(),
            (&keys, &assign),
            db,
        )?,
        SpecCommand::Set(args) => spec_set(*args, db)?,
        SpecCommand::List { tag } => {
            if let Some(tag) = tag {
                let specs = db.list_specs_by_tag(&tag)?;
                if specs.is_empty() {
                    println!("No field specs tagged '{tag}'.");
                } else {
                    for spec in &specs {
                        println!("  {}", spec.field);
                    }
                }
            } else {
                let reg = db.load_registry()?;
                if reg.fields.is_empty() {
                    println!("No field specs defined.");
                } else {
                    let mut names: Vec<&String> = reg.fields.keys().collect();
                    names.sort();
                    for name in names {
                        println!("  {name}");
                    }
                }
            }
        }
        SpecCommand::Remove { field } => {
            if let Some(hid) = db.delete_field_spec(&field)? {
                remember(db, hid);
            }
            println!("✓ Spec removed for '{field}' (derived aliases dropped)");
        }
    }
    Ok(())
}

fn spec_set(args: SpecSetArgs, db: &Db) -> Result<()> {
    // --from-json replaces the entire spec; individual flags merge in.
    let mut spec = if let Some(json) = &args.from_json {
        let mut s: FieldSpec =
            serde_json::from_str(json).with_context(|| "Invalid field spec JSON")?;
        s.field = args.field.clone();
        s
    } else {
        match db.load_spec(&args.field)? {
            Some(s) => s,
            // da-c8mh: a spec cannot be born verbless. On a field with a
            // log, `--verb` is optional and the log answers once; on a new
            // one the user is asked, because there is nothing to read.
            // Naming the states a field can be in says it is a switch
            // field, the same claim `spec add-state` makes.
            None if args.verb.is_none() && !args.states.is_empty() => {
                FieldSpec::new(&args.field, Action::Switch)
            }
            None => FieldSpec::new(
                &args.field,
                crate::commands::track::resolve_verb(&args.field, args.verb.as_deref(), db)?,
            ),
        }
    };

    if let Some(v) = &args.verb {
        spec.verb = v
            .parse::<Action>()
            .map_err(|e| anyhow::anyhow!("Invalid --verb: {e}"))?;
    }
    if !args.states.is_empty() {
        spec.allowed_states = Some(args.states.clone());
    }
    if let Some(c) = &args.color {
        spec.color = Some(c.clone());
    }
    if let Some(t) = &args.id_template {
        spec.id_template = Some(t.clone());
    }
    if let Some(d) = &args.description {
        spec.description = Some(d.clone());
    }
    if let Some(f) = &args.state_lookup_field {
        spec.state_lookup_field = Some(f.clone()).filter(|f| !f.is_empty());
    }
    let any = |_: &str, _: &str| Ok(());
    set_pairs(
        &mut spec.state_colors,
        &args.state_colors,
        "--state-color",
        "STATE=COLOR",
        any,
    )?;
    // da-zwqk: per-state descriptions, same STATE=TEXT shape as colors.
    set_pairs(
        &mut spec.state_descriptions,
        &args.state_descriptions,
        "--state-description",
        "STATE=TEXT",
        any,
    )?;
    // da-cej7q: the `--state-key` flags naming a state give its whole key
    // list, as `--data-key` gives the field's; `STATE=` clears it.
    let mut given: std::collections::HashMap<&str, Vec<&str>> = Default::default();
    for pair in &args.state_keys {
        let (state, key) = pair.split_once('=').with_context(|| {
            format!("Invalid --state-key format: expected STATE=KEY, got '{pair}'")
        })?;
        given.entry(state).or_default().push(key);
    }
    for (state, keys) in given {
        set_state_keys(&mut spec, state, &keys);
    }
    // da-ulc0v: `--state-assign STATE=KEY=VALUE` merges into the state's
    // assigned keys; an empty VALUE removes one.
    for pair in &args.state_assign {
        let (state, kv) = pair.split_once('=').with_context(|| {
            format!("Invalid --state-assign format: expected STATE=KEY=VALUE, got '{pair}'")
        })?;
        set_state_assign(&mut spec, state, std::slice::from_ref(&kv.to_string()))?;
    }
    // da-gzowt: occur-dot glyphs, in `color` / `state_colors`' shape. An
    // empty value clears, as it does for every STATE=VALUE flag.
    if let Some(g) = &args.glyph {
        if !g.is_empty() {
            check_glyph("--glyph", g)?;
        }
        spec.presentation.glyph = Some(g.clone()).filter(|g| !g.is_empty());
    }
    let pres = &mut spec.presentation;
    set_pairs(
        &mut pres.state_glyphs,
        &args.state_glyphs,
        "--state-glyph",
        "STATE=CHAR",
        check_glyph,
    )?;
    match args.glyph_by.as_deref() {
        Some("") => pres.glyph_by = None,
        // A new key starts a new map: the old values were another key's.
        Some(key) if pres.glyph_by.as_ref().map(|g| g.key.as_str()) != Some(key) => {
            pres.glyph_by = Some(GlyphBy {
                key: key.to_string(),
                map: Default::default(),
            })
        }
        _ => {}
    }
    if !args.glyph_map.is_empty() {
        let by = pres
            .glyph_by
            .as_mut()
            .context("--glyph-map needs --glyph-by <KEY> to say which data key it maps")?;
        let mut map = by.map.clone().into_iter().collect();
        set_pairs(
            &mut map,
            &args.glyph_map,
            "--glyph-map",
            "VALUE=CHAR",
            check_glyph,
        )?;
        by.map = map.into_iter().collect();
    }
    // da-45wgz: `--icon FILE` reads the file; an empty value clears. An
    // icon from `--from-json` is vetted the same way, so none is stored
    // unread.
    if let Some(path) = &args.icon {
        spec.presentation.icon = if path.is_empty() {
            None
        } else {
            Some(
                String::from_utf8(std::fs::read(path).with_context(|| format!("--icon {path}"))?)
                    .map_err(|_| not_svg())?,
            )
        };
    }
    if let Some(svg) = &spec.presentation.icon {
        spec.presentation.icon = Some(check_icon(svg)?);
    }
    let pres = &mut spec.presentation;
    pres.glyph_scale = args.glyph_scale.or(pres.glyph_scale);
    pres.glyph_aspect = args.glyph_aspect.or(pres.glyph_aspect);
    check_factors(&mut spec)?;
    // da-2sr: --track / --ignore on `spec set` write switch entries rather
    // than mutating the spec. The flags are kept on the parser surface for
    // backwards compatibility; they no longer touch the spec.
    if args.track {
        let client = client_id()?;
        let entry = Entry::new_legacy(
            &args.field,
            Verb::Switch,
            None,
            None,
            Some(DatumTime::now()),
            None,
            client,
        );
        db.insert_entry(&entry)?;
    }
    if args.ignore {
        let client = client_id()?;
        let entry = Entry::new_legacy(
            &args.field,
            Verb::Switch,
            Some(JsonValue::String(IGNORED_STATE_VALUE.to_string())),
            None,
            Some(DatumTime::now()),
            None,
            client,
        );
        db.insert_entry(&entry)?;
    }
    // da-ulc0v: a key lives in one place — each declaration takes its name
    // out of the others first.
    for key in args.id_keys.iter().chain(&args.data_keys) {
        spec.forget_key(parse_key_decl(key).0);
    }
    for (k, _) in assigned(args.assign.iter().chain(&args.id_assign)) {
        spec.forget_key(k);
    }
    if !args.id_keys.is_empty() {
        spec.id_keys = args.id_keys.clone();
    }
    if !args.data_keys.is_empty() {
        spec.data_keys = args.data_keys.clone();
    }
    set_assign(&mut spec.assign, &args.assign, "--assign")?;
    set_assign(&mut spec.id_assign, &args.id_assign, "--id-assign")?;
    // Turning the auto-aliases off drops them; once off, the list is the
    // user's own (da-cap5m) and a repeat of the flag leaves it alone.
    if args.no_autoalias && !spec.no_autoalias {
        spec.no_autoalias = true;
        spec.derived_aliases.clear();
    }
    if let Some(k) = args.only_listed_keys {
        spec.only_listed_keys = k;
    }
    if let Some(s) = args.only_listed_states {
        spec.only_listed_states = s;
    }
    if let Some(r) = &args.remainder {
        spec.remainder = Some(r.clone()).filter(|r| !r.is_empty());
    }
    if let Some(n) = args.never_false {
        spec.never_false = n;
    }
    if args.y.is_some() {
        spec.presentation.y = args.y;
    }
    if args.height.is_some() {
        spec.presentation.height = args.height;
    }
    if args.z_index.is_some() {
        spec.presentation.z_index = args.z_index;
    }
    // da-a7g: `--tag <name>` repeats replace the spec's full tag set
    // (mirrors how `--state <name>` replaces `allowed_states`). To clear
    // tags, re-set the spec from JSON with `tags: []`.
    if !args.tags.is_empty() {
        spec.tags = args.tags.clone();
    }

    spec.validate_verb().map_err(|e| anyhow::anyhow!(e))?;

    // Always regenerate derived aliases from the current shape.
    spec.regenerate_derived_aliases();

    let report = db.save_field_spec_with_report(&spec)?;
    remember(db, report.hid.clone());
    println!("✓ Spec saved for '{}'", spec.field);
    warn_alias_conflicts(&report);
    Ok(())
}

/// Point `_LAST` at the spec entry a command just wrote, so `undo _LAST`
/// steps it back like any doc's edit.
fn remember(db: &Db, hid: datum_core::ids::Hid) {
    let _ = datum_core::last_docs::set_last_docs(db, &[hid]);
}

/// Give `state` the keys its flags name, in order (da-cej7q): an empty one
/// clears those before it, and none left removes the entry.
fn set_state_keys(spec: &mut FieldSpec, state: &str, flags: &[&str]) {
    let keys = flags.iter().fold(Vec::new(), |mut keys, k| {
        match *k {
            "" => keys.clear(),
            k => keys.push(k.to_string()),
        }
        keys
    });
    for key in &keys {
        spec.forget_state_key(state, parse_key_decl(key).0);
    }
    if keys.is_empty() {
        spec.state_keys.remove(state);
    } else {
        spec.state_keys.insert(state.to_string(), keys);
    }
}

/// Merge `--assign`-style `KEY=VALUE` flags into an assigned-key map
/// (da-ulc0v), by [`set_pairs`]' convention: an empty VALUE removes KEY.
fn set_assign(map: &mut BTreeMap<String, String>, pairs: &[String], flag: &str) -> Result<()> {
    let mut pairs_map = std::mem::take(map).into_iter().collect();
    set_pairs(&mut pairs_map, pairs, flag, "KEY=VALUE", |_, _| Ok(()))?;
    *map = pairs_map.into_iter().collect();
    Ok(())
}

/// The `KEY=VALUE` pairs that assign — an empty VALUE removes instead.
fn assigned<'a>(
    pairs: impl IntoIterator<Item = &'a String>,
) -> impl Iterator<Item = (&'a str, &'a str)> {
    pairs
        .into_iter()
        .filter_map(|p| p.split_once('='))
        .filter(|(_, v)| !v.is_empty())
}

/// Merge `pairs` into `state`'s assigned keys; none left removes the entry.
fn set_state_assign(spec: &mut FieldSpec, state: &str, pairs: &[String]) -> Result<()> {
    for (k, _) in assigned(pairs) {
        spec.forget_state_key(state, k);
    }
    let mut map = spec.state_assign.remove(state).unwrap_or_default();
    set_assign(&mut map, pairs, "--state-assign")?;
    if !map.is_empty() {
        spec.state_assign.insert(state.to_string(), map);
    }
    Ok(())
}

/// Apply `KEY=VALUE` flags to a map — the clear convention shared by every
/// such flag here: an empty VALUE removes KEY's entry. `check` vets a
/// non-empty VALUE.
fn set_pairs(
    map: &mut std::collections::HashMap<String, String>,
    pairs: &[String],
    flag: &str,
    shape: &str,
    check: impl Fn(&str, &str) -> Result<()>,
) -> Result<()> {
    for pair in pairs {
        match pair.split_once('=') {
            Some((key, "")) => {
                map.remove(key);
            }
            Some((key, value)) => {
                check(flag, value)?;
                map.insert(key.to_string(), value.to_string());
            }
            None => anyhow::bail!("Invalid {flag} format: expected {shape}, got '{pair}'"),
        }
    }
    Ok(())
}

/// A glyph is ONE printable character (da-gzowt): the SVG draws it as
/// given; the terminal band draws it when it is one column wide and `●`
/// when it is two (most emoji), so no column of the band moves.
fn check_glyph(flag: &str, glyph: &str) -> Result<()> {
    let mut chars = glyph.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None)
            if !c.is_whitespace() && UnicodeWidthChar::width(c).is_some_and(|w| w > 0) =>
        {
            Ok(())
        }
        _ => anyhow::bail!("{flag} expects one printable character, got '{glyph}'"),
    }
}

/// The most SVG an icon may be (da-45wgz): a few kB keeps a spec something
/// a person could read, and one copy is all a dayview carries.
const ICON_MAX_BYTES: usize = 4096;

fn not_svg() -> anyhow::Error {
    anyhow::anyhow!("an icon must be an .svg file (one <svg> element)")
}

/// Vet an icon and return what is stored: the `<svg>` element alone, with
/// no prolog. It is drawn inside the dayview's own SVG and so inside the
/// page, so nothing that could run, restyle the page or reach outside the
/// picture is let in — and it needs a `viewBox` to scale to a dot.
fn check_icon(svg: &str) -> Result<String> {
    let whole = roxmltree::Document::parse(svg).map_err(|_| not_svg())?;
    let svg = &svg[whole.root_element().range()];
    if svg.len() > ICON_MAX_BYTES {
        anyhow::bail!(
            "the icon is {:.1} kB of SVG; an icon may be at most {} kB",
            svg.len() as f64 / 1024.0,
            ICON_MAX_BYTES / 1024
        );
    }
    let doc = roxmltree::Document::parse(svg).map_err(|_| not_svg())?;
    let root = doc.root_element();
    if !svg.starts_with("<svg") || root.tag_name().name() != "svg" {
        return Err(not_svg());
    }
    if root.attribute("viewBox").is_none() {
        anyhow::bail!("an icon needs a viewBox on its <svg>, to scale to the size of a dot");
    }
    if root.attribute("id").is_some() {
        anyhow::bail!("an icon's <svg> may not carry an id: the dayview names it");
    }
    for el in root.descendants().filter(|n| n.is_element()) {
        let name = el.tag_name().name();
        if matches!(name, "script" | "style" | "foreignObject") {
            anyhow::bail!("an icon may not contain <{name}>");
        }
        // `<set>` / `<animate>` can write an href or a handler later.
        let target = el.attribute("attributeName").unwrap_or("");
        if target.starts_with("on") || target.ends_with("href") {
            anyhow::bail!("an icon may not animate {target}");
        }
        for a in el.attributes() {
            if a.name().starts_with("on")
                || (a.name() == "href" && !a.value().starts_with('#'))
                || fetches(a.value())
            {
                anyhow::bail!("an icon may not carry {}=\"{}\"", a.name(), a.value());
            }
        }
    }
    Ok(svg.to_string())
}

/// A glyph's size factors are numbers above zero (da-c05v1) — whatever
/// route they came by, `--from-json` and a kit included — and 1, which is
/// what an unset factor draws, is stored as unset.
fn check_factors(spec: &mut FieldSpec) -> Result<()> {
    let pres = &mut spec.presentation;
    for (key, slot) in [
        ("glyph_scale", &mut pres.glyph_scale),
        ("glyph_aspect", &mut pres.glyph_aspect),
    ] {
        if let Some(n) = slot.filter(|n| !(*n > 0.0 && n.is_finite())) {
            anyhow::bail!("{key} must be a number above 0, got {n}");
        }
        *slot = slot.filter(|&n| n != 1.0);
    }
    Ok(())
}

/// A kit's field meets `spec set`'s checks (da-dtmlv): every glyph is one
/// printable character, the icon is vetted and stored as `--icon`'s is, and
/// so are the glyph's size factors (da-gah1q).
pub(crate) fn check_presentation(spec: &mut FieldSpec) -> Result<()> {
    let pres = &mut spec.presentation;
    // Each glyph names what it belongs to, as spec set's flags do (da-am7cq).
    if let Some(glyph) = &pres.glyph {
        check_glyph("glyph", glyph)?;
    }
    for (state, glyph) in &pres.state_glyphs {
        check_glyph(&format!("state '{state}' glyph"), glyph)?;
    }
    for (value, glyph) in pres.glyph_by.iter().flat_map(|b| &b.map) {
        check_glyph(&format!("glyph_by value '{value}' glyph"), glyph)?;
    }
    if let Some(svg) = &pres.icon {
        pres.icon = Some(check_icon(svg)?);
    }
    check_factors(spec)
}

/// Vet a STORED icon again where it is drawn (da-45wgz): a spec can arrive
/// by sync or a hand edit without passing `spec set`, so the stored text is
/// untrusted whatever route it came by. One that fails is dropped — its dots
/// draw as if it had none — and said once per field on stderr.
pub(crate) fn drop_unvetted_icon(spec: &mut FieldSpec) {
    static WARNED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    let Some(Err(e)) = spec.presentation.icon.as_deref().map(check_icon) else {
        return;
    };
    spec.presentation.icon = None;
    let mut warned = WARNED.lock().unwrap_or_else(|p| p.into_inner());
    if !warned.contains(&spec.field) {
        eprintln!("⚠ {}: icon not drawn — {e}", spec.field);
        warned.push(spec.field.clone());
    }
}

/// Whether an attribute value could make a browser fetch (da-45wgz). A
/// match on a function's NAME is one CSS escape from missing it — `\75 rl(`
/// IS `url(` — and `image-set('…')` fetches with no `url(` at all, so this
/// refuses the escape character outright and every function but those an
/// icon draws with: a transform, a colour, and `url()` to a `#fragment`.
fn fetches(value: &str) -> bool {
    const DRAWS: [&str; 10] = [
        "matrix",
        "translate",
        "scale",
        "rotate",
        "skewx",
        "skewy",
        "rgb",
        "rgba",
        "hsl",
        "hsla",
    ];
    let value = value.to_ascii_lowercase();
    value.contains('\\')
        || value.match_indices('(').any(|(i, _)| {
            let head = value[..i].trim_end();
            let name = &head[head
                .trim_end_matches(|c: char| c.is_ascii_alphanumeric() || c == '-')
                .len()..];
            match name {
                "url" => !value[i + 1..]
                    .trim_start()
                    .trim_start_matches(['\'', '"'])
                    .starts_with('#'),
                _ => !DRAWS.contains(&name),
            }
        })
}

/// Print the `⚠ auto-alias … skipped` lines from a save report to stderr.
/// Shared by every spec-mutating command that regenerates aliases.
fn warn_alias_conflicts(report: &datum_core::db::SaveSpecReport) {
    for conflict in &report.skipped {
        match &conflict.reason {
            datum_core::db::AliasConflictReason::UserAlias => eprintln!(
                "  ⚠ auto-alias '{}' skipped — user alias with the same name already exists",
                conflict.name
            ),
            datum_core::db::AliasConflictReason::OwnedByField(owner) => eprintln!(
                "  ⚠ auto-alias '{}' skipped — owned by field '{}'",
                conflict.name, owner
            ),
        }
    }
}

/// `datum spec add-state <FIELD> <NAME> [COLOR] [DESCRIPTION]` — merge one
/// state into a field spec without disturbing the others (da-t9ps).
///
/// The incremental, footgun-free counterpart to `spec set --state`, which
/// replaces the whole `allowed_states` list. Appends NAME to the states
/// (idempotent — re-adding an existing state just refreshes its color and
/// description) and, when supplied, records the per-state color and
/// description alongside it. Creates the spec (Occur default) if absent.
fn spec_add_state(
    field: &str,
    name: &str,
    color: Option<&str>,
    description: Option<&str>,
    glyph: Option<&str>,
    (keys, assign): (&[String], &[String]),
    db: &Db,
) -> Result<()> {
    if let Some(g) = glyph {
        check_glyph("GLYPH", g)?;
    }
    let mut spec = match db.load_spec(field)? {
        Some(s) => s,
        // A field named only by its states is a switch field — that is
        // what `add-state` says about it (da-c8mh).
        None => FieldSpec::new(field, Action::Switch),
    };

    let states = spec.allowed_states.get_or_insert_with(Vec::new);
    let existed = states.iter().any(|s| s == name);
    if !existed {
        states.push(name.to_string());
    }
    if let Some(c) = color {
        spec.state_colors.insert(name.to_string(), c.to_string());
    }
    if let Some(d) = description {
        spec.state_descriptions
            .insert(name.to_string(), d.to_string());
    }
    if let Some(g) = glyph {
        spec.presentation
            .state_glyphs
            .insert(name.to_string(), g.to_string());
    }
    if !keys.is_empty() {
        let keys: Vec<&str> = keys.iter().map(String::as_str).collect();
        set_state_keys(&mut spec, name, &keys);
    }
    set_state_assign(&mut spec, name, assign)?;

    spec.validate_verb().map_err(|e| anyhow::anyhow!(e))?;
    spec.regenerate_derived_aliases();

    let report = db.save_field_spec_with_report(&spec)?;
    remember(db, report.hid.clone());
    let verb = if existed { "updated" } else { "added" };
    println!("✓ State '{name}' {verb} on '{field}'");
    warn_alias_conflicts(&report);
    Ok(())
}

/// A state's own keys with their kinds, under the state's line (da-cej7q),
/// its assigned ones after them (da-ulc0v).
fn print_state_keys(spec: &FieldSpec, state: &str) {
    for key in spec.state_keys.get(state).into_iter().flatten() {
        let (name, kind) = datum_parse::spec_keys::parse_key_decl(key);
        println!("      {name:<14} {kind}");
    }
    for (name, v) in spec.state_assign.get(state).into_iter().flatten() {
        println!("      {name:<14} assign {v}");
    }
}

/// A spec as `spec show` and `fields --json` print it (da-p3hxn): the
/// stored spec, plus every key it declares ALREADY PARSED, so no reader
/// splits a spelling — `keys`, [`FieldSpec::declared_keys`] in binding
/// order as `{name, kind, value?, in_slug}`, and `keys_by_state`, the same
/// list for an entry in each state that declares keys of its own. Neither
/// is stored: `spec set --from-json` ignores both.
#[derive(serde::Serialize)]
pub struct SpecJson<'a> {
    #[serde(flatten)]
    spec: &'a FieldSpec,
    keys: Vec<JsonValue>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    keys_by_state: BTreeMap<&'a str, Vec<JsonValue>>,
}

pub fn spec_json(spec: &FieldSpec) -> SpecJson<'_> {
    let keys = |state: Option<&str>| -> Vec<JsonValue> {
        spec.declared_keys(state)
            .into_iter()
            .map(|k| {
                let (kind, value) = match k.kind {
                    KeyKind::Required => ("required", None),
                    KeyKind::Optional => ("optional", None),
                    KeyKind::Default(v) => ("default", Some(v)),
                    KeyKind::Assign(v) => ("assign", Some(v)),
                };
                let mut key = serde_json::json!({"name": k.name, "kind": kind, "in_slug": k.id});
                if let Some(v) = value {
                    key["value"] = v.into();
                }
                key
            })
            .collect()
    };
    let states = spec.state_keys.keys().chain(spec.state_assign.keys());
    SpecJson {
        spec,
        keys: keys(None),
        keys_by_state: states.map(|s| (s.as_str(), keys(Some(s)))).collect(),
    }
}

/// Human-readable summary of a FieldSpec — `datum spec show <FIELD> --text`.
///
/// JSON output (`spec show <FIELD>` without `--text`) is precise and good
/// for programmatic use, but a typed-out reading of the spec's intent reads
/// faster for humans. This format prints field-level metadata first, then
/// per-state lines listing color + description side by side (da-nhzo).
fn render_spec_text(spec: &FieldSpec) {
    println!("{}", spec.field);
    println!("  verb: {}", spec.verb);
    if let Some(d) = &spec.description {
        println!("  description: {d}");
    }
    if let Some(c) = &spec.color {
        println!("  color: {c}");
    }
    if let Some(g) = &spec.presentation.glyph {
        println!("  glyph: {g}");
    }
    if let Some(icon) = &spec.presentation.icon {
        println!("  icon: {:.1} kB of SVG", icon.len() as f64 / 1024.0);
    }
    if let Some(by) = &spec.presentation.glyph_by {
        let values: Vec<_> = by.map.iter().map(|(v, g)| format!("{v}={g}")).collect();
        println!("  glyph by {}: {}", by.key, values.join(" "));
    }
    if let Some(n) = spec.presentation.glyph_scale {
        println!("  glyph scale: ×{n}");
    }
    if let Some(n) = spec.presentation.glyph_aspect {
        println!("  glyph aspect: {n}");
    }
    if let Some(f) = &spec.state_lookup_field {
        println!("  state lookup field: {f}");
    }
    if let Some(states) = &spec.allowed_states {
        println!("  states:");
        for state in states {
            let mut color = spec
                .state_colors
                .get(state)
                .cloned()
                .unwrap_or_else(|| "(auto)".to_string());
            if let Some(g) = spec.presentation.state_glyphs.get(state) {
                color = format!("{color} {g}");
            }
            let desc = spec
                .state_descriptions
                .get(state)
                .map(String::as_str)
                .unwrap_or("");
            if desc.is_empty() {
                println!("    {state:<16} {color}");
            } else {
                println!("    {state:<16} {color}  — {desc}");
            }
            // da-cej7q: the state's own keys, under it.
            print_state_keys(spec, state);
        }
    } else if !spec.state_descriptions.is_empty() {
        // Descriptions without an allowed_states list — list them anyway.
        println!("  state_descriptions:");
        let mut names: Vec<&String> = spec.state_descriptions.keys().collect();
        names.sort();
        for name in names {
            let desc = &spec.state_descriptions[name];
            println!("    {name:<16} {desc}");
        }
    }
    // da-9s98k: every key the spec declares — `id_keys`, `data_keys` and
    // subfields alike — with its kind, `(id)` marking a slug key; the
    // assigned ones as a group of their own (da-ulc0v).
    let (assigned, slots): (Vec<_>, Vec<_>) = spec
        .declared_keys(None)
        .into_iter()
        .partition(|k| matches!(k.kind, KeyKind::Assign(_)));
    for (head, keys) in [("keys", slots), ("assign", assigned)] {
        if !keys.is_empty() {
            println!("  {head}:");
        }
        for k in keys {
            let id = if k.id { "  (id)" } else { "" };
            let kind = match k.kind {
                KeyKind::Assign(v) => v,
                kind => kind.to_string(),
            };
            println!("    {:<16} {kind}{id}", k.name);
        }
    }
    // A state holding keys the `states` list does not name — list it anyway.
    let listed = spec.allowed_states.as_deref().unwrap_or_default();
    let mut unlisted: Vec<&String> = spec
        .state_keys
        .keys()
        .chain(spec.state_assign.keys())
        .filter(|s| !listed.contains(s))
        .collect();
    unlisted.sort();
    unlisted.dedup();
    if !unlisted.is_empty() {
        println!("  state keys:");
        for state in unlisted {
            println!("    {state}");
            print_state_keys(spec, state);
        }
    }
    if !spec.tags.is_empty() {
        println!("  tags: {:?}", spec.tags);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datum_core::db::Db;
    use serde_json::json;

    #[test]
    fn cmd_spec_set_flag_flow_creates_derived_aliases() {
        let db = Db::open_in_memory().unwrap();

        // spec set mood --verb switch --state happy --state sad \
        //               --color green --id-template "%field%:%state%"
        let args = SpecSetArgs {
            field: "mood".to_string(),
            from_json: None,
            verb: Some("switch".to_string()),
            states: vec!["happy".to_string(), "sad".to_string()],
            color: Some("green".to_string()),
            state_colors: vec!["happy=00ff00".to_string()],
            state_descriptions: vec![],
            id_template: Some("%field%:%state%".to_string()),
            description: None,
            track: true,
            ignore: false,
            id_keys: vec![],
            data_keys: vec![],
            no_autoalias: false,
            y: None,
            height: None,
            z_index: None,
            tags: vec![],
            only_listed_keys: None,
            only_listed_states: None,
            remainder: None,
            ..Default::default()
        };
        spec_set(args, &db).unwrap();

        let spec = db.load_spec("mood").unwrap().unwrap();
        assert_eq!(spec.id_template.as_deref(), Some("%field%:%state%"));
        assert_eq!(spec.color.as_deref(), Some("green"));
        let states = spec.allowed_states.as_ref().unwrap();
        assert!(states.contains(&"happy".to_string()) && states.contains(&"sad".to_string()));
        // da-2sr: --track on `spec set` writes a switch entry; the spec
        // itself no longer carries tracked/ignored bits.
        let mood_tracked = db.get_tracked_fields(chrono::Utc::now()).unwrap();
        let mood_row = mood_tracked.iter().find(|f| f.field == "mood").unwrap();
        assert!(!mood_row.ignored);

        // Derived aliases landed in the aliases table (hot-path lookup).
        let happy = db.lookup_alias("happy").unwrap().unwrap();
        assert_eq!(happy.expands_to, "switch mood happy");
        let sad = db.lookup_alias("sad").unwrap().unwrap();
        assert_eq!(sad.expands_to, "switch mood sad");

        // Dropping "sad" from the states list drops its alias.
        let args2 = SpecSetArgs {
            field: "mood".to_string(),
            from_json: None,
            verb: None,
            states: vec!["happy".to_string()],
            color: None,
            state_colors: vec![],
            state_descriptions: vec![],
            id_template: None,
            description: None,
            track: false,
            ignore: false,
            id_keys: vec![],
            data_keys: vec![],
            no_autoalias: false,
            y: None,
            height: None,
            z_index: None,
            tags: vec![],
            only_listed_keys: None,
            only_listed_states: None,
            remainder: None,
            ..Default::default()
        };
        spec_set(args2, &db).unwrap();
        assert!(db.lookup_alias("sad").unwrap().is_none());
        assert!(db.lookup_alias("happy").unwrap().is_some());
    }

    #[test]
    fn no_autoalias_flag_drops_the_aliases_then_keeps_a_hand_list() {
        let db = Db::open_in_memory().unwrap();
        let set = |args: SpecSetArgs| spec_set(args, &db).unwrap();
        set(SpecSetArgs {
            field: "sleep".into(),
            verb: Some("start".into()),
            ..Default::default()
        });
        assert!(db.lookup_alias("sleep").unwrap().is_some());

        set(SpecSetArgs {
            field: "sleep".into(),
            no_autoalias: true,
            ..Default::default()
        });
        assert!(db.lookup_alias("sleep").unwrap().is_none());

        // A hand list survives the save, and a repeat of the flag.
        let mut spec = db.load_spec("sleep").unwrap().unwrap();
        spec.derived_aliases = vec![datum_core::field_spec::DerivedAlias {
            name: "zz".into(),
            expands_to: "start sleep".into(),
            kind: Default::default(),
        }];
        set(SpecSetArgs {
            field: "sleep".into(),
            from_json: Some(serde_json::to_string(&spec).unwrap()),
            ..Default::default()
        });
        set(SpecSetArgs {
            field: "sleep".into(),
            no_autoalias: true,
            ..Default::default()
        });
        assert_eq!(
            db.lookup_alias("zz").unwrap().unwrap().expands_to,
            "start sleep"
        );
        assert!(db.lookup_alias("sleep").unwrap().is_none());
    }

    #[test]
    fn cmd_spec_add_state_merges_without_dropping_others() {
        let db = Db::open_in_memory().unwrap();

        // A live multi-state field.
        spec_set(
            SpecSetArgs {
                field: "mood".to_string(),
                verb: Some("switch".to_string()),
                states: vec!["happy".to_string(), "sad".to_string()],
                state_descriptions: vec!["happy=joyful".to_string()],
                ..Default::default()
            },
            &db,
        )
        .unwrap();

        // Add one state with color + description — the others must survive.
        spec_add_state(
            "mood",
            "meh",
            Some("yellow"),
            Some("so-so"),
            None,
            (&[], &[]),
            &db,
        )
        .unwrap();

        let spec = db.load_spec("mood").unwrap().unwrap();
        let states = spec.allowed_states.as_ref().unwrap();
        assert_eq!(states, &["happy", "sad", "meh"]);
        assert_eq!(
            spec.state_colors.get("meh").map(String::as_str),
            Some("yellow")
        );
        assert_eq!(
            spec.state_descriptions.get("meh").map(String::as_str),
            Some("so-so")
        );
        // Pre-existing description is untouched.
        assert_eq!(
            spec.state_descriptions.get("happy").map(String::as_str),
            Some("joyful")
        );
        // The new state's derived alias landed.
        let meh = db.lookup_alias("meh").unwrap().unwrap();
        assert_eq!(meh.expands_to, "switch mood meh");
    }

    #[test]
    fn cmd_spec_add_state_creates_spec_and_reupdates_idempotently() {
        let db = Db::open_in_memory().unwrap();

        // No spec yet — add-state creates an Occur-default spec.
        spec_add_state(
            "energy",
            "high",
            Some("green"),
            Some("wired"),
            None,
            (&[], &[]),
            &db,
        )
        .unwrap();
        let spec = db.load_spec("energy").unwrap().unwrap();
        assert_eq!(
            spec.allowed_states.as_deref(),
            Some(&["high".to_string()][..])
        );

        // Re-adding refreshes the color but does not duplicate the state or
        // clobber a description that wasn't re-supplied.
        spec_add_state("energy", "high", Some("lime"), None, None, (&[], &[]), &db).unwrap();
        let spec = db.load_spec("energy").unwrap().unwrap();
        assert_eq!(
            spec.allowed_states.as_deref(),
            Some(&["high".to_string()][..])
        );
        assert_eq!(
            spec.state_colors.get("high").map(String::as_str),
            Some("lime")
        );
        assert_eq!(
            spec.state_descriptions.get("high").map(String::as_str),
            Some("wired")
        );
    }

    #[test]
    fn cmd_spec_set_empty_state_value_clears_entry() {
        let db = Db::open_in_memory().unwrap();

        // Set a color and a description for the same state...
        spec_set(
            SpecSetArgs {
                field: "mood".to_string(),
                verb: Some("switch".to_string()),
                state_colors: vec!["happy=00ff00".to_string()],
                state_descriptions: vec!["happy=joyful".to_string()],
                ..Default::default()
            },
            &db,
        )
        .unwrap();
        let spec = db.load_spec("mood").unwrap().unwrap();
        assert_eq!(
            spec.state_colors.get("happy").map(String::as_str),
            Some("00ff00")
        );
        assert_eq!(
            spec.state_descriptions.get("happy").map(String::as_str),
            Some("joyful")
        );

        // ...then clear both via the empty-VALUE convention.
        spec_set(
            SpecSetArgs {
                field: "mood".to_string(),
                state_colors: vec!["happy=".to_string()],
                state_descriptions: vec!["happy=".to_string()],
                ..Default::default()
            },
            &db,
        )
        .unwrap();
        let spec = db.load_spec("mood").unwrap().unwrap();
        assert!(!spec.state_colors.contains_key("happy"));
        assert!(!spec.state_descriptions.contains_key("happy"));
    }

    /// `spec show` hands a reader every key already parsed (da-p3hxn), so
    /// the web never splits a spelling: an assign, which no list spells,
    /// is `brand` of kind assign; a state's own key is in its state's list
    /// alone.
    #[test]
    fn spec_json_carries_the_keys_parsed() {
        let mut spec = FieldSpec::new("mood", datum_core::action::Action::Switch);
        spec.id_keys = vec!["day".into()];
        spec.data_keys = vec!["size=".into(), "temp=hot".into()];
        spec.assign.insert("brand".into(), "acme".into());
        spec.state_keys.insert("sad".into(), vec!["reason".into()]);
        let shown = serde_json::to_value(spec_json(&spec)).unwrap();
        let key =
            |name: &str, kind: &str, id: bool| json!({"name": name, "kind": kind, "in_slug": id});
        let mut temp = key("temp", "default", false);
        temp["value"] = "hot".into();
        let mut brand = key("brand", "assign", false);
        brand["value"] = "acme".into();
        let field = vec![
            key("day", "required", true),
            key("size", "optional", false),
            temp,
            brand,
        ];
        assert_eq!(shown["keys"], JsonValue::from(field.clone()));
        let mut sad = field;
        sad.insert(3, key("reason", "required", false));
        assert_eq!(shown["keys_by_state"], json!({"sad": sad}));
        assert_eq!(shown["data_keys"], json!(["size=", "temp=hot"]));
    }
}
