//! The wire crate in datum.day's sync worker (`web/src/lib/sync.worker.ts`,
//! da-6kpj5). The store is the real CLI: every read and write is a `datum
//! sync …` command the worker runs through the page's database runner, so
//! no storage logic exists in the browser apart from the CLI's own.

use anyhow::{bail, Result};
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
/// process's `{code, stdout, stderr}`.
struct Cli {
    run: Function,
    label: String,
}

impl Cli {
    /// stdout of `datum <argv>`, or its stderr as the error.
    async fn run(&self, argv: &[&str], stdin: Option<&str>) -> Result<String> {
        let args: Array = argv.iter().map(|a| JsValue::from_str(a)).collect();
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
            bail!("{}", err.trim().trim_start_matches("error: "));
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
        self.label.clone()
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

    async fn vector(&self) -> Result<Vector> {
        Ok(serde_json::from_str(
            &self.run(&["sync", "pipe", "vector"], None).await?,
        )?)
    }

    async fn ops_since(&self, peer: &str, since: &Vector) -> Result<Vec<String>> {
        let since = serde_json::to_string(since)?;
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

    async fn verify(&self, peer: &str) -> Result<String> {
        Ok(self
            .run(&["sync", "pipe", "verify", "--peer", peer], None)
            .await?
            .trim()
            .to_string())
    }

    async fn handshake(
        &self,
        peer: &str,
        label: &str,
        addr: &EndpointAddr,
        invite: Option<&str>,
        chain: Option<&str>,
        dialed: bool,
    ) -> Result<bool> {
        let addr = serde_json::to_string(addr)?;
        let mut argv = vec!["sync", "handshake", peer, "--label", label, "--addr", &addr];
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
        Ok(self.run(&argv, None).await?.trim() == "added")
    }

    async fn record_session(
        &self,
        peer: &str,
        vector: Option<&Vector>,
        verify: Option<bool>,
        error: Option<&str>,
    ) -> Result<()> {
        let vector = vector.map(serde_json::to_string).transpose()?;
        let mut argv = vec!["sync", "record-session", peer];
        if let Some(v) = &vector {
            argv.extend(["--vector", v]);
        }
        if let Some(m) = verify {
            argv.extend(["--verify", if m { "match" } else { "mismatch" }]);
        }
        if let Some(e) = error {
            argv.extend(["--error", e]);
        }
        self.run(&argv, None).await.map(drop)
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

#[wasm_bindgen]
impl Node {
    /// Bind the endpoint whose key is `key` (hex, `datum sync
    /// endpoint-key`), on n0's relays or the one `relay` names.
    pub async fn bind(
        key: String,
        relay: Option<String>,
        label: String,
        run: Function,
    ) -> Result<Node, JsError> {
        let bytes: Vec<u8> = (0..key.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(key.get(i..i + 2).unwrap_or("zz"), 16))
            .collect::<Result<_, _>>()
            .map_err(js)?;
        let secret: [u8; 32] = bytes.try_into().map_err(|_| js("not an endpoint key"))?;
        let net = relay.map_or(Net::N0, Net::Relay);
        let ep = bind(secret, &net).await.map_err(js)?;
        Ok(Node {
            ep,
            cli: Cli { run, label },
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
        report(dial(&self.cli, &self.ep, addr, None).await)
    }

    /// `datum sync chain connect`: redeem `ticket` and sync.
    pub async fn connect(&self, ticket: String) -> Result<String, JsError> {
        let ticket = Ticket::decode(&ticket).map_err(js)?;
        report(redeem(&self.cli, &self.ep, &ticket).await)
    }

    /// The ticket for an invite whose secret is `secret` — this endpoint's
    /// address, then `also`'s (a JSON list of addresses, a chain's members).
    pub fn ticket(&self, secret: String, also: Option<String>) -> Result<String, JsError> {
        let mut addrs = vec![self.ep.addr()];
        if let Some(also) = also {
            addrs.extend(serde_json::from_str::<Vec<EndpointAddr>>(&also).map_err(js)?);
        }
        Ticket { addrs, secret }.encode().map_err(js)
    }

    pub async fn close(&self) {
        self.ep.close().await;
    }
}
