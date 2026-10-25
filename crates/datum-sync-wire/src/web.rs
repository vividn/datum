//! The wire crate in datum.day's sync worker (`web/src/lib/sync.worker.ts`,
//! da-6kpj5). The store is the real CLI: every read and write is a `datum
//! sync …` command the worker runs through the page's database runner, so
//! no storage logic exists in the browser apart from the CLI's own.

use std::cell::Cell;
use std::rc::Rc;

use anyhow::Result;
use js_sys::{Array, Function, Promise, Reflect};
use serde::Deserialize;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use super::*;

fn js(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

fn text(v: &JsValue, key: &str) -> String {
    Reflect::get(v, &key.into())
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default()
}

/// The CLI, as the worker hands it over: `run(argv, stdin)` resolves to a
/// process's `{code, stdout, stderr}`; `progress(done, of)` hears an
/// apply's.
struct Cli {
    run: Function,
    progress: Function,
    /// The endpoint its node is bound as.
    id: String,
    /// What its Hello says it is: a join's name from then on (da-uyyyg).
    label: std::cell::RefCell<String>,
    version: Version,
    /// The sessions open, dialed and served: one at a time ([`Node::turn`]).
    open: Rc<Cell<usize>>,
}

/// How long a join refused as busy goes on trying ([`Node::connect`]).
const JOIN_WAITS: Duration = Duration::from_secs(30);

/// A dialed session's turn, held: the next may begin once it drops.
struct Turn(Rc<Cell<usize>>);

impl Drop for Turn {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1);
    }
}

impl Cli {
    /// stdout of `datum <argv>`, or its stderr as the error: a [`Refusal`]
    /// when stdout names its [`Why`], as `sync handshake`'s does (da-b6xh6).
    ///
    /// A `sync` command runs `--as` this node's endpoint: on a database
    /// purged or rekeyed under an open session it fails, and writes nothing
    /// (da-otz0d).
    async fn run(&self, argv: &[&str], stdin: Option<&str>) -> Result<String> {
        let as_me = ["--as", self.id.as_str()];
        let (head, tail) = argv.split_at(1.min(argv.len()));
        let as_me = if head == ["sync"] { &as_me[..] } else { &[] };
        let argv = head.iter().chain(as_me).chain(tail);
        let args: Array = argv.map(|a| JsValue::from_str(a)).collect();
        let stdin = stdin.map_or(JsValue::UNDEFINED, JsValue::from_str);
        let done = self
            .run
            .call2(&JsValue::NULL, &args, &stdin)
            .map_err(|e| anyhow!("{e:?}"))?;
        let out = JsFuture::from(Promise::from(done))
            .await
            .map_err(|e| anyhow!("{e:?}"))?;
        let code = Reflect::get(&out, &"code".into())
            .ok()
            .and_then(|c| c.as_f64());
        if code != Some(0.0) {
            let err = text(&out, "stderr");
            // main prints «Error: », the REPL «error: » (da-16d3s).
            let err = err.trim();
            let why = err
                .strip_prefix("Error: ")
                .or_else(|| err.strip_prefix("error: "))
                .unwrap_or(err)
                .to_string();
            return Err(match serde_json::from_str(text(&out, "stdout").trim()) {
                Ok(kind) => Refusal(kind, why).into(),
                Err(_) => anyhow!(why),
            });
        }
        Ok(text(&out, "stdout"))
    }
}

#[derive(Deserialize)]
struct Summary {
    applied: usize,
    conflicts: usize,
    refused: Vec<String>,
}

impl Store for Cli {
    fn label(&self) -> String {
        self.label.borrow().clone()
    }

    async fn chain(&self) -> Result<Option<(String, usize)>> {
        let status: serde_json::Value =
            serde_json::from_str(&self.run(&["sync", "status", "--json"], None).await?)?;
        let Some(id) = status["chain"].as_str() else {
            return Ok(None);
        };
        let all: serde_json::Value =
            serde_json::from_str(&self.run(&["sync", "chain", "list", "--json"], None).await?)?;
        let members = all["members"]
            .as_array()
            .map_or(0, |m| m.iter().filter(|m| m["revoked"].is_null()).count());
        Ok(Some((id.to_string(), members)))
    }

