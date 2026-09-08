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
to SQLite-WASM's *other* VFS, which this app does not use.

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
  deep link like `/now` must serve the app rather than a 404.

  **This one is load-bearing, and it was verified rather than assumed.** The build emits exactly
  one HTML file (`build/index.html`); there is no `now.html`. Served without a fallback, `/now`
  is a hard 404:

  ```
  $ python3 -m http.server        # a naive static server, no fallback
  /      -> 200 OK
  /now   -> 404 File not found
  ```

  So verify it on the deployed site, not in the dashboard:

  ```sh
  curl -sI https://datum.day/now | head -1     # must be 200, not 404
  ```

  Note this differs from local `npm run preview`, which redirects and falls back on its own — a
  route that works locally can still 404 in the bucket.
- Generate an API key scoped to this bucket only.

## Step 2: DNS and CDN

`datum.day` is registered. Point it at the bucket through Cloudflare:

- Add `datum.day` to Cloudflare; set the nameservers at the registrar.
- `CNAME` the apex (and `www`) at the bucket's **website** endpoint, **proxied** (orange cloud).

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
push to `main`, and on demand from the Actions tab. It is **inert until the repository is
configured** — it reads four values and none of them have defaults:

- a GitHub **Environment** named `production`, holding
- secrets `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY` (the same bucket-scoped key as above), and
- variables `DATUM_DAY_BUCKET` and `DATUM_DAY_ENDPOINT` (variables, not secrets — a bucket
  name is not a credential, and keeping them readable means a failed run says *which* bucket
  it tried).

The manual path above keeps working and stays the right one for a rollback, which is a
checkout of an older `web/` and so has no push behind it to trigger on.

## Step 4: headers — optional hardening, NOT a gate

**Do not treat this step as blocking.** It is measured above: with no COOP/COEP headers at all,
`crossOriginIsolated` is `false`, persistence still works, and an entry survives a reload. If the
Cloudflare rule is awkward or you are short of time before a demo, **skip this step and ship.**

What it buys is future headroom, not present correctness: `SharedArrayBuffer` and the
OPFS-in-a-Worker path (`da-5iqw`) will want isolation, so setting it now costs nothing and saves a
migration later. Set them at the Cloudflare layer, because object storage generally cannot attach
arbitrary response headers per object:

*Rules → Transform Rules → Modify Response Header → Create*

- If: hostname equals `datum.day`
- Then set static:
  - `Cross-Origin-Opener-Policy` = `same-origin`
  - `Cross-Origin-Embedder-Policy` = `require-corp`

If you set them, **verify from outside**, not from the dashboard — a rule can read as active and
not reach the response:

```sh
curl -sI https://datum.day/ | grep -i cross-origin
```

Both lines should appear. **If they do not, the site is fine** — it is simply not isolated, which is
the same state the measurement above was taken in. The console check tells you which state you are
in, and neither state loses data:

```js
crossOriginIsolated   // true if the rule reached the response; false is OK
```

## Step 5: mark it beta

Already in the app — a persistent banner stating it is early software and that entries live
only in this browser. **Keep that wording honest as persistence changes.** While storage is
memory-only the banner must say so; once OPFS-in-a-Worker lands (`da-5iqw`) the claim changes
and the banner has to change with it, in the same commit. A stale reassurance is worse than
no banner, because someone will trust it.

---

## Rollback

Deploys are a file sync, so rollback is redeploying a previous build:

```sh
git checkout <previous-commit> -- web/
./web/deploy.sh --yes
git checkout HEAD -- web/
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
