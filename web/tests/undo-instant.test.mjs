// An undo draws at once (da-gou47; vividn, 2026-11-10: «for at least 1 undo,
// the old state of everything maybe should be cached for a little bit, so
// that the swap back to how things were can feel instant»). The seam keeps
// what the reads answered before the last write (#lib/run.ts), and an undo
// of that write answers them from it until datum's own answer lands.
//
// Each pane is timed from the undo to the frame its markup is again what it
// was before the write, with the page's clock held so nothing else moves it.
// With the CLI's files held, nothing datum answers can be what drew it.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { ON_COLLECT, startApp, stopApp, withApp, todayAt } from './support/app.mjs';
import { DESKTOP, PHONE, OUT } from './support/bar.mjs';
import { rested } from './support/layout.mjs';

before(startApp);
after(stopApp);

const SCREENS = [['a phone', PHONE], ['a desktop', DESKTOP]];
const opts = (screen) => ({ ...screen, home: ON_COLLECT, clock: todayAt(15) });

/** Each widget's markup by its pane's title, the CLI's left out: it shows the commands, which an undo adds to. */
const panes = (page) =>
	page.evaluate(() => {
		window.__panes = () =>
			Object.fromEntries(
				[...document.querySelectorAll('[data-title]')]
					.filter((p) => !p.querySelector('[data-title]') && !/^(cli|repl)$/.test(p.dataset.title))
					.map((p) => [p.dataset.title, p.innerHTML])
			);
		return window.__panes();
	});

/** The panes as they stand, kept as the page's `__before`; their titles. */
const snapshot = (page) =>
	panes(page).then((now) => page.evaluate((now) => Object.keys((window.__before = now)), now));

/** `a` is `b`, or a failure quoting where they part. */
function same(a, b, what) {
	if (a === b) return;
	const i = [...a].findIndex((c, k) => c !== b[k]);
	assert.fail(`${what}: they part at ${i}:\n  ${a.slice(i - 200, i + 200)}\n  ${b?.slice(i - 200, i + 200)}`);
}

/** `start walk` pressed, as a widget's press is, and its panes settled. */
async function write(app) {
	await app.page.evaluate(() => window.datum.run(['start', 'walk']));
	await settle(app);
}

/** Wait for the write's panes to settle, so the undo starts from them. */
async function settle(app) {
	await app.idle();
	await rested(app.page, null, { app });
}

/**
 * Undo by `how` ('typed': the bar's line, as the console's `datum.run` is;
 * 'button': the newest command's ↶ in the bar's output), and for each pane
 * the ms from it to the first frame it is as it was, or null within `ms`.
 * `held`: the CLI's files are held throughout, so datum answers nothing.
 */
function undo(page, how, { held = false, ms = 5000, stay = false } = {}) {
	return page.evaluate(
		async ([how, held, ms, stay, OUT]) => {
			const frame = () => new Promise((resolve) => requestAnimationFrame(resolve));
			let release = () => {};
			if (held) {
				await new Promise((holding) =>
					navigator.locks.request('datum-cli-files', () => {
						holding();
						return new Promise((r) => (release = r));
					})
				);
			}
			const at = {};
			const moved = {};
			const t0 = performance.now();
			let answered = null;
			const sent = how === 'typed' ? window.datum.run(['undo']) : (document.querySelector(`${OUT} .step button.undo`).click(), null);
			sent?.then(() => (answered = performance.now() - t0));
			while (performance.now() - t0 < ms) {
				await frame();
				const now = window.__panes();
				for (const [title, html] of Object.entries(window.__before)) {
					if (at[title] === undefined && now[title] === html) at[title] = Math.round(performance.now() - t0);
					if (moved[title] === undefined && now[title] !== html) moved[title] = Math.round(performance.now() - t0);
				}
				if (!stay && Object.keys(window.__before).every((t) => at[t] !== undefined)) break;
			}
			release();
			return { at: Object.fromEntries(Object.keys(window.__before).map((t) => [t, at[t] ?? null])), moved, answered: answered && Math.round(answered) };
		},
		[how, held, ms, stay, OUT]
	);
}

for (const [name, screen] of SCREENS)
	for (const how of ['typed', 'button'])
		test(`on ${name}, an undo ${how === 'typed' ? 'typed' : 'by its ↶'} draws every pane as it was before the write within a frame, with datum not yet answering, and datum's answer then draws it the same`, async (t) => {
			await withApp(async (app) => {
				const { page } = app;
				await settle(app);
				const titles = await snapshot(page);
				assert.ok(titles.length >= 3, `the collect tab's panes: ${titles}`);
				await write(app);
				const changed = await page.evaluate(() => Object.keys(window.__before).filter((t) => window.__panes()[t] !== window.__before[t]));
				assert.ok(changed.length >= 2, `the write changes panes: ${changed}`);

				const { at } = await undo(page, how, { held: true, ms: 1500 });
				t.diagnostic(`${name}, ${how}, files held: ${JSON.stringify(at)}`);
				for (const title of titles) assert.ok(at[title] !== null && at[title] < 100, `${title} drawn as before the write while datum is held: ${JSON.stringify(at)}`);

				// Datum's own answer, once the files are free, draws them the same.
				await settle(app);
				const after = await panes(page);
				const was = await page.evaluate(() => window.__before);
				for (const title of titles) same(after[title], was[title], `${title} once datum answered`);
				await app.reload();
				await settle(app);
				const fresh = await panes(page);
				// Less the ids a page numbers as it mounts (a picture's tooltip), which a reload counts afresh.
				const unnumbered = (html) => html.replace(/ (id|aria-describedby)="[^"]*"/g, '');
				for (const title of changed) same(unnumbered(fresh[title]), unnumbered(after[title]), `${title}: a fresh draw is what the undo settled on`);
			}, opts(screen));
		});

// The numbers for the PR: the same undo, datum free, timed per pane.
for (const [name, screen] of SCREENS)
	test(`on ${name}, how long each pane takes to show an undo`, async (t) => {
		await withApp(async (app) => {
			const { page } = app;
			for (const how of ['typed', 'button']) {
				await settle(app);
				await snapshot(page);
				await write(app);
				const got = await undo(page, how);
				t.diagnostic(`${name}, ${how}: ${JSON.stringify(got)}`);
				assert.ok(Object.values(got.at).every((ms) => ms !== null), JSON.stringify(got));
			}
		}, opts(screen));
	});

test('a second tab writing between the write and its undo: the undo draws nothing of its own until datum answers', async () => {
	await withApp(async (app) => {
		const { page } = app;
		await settle(app);
		await write(app);
		const other = await app.newTab();
		await other.page.evaluate(() => window.datum.run(['occur', 'coffee']));
		await other.idle();
		// This tab heard it and drew it.
		await page.locator('[data-title="tail"]', { hasText: 'coffee' }).waitFor();
		await settle(app);
		await snapshot(page);
		// Nothing moved while datum was held: no pane took a state from before the write.
		const { moved } = await undo(page, 'typed', { held: true, ms: 600, stay: true });
		assert.deepEqual(moved, {}, 'a pane changed with datum held');
		// And datum's answer still lands.
		await settle(app);
	}, opts(DESKTOP));
});
