//! The wire crate in datum.day's sync worker (`web/src/lib/sync.worker.ts`,
//! da-6kpj5). The store is the real CLI: every read and write is a `datum
//! sync …` command the worker runs through the page's database runner, so
//! no storage logic exists in the browser apart from the CLI's own.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use anyhow::Result;
use futures_util::future::{AbortHandle, Abortable};
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
/// apply's, and `serving(peer, on)` a served session begin and end.
struct Cli {
    run: Function,
    progress: Function,
    serving: Function,
    /// The endpoint its node is bound as.
    id: String,
    /// What its Hello says it is: a join's name from then on (da-uyyyg).
    label: RefCell<String>,
    version: Version,
    /// The sessions open, dialed and served: one at a time ([`Node::turn`]).
    open: Rc<Cell<usize>>,
    /// The page's own dial ([`Node::dial`]), while there is one.
    dialing: RefCell<Option<Dialing>>,
}

/// The page's own dial, holding its turn. Until it is past its handshake a
/// session served takes the turn from it (da-gvia0): a dial arriving is a
/// peer online with something to send, and this one may be connecting to a
/// copy that is offline. Of two copies dialing each other the lower
/// endpoint id's dial goes on, as natively (da-vj3ly).
struct Dialing {
    peer: String,
    /// Past its handshake: never given way.
    carried: bool,
    /// Once it reached the peer: closed as busy, which the peer records
    /// as nothing (da-h31wt).
    conn: Option<Connection>,
    abort: AbortHandle,
    /// Who it gave way to.
    by: Rc<RefCell<Option<String>>>,
    _turn: Turn,
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

    /// `chain list` names no chain where `sync status` names none: in
    /// none, joining one, or one a purge dissolved (da-tvv4j).
    async fn chain(&self) -> Result<Option<(String, usize)>> {
        let all: serde_json::Value =
            serde_json::from_str(&self.run(&["sync", "chain", "list", "--json"], None).await?)?;
        let Some(id) = all["chain"].as_str() else {
            return Ok(None);
        };
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

    /// The session open may be a dial still connecting, applying nothing,
    /// and a copy that dialed without serving (a bare `datum sync`) is
    /// dialed back by no one: the native words promised both (da-qlffb).
    fn busy_why(&self) -> &'static str {
        "is in another sync session and runs one at a time; sync again once it is over"
    }

    /// A page's dial has waited its turn ([`Node::turn`]): a session served
    /// since is the one refused, never the dial (da-tqg19).
    fn dial_busy(&self) -> bool {
        false
    }

    fn serving(&self, peer: &str, on: bool) {
        let open = self.open.get();
        self.open.set(if on { open + 1 } else { open - 1 });
        let gives_way = |d: &Dialing| !d.carried && (d.peer != peer || self.id.as_str() > peer);
        if on && self.dialing.borrow().as_ref().is_some_and(gives_way) {
            let d = self.dialing.take().expect("a dial");
            if let Some(conn) = &d.conn {
                conn.close(0u32.into(), CLOSED_BUSY);
            }
            d.by.replace(Some(peer.to_string()));
            d.abort.abort();
        }
        let _ = self.serving.call2(&JsValue::NULL, &peer.into(), &on.into());
    }

    /// The page's dial to `peer`, when it is the one of two crossed that
    /// goes on, or is past its handshake: the peer's is refused as given way.
    fn carries(&self, peer: &str) -> bool {
        let dialing = self.dialing.borrow();
        dialing
            .as_ref()
            .is_some_and(|d| d.peer == peer && (d.carried || self.id.as_str() < peer))
    }

    fn carrying(&self, peer: &str, on: bool) {
        if let Some(d) = self
            .dialing
            .borrow_mut()
            .as_mut()
            .filter(|d| d.peer == peer)
        {
            d.carried |= on;
        }
    }

