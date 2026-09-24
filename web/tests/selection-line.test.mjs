// The selection is bright WHITE, yellow is NOW's alone, and every pill —
// the time pill too — is filled with the database's accent (da-ex56).
// vividn: "the selection color is too subtle right now, let's have it be
// bright white. The dotted yellow line should be reserved for right now …
// the pills should get the accent color of the current db".

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';

before(startApp);
after(stopApp);

// 15:00 UTC on 2026-09-10, work since 10:00; the cursor goes to noon.
const NOW = Date.UTC(2026, 8, 10, 15, 0, 0);
const AT = '2026-09-10T12:00:00Z';
const TODAY = '.band[data-days-ago="0"]';
const NOWVIEW = 'section.pane[data-title="nowview"]';
const PILL = '.prompt-bar .pill.time';

async function withDay(fn, query = '') {
	await withApp(
		async (app) => {
			const logged = await app.run(['start', 'work', '-t', '10:00']);
			assert.equal(logged.code, 0, logged.stderr);
			await app.reload();
			await app.page.locator(`${TODAY} rect[data-field="work"]`).waitFor();
			await app.page.evaluate((iso) => window.datum.cursor.set(new Date(iso)), AT);
			await fn(app);
		},
		{
			query,
			clock: NOW
		}
	);
}

/** The colour the page paints for `--accent`, through an element drawn in it. */
const accentOf = (page) =>
	page.evaluate(() => {
		const probe = document.createElement('span');
		probe.style.color = 'var(--accent)';
		document.body.append(probe);
		const painted = getComputedStyle(probe).color;
		probe.remove();
		return painted;
	});

/** The computed stroke of the cursor line in `scope`, once it is drawn. */
async function strokeIn(page, scope) {
	const line = page.locator(`${scope} line.cursor`);
	await line.waitFor({ state: 'attached' });
	return line.evaluate((l) => getComputedStyle(l).stroke);
}

const WHITE = 'rgb(255, 255, 255)';

describe('the selection and the pills', () => {
	for (const query of ['', '?db=finance']) {
		test(`${query || 'the default database'}: the line is white in the dayview and the nowview, the time pill the accent`, async () => {
			await withDay(async ({ page }) => {
				assert.equal(await strokeIn(page, TODAY), WHITE, 'the dayview');
				assert.equal(await strokeIn(page, NOWVIEW), WHITE, 'the nowview');

				const accent = await accentOf(page);
				const fill = () => page.locator(PILL).evaluate((p) => getComputedStyle(p).backgroundColor);
				assert.equal(await fill(), accent, 'the time pill');

				// Locked: the same fill, the ring is the only difference.
				await page.locator(`${PILL} button.lock`).click();
				await page.locator(`${PILL}.locked`).waitFor();
				assert.equal(await fill(), accent, 'the locked time pill');
				await page.locator(`${PILL} button.lock`).click();
				await page.locator(`${PILL}:not(.locked)`).waitFor();

				// Spent: a line logged with the unlocked pill leaves it fading.
				await page.locator('.prompt-bar input').fill('occur tea');
				await page.locator('.prompt-bar input').press('Enter');
				const spent = await page.waitForFunction(() => {
					const s = document.querySelector('.prompt-bar .spent.time');
					return s && getComputedStyle(s).backgroundColor;
				});
				assert.equal(await spent.jsonValue(), accent, 'the spent time pill');
			}, query);
		});
	}

	test('no mark on any picture is yellow but the now-line', async () => {
		await withDay(async ({ page }) => {
			await page.locator(`${NOWVIEW} line.cursor`).waitFor({ state: 'attached' });
			const yellow = await page.evaluate(() => {
				// Hue 40°–70°, saturated and bright: #ffcc00, #ffd166, #ffff00.
				const isYellow = (paint) => {
					const m = /^rgba?\((\d+), (\d+), (\d+)/.exec(paint);
					if (!m) return false;
					const [r, g, b] = m.slice(1, 4).map((c) => c / 255);
					const hi = Math.max(r, g, b);
					const lo = Math.min(r, g, b);
					if (hi < 0.6 || hi - lo < 0.4 || hi !== r) return false;
					const hue = 60 * ((g - b) / (hi - lo));
					return hue >= 40 && hue <= 70;
				};
				return [...document.querySelectorAll('svg.svg *')]
					.filter((el) => !/^current-time-/.test(el.getAttribute('class') ?? ''))
					// A field's own colour is the person's, not the page's.
					.filter((el) => !el.closest('[data-field]'))
					.filter((el) => {
						const style = getComputedStyle(el);
						return isYellow(style.stroke) || isYellow(style.fill);
					})
					.map((el) => el.outerHTML.slice(0, 120));
			});
			assert.deepEqual(yellow, []);
			// And the now-line is there to be the exception.
			assert.ok((await page.locator('svg.svg .current-time-line').count()) > 0);
		});
	});
});
