# Deploying datum.day

The web app is a **static bundle** — no server, no backend, no sync. `npm run build`
emits `web/build/`, and that directory is the whole site. It goes in a bucket with a
CDN in front.

vividn provisions the infrastructure; `web/deploy.sh` does the upload.

---

## Storage, and a correction to an earlier draft of this file

**Entries persist without any special headers. This was measured, and it corrects what this
document said before.**

An earlier draft called COOP/COEP a hard requirement and warned that without them the site would
silently fall back to an in-memory database. **That is wrong.** The app stores through
SQLite-WASM's `SyncAccessHandlePoolVFS`, which uses OPFS sync access handles inside a Worker and
needs neither `SharedArrayBuffer` nor cross-origin isolation. The isolation requirement belongs
to SQLite-WASM's *other* VFS — the one that lets **two tabs hold the database at once**, which is
the reason isolation matters now (see step 4). Single-tab persistence never needed it.

Measured against the real bundle on a plain `python3 -m http.server`, sending no headers at all:

```
crossOriginIsolated: false
logged an entry      ->  visible
reloaded the page    ->  STILL VISIBLE
```

So headers are not a launch blocker. `npm run dev`/`preview` set them via a vite plugin, and
setting them in production is harmless and mildly future-proof — it is what `SharedArrayBuffer`
would need if anything ever wants it — but **nothing about persistence depends on them.**

### What IS worth verifying, because it does fail quietly

Storage can still be unavailable for other reasons: a browser with no OPFS support, private
browsing, a locked profile. The app then falls back to an in-memory database and **says so**. The
banner reports the storage the database actually opened with, so it is evidence rather than
decoration:

- *"Your entries are stored in this browser only"* — persistent, working
- *"held in memory only — reloading this page discards them"* — degraded; entries vanish on reload
- *"The database could not be opened, so nothing you log here will be saved"* — broken

**Before the demo: load datum.day, read the banner, log something, reload, confirm it is still
there.** One minute, and it tests the whole chain rather than a proxy for it.

## Step 1: the bucket

Any S3-compatible object storage. Scaleway Object Storage is the intended one.

- Create a bucket, e.g. `datum-day`, in a region near the users (`fr-par`).
- Enable **static website hosting**; index document `index.html`.
- Set the **error document to `index.html` as well** — SvelteKit is a single-page app, so a
  deep link like `/nowview` must serve the app rather than a 404.

  **This one is load-bearing, and it was verified rather than assumed.** The build emits exactly
  one HTML file (`build/index.html`); there is no `nowview.html`. Served without a fallback,
  `/nowview` is a hard 404:

  ```
  $ python3 -m http.server        # a naive static server, no fallback
  /          -> 200 OK
  /nowview   -> 404 File not found
  ```

  So verify it on the deployed site, not in the dashboard — **and verify it by CONTENT, not by
  status code.** S3 website hosting serves the error document *with the error status*, so a correctly
  configured bucket answers `/nowview` with **404 carrying the whole app**, and the browser routes
  client-side. Asserting 200 there failed against a bucket that was already right, for an hour on
  2026-09-08, and sent someone to re-check a setting that needed no change (`justfile`'s `verify`
  recipe carries the same warning, and is what you should actually run):

  ```sh
  just verify        # / is 200, and /nowview's body is compared against /'s

  # by hand, if you want to see it:
  curl -sS -o /tmp/root.html https://datum.day/
  curl -sS -o /tmp/deep.html https://datum.day/nowview
  cmp /tmp/root.html /tmp/deep.html && echo "the deep link serves the app"
  ```

  A deep link is BROKEN when the body is the bucket's own XML or an error page — not when the status
  says 404. Note this differs from local `npm run preview`, which redirects and falls back on its
  own, so a route can behave differently there than in the bucket.

  What this `cmp` proves is the FALLBACK, not `/nowview` in particular: every path is served by the
  same `index.html` (`web/src/routes/[name]/+page.svelte` is one catch-all route, and the build emits
  no file per name), so `/banana` would compare equal too. The path is `/nowview` because that is the
  CANONICAL built-in layout name (`web/src/lib/layout/defaults.ts`); it was `/now`, which still works
  but is a legacy alias in that file's `ALIASES`, and a deploy check should not be the last place a
  retired spelling lives. That the NAME resolves is the browser's job, tested in
  `web/tests/shell.test.mjs`; that the BUCKET serves the app for it is this check's job.
- Generate an API key scoped to this bucket only.

## Step 2: DNS and CDN

`datum.day` is registered. Point it at the bucket through Cloudflare:

