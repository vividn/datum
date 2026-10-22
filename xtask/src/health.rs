//! `cargo xtask health`: how readable and how tangled the Rust under
//! `crates/*/src` is, measured from the syntax tree (`syn`), so it can be
//! watched over time rather than argued about once.
//!
//! Per function: its lines, its **cyclomatic** complexity (1 + each decision:
//! `if`, `else if`, a `match` arm past the first, an arm guard, `while`,
//! `for`, `&&`, `||`), its **cognitive** complexity (SonarSource's idea,
//! simplified: each `if`, `match` and loop costs 1 plus how deeply it is
//! nested, `else` and `else if` cost 1, each `&&`/`||` costs 1), its deepest
//! nesting and its parameter count. Code inside macro invocations is tokens,
//! not syntax, so it counts for nothing, and `?` is not counted as a branch.
//! Test code (`#[cfg(test)]` modules, `#[test]` functions) is measured but
//! kept apart.
//!
//! Per type: **cohesion** as LCOM4 over its inherent methods that take
//! `self`, two methods joined when they touch a common `self.field` or one
//! calls the other. One component is a type doing one job; several are
//! jobs that share a name. Associated functions without `self` and trait
//! impls are left out: neither says what the type's own state holds together
//! (a trait impl answers the trait's contract).
//!
//! Per crate: panics outside tests (`.unwrap()`, `.expect()`, `panic!`,
//! `unreachable!`, `todo!`, `unimplemented!`), `#[allow]`s and `.clone()`s;
//! function bodies duplicated token for token; and module coupling, the
//! `crate::<module>` paths each file names (fan-out) and the files naming
//! each module (fan-in).
//!
//! `--json` prints the whole report, for keeping the numbers over time
//! (vividn, 2026-10-22: tracked, not a gate).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use quote::ToTokens;
use serde::{Deserialize, Serialize};
use syn::spanned::Spanned;
use syn::visit::{self, Visit};

/// Over any of these a function is listed in the summary. Set from the
/// distribution on 2026-10-21 so that roughly the worst percent of non-test
/// functions crosses one; a deliberate commit moves them.
const MAX_COGNITIVE: u32 = 30;
const MAX_LINES: usize = 150;
const MAX_NESTING: u32 = 6;
const MAX_PARAMS: usize = 8;

/// A body this long or longer is compared for duplication.
const DUP_MIN_LINES: usize = 8;

/// Each file of a crate: its module, and the `crate::…` paths it names.
type FileNames = Vec<(Vec<String>, BTreeSet<Vec<String>>)>;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FnMetrics {
    pub krate: String,
    /// `module::Type::name` within the crate.
    pub path: String,
    pub file: String,
    pub line: usize,
    pub lines: usize,
    pub cyclomatic: u32,
    pub cognitive: u32,
    pub nesting: u32,
    pub params: usize,
    pub test: bool,
}

impl FnMetrics {
    fn over(&self) -> Vec<&'static str> {
        let mut why = Vec::new();
        if self.cognitive > MAX_COGNITIVE {
            why.push("cognitive");
        }
        if self.lines > MAX_LINES {
            why.push("lines");
        }
        if self.nesting > MAX_NESTING {
            why.push("nesting");
        }
        if self.params > MAX_PARAMS {
            why.push("params");
        }
        why
    }
}

#[derive(Serialize, Default, Clone, Debug)]
pub struct CrateStats {
    pub files: usize,
    pub code_lines: usize,
    pub fns: usize,
    pub test_fns: usize,
    pub unwraps: usize,
    pub expects: usize,
    pub panics: usize,
    pub allows: usize,
    pub clones: usize,
}

