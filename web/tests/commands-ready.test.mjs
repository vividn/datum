// The command pane at the bottom of every tab, and the command the pills
// make ready leading it (da-h3jcu).
//
// vividn, 2026-11-03: «let's keep the command pane at the bottom with the
// circular commands like we just discussed, keep that design unified across
// the app», and when pills make a command ready, «it's the pane's suggested
// action: a larger circle, dashed while something's missing, filled in its
// field's color once it can run». Which command, and when it is ready, is
// `suggestionOf` (commands-pure.test.mjs); this file is what a reader sees
// and what a tap does.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { ON_COLLECT, seedLines, sharedApp, startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP, along, widgetPane } from './support/layout.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const PAGE = JSON.stringify(
	along('column', widgetPane('repl', 'cli'), widgetPane('cmd', 'command'), widgetPane('bar', 'widgetbar'))
);

const shared = sharedApp(DESKTOP);
const onPage = (fn) => shared.solo(fn, PAGE);

const SEED = [
	"setup coffee --verb moment --color '#b87333'",
	"setup mood --verb switch --states good,bad --color '#336699' --state-color bad=#993366",
	'setup light --verb switch --states on,off'
];

const lead = (page) => page.locator('main section.pane[data-pane="cmd"] button.lead');
const load = (page, all) => page.evaluate((all) => window.datum.bar.setPills(all), all);
const field = (text) => ({ id: 'field', kind: 'field', text });
const state = (text) => ({ id: 'state', kind: 'state', text });

/** The lead circle as drawn: its name, whether it runs, its border style and fill. */
const drawn = (page) =>
	lead(page).evaluate((b) => {
		const css = getComputedStyle(b);
		return { name: b.textContent.trim(), ready: b.dataset.ready, border: css.borderTopStyle, fill: css.backgroundColor };
	});

/** The entries the database holds, newest last, as `field state`. */
const entries = async (app) =>
	JSON.parse((await app.runLine('tail --json -n 5')).stdout)
		.rows.map((r) => `${r.field} ${r.state ?? ''}`.trim());

const line = (page) => page.evaluate(() => document.querySelector('.prompt-bar input').value);

describe('the ready command leads the command pane', () => {
	test('with nothing loaded there is none', async () => {
		await onPage(async (app) => {
			await seedLines(app, SEED);
			assert.equal(await lead(app.page).count(), 0);
		});
	});

	test("a moment field's pill leads with a filled «moment coffee», and its tap logs it", async () => {
		await onPage(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);
			await load(page, [field('coffee')]);
			await lead(page).waitFor();
			assert.deepEqual(await drawn(page), { name: 'moment coffee', ready: 'true', border: 'solid', fill: 'rgb(184, 115, 51)' });
			await lead(page).click();
			await page.waitForFunction(() => !window.datum.bar.pills().length);
			assert.equal((await entries(app)).at(-1), 'coffee');
			await lead(page).waitFor({ state: 'detached' });
		});
	});

	test("a state field's pill with no state is dashed, and a tap writes the line rather than running", async () => {
		await onPage(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);
			const before = await entries(app);
			await load(page, [field('mood')]);
			await lead(page).waitFor();
			const { name, ready, border } = await drawn(page);
			assert.deepEqual({ name, ready, border }, { name: 'switch → ?', ready: 'false', border: 'dashed' });
			await lead(page).click();
			await page.waitForFunction(() => document.querySelector('.prompt-bar input').value === 'switch mood ');
			assert.deepEqual(await entries(app), before, 'the tap ran something');
			assert.deepEqual(await page.evaluate(() => window.datum.bar.pills()), [], 'the field pill was not spent onto the line');
		});
	});

	test('with its state pill too it fills in the state color and runs, spending both pills', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);
			await load(page, [field('mood'), state('bad')]);
			await page.waitForFunction(() => document.querySelector('button.lead')?.dataset.ready === 'true');
			assert.deepEqual(await drawn(page), { name: 'switch → bad', ready: 'true', border: 'solid', fill: 'rgb(153, 51, 102)' });
			await lead(page).click();
			await page.waitForFunction(() => !window.datum.bar.pills().length);
			assert.equal((await entries(app)).at(-1), 'mood bad');
			assert.equal(await line(page), '');
		});
	});
});

// vremya on #1636: a state pill left on the bar (a `switch` that failed keeps
// its pills) is its field's, so a field pill for another field, or none,
// takes it too; kept, the lead would read «switch → bad» on the wrong field.
test("another field's pill, or none, takes a state pill left on the bar, and the lead waits for a state again", async () => {
	await onPage(async (app) => {
		const { page } = app;
		await seedLines(app, SEED);
		const kinds = () => page.evaluate(() => window.datum.bar.pills().map((p) => `${p.kind}:${p.text}`));
		// Held, a field in collect is its pill: the gesture that sets another's (#lib/bar.ts's setFieldPill).
		const hold = (name) => page.locator(`main section.pane[data-pane="bar"] [data-field="${name}"]`).click({ button: 'right' });
		await load(page, [field('mood'), state('bad')]);
		await page.waitForFunction(() => document.querySelector('main button.lead')?.dataset.ready === 'true');

		await hold('light');
		await until(kinds, (k) => k.join() === 'field:light');
		await page.waitForFunction(() => document.querySelector('main button.lead')?.dataset.ready === 'false');
		assert.equal((await drawn(page)).name, 'switch → ?');

		await load(page, [field('mood'), state('bad')]);
		await hold('mood');
		await until(kinds, (k) => k.length === 0);
		await lead(page).waitFor({ state: 'detached' });
	});
});

/**
 * The command pane is every first-visit tab's last pane but welcome's: as
 * wide as the tab, and nothing under it.
 */
const TABS = { setup: 1, collect: 2, dayview: 3 };
const PHONE = { viewport: { width: 390, height: 844 }, device: { isMobile: true, hasTouch: true } };

for (const [device, options] of [
	['a desktop', { ...DESKTOP, desk: null }],
	['a phone', PHONE]
]) {
	test(`on ${device}, setup, collect and dayview each close on the command pane`, async () => {
		await withApp(
			async ({ page }) => {
				for (const [name, index] of Object.entries(TABS)) {
					await page.evaluate((i) => window.datum.shell.activate(i), index);
					// Every root tab stays mounted, clipped behind the one in front (TabBody.svelte): measure that one's.
					const drawn = () =>
						page.evaluate(() => {
							const front = [...document.querySelectorAll('main section.pane')].filter((p) => !p.closest('.tab-body.behind'));
							const box = (el) => el.getBoundingClientRect();
							const foot = front.filter((p) => p.dataset.title === 'command');
							return { panes: front.map(box), feet: foot.length, foot: foot[0] && box(foot[0]) };
						});
					// One command pane in front, once the tab left has gone behind.
					const { panes, foot } = await until(drawn, (d) => d.feet === 1);
					const left = Math.min(...panes.map((b) => b.left));
					const right = Math.max(...panes.map((b) => b.right));
					assert.ok(panes.every((b) => b.bottom <= foot.bottom + 0.5), `${name}: a pane under the commands`);
					assert.ok(Math.abs(foot.left - left) < 1 && Math.abs(foot.right - right) < 1, `${name}: the commands are not the tab's width`);
				}
			},
			{ ...options, home: ON_COLLECT }
		);
	});
}
