// The datum.day build kept at a sync floor (da-ec8y8): built with
// DATUM_BASE=/v/<version>, it is served at that path beside the root's build,
// as the bucket holds the two (web/deploy.sh), with the root's index.html for
// any path the bucket lacks, as its error document is. It runs the CLI there,
// its worker's scope is its path, and neither build's worker clears the
// other's cache, though both are one origin's.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import http from 'node:http';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const web = fileURLToPath(new URL('..', import.meta.url));
const root = path.join(web, 'build');
const BASE = '/v/3.0.1';
const kept = fs.mkdtempSync(path.join(os.tmpdir(), 'datum-kept-'));
// Its own `.svelte-kit`: the other files' `vite preview` serves `.svelte-kit/output`,
// which a second build there would replace under them.
const kitOut = path.join(web, '.svelte-kit-kept');
const profile = fs.mkdtempSync(path.join(os.tmpdir(), 'datum-kept-profile-'));
const TYPES = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.wasm': 'application/wasm', '.json': 'application/json', '.svg': 'image/svg+xml' };

let server;
let url;

before(async () => {
	// vite alone: `pnpm run build`'s prebuild already put this tree's module in place.
	execFileSync(path.join(web, 'node_modules/.bin/vite'), ['build', '--logLevel', 'error'], {
		cwd: web,
		env: { ...process.env, DATUM_BASE: BASE, DATUM_BUILD_DIR: kept, DATUM_KIT_OUT: kitOut },
		stdio: 'inherit'
	});
	server = http.createServer((req, res) => {
		const where = decodeURIComponent(new URL(req.url, 'http://x').pathname);
		const [dir, rel] = where.startsWith(`${BASE}/`) ? [kept, where.slice(BASE.length)] : [root, where];
		let file = path.join(dir, rel);
		if (!file.startsWith(dir)) return res.writeHead(403).end();
		if (fs.existsSync(file) && fs.statSync(file).isDirectory()) file = path.join(file, 'index.html');
		const found = fs.existsSync(file);
		if (!found) file = path.join(root, 'index.html');
		res.writeHead(found ? 200 : 404, {
			'Cross-Origin-Opener-Policy': 'same-origin',
			'Cross-Origin-Embedder-Policy': 'require-corp',
			'cache-control': 'no-cache',
			'content-type': TYPES[path.extname(file)] ?? 'application/octet-stream'
		});
		fs.createReadStream(file).pipe(res);
	});
	await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
	url = `http://127.0.0.1:${server.address().port}`;
});

after(() => {
	fs.rmSync(kept, { recursive: true, force: true });
	fs.rmSync(kitOut, { recursive: true, force: true });
	fs.rmSync(profile, { recursive: true, force: true });
	return new Promise((resolve) => server.close(resolve));
});

/** The page's worker, active, and the origin's cache names. */
async function settled(page) {
	return page.evaluate(async () => {
		const reg = await navigator.serviceWorker.ready;
		for (const t = Date.now(); !reg.active || reg.active.state !== 'activated'; ) {
			if (Date.now() - t > 30_000) throw new Error('the worker never activated');
			await new Promise((resolve) => setTimeout(resolve, 50));
		}
		return { scope: new URL(reg.scope).pathname, caches: await caches.keys() };
	});
}

test('the kept build runs at its path, under a worker of its own, beside the root', async () => {
	const context = await chromium.launchPersistentContext(profile);
	try {
		const page = await context.newPage();
		await page.goto(`${url}${BASE}/`);
		await page.waitForFunction(() => typeof window.datum?.run === 'function', null, { timeout: 120_000 });
		const scripts = await page.evaluate(() => [...document.querySelectorAll('script[src], link[href*="_app"]')].map((e) => new URL(e.src || e.href).pathname));
		assert.ok(scripts.length > 0 && scripts.every((p) => p.startsWith(`${BASE}/_app/`)), JSON.stringify(scripts));
		const { stdout } = await page.evaluate(() => window.datum.runQuiet(['--version']));
		assert.match(stdout, /^datum \d+\.\d+\.\d+/);

		const mine = await settled(page);
		assert.equal(mine.scope, `${BASE}/`);
		const keptCache = mine.caches.find((k) => k.startsWith(`datum${BASE}-`));
		assert.ok(keptCache, JSON.stringify(mine.caches));

		// The root's worker activates after it, and clears no cache but its own base's.
		await page.goto(`${url}/`);
		const theirs = await settled(page);
		assert.equal(theirs.scope, '/');
		assert.ok(theirs.caches.some((k) => /^datum-/.test(k)), JSON.stringify(theirs.caches));
		assert.ok(theirs.caches.includes(keptCache), JSON.stringify(theirs.caches));
	} finally {
		await context.close();
	}
});