#[derive(Serialize, Clone, Debug)]
pub struct Cohesion {
    pub krate: String,
    pub ty: String,
    pub methods: usize,
    pub components: usize,
    /// The methods of each component, largest first.
    pub groups: Vec<Vec<String>>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Coupling {
    pub krate: String,
    pub module: String,
    /// Other modules of the crate naming this one.
    pub fan_in: usize,
    /// Other modules of the crate this one names.
    pub fan_out: usize,
}

/// A file whose code is both complex and often changed: the product of its
/// non-test functions' cognitive complexity and the commits touching it in
/// the last `HOTSPOT_DAYS` days. Where refactoring pays first.
#[derive(Serialize, Clone, Debug)]
pub struct Hotspot {
    pub file: String,
    pub cognitive: u32,
    pub commits: usize,
    pub score: u64,
}

const HOTSPOT_DAYS: u32 = 90;

/// The headline numbers, computed once here so a record kept over time
/// (daydumb's `health`) reads the thresholds from one place.
#[derive(Serialize, Default, Clone, Debug)]
pub struct Summary {
    pub fns: usize,
    pub test_fns: usize,
    pub code_lines: usize,
    pub cognitive_median: u32,
    pub cognitive_p90: u32,
    pub cognitive_p99: u32,
    pub cognitive_max: u32,
    /// Functions outside tests over any threshold.
    pub over: usize,
    /// Types whose methods fall into more than one LCOM4 component.
    pub split_types: usize,
    pub duplicates: usize,
    pub panics: usize,
    pub allows: usize,
    pub clones: usize,
}

#[derive(Serialize, Clone, Debug)]
pub struct Report {
    pub summary: Summary,
    pub hotspots: Vec<Hotspot>,
    pub fns: Vec<FnMetrics>,
    pub crates: BTreeMap<String, CrateStats>,
    pub cohesion: Vec<Cohesion>,
    pub duplicates: Vec<Vec<String>>,
    pub coupling: Vec<Coupling>,
}

/// Entry for `cargo xtask health [--json] [<dir>]`: `<dir>` is another
/// checkout to measure, so one build can measure any commit (a backfill).
pub fn run(args: Vec<String>, root: &Path) -> Result<()> {
    let json = args.iter().any(|a| a == "--json");
    let dirs: Vec<&String> = args.iter().filter(|a| *a != "--json").collect();
    let root = match dirs.as_slice() {
        [] => root.to_path_buf(),
        [d] if !d.starts_with('-') => PathBuf::from(d),
        _ => bail!("usage: health [--json] [<dir>]"),
    };
    let report = measure(&root)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_summary(&report);
    }
    Ok(())
}

/// Every `.rs` under `crates/*/src`, measured.
pub fn measure(root: &Path) -> Result<Report> {
    let mut fns = Vec::new();
    let mut crates: BTreeMap<String, CrateStats> = BTreeMap::new();
    let mut methods: BTreeMap<(String, String), Vec<Method>> = BTreeMap::new();
    let mut bodies: HashMap<String, Vec<String>> = HashMap::new();
    // Per crate: each file's module and the `crate::…` paths it names; and every module the crate defines.
    let mut named: BTreeMap<String, FileNames> = BTreeMap::new();
    let mut defined: BTreeMap<String, BTreeSet<Vec<String>>> = BTreeMap::new();
    let mut crate_dirs: Vec<PathBuf> = fs::read_dir(root.join("crates"))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join("src").is_dir())
        .collect();
    crate_dirs.sort();
    for dir in crate_dirs {
        let krate = dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let src = dir.join("src");
        let stats = crates.entry(krate.clone()).or_default();
        for file in rs_files(&src)? {
            let text =
                fs::read_to_string(&file).with_context(|| format!("reading {}", file.display()))?;
            let ast =
                syn::parse_file(&text).with_context(|| format!("parsing {}", file.display()))?;
            let rel = file
                .strip_prefix(root)
                .unwrap_or(&file)
                .display()
                .to_string();
            stats.files += 1;
            stats.code_lines += text.lines().filter(|l| !l.trim().is_empty()).count();
            let module = module_of(&src, &file);
            let mut c = Collector {
                krate: &krate,
                file: &rel,
                module: module.clone(),
                self_ty: None,
                test: false,
                fns: &mut fns,
                stats: &mut *stats,
                methods: &mut methods,
                bodies: &mut bodies,
                names: BTreeSet::new(),
                defines: BTreeSet::new(),
            };
            c.visit_file(&ast);
            let (paths, mods) = (c.names, c.defines);
            let known = defined.entry(krate.clone()).or_default();
            for i in 0..=module.len() {
                known.insert(module[..i].to_vec());
            }
            known.extend(mods);
            named
                .entry(krate.clone())
                .or_default()
                .push((module, paths));
        }
    }
    for f in &fns {
        let s = crates.entry(f.krate.clone()).or_default();
        if f.test {
            s.test_fns += 1;
        } else {
            s.fns += 1;
        }
    }
    let cohesion = methods
        .into_iter()
        .filter_map(|((krate, ty), ms)| lcom4(krate, ty, ms))
        .collect();
    let mut duplicates: Vec<Vec<String>> = bodies.into_values().filter(|v| v.len() > 1).collect();
    duplicates.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
    let hotspots = hotspots(root, &fns)?;
    let mut r = Report {
        summary: Summary::default(),
        hotspots,
        fns,
        crates,
        cohesion,
        duplicates,
        coupling: coupling(&named, &defined),
    };
    r.summary = summarise(&r);
    Ok(r)
}