- Add `datum.day` to Cloudflare; set the nameservers at the registrar.
- `CNAME` the apex at the bucket's **website** endpoint, **proxied** (orange cloud).
- `www` is a **redirect, not a second CNAME**. Scaleway selects the bucket from the Host
  header, so a `www.datum.day` request reaching the bucket answers
  `NoSuchBucket: www.datum.day` — measured 2026-09-09 14:2x on the live site (HTTP 404, XML
  body, `server: cloudflare`). Give `www` a proxied DNS record (any target; Cloudflare answers
  before the origin is reached) and a Redirect Rule:

  ```
  Rules -> Redirect Rules -> Create
    when:  Hostname equals www.datum.day
    then:  Dynamic redirect, expression concat("https://datum.day", http.request.uri.path)
           status 301, preserve query string
  ```

  Verify on the deployed site: `curl -sI https://www.datum.day/nowview | head -3` must show a
  redirect with `location: https://datum.day/nowview`. Live since 2026-09-09 15:0x as a **302**;
  a **301** is the better setting for a permanent www → apex move (browsers cache it, and
  search engines treat the apex as canonical) — flip it when convenient, nothing depends on it.

  ⚠ The website endpoint is not the S3 API endpoint, and the console tends to show you the latter:

  ```
  datum-webview.s3.fr-par.scw.cloud          <- S3 API. Serves XML errors. NOT this one.
  datum-webview.s3-website.fr-par.scw.cloud  <- website endpoint. Index and error documents. This one.
  ```

  The API endpoint has no index document and no SPA fallback, so pointing the CNAME at it
  gives `AccessDenied` XML in place of the app.

- ⚑ **Name the bucket after the domain, and CNAME at the GENERIC endpoint.** Scaleway identifies
  the bucket from the **Host header**, on the website endpoint as well as the API one. A proxied
  Cloudflare CNAME forwards the visitor's Host — so if the bucket is named `datum.day`, the Host
  selects it and nothing else is needed:

  ```
  CNAME  datum.day  ->  s3-website.fr-par.scw.cloud     (generic — NO bucket prefix, proxied)
  ```

  **Do not use `<bucket>.s3-website.<region>.scw.cloud` as the CNAME target.** Measured 2026-09-08
  against the live endpoint:

  ```
  https://datum.day.s3-website.fr-par.scw.cloud/       TLS FAILS TO VERIFY
      cert SANs: *.s3-website.fr-par.scw.cloud, *.s3.fr-par.scw.cloud (+ the bare names)
      a wildcard matches ONE label; `datum.day` is two -> Full (strict) has no valid origin cert

  https://s3-website.fr-par.scw.cloud/  Host: datum.day  -> 404 NoSuchKey: index.html   (correct)
  https://s3-website.fr-par.scw.cloud/  no Host override -> 301 "Request does not contain a bucket name"
  ```

  The second pair is the control: same URL, same moment, differing only in the header, so it is the
  Host that selects the bucket. Pointing at the generic endpoint keeps SNI on a name the wildcard
  covers, so **Full (strict)** validates.

  *(An earlier draft of this file advised the opposite — a bucket with a neutral name plus an Origin
  Rule Host override. That works too, and it is strictly more configuration. The TLS objection to a
  dotted bucket name was correct about the bucket-prefixed hostname and wrong as an argument against
  the name, because the generic endpoint was never tested.)*
- SSL/TLS mode **Full (strict)**.

The proxy is not optional-in-practice: it is where the headers get set (step 4).

## Step 0: the one tool you need

`deploy.sh` uses the **AWS CLI** — Scaleway Object Storage is S3-compatible, so `aws s3` talks to
it with `--endpoint-url`. Nothing else is required; the script refuses with a pointer to this file
if `aws` is missing.

```sh
aws --version                      # already there?
# Debian/Ubuntu:  apt install awscli
# or:             pipx install awscli
```

*(This section exists because `deploy.sh` said "aws CLI not found; see web/DEPLOY.md" and this file
did not mention the aws CLI anywhere — a pointer to a document that did not answer the question.
Caught 2026-09-04 by reading these instructions as someone following them.)*

### Make the objects publicly readable

`deploy.sh` sets **no ACL**. It passes `--cache-control public,max-age=...`, which is a caching
directive and not an access one — easy to misread as covering this. Without a grant, the upload
succeeds and every object is private, so the website endpoint returns **403 on a deploy that
reported success**.

Grant it once at the bucket rather than per object, so future uploads are covered too: a bucket
policy allowing `s3:GetObject` on `arn:aws:s3:::<bucket>/*` to `*`.

## Step 3: credentials and first deploy

```sh
export DATUM_DAY_BUCKET=datum-day
export DATUM_DAY_ENDPOINT=https://s3.fr-par.scw.cloud
export AWS_ACCESS_KEY_ID=...
export AWS_SECRET_ACCESS_KEY=...

just deploy              # build and upload
just deploy-dry          # the same thing without writing
just verify              # what the live site actually serves
```

The recipes wrap `web/deploy.sh`, which still works directly if you prefer it.

