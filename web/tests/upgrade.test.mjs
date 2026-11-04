// The page's own upgrade (da-ylkin): «↑ Upgrade» between the gear and the (?)
// once `/_app/version.json` names another build, and pressed, a reload that
// reaches the deployed page past the service worker and its caches.
//
// Served from a plain server, as isolation.test.mjs is, so the test decides
// what version.json says and when the page itself answers. Each HTML response
// carries its number, so a document says which response it was. With the
// origin slower than the worker's 3 s, a reload the worker still controls is
// answered from the shell it cached: the CONTROL step proves that, so the
// upgrade's fresh document is the press's doing.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const build = fileURLToPath(new URL('../build', import.meta.url));
const TYPES = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.wasm': 'application/wasm', '.json': 'application/json', '.svg': 'image/svg+xml' };

/** What version.json says: the build's own while null. */
let deployed = null;
/** How long each HTML response waits, in ms. */
let slow = 0;
/** HTML responses so far, and version.json requests so far. */
let pages = 0;
let checks = 0;

let server;
let url;
const profile = fs.mkdtempSync(path.join(os.tmpdir(), 'datum-upgrade-'));

before(async () => {
	server = http.createServer(async (req, res) => {
		const where = decodeURIComponent(new URL(req.url, 'http://x').pathname);
		// Isolated from the first byte, so the page never reloads for it.
		const headers = { 'Cross-Origin-Opener-Policy': 'same-origin', 'Cross-Origin-Embedder-Policy': 'require-corp', 'cache-control': 'no-cache' };
		if (where === '/_app/version.json') {
			checks++;
			if (deployed !== null) {
				res.writeHead(200, { ...headers, 'content-type': 'application/json' });
				return res.end(JSON.stringify({ version: deployed }));
			}
		}
		let file = path.join(build, where);
		if (!file.startsWith(build)) return res.writeHead(403).end();
		if (!fs.existsSync(file) || fs.statSync(file).isDirectory()) file = path.join(build, 'index.html');
		if (file.endsWith('.html')) {
			const n = ++pages;
			if (slow) await new Promise((resolve) => setTimeout(resolve, slow));
			res.writeHead(200, { ...headers, 'content-type': 'text/html' });
			return res.end(fs.readFileSync(file, 'utf8').replace('<head>', `<head><meta name="served" content="${n}">`));
		}
		res.writeHead(200, { ...headers, 'content-type': TYPES[path.extname(file)] ?? 'application/octet-stream' });
		fs.createReadStream(file).pipe(res);
	});
	await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
	url = `http://127.0.0.1:${server.address().port}/`;
});
after(() => {
	fs.rmSync(profile, { recursive: true, force: true });
	return new Promise((resolve) => server.close(resolve));
});

const served = (page) => page.evaluate(() => Number(document.querySelector('meta[name=served]')?.content));
const mounted = (page) => page.waitForFunction(() => typeof window.datum?.run === 'function');

/** A version check as the tab takes focus, waited for until the server has answered it. */
async function check(page) {
	const before = checks;
	await page.evaluate(() => dispatchEvent(new Event('focus')));
	for (const t = Date.now(); checks === before; ) {
		assert.ok(Date.now() - t < 10_000, 'no version check reached the server');
		await new Promise((resolve) => setTimeout(resolve, 20));
	}
	// Its answer is read, and drawn, a frame or two later.
	await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
}

test('«↑ Upgrade» shows for a newer build, and reloads onto the deployed page past the worker', async () => {
	const context = await chromium.launchPersistentContext(profile, {});
	try {
		const page = context.pages()[0] ?? (await context.newPage());
		await page.goto(url, { waitUntil: 'load' });
		await mounted(page);
		await page.waitForFunction(() => !!navigator.serviceWorker.controller);
		const bar = page.locator('nav.root-bar');
		const upgrade = bar.locator('button.upgrade');

		// The (?) is filled with the accent.
		const [fill, accent] = await page.evaluate(() => {
			const probe = document.body.appendChild(document.createElement('i'));
			probe.style.background = 'var(--accent)';
			const colors = [getComputedStyle(document.querySelector('nav.root-bar button.help')).backgroundColor, getComputedStyle(probe).backgroundColor];
			probe.remove();
			return colors;
		});
		assert.equal(fill, accent);

		// The same build deployed: no button.
		await check(page);
		assert.equal(await upgrade.count(), 0, 'shown with this build deployed');

		// CONTROL: a reload under the worker, with the origin slow, is the cached shell.
		slow = 4000;
		await page.reload({ waitUntil: 'load' });
		await mounted(page);
		assert.notEqual(await served(page), pages, 'the worker let a slow reload reach the origin');

		// A newer build deployed: the button, between the gear and the (?).
		deployed = 'newer';
		await check(page);
		await upgrade.waitFor();
		const [gear, up, help] = await Promise.all(['.gear', 'button.upgrade', 'button.help'].map((s) => bar.locator(s).boundingBox()));
		assert.ok(gear.x + gear.width <= up.x && up.x + up.width <= help.x, `gear ${gear.x}, upgrade ${up.x}, help ${help.x}`);

		// A press that fails, here at finding the worker, leaves the button to press again.
		await page.evaluate(() => (navigator.serviceWorker.getRegistration = () => Promise.reject(new Error('refused'))));
		await upgrade.click();
		await bar.locator('button.upgrade:not([disabled])').waitFor();
		await page.evaluate(() => delete navigator.serviceWorker.getRegistration);
		assert.ok(await page.evaluate(async () => !!(await navigator.serviceWorker.getRegistration())), 'a failed press unregistered the worker');

		// Pressed while a write is out to the CLI: held here by the files' lock,
		// which every command takes. Nothing moves until it answers.
		await page.evaluate(
			() => new Promise((granted) => navigator.locks.request('datum-cli-files', () => (granted(), new Promise((release) => (window.release = release)))))
		);
		const input = page.locator('.prompt-bar input:not([disabled])');
		await input.fill('moment upgradeprobe');
		await input.press('Enter');
		const asked = pages;
		await upgrade.click();
		// Long enough for an unguarded press to have unregistered and asked the
		// origin for the page, which it holds for `slow`.
		await page.waitForTimeout(1500);
		assert.equal(pages, asked, 'the page reloaded under an unanswered write');
		assert.ok(await page.evaluate(async () => !!(await navigator.serviceWorker.getRegistration())), 'the worker went before the write answered');

		// Answered: then the page the origin answers, however slow, holding the entry.
		await Promise.all([page.waitForEvent('load'), page.evaluate(() => window.release())]);
		await mounted(page);
		assert.ok((await served(page)) > asked, `served ${await served(page)}, the origin's last before the press ${asked}`);
		const { stdout } = await page.evaluate(() => window.datum.runQuiet(['tail', 'upgradeprobe']));
		assert.match(stdout, /upgradeprobe/, 'the write was lost to the upgrade');
	} finally {
		slow = 0;
		deployed = null;
		await context.close();
	}
});