fn summarise(r: &Report) -> Summary {
    let live: Vec<&FnMetrics> = r.fns.iter().filter(|f| !f.test).collect();
    let mut cog: Vec<u32> = live.iter().map(|f| f.cognitive).collect();
    cog.sort_unstable();
    let sum = |f: fn(&CrateStats) -> usize| r.crates.values().map(f).sum();
    Summary {
        fns: live.len(),
        test_fns: r.fns.len() - live.len(),
        code_lines: sum(|c| c.code_lines),
        cognitive_median: percentile(&cog, 0.5),
        cognitive_p90: percentile(&cog, 0.9),
        cognitive_p99: percentile(&cog, 0.99),
        cognitive_max: cog.last().copied().unwrap_or(0),
        over: live.iter().filter(|f| !f.over().is_empty()).count(),
        split_types: r.cohesion.iter().filter(|c| c.components > 1).count(),
        duplicates: r.duplicates.len(),
        panics: sum(|c| c.unwraps + c.expects + c.panics),
        allows: sum(|c| c.allows),
        clones: sum(|c| c.clones),
    }
}

/// Commits per file under `crates/` in the last `HOTSPOT_DAYS` days, from
/// `git log`, joined to each file's summed cognitive complexity.
fn hotspots(root: &Path, fns: &[FnMetrics]) -> Result<Vec<Hotspot>> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "log",
            &format!("--since={HOTSPOT_DAYS}.days"),
            "--format=",
            "--name-only",
            "--",
            "crates/",
        ])
        .output();
    // Hotspots are a reading, not a gate: without git (or its history) the
    // rest of the report and the ratchet still stand.
    let out = match out {
        Ok(o) if o.status.success() => o,
        Ok(o) => {
            eprintln!(
                "health: no hotspots, git log failed: {}",
                String::from_utf8_lossy(&o.stderr).trim()
            );
            return Ok(Vec::new());
        }
        Err(e) => {
            eprintln!("health: no hotspots, git did not run: {e}");
            return Ok(Vec::new());
        }
    };
    let mut commits: HashMap<String, usize> = HashMap::new();
    for line in String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.is_empty())
    {
        *commits.entry(line.to_string()).or_default() += 1;
    }
    let mut cognitive: HashMap<&str, u32> = HashMap::new();
    for f in fns.iter().filter(|f| !f.test) {
        *cognitive.entry(f.file.as_str()).or_default() += f.cognitive;
    }
    let mut spots: Vec<Hotspot> = cognitive
        .into_iter()
        .map(|(file, cog)| {
            let n = commits.get(file).copied().unwrap_or(0);
            Hotspot {
                file: file.to_string(),
                cognitive: cog,
                commits: n,
                score: cog as u64 * n as u64,
            }
        })
        .collect();
    spots.sort_by(|a, b| b.score.cmp(&a.score).then(a.file.cmp(&b.file)));
    Ok(spots)
}

fn rs_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in fs::read_dir(&d).with_context(|| format!("reading {}", d.display()))? {
            let p = entry?.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|e| e == "rs") {
                out.push(p);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// `src/a/b.rs` and `src/a/b/mod.rs` are module `a::b`; `src/lib.rs` and
/// `src/main.rs` are the crate root, written as the empty path.
fn module_of(src: &Path, file: &Path) -> Vec<String> {
    let rel = file.strip_prefix(src).unwrap_or(file);
    let mut parts: Vec<String> = rel
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    if let Some(last) = parts.pop() {
        let stem = last.trim_end_matches(".rs").to_string();
        if !matches!(stem.as_str(), "mod" | "lib" | "main") {
            parts.push(stem);
        }
    }
    parts
}

/// A module as a coupling node: its first two segments, enough to tell
/// `commands::log_core` from `commands::sync` where everything lives under
/// `commands`.
fn node(module: &[String]) -> String {
    if module.is_empty() {
        "(root)".into()
    } else {
        module[..module.len().min(2)].join("::")
    }
}

struct Method {
    name: String,
    fields: BTreeSet<String>,
    calls: BTreeSet<String>,
}

struct Collector<'a> {
    krate: &'a str,
    file: &'a str,
    module: Vec<String>,
    self_ty: Option<String>,
    test: bool,
    fns: &'a mut Vec<FnMetrics>,
    stats: &'a mut CrateStats,
    methods: &'a mut BTreeMap<(String, String), Vec<Method>>,
    bodies: &'a mut HashMap<String, Vec<String>>,
    /// The `crate::…` paths this file names, without the `crate`.
    names: BTreeSet<Vec<String>>,
    /// Inline `mod x { … }`s this file defines, as full module paths.
    defines: BTreeSet<Vec<String>>,
}

