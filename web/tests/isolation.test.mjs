// Cross-origin isolation from the service worker, on a host that sends no
// headers of its own.
//
// The dev and preview servers add COOP/COEP to every response (vite.config.ts),
// so against them `crossOriginIsolated` is true before the worker does
// anything, and a test there would pass against a worker that adds nothing.
// This one serves the build from a plain `http.createServer` that sends NO
// isolation headers — the shape of a bare bucket — so the only way the page
// can become isolated is the worker's doing. The control proves that: with
// service workers blocked, it never does.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const build = fileURLToPath(new URL('../build', import.meta.url));
const TYPES = {
	'.html': 'text/html',
	'.js': 'text/javascript',
	'.mjs': 'text/javascript',
	'.css': 'text/css',
	'.wasm': 'application/wasm',
	'.svg': 'image/svg+xml',
	'.json': 'application/json',
	'.webmanifest': 'application/manifest+json',
	'.png': 'image/png',
	'.ico': 'image/x-icon',
	'.txt': 'text/plain'
};

let server;
let url;

before(async () => {
	server = http.createServer((req, res) => {
		let file = path.join(build, decodeURIComponent(new URL(req.url, 'http://x').pathname));
		if (!file.startsWith(build)) {
			res.writeHead(403);
			res.end();
			return;
		}
		// The bucket serves index.html for unknown paths; so does this.
		if (!fs.existsSync(file) || fs.statSync(file).isDirectory()) file = path.join(build, 'index.html');
		// Deliberately NO Cross-Origin-* headers here.
		res.writeHead(200, { 'content-type': TYPES[path.extname(file)] ?? 'application/octet-stream' });
		fs.createReadStream(file).pipe(res);
	});
	await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
	url = `http://127.0.0.1:${server.address().port}/`;
});
after(() => new Promise((resolve) => server.close(resolve)));

async function visit(options) {
	const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'datum-coi-'));
	const context = await chromium.launchPersistentContext(dir, options);
	const page = context.pages()[0] ?? (await context.newPage());
	let loads = 0;
	page.on('load', () => loads++);
	await page.goto(url, { waitUntil: 'load' });
	return { context, page, loads: () => loads };
}

/**
 * The layout has mounted. `window.datum` is installed by the same onMount
 * that then decides on the reload (+layout.svelte), synchronously — so a
 * page where it exists has made that decision, and any reload it asked
 * for would already have replaced it.
 */
async function mounted(page) {
	await page.waitForLoadState('load');
	await page.waitForFunction(() => typeof window.datum?.run === 'function');
}

describe('cross-origin isolation from the service worker, on a host sending no headers', () => {
	test('the app isolates itself with exactly one reload, and never again', async () => {
		const { context, page, loads } = await visit({});
		// try/finally, because a browser left open on a failed assertion keeps
		// the process alive and turns a red test into a hung job.
		try {
			await page.waitForFunction(() => crossOriginIsolated, null, { timeout: 20000 });
			// The isolated page's mount asks for no reload; once it has
			// mounted, nothing further is coming — then count.
			await mounted(page);
			assert.equal(loads(), 2, 'one initial load plus exactly one reload');
			assert.ok(await page.evaluate(() => !!navigator.serviceWorker.controller), 'a worker controls the page');

			// From here on isolation is simply there: a fresh navigation in the same
			// session is isolated from its first byte, with no reload at all.
			const before = loads();
			await page.goto(url, { waitUntil: 'load' });
			assert.equal(await page.evaluate(() => crossOriginIsolated), true);
			await mounted(page);
			assert.equal(loads(), before + 1, 'no reload the second time');
		} finally {
			await context.close();
		}
	});

	test('CONTROL: with service workers blocked, the page stays un-isolated', async () => {
		const { context, page } = await visit({ serviceWorkers: 'block' });
		try {
			// A document's isolation is fixed for its life, and with no worker
			// there is no reload to bring another: the mounted page is the
			// only one there will be.
			await mounted(page);
			assert.equal(
				await page.evaluate(() => crossOriginIsolated),
				false,
				'isolation came from the worker, not from the server'
			);
		} finally {
			await context.close();
		}
	});
});