    fn reached(&self, conn: &Connection) {
        if let Some(d) = self.dialing.borrow_mut().as_mut() {
            d.conn = Some(conn.clone());
        }
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
        let floor = version.floor.map(|f| f.to_string());
        let mut argv = vec!["sync", "handshake", peer, "--label", label, "--addr", &addr];
        argv.extend(["--holds", &holds]);
        if version.schema > 0 {
            argv.extend(["--datum", &version.datum, "--schema", &schema]);
        }
        if let Some(floor) = &floor {
            argv.extend(["--floor", floor]);
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

    async fn hold(&self, records: &[String]) -> Result<()> {
        self.run(&["sync", "pipe", "hold"], Some(&records.join("\n")))
            .await?;
        Ok(())
    }

    fn version(&self) -> Version {
        self.version.clone()
    }

    async fn mark_gave_way(&self) -> Result<()> {
        self.run(&["sync", "pending-join", "--gave-way"], None)
            .await?;
        Ok(())
    }

    async fn marked_gave_way(&self) -> Result<bool> {
        let held = self.run(&["sync", "pending-join"], None).await?;
        let j: serde_json::Value = serde_json::from_str(held.trim())?;
        Ok(j["gave_way"].as_bool().unwrap_or(false))
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

    /// The native store's `known`, not `sync status`, the dearest read a
    /// session made (da-tvv4j).
    async fn known(&self) -> Result<Vec<EndpointAddr>> {
        let out = self.run(&["sync", "pipe", "known"], None).await?;
        Ok(out
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect())
    }
}

/// One database's endpoint, and the CLI that is its store.
#[wasm_bindgen]
pub struct Node {
    ep: Endpoint,
    cli: Cli,
}

/// A session arriving, not yet served ([`accept`]).
#[wasm_bindgen]
pub struct Pending(Arriving);

fn report(rep: Result<Report>) -> Result<String, JsError> {
    serde_json::to_string(&rep.map_err(|e| js(format!("{e:#}")))?).map_err(js)
}

/// What a ticket or join link says of itself ([`About`], as JSON): what a
/// page asks before it redeems one (da-l9kdr). Never the secret. `relay`
/// says whether the inviter's address names its relay: one that does not,
/// made before the page reached it, no other device reaches (da-y9voq).
#[wasm_bindgen]
pub fn about(ticket: String) -> Result<String, JsError> {
    let ticket = Ticket::decode(&ticket).map_err(js)?;
    let mut about = serde_json::to_value(&ticket.about).map_err(js)?;
    let relay = ticket
        .addrs
        .first()
        .is_some_and(|a| a.relay_urls().next().is_some());
    about["relay"] = relay.into();
    serde_json::to_string(&about).map_err(js)
}

/// Sign a waiting `datum sync login` in (da-mzqs4): send `message`, the
/// account sealed to its device key, to the CLI `link` names, from an
/// endpoint of its own under `key` (hex, fresh), on n0's relays or `relay`.
/// Its refusal is the error.
#[wasm_bindgen]
pub async fn login(
    link: String,
    key: String,
    relay: Option<String>,
    message: String,
) -> Result<(), JsError> {
    let to = Login::decode(&link).map_err(js)?;
    let ep = bind_dialer(secret_of(&key)?, &relay.map_or(Net::N0, Net::Relay))
        .await
        .map_err(js)?;
    let sent = login_send(&ep, &to, message.as_bytes()).await;
    ep.close().await;
    sent.map_err(|e| js(format!("{e:#}")))
}

/// 32 bytes of hex: a key.
fn secret_of(key: &str) -> Result<[u8; 32], JsError> {
    let bytes: Vec<u8> = (0..key.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(key.get(i..i + 2).unwrap_or("zz"), 16))
        .collect::<Result<_, _>>()
        .map_err(js)?;
    bytes.try_into().map_err(|_| js("not an endpoint key"))
}

#[wasm_bindgen]
impl Node {
    /// Bind the endpoint whose key is `key` (hex, `datum sync
    /// endpoint-key`), on n0's relays or the one `relay` names. `datum`
    /// `schema` and `floor` are what the CLI runs (`sync status --json`'s
    /// `this`).
    /// `idle` is `DATUM_SYNC_TIMEOUT_S` for a page's tests, every node's.
    #[allow(clippy::too_many_arguments)]
    pub async fn bind(
        key: String,
        relay: Option<String>,
        idle: Option<u32>,
        label: String,
        datum: String,
        schema: u32,
        floor: u32,
        run: Function,
        progress: Function,
        serving: Function,
    ) -> Result<Node, JsError> {
        let secret = secret_of(&key)?;
        IDLE_S.store(idle.unwrap_or(0).into(), Relaxed);
        let net = relay.map_or(Net::N0, Net::Relay);
        let ep = bind(secret, &net).await.map_err(js)?;
        Ok(Node {
            cli: Cli {
                run,
                progress,
                serving,
                id: ep.id().to_string(),
                label: label.into(),
                version: Version {
                    datum,
                    schema,
                    floor: Some(floor),
                },
                open: Rc::default(),
                dialing: RefCell::default(),
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

    /// The next session arriving, on a new connection or one kept
    /// (da-3jcth); `undefined` once the endpoint closed.
    pub async fn accept(&self) -> Option<Pending> {
        accept(&self.ep).await.map(Pending)
    }

    /// Serve `pending`; its report, as JSON.
    pub async fn serve(&self, pending: Pending) -> Result<String, JsError> {
        report(serve(&self.cli, &self.ep, pending.0).await)
    }

    /// Sync with the peer at `addr` (the JSON `sync_peers` keeps); the
    /// report, as JSON. A session served before this one is past its
    /// handshake takes its turn ([`Dialing`]), and it fails as busy.
    pub async fn dial(&self, addr: String) -> Result<String, JsError> {
        let addr: EndpointAddr = serde_json::from_str(&addr).map_err(js)?;
        let peer = addr.id.to_string();
        let turn = self.turn().await;
        let (abort, registration) = AbortHandle::new_pair();
        let by = Rc::default();
        self.cli.dialing.replace(Some(Dialing {
            peer: peer.clone(),
            carried: false,
            conn: None,
            abort,
            by: Rc::clone(&by),
            _turn: turn,
        }));
        let dialed = dial(&self.cli, &self.ep, addr, None, None);
        let Ok(rep) = Abortable::new(dialed, registration).await else {
            let by = by.take().unwrap_or_default();
            return report(Err(match by == peer {
                true => closed_carried(short(&peer)),
                false => Failed(
                    Failure::Refused(Why::Busy),
                    format!("gave way to the session {} dialed meanwhile", short(&by)),
                )
                .into(),
            }));
        };
        self.cli.dialing.take();
        report(rep)
    }

    /// `datum sync chain join [--label]`: keep `ticket` as this database's
    /// pending join, as `label`, which the admit records, and try it once
    /// for a few seconds an address (da-4qpwf). Admitted, it syncs and its
    /// report says so; kept ([`join_kept`]), the report is `{"pending": true,
    /// "label": <the inviter's>, "said": <what a member answered>}`
    /// and the leader redeems it later ([`Node::redeem_pending`]). A copy
    /// that may not join is refused before anything is kept, and a member's
    /// refusal ends the join with its words.
    pub async fn connect(&self, ticket: String, label: Option<String>) -> Result<String, JsError> {
        let t = Ticket::decode(&ticket).map_err(js)?;
        let about = serde_json::to_string(&t.about).map_err(js)?;
        let mut hold = vec!["sync", "pending-join", "--hold", &ticket, "--about", &about];
        if let Some(label) = &label {
            hold.extend(["--label", label]);
            self.cli.label.replace(label.clone());
        }
        let held = Cell::new(false);
        let rep = self.redeeming(&t, true, Some((&hold, &held))).await;
        match rep {
            // Kept while a retry may change it, with what a member answered.
            Err(e) if join_kept(&t, &e) => Ok(serde_json::json!({
                "pending": true, "label": t.about.from, "said": answered(&e),
            })
            .to_string()),
            Err(e) => {
                // A join refused before it was kept leaves an older one alone.
                let lost = matches!(e.downcast_ref::<Lost>(), Some(Lost(r, _)) if r.redeemed);
                if held.get() && !lost {
                    let _ = self
                        .cli
                        .run(&["sync", "pending-join", "--clear"], None)
                        .await;
                }
                report(Err(e))
            }
            rep => {
                // A plain dial's handshake carries no invite, which ends it.
                let _ = self
                    .cli
                    .run(&["sync", "pending-join", "--clear"], None)
                    .await;
                report(rep)
            }
        }
    }

    /// Redeem this database's pending join, if it holds one a member has not
    /// refused for good, as it was asked to join (da-4qpwf): `null` with none.
    /// A refusal no retry changes is kept on the join, and ends its retries.
    #[wasm_bindgen(js_name = redeemPending)]
    pub async fn redeem_pending(&self) -> Result<String, JsError> {
        let held = self
            .cli
            .run(&["sync", "pending-join"], None)
            .await
            .map_err(js)?;
        let j: serde_json::Value = serde_json::from_str(held.trim()).map_err(js)?;
        let Some(ticket) = j["ticket"].as_str().filter(|_| j["refused"].is_null()) else {
            return Ok("null".into());
        };
        let t = Ticket::decode(ticket).map_err(js)?;
        if let Some(label) = j["label"].as_str() {
            self.cli.label.replace(label.into());
        }
        let rep = self.redeeming(&t, false, None).await;
        if rep.is_ok() {
            let _ = self
                .cli
                .run(&["sync", "pending-join", "--clear"], None)
                .await;
        }
        if let Err(e) = &rep {
            let lost = matches!(e.downcast_ref::<Lost>(), Some(Lost(r, _)) if r.redeemed);
            if !lost && !retries(&t, e) {
                let why = format!("{e:#}");
                let _ = self
                    .cli
                    .run(&["sync", "pending-join", "--refused", &why], None)
                    .await;
            }
        }
        report(rep)
    }

    /// Redeem `t`, `asked` with a person waiting. One refused as busy gives
    /// up its turn and tries again, for [`JOIN_WAITS`] (da-tqg19): two pages
    /// each holding a turn refuse each other's session, and a member's sync
    /// met in passing is no reason a join fails. The wait is jittered, so two
    /// pages that met do not meet again in step. A background redeem
    /// (`!asked`) gives up at the first: the next pull tries again.
    ///
    /// `keep`, a person's join: in its first turn, before it dials, whether
    /// this copy may join, and then the CLI line keeping it, `true` once
    /// kept. The join holds its turn from its first process, as it did
    /// before it was kept (da-tqg19): a session served meanwhile is the one
    /// refused, never the join.
    async fn redeeming(
        &self,
        t: &Ticket,
        asked: bool,
        mut keep: Option<(&[&str], &Cell<bool>)>,
    ) -> Result<Report> {
        let began = n0_future::time::Instant::now();
        loop {
            let rep = {
                let _turn = self.turn().await;
                if let Some((hold, held)) = keep.take() {
                    may_redeem(&self.cli, t).await?;
                    self.cli.run(hold, None).await?;
                    held.set(true);
                }
                redeem(&self.cli, &self.ep, t, asked).await
            };
            let busy = rep.as_ref().err().and_then(failure) == Some(Failure::Refused(Why::Busy));
            if !busy || !asked || began.elapsed() > JOIN_WAITS {
                return rep;
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
    /// and `about` (JSON, [`About`]). This address names the relay once the
    /// endpoint has reached it, which [`bind`] waits only 10 s for and a
    /// loaded machine can miss: a bare address, which no redeemer reaches
    /// without discovery (da-kfhty).
    pub async fn ticket(
        &self,
        secret: String,
        also: Option<String>,
        about: Option<String>,
    ) -> Result<String, JsError> {
        let _ = timeout(Duration::from_secs(30), self.ep.online()).await;
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