/// `#[test]` (or `#[tokio::test]` and the like), or a `cfg` naming the
/// `test` predicate. The predicate is an identifier token, so
/// `cfg(feature = "test-utils")`, a string, is not test code (canary-kimi-k3
/// on #1028).
fn is_test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        let p = a.path();
        p.segments.last().is_some_and(|s| s.ident == "test")
            || (p.is_ident("cfg") && names_test(a.meta.to_token_stream()))
    })
}

/// Whether a token stream holds the identifier `test`, at any depth.
fn names_test(tokens: proc_macro2::TokenStream) -> bool {
    tokens.into_iter().any(|t| match t {
        proc_macro2::TokenTree::Ident(i) => i == "test",
        proc_macro2::TokenTree::Group(g) => names_test(g.stream()),
        _ => false,
    })
}

impl Collector<'_> {
    fn record(
        &mut self,
        name: &str,
        attrs: &[syn::Attribute],
        sig: &syn::Signature,
        body: &syn::Block,
        whole: proc_macro2::Span,
    ) {
        let mut cx = Complexity {
            cyclomatic: 1,
            ..Complexity::default()
        };
        cx.visit_block(body);
        let mut path = self.module.clone();
        if let Some(t) = &self.self_ty {
            path.push(t.clone());
        }
        path.push(name.to_string());
        let start = whole.start().line;
        let lines = whole.end().line.saturating_sub(start) + 1;
        let test = self.test || is_test_attr(attrs);
        let params = sig
            .inputs
            .iter()
            .filter(|i| matches!(i, syn::FnArg::Typed(_)))
            .count();
        let m = FnMetrics {
            krate: self.krate.to_string(),
            path: path.join("::"),
            file: self.file.to_string(),
            line: start,
            lines,
            cyclomatic: cx.cyclomatic,
            cognitive: cx.cognitive,
            nesting: cx.max_nesting,
            params,
            test,
        };
        if !test && lines >= DUP_MIN_LINES {
            self.bodies
                .entry(body.to_token_stream().to_string())
                .or_default()
                .push(format!("{}:{} {}", m.file, m.line, m.path));
        }
        self.fns.push(m);
    }

    fn with_test<F: FnOnce(&mut Self)>(&mut self, attrs: &[syn::Attribute], f: F) {
        let was = self.test;
        self.test = was || is_test_attr(attrs);
        f(self);
        self.test = was;
    }
}

fn type_name(ty: &syn::Type) -> String {
    match ty {
        syn::Type::Path(p) => p
            .path
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_default(),
        other => other.to_token_stream().to_string(),
    }
}