The script sets cache headers deliberately: hashed assets under `_app/immutable/` get a
one-year immutable cache; HTML and the service worker get `no-cache` so a deploy is visible
immediately. It also forces `Content-Type: application/wasm`, because a `.wasm` served as
`application/octet-stream` cannot be stream-compiled.

### Step 3b: the same thing from CI

`.github/workflows/deploy.yml` runs `web/deploy.sh --yes` and then `just verify` on every
push to the **`wasm` branch**, and on demand from the Actions tab (dispatched on `wasm`).
Not on `main`: the CLI module is no longer committed (da-0qa4x). The forge's
`.forgejo/workflows/wasm.yml` builds it for each main commit and commits it to `wasm` with
`SOURCE`, the main sha it is for, and a copy of main@SOURCE's `deploy.yml` — GitHub runs a
branch's push workflows from that branch's tree. The deploy checks out main at `SOURCE`, puts
the module beside it, and runs `deploy.sh`, which needs no Rust toolchain. So a push to main
deploys once its module is built, and a newer one cancels a deploy still running.

For that to reach GitHub, **the forge's push mirror must push all branches**, not only `main`,
and if the `production` environment restricts deployment branches it must allow `wasm`.

It is **inert until the repository is configured** — it reads four values and none of them
have defaults:

- a GitHub **Environment** named `production`, holding
- secrets `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY` (the same bucket-scoped key as above), and
- variables `DATUM_DAY_BUCKET` and `DATUM_DAY_ENDPOINT` (variables, not secrets — a bucket
  name is not a credential, and keeping them readable means a failed run says *which* bucket
  it tried).

The manual path above keeps working and stays the right one for a rollback, which is a
checkout of an older `web/` and so has no push behind it to trigger on.

## Step 4: headers — the app sends them itself; the rule makes the first load isolated too

**What isolation gates has changed, so read this even if you read the old version.** With no
COOP/COEP headers, persistence still works and an entry survives a reload — that measurement stands.
What isolation buys is **two tabs durable at the same time**: the OPFS pool is exclusive per origin,
so without it a second tab opens in memory (and, since #146, takes the database over when the first
tab closes, keeping what was typed). The shared VFS that lets both tabs write at once needs
`SharedArrayBuffer`, which needs isolation.

**The app now sends the headers itself.** Its service worker adds both to every response it serves,
and the page reloads itself once — on a first visit, while the prompt is still empty — when the
worker takes control. So on a bare bucket the site is isolated from its second document onward,
with or without anything in Cloudflare. This is measured (`web/tests/isolation.test.mjs`): a plain
server sending no headers, one reload, `crossOriginIsolated === true`, and a control with service
workers blocked that stays `false`.

**Still set the Cloudflare rule.** It isolates the *first* document too, so the reload never happens,
and it covers a visitor whose browser blocks service workers. Object storage cannot attach response
headers per object, so it goes at the Cloudflare layer:

*Rules → Transform Rules → Modify Response Header → Create*

- If: hostname equals `datum.day`
- Then set static:
  - `Cross-Origin-Opener-Policy` = `same-origin`
  - `Cross-Origin-Embedder-Policy` = `require-corp`

**Verify from outside**, not from the dashboard — a rule can read as active and not reach the
response:

```sh
curl -sI https://datum.day/ | grep -i cross-origin
```

Both lines should appear. `curl` sees only the rule, never the service worker, so an empty result
means the rule is not reaching the response — the site still isolates itself from the second load,
but the first-load case is not covered. The check that tells you what a visitor actually gets is in
the browser, on a second load:

```js
crossOriginIsolated   // true from the second load regardless; true on the FIRST only if the rule reached it
```

## Step 5: mark it beta

Already in the app — a persistent banner stating it is early software and that entries live
only in this browser. **Keep that wording honest as persistence changes.** While storage is
memory-only the banner must say so; once OPFS-in-a-Worker lands (`da-5iqw`) the claim changes
and the banner has to change with it, in the same commit. A stale reassurance is worse than
no banner, because someone will trust it.

---

## Rollback

Deploys are a file sync, so rollback is redeploying a previous build. Check out the WHOLE
commit, not just `web/`: the module is not in the tree, and `deploy.sh` fetches the one the
`wasm` branch holds for that commit (or builds it) and refuses one that is not HEAD's:

```sh
git checkout --detach <previous-main-commit>
./web/deploy.sh --yes
git checkout -
```

There is no server state to migrate. Users' data lives in their own browsers and is not
touched by a deploy — which is the point of local-first, and also means a bad deploy cannot
destroy anyone's entries.

## What is deliberately not here

- **No sync**, by decision. The app is entirely client-side.
- **No analytics.** If that changes, note that a third-party script will fail under
  `require-corp` unless it sends CORP headers — another reason isolation is load-bearing.
- **No approval gate on the deploy.** Every push to `main` goes to production (step 3b). That
  is affordable only because a deploy is a file sync with no state behind it; if that stops
  being true, the `production` environment is where a required reviewer goes.
