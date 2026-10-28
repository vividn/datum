// An entry selected on the dayview is labeled `field: state`, and
// `[dur]` only where the entry gave one, and a light tan dotted line
// stands at the instant it was LOGGED, with that time above it, when that
// instant is on a day in view (da-k16pi). vividn, 2026-10-28: «on dayview
// make them more succinct, so just field: state [dur, if dur was specified
// on the entry that created it]. a light dotted line can be shown at the
// timestamp where the entry was entered if it is in view».

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, sharedApp, todayAt } from './support/app.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const NOW = todayAt(15);
const DAY = new Date(NOW).toISOString().slice(0, 10);
const TODAY = '.band[data-days-ago="0"]';
const shared = sharedApp({ clock: NOW, viewport: { width: 1280, height: 900 } });

/** Log `argv` with the clock at `at`, then stand it back at NOW and redraw. */
async function logAt(app, at, argv) {
	await app.clock.set(at);
	const ran = await app.run(argv);
	assert.equal(ran.code, 0, `${argv.join(' ')}: ${ran.stderr}`);
	await app.clock.set(NOW);
}

/** Click today's mark of `field`, and read the label it shows and the logged line drawn. */
async function select(page, field) {
	const mark = page.locator(`${TODAY} svg.svg rect[data-field="${field}"][data-id]`).first();
	await mark.waitFor({ state: 'attached' });
	const box = await mark.boundingBox();
	await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
	await until(
		() => page.evaluate(() => window.datum.bar.pills().map((p) => p.kind).join()),
		(kinds) => kinds === 'hid'
	);
	return page.locator(TODAY).evaluate((band) => {
		const tip = band.querySelector('.tip');
		const svg = band.querySelector('svg.svg');
		const line = svg.querySelector('line.logged');
		const day = svg.querySelector('[data-lo]');
		const n = (el, a) => Number(el.getAttribute(a));
		const at = line
			? Date.parse(day.getAttribute('data-from')) +
				((n(line, 'x1') - n(day, 'data-lo')) / (n(day, 'data-hi') - n(day, 'data-lo'))) *
					(Date.parse(day.getAttribute('data-to')) - Date.parse(day.getAttribute('data-from')))
			: null;
		return {
			label: tip && !tip.hidden ? tip.textContent : null,
			at,
			dotted: line?.getAttribute('stroke-dasharray') ?? null,
			above: svg.querySelector('text.logged')?.textContent ?? null,
			lines: band.closest('.strip').querySelectorAll('.logged').length
		};
	});
}

describe('the dayview names a selected entry, and marks when it was logged', () => {
	test('logged today without -d: `field: state [dur]`, and the line at its log time', async () => {
		await shared.fresh(async (app) => {
			await logAt(app, NOW, ['start', 'run', '30m', '-t', '10:00']);
			await app.redraw();
			const seen = await select(app.page, 'run');
			assert.equal(seen.label, 'run: on [30m]');
			assert.equal(Math.round(seen.at / 60_000), NOW / 60_000, `the line is at ${new Date(seen.at).toISOString()}`);
			assert.equal(seen.above, '15:00');
			assert.equal(seen.dotted, '1 3');

			// Tapped again, the entry goes, and its line with it.
			const mark = await app.page.locator(`${TODAY} svg.svg rect[data-field="run"]`).first().boundingBox();
			await app.page.mouse.click(mark.x + mark.width / 2, mark.y + mark.height / 2);
			await until(
				() => app.page.locator('.strip .logged').count(),
				(n) => n === 0
			);
		});
	});

	test('logged weeks ahead with -d: `field: state` alone, and no line, its log time on no day in view', async () => {
		await shared.fresh(async (app) => {
			await logAt(app, NOW - 40 * 86_400_000, ['switch', 'mood', 'happy', '-d', DAY, '-t', '09:00']);
			await app.redraw();
			const seen = await select(app.page, 'mood');
			assert.equal(seen.label, 'mood: happy');
			assert.equal(seen.lines, 0);
		});
	});
});