impl<'ast> Visit<'ast> for Collector<'_> {
    fn visit_item_mod(&mut self, m: &'ast syn::ItemMod) {
        if m.content.is_none() {
            return;
        }
        self.module.push(m.ident.to_string());
        self.defines.insert(self.module.clone());
        self.with_test(&m.attrs, |c| visit::visit_item_mod(c, m));
        self.module.pop();
    }

    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        self.with_test(&f.attrs, |c| {
            let outer = c.self_ty.take();
            c.record(
                &f.sig.ident.to_string(),
                &f.attrs,
                &f.sig,
                &f.block,
                f.span(),
            );
            visit::visit_item_fn(c, f);
            c.self_ty = outer;
        });
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        let ty = type_name(&i.self_ty);
        self.with_test(&i.attrs, |c| {
            let outer = c.self_ty.replace(ty.clone());
            for item in &i.items {
                if let syn::ImplItem::Fn(f) = item {
                    c.with_test(&f.attrs, |c| {
                        c.record(
                            &f.sig.ident.to_string(),
                            &f.attrs,
                            &f.sig,
                            &f.block,
                            f.span(),
                        );
                        visit::visit_block(c, &f.block);
                    });
                    if i.trait_.is_none() && f.sig.receiver().is_some() && !c.test {
                        let mut s = SelfUse::default();
                        s.visit_block(&f.block);
                        c.methods
                            .entry((c.krate.to_string(), ty.clone()))
                            .or_default()
                            .push(Method {
                                name: f.sig.ident.to_string(),
                                fields: s.fields,
                                calls: s.calls,
                            });
                    }
                }
            }
            c.self_ty = outer;
        });
    }

    fn visit_item_trait(&mut self, t: &'ast syn::ItemTrait) {
        let outer = self.self_ty.replace(t.ident.to_string());
        for item in &t.items {
            if let syn::TraitItem::Fn(f) = item {
                if let Some(body) = &f.default {
                    self.record(&f.sig.ident.to_string(), &f.attrs, &f.sig, body, f.span());
                    visit::visit_block(self, body);
                }
            }
        }
        self.self_ty = outer;
    }

    fn visit_expr_method_call(&mut self, e: &'ast syn::ExprMethodCall) {
        if !self.test {
            match e.method.to_string().as_str() {
                "unwrap" => self.stats.unwraps += 1,
                "expect" => self.stats.expects += 1,
                "clone" => self.stats.clones += 1,
                _ => {}
            }
        }
        visit::visit_expr_method_call(self, e);
    }

    fn visit_macro(&mut self, m: &'ast syn::Macro) {
        if !self.test
            && m.path.segments.last().is_some_and(|s| {
                matches!(
                    s.ident.to_string().as_str(),
                    "panic" | "unreachable" | "todo" | "unimplemented"
                )
            })
        {
            self.stats.panics += 1;
        }
        visit::visit_macro(self, m);
    }

    fn visit_attribute(&mut self, a: &'ast syn::Attribute) {
        if !self.test && a.path().is_ident("allow") {
            self.stats.allows += 1;
        }
    }

    fn visit_path(&mut self, p: &'ast syn::Path) {
        if p.segments.first().is_some_and(|s| s.ident == "crate") {
            self.names.insert(
                p.segments
                    .iter()
                    .skip(1)
                    .map(|s| s.ident.to_string())
                    .collect(),
            );
        }
        visit::visit_path(self, p);
    }

    fn visit_use_tree(&mut self, t: &'ast syn::UseTree) {
        if let syn::UseTree::Path(p) = t {
            if p.ident == "crate" {
                use_paths(&p.tree, &mut Vec::new(), &mut self.names);
                return;
            }
        }
        visit::visit_use_tree(self, t);
    }
}

/// Every path under `use crate::{…}`, as segments (a glob names its prefix).
fn use_paths(t: &syn::UseTree, prefix: &mut Vec<String>, out: &mut BTreeSet<Vec<String>>) {
    match t {
        syn::UseTree::Path(p) => {
            prefix.push(p.ident.to_string());
            use_paths(&p.tree, prefix, out);
            prefix.pop();
        }
        syn::UseTree::Name(n) => {
            let mut v = prefix.clone();
            v.push(n.ident.to_string());
            out.insert(v);
        }
        syn::UseTree::Rename(r) => {
            let mut v = prefix.clone();
            v.push(r.ident.to_string());
            out.insert(v);
        }
        syn::UseTree::Group(g) => g.items.iter().for_each(|i| use_paths(i, prefix, out)),
        syn::UseTree::Glob(_) => {
            out.insert(prefix.clone());
        }
    }
}

#[derive(Default)]
struct Complexity {
    cyclomatic: u32,
    cognitive: u32,
    nesting: u32,
    max_nesting: u32,
}

impl Complexity {
    fn nested<F: FnOnce(&mut Self)>(&mut self, f: F) {
        self.nesting += 1;
        self.max_nesting = self.max_nesting.max(self.nesting);
        f(self);
        self.nesting -= 1;
    }

    /// A structure that branches: +1 decision, and costs 1 plus its depth.
    fn structure(&mut self) {
        self.cyclomatic += 1;
        self.cognitive += 1 + self.nesting;
    }

    fn if_chain(&mut self, e: &syn::ExprIf, first: bool) {
        if first {
            self.structure();
        } else {
            self.cyclomatic += 1;
            self.cognitive += 1;
        }
        self.visit_expr(&e.cond);
        self.nested(|c| c.visit_block(&e.then_branch));
        if let Some((_, els)) = &e.else_branch {
            match &**els {
                syn::Expr::If(elif) => self.if_chain(elif, false),
                other => {
                    self.cognitive += 1;
                    self.nested(|c| c.visit_expr(other));
                }
            }
        }
    }
}

