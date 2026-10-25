// The fields widget never looks disabled while datum boots, syncs or draws
// (da-ch6oc). vividn, 2026-10-25: «when switching back and forth between
// collect and dayview tabs, while the new dayview is drawing and you switch
// back to the collect tab. The fields widget is dimmed for some reason … It
// also seems to dim during the initial sync and redraw. This would
// discourage tracking data quickly».
//
// What dimmed it both times was the copy of the last visit (Stale.svelte),
// at half opacity, laid over the widget until the worker's queue ran dry: on
// the boot over a widget not yet mounted, and on every return to a root tab,
// which mounts its layout afresh, over a live widget the copy hid. The
// fields widget asks the CLI nothing of its own, so it takes no copy: it is
// drawn live from the start, from the bar it last showed on this database,
// and a press is run as soon as the queue gets to it.
//
// Slowness is made inside the worker: every `dayview` busy-waits as it reads
// its argv, so a strip's bands keep the queue full; and the boot holds every
// answer for HOLD ms, as a slow first sync or an upgrade does.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { inWorker, seedLines, startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP } from './support/layout.mjs';

before(startApp);
after(stopApp);

const HOLD = 3000;

/** Every `dayview` takes `ms` longer, spent inside its own process. */
const slowDayview = (ms) => `
	const instantiate = WebAssembly.instantiate;
	WebAssembly.instantiate = async function (module, imports) {
		const wasi = imports?.wasi_snapshot_preview1;
		if (!wasi) return instantiate.apply(this, arguments);
		let instance;
		let size = 0;
		const sizes = wasi.args_sizes_get;
		const get = wasi.args_get;
		wasi.args_sizes_get = (count, at) => {
			const code = sizes(count, at);
			size = new DataView(instance.exports.memory.buffer).getUint32(at, true);
			return code;
		};
		wasi.args_get = (pointers, buffer) => {
			const code = get(pointers, buffer);
			const argv = new TextDecoder().decode(new Uint8Array(instance.exports.memory.buffer, buffer, size)).split('\\0');
			if (argv[1] === 'dayview') for (const end = performance.now() + ${ms}; performance.now() < end; );
			return code;
		};
		instance = await instantiate.call(this, module, imports);
		return instance;
	};
`;

/** Every answer held for HOLD ms from the page's first request (first-paint.test.mjs's `held`). */
const heldBoot = `
	const post = self.postMessage.bind(self);
	let until = null;
	const queue = [];
	self.addEventListener('message', () => (until ??= performance.now() + ${HOLD}));
	self.postMessage = (message) => {
		if (message && 'notice' in message) return post(message);
		if (!queue.length && !(performance.now() < until)) return post(message);
		if (!queue.length) setTimeout(() => queue.splice(0).forEach((m) => post(m)), until - performance.now());
		queue.push(message);
	};
`;

/**
 * Watch the fields pane frame by frame from now: `window.__dim` collects
 * each frame it looked disabled — a copy over it, or a button drawn at less
 * than full opacity — and why.
 */
function watchDim() {
	const dim = (window.__dim = []);
	const opacity = (el) => {
		let o = 1;
		for (; el; el = el.parentElement) o *= Number(getComputedStyle(el).opacity);
		return o;
	};
	const tick = () => {
		const pane = document.querySelector('section.pane[data-title="fields"]');
		if (pane?.querySelector('[data-stale]')) dim.push('a copy over it');
		for (const b of pane?.querySelectorAll('button[data-field]') ?? [])
			if (opacity(b) < 1) dim.push(`${b.dataset.field} at ${opacity(b).toFixed(2)}${b.disabled ? ', disabled' : ''}`);
		if (!window.__dimStop) requestAnimationFrame(tick);
	};
	tick();
}

const dimmed = (page) => page.evaluate(() => ((window.__dimStop = true), [...new Set(window.__dim)]));

const button = (page, field) => page.locator(`section.pane[data-title="fields"] button[data-field="${field}"]`);
const rootTab = (page, name) => page.locator('.root-bar [role="tab"]', { hasText: name });
/** How many `tea` entries the database holds. */
const teas = async (app) => JSON.parse((await app.run(['tail', 'tea', '--json'])).stdout).rows.length;

test('back on a tab while the strip draws, the fields are live and a press is taken', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			await seedLines(app, ['occur coffee -t -60', 'occur tea -t -60']);
			await button(page, 'tea').waitFor();
			// A visit before this one, whose copies the page keeps.
			await app.reload();
			await app.idle();
			await button(page, 'tea').waitFor();

			await rootTab(page, 'dayview').click();
			await page.locator('section.pane[data-title="dayview"] svg').first().waitFor({ state: 'attached' });
			await page.evaluate(watchDim);
			await rootTab(page, 'main').click();
			await button(page, 'tea').click();
			assert.ok(await page.evaluate(() => window.__datumInflight > 0), 'the strip was done drawing: nothing to wait behind');
			await app.idle();
			assert.deepEqual(await dimmed(page), [], 'the fields widget looked disabled');
			assert.equal(await teas(app), 2, 'the press was not taken');
		},
		{ ...DESKTOP, home: { names: ['main', 'dayview'], active: 0 }, prepare: inWorker(slowDayview(300)) }
	);
});

test('while datum boots, the fields are live and a press is taken', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			await seedLines(app, ['occur coffee -t -60', 'occur tea -t -60']);
			await button(page, 'tea').waitFor();
			await inWorker(heldBoot)(page.context());
			// From the first frame of the boot.
			await page.context().addInitScript(watchDim);
			// Caught, so a press refused fails as itself and not as the boot it left behind.
			const reloading = app.reload().catch((e) => e);
			const asked = Date.now();
			await button(page, 'tea').click({ timeout: HOLD / 2 });
			assert.ok(Date.now() - asked < HOLD, 'pressed only after the hold');
			const booted = await reloading;
			if (booted instanceof Error) throw booted;
			await app.idle();
			assert.deepEqual(await dimmed(page), [], 'the fields widget looked disabled');
			assert.equal(await teas(app), 2, 'the press was not taken');
		},
		DESKTOP
	);
});