    async fn leave(&self, dry_run: bool) -> Result<()> {
        let argv: &[&str] = match dry_run {
            true => &["sync", "chain", "leave", "--alone", "--dry-run"],
            false => &["sync", "chain", "leave", "--alone"],
        };
        self.run(argv, None).await.map(drop)
    }

    async fn vector(&self) -> Result<Vector> {
        Ok(serde_json::from_str(
            &self.run(&["sync", "pipe", "vector"], None).await?,
        )?)
    }

    async fn ops_since(&self, peer: &str, since: &Vector, limit: usize) -> Result<Vec<String>> {
        let (since, limit) = (serde_json::to_string(since)?, limit.to_string());
        let out = self
            .run(
                &[
                    "sync",
                    "pipe",
                    "ops",
                    "--since",
                    &since,
                    "--no-overlap",
                    "--peer",
                    peer,
                    "--limit",
                    &limit,
                ],
                None,
            )
            .await?;
        Ok(out
            .lines()
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect())
    }

    async fn apply(&self, ops: &[String]) -> Result<Applied> {
        let out = self
            .run(&["sync", "pipe", "apply", "--json"], Some(&ops.join("\n")))
            .await?;
        let s: Summary = serde_json::from_str(&out)?;
        Ok(Applied {
            applied: s.applied,
            conflicts: s.conflicts,
            refused: s.refused,
        })
    }

    fn yields(&self) -> bool {
        true
    }

    /// A page runs one session at a time (da-dwm2y): its CLI runs one
    /// process at a time, so a second session's handshake and vector waited
    /// behind the first's apply or verify — minutes on a big history — with
    /// nothing sent, and after a big join every member but one timed out on
    /// it. One served while another is open, itself counted, is refused as
    /// busy, no failure, as natively while a copy applies (da-h31wt).
    fn applying(&self) -> bool {
        self.open.get() > 1
    }

    /// A page's dial has waited its turn ([`Node::turn`]): a session served
    /// since is the one refused, never the dial (da-tqg19).
    fn dial_busy(&self) -> bool {
        false
    }

    fn serving(&self, _peer: &str, on: bool) {
        let open = self.open.get();
        self.open.set(if on { open + 1 } else { open - 1 });
    }

    fn progress(&self, done: usize, of: usize) {
        let _ = self
            .progress
            .call2(&JsValue::NULL, &(done as u32).into(), &(of as u32).into());
    }

    async fn verify(&self, peer: &str, sums: bool) -> Result<String> {
        let mut argv = vec!["sync", "pipe", "verify", "--peer", peer];
        if !sums {
            argv.push("--ordered");
        }
        Ok(self.run(&argv, None).await?.trim().to_string())
    }

    async fn digest(&self, peer: &str) -> Result<(String, Digest)> {
        let argv = ["sync", "pipe", "digest", "--peer", peer];
        Ok(serde_json::from_str(&self.run(&argv, None).await?)?)
    }

    async fn held(&self, peer: &str, hids: &[String]) -> Result<Vec<serde_json::Value>> {
        let argv = ["sync", "pipe", "held", "--peer", peer];
        let hids = serde_json::to_string(hids)?;
        Ok(serde_json::from_str(&self.run(&argv, Some(&hids)).await?)?)
    }

