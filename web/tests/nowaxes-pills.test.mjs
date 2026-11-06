// The nowview's axes: a tap on a label, the pills it puts, a scrub into the now strip.
// Its story and helpers are support/nowaxes.mjs; one of four files, so each stays
// well inside test.mjs's 300 s file cap (da-9utsv).

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp } from './support/app.mjs';
import { finger } from './support/press.mjs';
import { until } from './support/until.mjs';
import { bothWays, NOW, NOWVIEW, withAxes, read, pt, pillsOf, timePill, drewPill } from './support/nowaxes.mjs';

before(startApp);
after(stopApp);

bothWays((vertical) => {
	test('a tap on an axis label selects that label’s time, on either axis', async () => {
		await withAxes(vertical, async (app) => {
			const { page } = app;
			const label = (sel, text) =>
				page.locator(`${NOWVIEW} svg.svg ${sel}`, { hasText: new RegExp(`^${text}$`) }).first().boundingBox();
			const middle = (b) => ({ x: b.x + b.width / 2, y: b.y + b.height / 2 });
			// Off its middle, as a finger is: it is still the label's time.
			const l = await label('.clock-label', vertical ? '14:45' : '14:50');
			await finger(page, { x: middle(l).x + 2, y: middle(l).y + 1 });
			await timePill(page, `-t ${vertical ? '14:45' : '14:50'}`);
			await drewPill(app);
			await page.locator(`${NOWVIEW} .nowview-now`).click();
			await page.locator(`${NOWVIEW} svg.svg .instant-line`).first().waitFor({ state: 'detached' });
			await app.idle();
			await finger(page, middle(await label('.tick-label', '-10m')));
			await timePill(page, '-qq');
		});
	});

	test('with a time selected the relative labels read t-5m and t+5m, and a tap on one counts from the selection', async () => {
		await withAxes(vertical, async (app) => {
			const { page } = app;
			// 14:33, a whole minute, as a time set from outside is.
			await page.evaluate((ms) => window.datum.cursor.set(new Date(ms)), NOW - 29.5 * 60_000);
			await drewPill(app);
			const after = page.locator(`${NOWVIEW} svg.svg .tick-label`, { hasText: /^t\+15m$/ }).first();
			await after.waitFor({ state: 'attached' });
			const words = await page.locator(`${NOWVIEW} svg.svg .tick-label`).allTextContents();
			for (const w of words) assert.match(w, /^t[-+]\d/);
			const at = await after.boundingBox();
			await finger(page, { x: at.x + at.width / 2, y: at.y + at.height / 2 });
			// Fifteen after the selection: 14:48, 14m30s back from now.
			await timePill(page, '-t -14m30s');
		});
	});

	test('a relative pill leaves the relative axis as it was, in words and in place, and its handle on now', async () => {
		await withAxes(vertical, async (app) => {
			const { page } = app;
			const ruler = () =>
				page.evaluate((sel) => {
					const of = (e) => e.getBoundingClientRect();
					const home = of(document.querySelector(`${sel} .nowview-band`));
					return [...document.querySelectorAll(`${sel} svg.svg .tick-label`)]
						.filter((l) => l.style.visibility !== 'hidden' && l.textContent)
						.map((l) => [l.textContent, of(l).left - home.left, of(l).top - home.top])
						.filter(([, x, y]) => x >= 0 && y >= 0 && x <= home.width && y <= home.height)
						.map(([w, x, y]) => [w, Math.round(x), Math.round(y)]);
				}, NOWVIEW);
			const handle = () => page.locator(`${NOWVIEW} .handle[data-axis="rel"]`).getAttribute('data-at');
			const was = await ruler();
			assert.ok(was.some(([w]) => /^-\d/.test(w)), 'no relative labels to keep');
			const washeld = await handle();
			const g = await read(page);
			await finger(page, pt(g, (g.clock[1].p + g.clock[2].p) / 2, g.relRow));
			await timePill(page, (t) => /^(-t -|-q)/.test(t));
			await drewPill(app);
			const on = await ruler();
			assert.deepEqual(on.map(([w]) => w), was.map(([w]) => w), 'the words moved');
			for (const [i, [w, x, y]] of on.entries()) assert.ok(Math.abs(x - was[i][1]) <= 1 && Math.abs(y - was[i][2]) <= 1, `${w} moved: ${[x, y]} from ${was[i].slice(1)}`);
			assert.equal(await handle(), washeld);
		});
	});

	test('an absolute pill turns the relative axis to t±, and dismissed it reads from now again', async () => {
		await withAxes(vertical, async (app) => {
			const { page } = app;
			const g = await read(page);
			const [a, b] = g.clock.slice(1);
			await finger(page, pt(g, (a.p + b.p) / 2, g.absRow));
			await timePill(page, (t) => /^-t \d\d:\d\d$/.test(t));
			await page.locator(`${NOWVIEW} svg.svg .tick-label`, { hasText: /^t[-+]/ }).first().waitFor({ state: 'attached' });
			await page.locator('.prompt-bar .pill.time button.remove').click();
			await page.locator(`${NOWVIEW} svg.svg .tick-label`, { hasText: /^-\d/ }).first().waitFor({ state: 'attached' });
			assert.equal(await page.locator(`${NOWVIEW} svg.svg .tick-label`, { hasText: /^t[-+]/ }).count(), 0);
		});
	});

	test('a tap on an axis’s pill dismisses it, and a drag from it scrubs its axis', async () => {
		await withAxes(vertical, async (app) => {
			const { page } = app;
			let g = await read(page);
			const [a, b] = g.clock.slice(1);
			await finger(page, pt(g, (a.p + b.p) / 2, g.absRow));
			const first = await timePill(page, (t) => /^-t \d\d:\d\d$/.test(t));
			await drewPill(app);
			const pill = await page.locator(`${NOWVIEW} .axis-pill`).boundingBox();
			const on = { x: pill.x + pill.width / 2, y: pill.y + pill.height / 2 };
			g = await read(page);
			const hit = await page.evaluate(({ x, y }) => !!document.elementFromPoint(x, y)?.closest('.axis-pill'), on);
			assert.ok(hit, 'the press does not reach the pill');
			const along = vertical ? on.y : on.x;
			const across = vertical ? on.x : on.y;
			await finger(page, on, pt(g, along + 40, across));
			const moved = await timePill(page, (t) => t !== first && /^-t /.test(t));
			assert.notEqual(moved, first, 'the drag from the pill did not scrub');
			await drewPill(app);
			const again = await page.locator(`${NOWVIEW} .axis-pill`).boundingBox();
			await finger(page, { x: again.x + again.width / 2, y: again.y + again.height / 2 });
			await until(
				() => pillsOf(page),
				(p) => !p.some((x) => x.startsWith('time:'))
			);
		});
	});

	// What «scrubbing is a bit inconsistent» came to on a phone: past the
	// now-line the clock's ran on into the future, and the relative one's
	// dropped its pill and brought it back minutes before now.
	for (const row of ['absRow', 'relRow'])
		test(`a scrub of the ${row === 'absRow' ? 'clock' : 'relative'} axis into the now strip says no time, and no earlier one`, async () => {
			await withAxes(vertical, async ({ page }) => {
				const g = await read(page);
				const [lo, hi] = vertical ? [g.frame.top, g.frame.bottom] : [g.frame.left, g.frame.right];
				const cdp = await page.context().newCDPSession(page);
				const said = [];
				const at = (p) => pt(g, p, g[row]);
				await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [at(lo + 0.6 * (hi - lo))] });
				for (let i = 0; i <= 10; i++) {
					await cdp.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: [at(lo + (0.6 + (0.38 * i) / 10) * (hi - lo))] });
					// A frame, for the page to have read the move.
					await page.evaluate(() => new Promise(requestAnimationFrame));
					said.push(await page.evaluate(() => window.datum.cursor.at()?.getTime() ?? null));
				}
				await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
				assert.ok(said.at(-1) === null && said.some((t) => t !== null), `the scrub said ${said}`);
				const first = said.indexOf(null);
				assert.ok(said.slice(first).every((t) => t === null), `a time came back past now: ${said}`);
				const before = said.slice(0, first);
				assert.ok(before.every((t, i) => t <= NOW && (!i || t >= before[i - 1])), `the scrub went back or on past now: ${said}`);
				const times = await page.evaluate(() => window.datum.bar.pills().filter((p) => p.kind === 'time').map((p) => p.text));
				assert.deepEqual(times, [], 'the lift in the strip left a time');
			});
		});
});
