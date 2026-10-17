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
/** Every visit's browser profile is made in here, removed after the file (da-yqc4v). */
const profiles = fs.mkdtempSync(path.join(os.tmpdir(), 'datum-coi-'));
/** While set, the worker's script is not served until it settles — see the last test. */
let held = null;

before(async () => {
	server = http.createServer(async (req, res) => {
		if (held && req.url.startsWith('/service-worker.js')) await held;
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
after(() => {
	fs.rmSync(profiles, { recursive: true, force: true });
	return new Promise((resolve) => server.close(resolve));
});

async function visit(options, where = '') {
	const dir = fs.mkdtempSync(path.join(profiles, 'visit-'));
	const context = await chromium.launchPersistentContext(dir, options);
	const page = context.pages()[0] ?? (await context.newPage());
	let loads = 0;
	page.on('load', () => loads++);
	await page.goto(url + where, { waitUntil: 'load' });
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
			await page.waitForFunction(() => crossOriginIsolated);
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

	// da-6ud02. On datum.day — a host that sends no isolation headers — the
	// worker takes control of a first visit while the wasm is still being
	// precached, which on a real network is seconds: after the visitor's
	// first command, more often than not. The reload used to fire then,
	// because the box it guarded was already empty again, and took the
	// command's ERROR with it — the visitor mistyped their first line and
	// was told nothing. Here the worker is HELD until the command has
	// answered, so the order is the live one every time, not a race.
	test("a reload never takes away what the visitor's first command said", async () => {
		let release;
		held = new Promise((resolve) => (release = resolve));
		const { context, page, loads } = await visit({
			viewport: { width: 390, height: 844 },
			isMobile: true,
			hasTouch: true
		});
		try {
			await page.locator('.prompt-bar input:not([disabled])').waitFor();
			const input = page.locator('.prompt-bar input');
			await input.fill('start walk -y 13:00');
			await input.press('Enter');
			// Folded, a failure is a red ERROR by the `❯` for a minute.
			await page.locator('.prompt-bar button.failed').waitFor();

			release();
			held = null;
			await page.waitForFunction(() => !!navigator.serviceWorker.controller);
			// A reload asked for in `controllerchange` would be on its way by
			// now; give it the time a navigation takes to land.
			await page.waitForTimeout(1500);
			await page.waitForLoadState('load');
			assert.equal(loads(), 1, 'the page reloaded under the command');
			assert.equal(await page.locator('.prompt-bar button.failed').count(), 1, 'the ERROR is gone');
		} finally {
			release();
			held = null;
			await context.close();
		}
	});

	// da-amhrv. A join or approve link opened on a browser's first visit:
	// the route reads the `#` and drops it from the URL, and the isolation
	// reload then wiped the in-memory ticket — no card, no error. A phone
	// scanning the QR is almost always on its first visit. The worker is
	// HELD until the `#` is gone, as on a real network: a worker taking
	// control sooner reloads the link with its `#` and hides the bug.
	const T = 'MFRGGZDFMZTWQ2LKNNWG23TPOBYXE43UOV3HO6DZPJQWEY3EMVTGO2DJNJVWY3LONZXXA4LSON2HK5TXPB4XU';
	for (const [where, card] of [
		[`sync/join#${T}`, '.card.join'],
		['sync/approve#eyJ9', '.card.approve']
	])
		test(`/${where.split('#')[0]}#… on a first visit shows its card after the reload, until dismissed`, async () => {
			let release;
			held = new Promise((resolve) => (release = resolve));
			const { context, page, loads } = await visit({}, where);
			try {
				await page.waitForFunction(() => location.pathname === '/' && !location.hash);
				release();
				held = null;
				await page.waitForFunction(() => crossOriginIsolated);
				await mounted(page);
				assert.equal(loads(), 2, 'the isolation reload happened');
				await page.locator(card).waitFor();

				await page.locator(`${card} button.close`).click();
				await page.reload({ waitUntil: 'load' });
				await mounted(page);
				await page.waitForTimeout(1000);
				assert.equal(await page.locator(card).count(), 0, 'a dismissed card came back on a reload');

				// Warm: an isolated page opening the link shows the card, as ever.
				await page.goto(url + where, { waitUntil: 'load' });
				await page.locator(card).waitFor();
			} finally {
				release();
				held = null;
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