impl<'ast> Visit<'ast> for Complexity {
    fn visit_item(&mut self, _: &'ast syn::Item) {
        // A nested item is measured on its own, not as part of this body.
    }

    fn visit_expr_if(&mut self, e: &'ast syn::ExprIf) {
        self.if_chain(e, true);
    }

    fn visit_expr_match(&mut self, e: &'ast syn::ExprMatch) {
        self.cyclomatic += e.arms.len().saturating_sub(1) as u32;
        self.cognitive += 1 + self.nesting;
        self.visit_expr(&e.expr);
        self.nested(|c| {
            for arm in &e.arms {
                if let Some((_, guard)) = &arm.guard {
                    c.cyclomatic += 1;
                    c.visit_expr(guard);
                }
                c.visit_expr(&arm.body);
            }
        });
    }

    fn visit_expr_while(&mut self, e: &'ast syn::ExprWhile) {
        self.structure();
        self.visit_expr(&e.cond);
        self.nested(|c| c.visit_block(&e.body));
    }

    fn visit_expr_for_loop(&mut self, e: &'ast syn::ExprForLoop) {
        self.structure();
        self.visit_expr(&e.expr);
        self.nested(|c| c.visit_block(&e.body));
    }

    fn visit_expr_loop(&mut self, e: &'ast syn::ExprLoop) {
        self.cognitive += 1 + self.nesting;
        self.nested(|c| c.visit_block(&e.body));
    }

    fn visit_expr_closure(&mut self, e: &'ast syn::ExprClosure) {
        self.nested(|c| c.visit_expr(&e.body));
    }

    fn visit_expr_binary(&mut self, e: &'ast syn::ExprBinary) {
        if matches!(e.op, syn::BinOp::And(_) | syn::BinOp::Or(_)) {
            self.cyclomatic += 1;
            self.cognitive += 1;
        }
        visit::visit_expr_binary(self, e);
    }
}

/// The `self.field`s a method touches and the `self.method()`s it calls.
#[derive(Default)]
struct SelfUse {
    fields: BTreeSet<String>,
    calls: BTreeSet<String>,
}

fn is_self(e: &syn::Expr) -> bool {
    matches!(e, syn::Expr::Path(p) if p.path.is_ident("self"))
}

impl<'ast> Visit<'ast> for SelfUse {
    fn visit_expr_field(&mut self, e: &'ast syn::ExprField) {
        if is_self(&e.base) {
            self.fields.insert(e.member.to_token_stream().to_string());
        } else {
            visit::visit_expr_field(self, e);
        }
    }

    fn visit_expr_method_call(&mut self, e: &'ast syn::ExprMethodCall) {
        if is_self(&e.receiver) {
            self.calls.insert(e.method.to_string());
            e.args.iter().for_each(|a| self.visit_expr(a));
        } else {
            visit::visit_expr_method_call(self, e);
        }
    }

    /// `self` used whole (`match self`, `*self`, passed on): the state an
    /// enum's methods share, written as the field `<self>`.
    fn visit_expr_path(&mut self, e: &'ast syn::ExprPath) {
        if e.path.is_ident("self") {
            self.fields.insert("<self>".into());
        }
    }
}

/// LCOM4: connected components among the methods, joined by a shared field
/// or a call. `None` below four methods, where the number says little.
fn lcom4(krate: String, ty: String, methods: Vec<Method>) -> Option<Cohesion> {
    if methods.len() < 4 {
        return None;
    }
    let n = methods.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut [usize], i: usize) -> usize {
        let mut r = i;
        while p[r] != r {
            r = p[r];
        }
        p[i] = r;
        r
    }
    let index: HashMap<&str, usize> = methods
        .iter()
        .enumerate()
        .map(|(i, m)| (m.name.as_str(), i))
        .collect();
    for i in 0..n {
        for j in (i + 1)..n {
            let joined = !methods[i].fields.is_disjoint(&methods[j].fields)
                || methods[i].calls.contains(&methods[j].name)
                || methods[j].calls.contains(&methods[i].name);
            if joined {
                let (a, b) = (find(&mut parent, i), find(&mut parent, j));
                parent[a] = b;
            }
        }
        for callee in &methods[i].calls {
            if let Some(&j) = index.get(callee.as_str()) {
                let (a, b) = (find(&mut parent, i), find(&mut parent, j));
                parent[a] = b;
            }
        }
    }
    let mut groups: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for (i, m) in methods.iter().enumerate() {
        let r = find(&mut parent, i);
        groups.entry(r).or_default().push(m.name.clone());
    }
    let mut groups: Vec<Vec<String>> = groups.into_values().collect();
    groups.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
    Some(Cohesion {
        krate,
        ty,
        methods: n,
        components: groups.len(),
        groups,
    })
}

