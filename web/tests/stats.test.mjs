// The stats widget draft at /stats (dashboard D3, da-n7r0).
//
// The widget is a picker and a frame: every number on it is `datum stats`'s.
// So each assertion holds what is on screen to what the CLI prints for the
// same argv against the same database — run through the same seam in the
// same test — never to "some numbers appeared".

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp as withAnyApp, workerClock } from './support/app.mjs';

before(startApp);
after(stopApp);

// `stats` measures up to now, so two runs a second apart can print two
// different durations. Every command here sees one fixed clock, which is
// what lets the widget's run and the test's run of the same argv agree.
const clock = workerClock(Date.UTC(2026, 8, 10, 15, 0));
const withApp = (fn, { prepare, ...options } = {}) =>
	withAnyApp(fn, {
		...options,
		prepare: async (context) => {
			await clock.prepare(context);
			await prepare?.(context);
		}
	});

/** Terminal text as a reader sees it: no colour codes, runs of space as one. */
const plain = (s) => s.replace(/\x1b\[[0-9;]*m/g, '').replace(/\s+/g, ' ').trim();

/** Log a small history: two coffees and a mood. */
async function seed({ run }) {
	for (const argv of [
		['occur', 'coffee', '-t', '-30'],
		['occur', 'coffee', '-t', '-10'],
		['switch', 'mood', 'happy', '-t', '-20']
	]) {
		const r = await run(argv);
		assert.equal(r.code, 0, `${argv.join(' ')}: ${r.stderr}`);
	}
}

/** Go to /stats the way a reader does: by its URL, which opens the stats tab (da-0a5o). */
async function openStats(app) {
	await app.goto('/stats');
	await app.page.locator('section.pane[data-title="stats"]').waitFor();
}

const field = (page, name) => page.locator('.stats button.field', { hasText: name });

/**
 * Wait until the widget shows `argv` and the CLI's own text for it.
 *
 * The expected text is captured through the seam first, so the comparison
 * is against this database, not against a string written down here.
 */
async function expectStats({ page, run }, argv) {
	const cli = await run(argv);
	assert.equal(cli.code, 0, `${argv.join(' ')}: ${cli.stderr}`);
	const want = { argv: argv.join(' '), text: plain(cli.stdout) };
	await page
		.waitForFunction(
			({ argv, text }) =>
				document.querySelector('.stats .argv')?.textContent === argv &&
				(document.querySelector('.stats .body')?.textContent ?? '').replace(/\s+/g, ' ').trim() === text,
			want
		)
		.catch(async () => {
			const shown = await page.locator('.stats section').innerText();
			assert.fail(`wanted ${JSON.stringify(want)}, the widget shows:\n${shown}`);
		});
}

describe('/stats', () => {
	test('no field chosen: the field list and a hint, all time selected', async () => {
		await withApp(async (app) => {
			await seed(app);
			await openStats(app);
			await field(app.page, 'coffee').waitFor();
			assert.deepEqual(await app.page.locator('.stats button.field').allInnerTexts(), ['coffee', 'mood']);
			assert.equal(await app.page.locator('.stats section .quiet').innerText(), 'pick a field');
			assert.equal(
				await app.page.locator('.stats .range button[aria-pressed="true"]').innerText(),
				'all time'
			);
			assert.deepEqual(app.errors, []);
		});
	});

	test("a field shows the CLI's stats; a state narrows it to field:state", async () => {
		await withApp(async (app) => {
			await seed(app);
			await openStats(app);

			await field(app.page, 'coffee').click();
			await expectStats(app, ['stats', 'coffee']);
			assert.equal(await app.page.locator('.stats .tile.entries dd').innerText(), '2');

			await field(app.page, 'mood').click();
			await expectStats(app, ['stats', 'mood']);
			await app.page.locator('.stats button.state', { hasText: 'happy' }).click();
			await expectStats(app, ['stats', 'mood:happy']);
		});
	});

	test('the range adds its flags: a period, or --since / --until', async () => {
		await withApp(async (app) => {
			await seed(app);
			await openStats(app);
			await field(app.page, 'coffee').click();

			await app.page.locator('.stats .range button', { hasText: 'today' }).click();
			await expectStats(app, ['stats', 'coffee', '--today']);

			await app.page.locator('.stats .range button', { hasText: 'custom' }).click();
			await expectStats(app, ['stats', 'coffee']);
			await app.page.locator('.stats .custom input').nth(0).fill('2026-01-01T00:00');
			await expectStats(app, ['stats', 'coffee', '--since', '2026-01-01T00:00']);
			await app.page.locator('.stats .custom input').nth(1).fill('2099-01-01T00:00');
			await expectStats(app, [
				'stats',
				'coffee',
				'--since',
				'2026-01-01T00:00',
				'--until',
				'2099-01-01T00:00'
			]);
		});
	});

	test('a write in another tab changes the numbers without a reload', async () => {
		await withApp(async (app) => {
			await seed(app);
			await openStats(app);
			await field(app.page, 'coffee').click();
			await expectStats(app, ['stats', 'coffee']);
			const count = app.page.locator('.stats .tile.entries dd');
			assert.equal(await count.innerText(), '2');

			// The day frame, in a second tab on the same database.
			const other = await app.page.context().newPage();
			await other.goto(new URL('/', app.page.url()).href);
			await other.waitForFunction(() => typeof window.datum?.run === 'function');
			// The loud door (da-6918): a real tab logging an entry, echoed and
			// receipted there as a hand at its console would see it. What this
			// test is about is the FIRST tab, which hears it over the change
			// channel either way.
			const r = await other.evaluate(() => window.datum.run(['occur', 'coffee', '-t', '-1']));
			assert.equal(r.code, 0, r.stderr);

			await app.page.waitForFunction(
				() => document.querySelector('.stats .tile.entries dd')?.textContent === '3'
			);
			await expectStats(app, ['stats', 'coffee']);
		});
	});

	test('a reload keeps the selection', async () => {
		await withApp(async (app) => {
			await seed(app);
			await openStats(app);
			await field(app.page, 'mood').click();
			await app.page.locator('.stats button.state', { hasText: 'happy' }).click();
			await app.page.locator('.stats .range button', { hasText: 'week' }).click();
			await expectStats(app, ['stats', 'mood:happy', '--week']);

			await app.page.reload();
			await app.page.waitForFunction(() => typeof window.datum?.run === 'function');
			await expectStats(app, ['stats', 'mood:happy', '--week']);
		});
	});

	test('blocked storage: the default selection, and no error', async () => {
		await withApp(
			async (app) => {
				await seed(app);
				await openStats(app);
				await field(app.page, 'coffee').waitFor();
				assert.equal(await app.page.locator('.stats section .quiet').innerText(), 'pick a field');

				await field(app.page, 'coffee').click();
				await expectStats(app, ['stats', 'coffee']);
				assert.deepEqual(app.errors, [], 'the page threw with storage blocked');
			},
			{
				prepare: (context) =>
					context.addInitScript(() => {
						Object.defineProperty(window, 'localStorage', {
							configurable: true,
							get() {
								throw new DOMException('storage is blocked', 'SecurityError');
							}
						});
					})
			}
		);
	});

	test('at 360px the field list stacks above the body, and nothing scrolls sideways', async () => {
		await withApp(
			async (app) => {
				await seed(app);
				await openStats(app);
				await field(app.page, 'coffee').click();
				await expectStats(app, ['stats', 'coffee']);

				const fit = await app.page.evaluate(() => {
					// The widget's own box, not the pane's: the frame around it is
					// named `.stats` too (PaneFrame.svelte's host class).
					const widget = document.querySelector('section.pane .stats');
					const aside = widget.querySelector('aside').getBoundingClientRect();
					const section = widget.querySelector('section').getBoundingClientRect();
					const main = document.querySelector('main');
					return {
						stacked: aside.bottom <= section.top,
						sideways: main.scrollWidth - main.clientWidth
					};
				});
				assert.ok(fit.stacked, 'the field list is not above the body');
				assert.equal(fit.sideways, 0, 'the page scrolls sideways');
			},
			{ viewport: { width: 360, height: 640 } }
		);
	});
});
