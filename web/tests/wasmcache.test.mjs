// The deploy that broke every returning browser.
//
// datum's three Rust modules used to live in web/static/, which the build
// copies to the site root under the name it finds there. Two layers then
// conspired: the service worker precaches `files` and serves them
// cache-first, and web/deploy.sh gave every `.wasm` a year of
// `public,max-age=31536000,immutable`. So after a deploy the new HTML and
// its new hashed JavaScript came from the network — their names were new —
// while `/datum-render.wasm` came from the old cache, and the browser's
// HTTP cache kept handing back those same bytes for a year, which the next
// worker install then wrote into the next cache. vividn saw it as "coloring
// is still not working, and tail is completely broken": the new JS calling
// an export the old module does not have (da-tvg0).
//
// The fix is that a runtime artifact has no stable name any more — the
// wasm is a `?url` import from src/lib/wasm/ (the CLI, since da-g8o8), so
// Vite emits it content-hashed under /_app/immutable/, where a name means
// one set of bytes forever. These tests hold that: nothing the app fetches is
// stable-named, poisoning the old name changes nothing, the deploy refuses
// a build that reintroduces one, and the precache stops trusting the HTTP
// cache for the names that are stable.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { startApp, stopApp, withApp } from './support/app.mjs';

const web = fileURLToPath(new URL('..', import.meta.url));
const built = path.join(web, 'build');

/** md5("coffee") = 24eb05… → rgb(36, 235, 5): the field badge's own colour. */
const COFFEE_BADGE = /\x1b\[48;2;36;235;5m/;

before(startApp);
after(stopApp);

describe('the wasm a returning browser runs', () => {
	test('the previous deploy cached under the old name cannot reach the app', async () => {
		await withApp(async ({ page, run, reload, paneTable }) => {
			assert.equal((await run(['occur', 'coffee', '-t', '0700'])).code, 0);
			// `controller` is non-null only once a worker has installed —
			// precache included — and claimed this page, so its caches exist.
			await page.waitForFunction(() => navigator.serviceWorker.controller !== null);

			// Stand in for the previous deploy's module: a valid wasm module
			// (magic + version) that exports nothing. Instantiating it
			// succeeds and every call through it fails, which is exactly the
			// shape of the reported break — the minified `tail` export
			// reported as "o is not a function". Stronger than the real
			// older artifact, which happens to answer `render` and would let
			// half the app pass.
			const seeded = await page.evaluate(async () => {
				const empty = new Uint8Array([0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]);
				const keys = await caches.keys();
				for (const key of keys) {
					const cache = await caches.open(key);
					// The three modules the app used to fetch by these names, and
					// the CLI's own name for good measure.
					for (const name of ['datum-render.wasm', 'datum-parse.wasm', 'datum-layout.wasm', 'datum-cli.wasm']) {
						await cache.put(
							`/${name}`,
							new Response(empty, { headers: { 'content-type': 'application/wasm' } })
						);
					}
				}
				return keys;
			});
			assert.ok(seeded.length > 0, 'the service worker opened no cache to poison');

			await reload();

			// Colour, which is the CLI answering with the paint on: the exact
			// escape, so "no error" cannot pass for it.
			const listed = await run(['tail']);
			assert.equal(listed.code, 0, listed.stderr);
			assert.match(listed.stdout, COFFEE_BADGE, 'the field badge is unpainted');

			// And `tail` itself, which is the export the old render module
			// lacked — read off the pane as header words, not as an absence
			// of errors.
			// The CLI's own header words, whole (da-qrc6). `time (+0)`
			// because the harness pins the reader's zone to UTC.
			const table = await paneTable('tail');
			assert.deepEqual(table.headers, ['_n', 'time (+0)', 'field', 'state', 'hid']);
			// Two rows: the entry, and the TRACK document that declared its
			// field (da-c8mh) — which the pane draws without an action
			// column of its own.
			assert.deepEqual(
				table.rows.map((cells) => cells[table.headers.indexOf('field')]),
				['coffee', 'coffee']
			);
		});
	});

	test('every wasm the app fetches is content-hashed under /_app/immutable/', async () => {
		await withApp(async ({ page, run, reload }) => {
			const asked = [];
			page.on('request', (request) => {
				const { pathname } = new URL(request.url());
				if (pathname.endsWith('.wasm')) asked.push(pathname);
			});

			// A reload re-fetches everything the panes mount with, which is
			// the CLI — the one module every command runs in.
			await reload();
			assert.equal((await run(['occur', 'coffee', '-t', '0700'])).code, 0);

			assert.ok(asked.length >= 1, `no wasm requests at all`);
			for (const pathname of asked) {
				assert.ok(
					pathname.startsWith('/_app/immutable/'),
					`${pathname} is served from a stable name — a deploy will hand it to the wrong build`
				);
				// Under _app/immutable/ by itself is not enough: the promise
				// that the name never means anything else is the hash.
				assert.match(
					pathname,
					/[.-][A-Za-z0-9_-]{8,}\.wasm$/,
					`${pathname} carries no content hash`
				);
			}
			for (const name of ['datum-cli']) {
				assert.ok(
					asked.some((pathname) => pathname.includes(`/${name}-`)),
					`${name} was never fetched, so this proves nothing about it: ${asked.join(', ')}`
				);
			}
		});
	});
});

describe('what the deploy and the precache promise', () => {
	// deploy.sh's own assertion, run against the built tree rather than
	// restated here — so the rule the deploy enforces is the rule tested.
	const checkDist = (dir) =>
		execFileSync('bash', [path.join(web, 'deploy.sh'), '--check-dist', dir], {
			encoding: 'utf8',
			stdio: ['ignore', 'pipe', 'pipe']
		});

	test('the deploy accepts this build', () => {
		assert.match(checkDist(built), /^OK: /m);
	});

	test('the deploy refuses a build that puts a wasm back under a stable name', () => {
		const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'datum-dist-'));
		try {
			fs.mkdirSync(path.join(dir, '_app', 'immutable', 'assets'), { recursive: true });
			fs.writeFileSync(path.join(dir, '_app', 'immutable', 'assets', 'm.DEADBEEF.wasm'), '');
			fs.writeFileSync(path.join(dir, 'datum-render.wasm'), '');
			assert.throws(
				() => checkDist(dir),
				(e) => /REFUSING/.test(e.stderr) && /datum-render\.wasm/.test(e.stderr)
			);
		} finally {
			fs.rmSync(dir, { recursive: true, force: true });
		}
	});

	test('the precache does not let the HTTP cache answer for a stable name', () => {
		// The other half of the bug, and the one a fresh browser cannot
		// show: `cache.add` reuses the HTTP cache, so a browser still
		// holding a year-long `immutable` entry for a stable name copies
		// those stale bytes into the NEW worker's cache. `cache: 'reload'`
		// is what refuses that, and it is in the built worker or it is
		// nowhere.
		const sw = fs.readFileSync(path.join(built, 'service-worker.js'), 'utf8');
		assert.match(sw, /cache\s*:\s*["']reload["']/, 'the precache still trusts the HTTP cache');
	});
});