/// Module nodes (two segments deep) and the edges between them: a file in
/// one naming a path that resolves, by its longest prefix the crate defines
/// as a module, into another.
fn coupling(
    named: &BTreeMap<String, FileNames>,
    defined: &BTreeMap<String, BTreeSet<Vec<String>>>,
) -> Vec<Coupling> {
    let mut out = Vec::new();
    for (krate, files) in named {
        let known = &defined[krate];
        let mut edges: BTreeSet<(String, String)> = BTreeSet::new();
        let mut nodes: BTreeSet<String> = BTreeSet::new();
        for (module, paths) in files {
            let from = node(module);
            nodes.insert(from.clone());
            for p in paths {
                let depth = (0..=p.len())
                    .rev()
                    .find(|&i| known.contains(&p[..i].to_vec()))
                    .unwrap_or(0);
                let to = node(&p[..depth]);
                if to != from {
                    edges.insert((from.clone(), to));
                }
            }
        }
        for n in nodes {
            let fan_in = edges.iter().filter(|(_, t)| *t == n).count();
            let fan_out = edges.iter().filter(|(f, _)| *f == n).count();
            out.push(Coupling {
                krate: krate.clone(),
                module: n,
                fan_in,
                fan_out,
            });
        }
    }
    out
}

fn percentile(sorted: &[u32], p: f64) -> u32 {
    if sorted.is_empty() {
        return 0;
    }
    let i = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[i]
}