    async fn handshake(
        &self,
        peer: &str,
        label: &str,
        addr: &EndpointAddr,
        invite: Option<&str>,
        chain: Option<&str>,
        holds: u64,
        version: &Version,
        records: Option<&[String]>,
        dialed: bool,
    ) -> Result<bool> {
        let addr = serde_json::to_string(addr)?;
        let (holds, schema) = (holds.to_string(), version.schema.to_string());
        let mut argv = vec!["sync", "handshake", peer, "--label", label, "--addr", &addr];
        argv.extend(["--holds", &holds]);
        if version.schema > 0 {
            argv.extend(["--datum", &version.datum, "--schema", &schema]);
        }
        if let Some(secret) = invite {
            argv.extend(["--invite", secret]);
        }
        if let Some(chain) = chain {
            argv.push("--chain");
            argv.push(chain);
        }
        if dialed {
            argv.push("--dialed");
        }
        let records = records.map(|r| r.join("\n"));
        if records.is_some() {
            argv.push("--records");
        }
        Ok(self.run(&argv, records.as_deref()).await?.trim() == "added")
    }

    async fn records(&self) -> Result<Vec<String>> {
        let out = self.run(&["sync", "pipe", "records"], None).await?;
        Ok(out.lines().map(String::from).collect())
    }

    fn version(&self) -> Version {
        self.version.clone()
    }

    async fn holds(&self) -> Result<u64> {
        Ok(self.run(&["sync", "holds"], None).await?.trim().parse()?)
    }

    async fn record_session(
        &self,
        peer: &str,
        vector: Option<&Vector>,
        verify: Option<bool>,
        error: Option<&anyhow::Error>,
        offline: bool,
        met: &[EndpointAddr],
    ) -> Result<()> {
        let vector = vector.map(serde_json::to_string).transpose()?;
        let error = error.map(|e| format!("{e:#}"));
        let said = serde_json::to_string(met)?;
        let mut argv = vec!["sync", "record-session", peer];
        if !met.is_empty() {
            argv.extend(["--met", &said]);
        }
        if let Some(v) = &vector {
            argv.extend(["--vector", v]);
        }
        if let Some(m) = verify {
            argv.extend(["--verify", if m { "match" } else { "mismatch" }]);
        }
        if let Some(e) = &error {
            argv.extend(["--error", e]);
        }
        if offline {
            argv.push("--offline");
        }
        self.run(&argv, None).await.map(drop)
    }

    async fn known(&self) -> Result<Vec<EndpointAddr>> {
        let status: serde_json::Value =
            serde_json::from_str(&self.run(&["sync", "status", "--json"], None).await?)?;
        let members = status["members"].as_array().into_iter().flatten();
        Ok(members
            .filter(|m| m["revoked"] != true)
            .filter_map(|m| serde_json::from_str(m["addr"].as_str()?).ok())
            .collect())
    }
}

/// One database's endpoint, and the CLI that is its store.
#[wasm_bindgen]
pub struct Node {
    ep: Endpoint,
    cli: Cli,
}

/// An incoming connection, not yet served.
#[wasm_bindgen]
pub struct Pending(Incoming);

fn report(rep: Result<Report>) -> Result<String, JsError> {
    serde_json::to_string(&rep.map_err(|e| js(format!("{e:#}")))?).map_err(js)
}

/// What a ticket or join link says of itself ([`About`], as JSON): what a
/// page asks before it redeems one (da-l9kdr). Never the secret.
#[wasm_bindgen]
pub fn about(ticket: String) -> Result<String, JsError> {
    serde_json::to_string(&Ticket::decode(&ticket).map_err(js)?.about).map_err(js)
}

#[wasm_bindgen]
impl Node {
    /// Bind the endpoint whose key is `key` (hex, `datum sync
    /// endpoint-key`), on n0's relays or the one `relay` names. `datum`
    /// and `schema` are what the CLI runs (`sync status --json`'s `this`).
    /// `idle` is `DATUM_SYNC_TIMEOUT_S` for a page's tests, every node's.
    #[allow(clippy::too_many_arguments)]
    pub async fn bind(
        key: String,
        relay: Option<String>,
        idle: Option<u32>,
        label: String,
        datum: String,
        schema: u32,
        run: Function,
        progress: Function,
    ) -> Result<Node, JsError> {
        let bytes: Vec<u8> = (0..key.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(key.get(i..i + 2).unwrap_or("zz"), 16))
            .collect::<Result<_, _>>()
            .map_err(js)?;
        let secret: [u8; 32] = bytes.try_into().map_err(|_| js("not an endpoint key"))?;
        IDLE_S.store(idle.unwrap_or(0).into(), Relaxed);
        let net = relay.map_or(Net::N0, Net::Relay);
        let ep = bind(secret, &net).await.map_err(js)?;
        Ok(Node {
            cli: Cli {
                run,
                progress,
                id: ep.id().to_string(),
                label: label.into(),
                version: Version { datum, schema },
                open: Rc::default(),
            },
            ep,
        })
    }

    pub fn id(&self) -> String {
        self.ep.id().to_string()
    }

    /// Where a peer reaches this endpoint, as the JSON `sync_peers` keeps.
    pub fn addr(&self) -> Result<String, JsError> {
        serde_json::to_string(&self.ep.addr()).map_err(js)
    }

    /// The next incoming connection; `undefined` once the endpoint closed.
    pub async fn accept(&self) -> Option<Pending> {
        self.ep.accept().await.map(Pending)
    }

    /// Serve `pending`; its report, as JSON.
    pub async fn serve(&self, pending: Pending) -> Result<String, JsError> {
        report(serve(&self.cli, &self.ep, pending.0).await)
    }

    /// Sync with the peer at `addr` (the JSON `sync_peers` keeps); the
    /// report, as JSON.
    pub async fn dial(&self, addr: String) -> Result<String, JsError> {
        let addr: EndpointAddr = serde_json::from_str(&addr).map_err(js)?;
        let _turn = self.turn().await;
        report(dial(&self.cli, &self.ep, addr, None).await)
    }

    /// `datum sync chain connect [--label]`: redeem `ticket` as `label`,
    /// which the admit records, and sync.
    pub async fn connect(&self, ticket: String, label: Option<String>) -> Result<String, JsError> {
        let ticket = Ticket::decode(&ticket).map_err(js)?;
        if let Some(label) = label {
            self.cli.label.replace(label);
        }
        // A join refused as busy gives up its turn and tries again, for
        // [`JOIN_WAITS`] (da-tqg19): two pages each holding a turn refuse
        // each other's session, and a member's sync met in passing is no
        // reason a join fails. The wait is jittered, so two pages that met
        // do not meet again in step.
        let began = n0_future::time::Instant::now();
        loop {
            let rep = {
                let _turn = self.turn().await;
                redeem(&self.cli, &self.ep, &ticket).await
            };
            let busy = rep.as_ref().err().and_then(failure) == Some(Failure::Refused(Why::Busy));
            if !busy || began.elapsed() > JOIN_WAITS {
                return report(rep);
            }
            let ms = 250.0 + 750.0 * js_sys::Math::random();
            n0_future::time::sleep(Duration::from_millis(ms as u64)).await;
        }
    }

    /// Wait until no session is open, and open one: a dial waits its turn
    /// before it reaches the peer, so the peer waits on nothing
    /// ([`Cli::applying`]).
    async fn turn(&self) -> Turn {
        while self.cli.open.get() > 0 {
            n0_future::time::sleep(Duration::from_millis(100)).await;
        }
        self.cli.open.set(self.cli.open.get() + 1);
        Turn(self.cli.open.clone())
    }

    /// The ticket for an invite whose secret is `secret` — this endpoint's
    /// address, then `also`'s (a JSON list of addresses, a chain's members),
    /// and `about` (JSON, [`About`]).
    pub fn ticket(
        &self,
        secret: String,
        also: Option<String>,
        about: Option<String>,
    ) -> Result<String, JsError> {
        let mut addrs = vec![self.ep.addr()];
        if let Some(also) = also {
            addrs.extend(serde_json::from_str::<Vec<EndpointAddr>>(&also).map_err(js)?);
        }
        let about = about
            .map(|a| serde_json::from_str(&a))
            .transpose()
            .map_err(js)?
            .unwrap_or_default();
        Ticket {
            addrs,
            secret,
            about,
        }
        .encode()
        .map_err(js)
    }

    pub async fn close(&self) {
        self.ep.close().await;
    }
}