fn print_summary(r: &Report) {
    let live: Vec<&FnMetrics> = r.fns.iter().filter(|f| !f.test).collect();
    let s = &r.summary;
    println!(
        "datum code health: {} functions outside tests, {} in tests",
        s.fns, s.test_fns
    );
    println!(
        "cognitive complexity: median {}, p90 {}, p99 {}, max {}",
        s.cognitive_median, s.cognitive_p90, s.cognitive_p99, s.cognitive_max
    );
    println!();
    println!(
        "{:<20} {:>5} {:>7} {:>5} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6}",
        "crate", "files", "lines", "fns", "tests", "unwrap", "expect", "panic", "allow", "clone"
    );
    for (k, s) in &r.crates {
        println!(
            "{:<20} {:>5} {:>7} {:>5} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6}",
            k,
            s.files,
            s.code_lines,
            s.fns,
            s.test_fns,
            s.unwraps,
            s.expects,
            s.panics,
            s.allows,
            s.clones
        );
    }
    let top = |title: &str, key: &dyn Fn(&FnMetrics) -> u64| {
        let mut v = live.clone();
        v.sort_by_key(|f| std::cmp::Reverse(key(f)));
        println!("\n{title}");
        for f in v.iter().take(15) {
            println!(
                "  {:>4} cog {:>4} cyc {:>4} lines {:>2} deep {:>2} args  {}::{}  ({}:{})",
                f.cognitive,
                f.cyclomatic,
                f.lines,
                f.nesting,
                f.params,
                f.krate,
                f.path,
                f.file,
                f.line
            );
        }
    };
    println!("\nhotspots: complexity × change (summed cognitive × commits in {HOTSPOT_DAYS} days)");
    for h in r.hotspots.iter().take(12) {
        println!(
            "  {:>7}  {:>5} cog × {:>3} commits  {}",
            h.score, h.cognitive, h.commits, h.file
        );
    }
    top("most cognitively complex functions", &|f| {
        f.cognitive as u64
    });
    top("longest functions", &|f| f.lines as u64);
    println!(
        "\n{} function(s) over a threshold (cognitive {MAX_COGNITIVE}, lines {MAX_LINES}, nesting {MAX_NESTING}, params {MAX_PARAMS})",
        s.over
    );
    let mut low: Vec<&Cohesion> = r.cohesion.iter().filter(|c| c.components > 1).collect();
    low.sort_by(|a, b| {
        b.components
            .cmp(&a.components)
            .then(b.methods.cmp(&a.methods))
    });
    println!(
        "\ntypes whose methods fall into separate groups (LCOM4 > 1, of {} types with 4+ methods)",
        r.cohesion.len()
    );
    for c in low.iter().take(15) {
        let shown: Vec<String> = c.groups.iter().take(4).map(|g| g.join(" ")).collect();
        println!(
            "  {} groups / {} methods  {}::{}  [{}]",
            c.components,
            c.methods,
            c.krate,
            c.ty,
            shown.join(" | ")
        );
    }
    println!(
        "\nfunction bodies duplicated token for token ({DUP_MIN_LINES}+ lines): {} group(s)",
        r.duplicates.len()
    );
    for g in r.duplicates.iter().take(10) {
        println!("  {}", g.join("\n    = "));
    }
    let mut cp = r.coupling.clone();
    cp.sort_by_key(|c| std::cmp::Reverse(c.fan_in));
    println!("\nmost depended-on modules within their crate (fan-in = other modules naming it)");
    for c in cp.iter().take(12) {
        println!(
            "  in {:>3} out {:>3}  {}::{}",
            c.fan_in, c.fan_out, c.krate, c.module
        );
    }
    cp.sort_by_key(|c| std::cmp::Reverse(c.fan_out));
    println!("\nmodules that reach into the most others (fan-out)");
    for c in cp.iter().take(8) {
        println!(
            "  out {:>3} in {:>3}  {}::{}",
            c.fan_out, c.fan_in, c.krate, c.module
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn complexity_of(src: &str) -> Complexity {
        let f: syn::ItemFn = syn::parse_str(src).unwrap();
        let mut cx = Complexity {
            cyclomatic: 1,
            ..Complexity::default()
        };
        cx.visit_block(&f.block);
        cx
    }

    #[test]
    fn complexity_counts_each_decision_and_weights_nesting() {
        // cyclomatic 1 + if + && + for + inner if + else if + two match arms past the first = 8;
        // cognitive if 1 + && 1 + for (depth 1) 2 + inner if (depth 2) 3 + else if 1 + else 1 + match 1 = 10.
        let cx = complexity_of(
            "fn f(a: bool, b: bool, xs: &[u8]) -> u8 {
                if a && b {
                    for x in xs {
                        if *x > 3 { return *x; }
                    }
                } else if b {
                    return 1;
                } else {
                    return 2;
                }
                match xs.len() { 0 => 0, 1 => 1, _ => 2 }
            }",
        );
        assert_eq!((cx.cyclomatic, cx.cognitive, cx.max_nesting), (8, 10, 3));
    }

    #[test]
    fn duplicates_compare_tokens_not_layout_or_comments() {
        let a: syn::Block = syn::parse_str("{ let x = 1; // one\n x + 2 }").unwrap();
        let b: syn::Block = syn::parse_str("{\n    let x=1;\n\n    /* a note */ x+2\n}").unwrap();
        assert_eq!(
            a.to_token_stream().to_string(),
            b.to_token_stream().to_string()
        );
    }

    #[test]
    fn a_cfg_is_test_code_only_when_it_names_the_test_predicate() {
        let attrs = |src: &str| {
            syn::parse_str::<syn::ItemMod>(&format!("{src} mod m {{}}"))
                .unwrap()
                .attrs
        };
        assert!(is_test_attr(&attrs("#[cfg(test)]")));
        assert!(is_test_attr(&attrs(
            "#[cfg(all(test, feature = \"native\"))]"
        )));
        assert!(!is_test_attr(&attrs("#[cfg(feature = \"test-utils\")]")));
        assert!(!is_test_attr(&attrs("#[cfg(testing)]")));
    }

    #[test]
    fn a_nested_fn_is_not_part_of_the_body_around_it() {
        let cx = complexity_of("fn f() { fn g(a: bool) { if a { if a {} } } }");
        assert_eq!((cx.cyclomatic, cx.cognitive), (1, 0));
    }

    fn method(name: &str, fields: &[&str], calls: &[&str]) -> Method {
        Method {
            name: name.into(),
            fields: fields.iter().map(|s| s.to_string()).collect(),
            calls: calls.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn lcom4_joins_methods_by_shared_field_or_call() {
        let c = lcom4(
            "k".into(),
            "T".into(),
            vec![
                method("a", &["x"], &[]),
                method("b", &["x"], &[]),
                method("c", &[], &["d"]),
                method("d", &["y"], &[]),
                method("e", &[], &[]),
            ],
        )
        .unwrap();
        assert_eq!(c.components, 3);
        assert_eq!(c.groups[0].len(), 2);
    }
}
