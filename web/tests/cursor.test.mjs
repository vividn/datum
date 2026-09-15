// The time cursor (da-wnt2): a click on the dayview points the page at the
// time under it — a dotted blue line there, ONE `-t` pill in the bar, and
// the nowview centred on it with a dotted line of its own. A line typed
// with the pill is logged at it; a press on the pill takes all of it away.
//
// Delta-blind on purpose: every time is held to what the picture's own
// geometry puts under the click, every argv to what the seam was handed,
// and every stored row to what the CLI wrote — never to "a pill appeared".

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { devices } from 'playwright';
import { startApp, stopApp, withApp, workerClock } from './support/app.mjs';

before(startApp);
after(stopApp);

// 15:00 UTC on 2026-09-10: a coffee at 09:00 and work since 10:00 are
// that morning, in today's band of the day strip.
const NOW = Date.UTC(2026, 8, 10, 15, 0, 0);
const TODAY = '.band[data-days-ago="0"]';
const PAST = '.band[data-days-ago="1"]';
const NOWVIEW = 'section.pane[data-title="nowview"]';
const PILL = '.prompt-bar .pill';

async function withDay(fn, options = {}) {
	const clock = workerClock(NOW);
	await withApp(
		async (app) => {
			for (const argv of [
				['occur', 'coffee', '-t', '09:00'],
				['start', 'work', '-t', '10:00']
			]) {
				const logged = await app.run(argv);
				assert.equal(logged.code, 0, logged.stderr);
			}
			await app.reload();
			await app.page.locator(`${TODAY} rect[data-field="work"]`).waitFor();
			await fn(app);
		},
		{
			...options,
			prepare: async (context) => {
				await context.clock.setFixedTime(new Date(NOW));
				await clock.prepare(context);
			}
		}
	);
}

/**
 * Where on screen the instant `f` of the way through a picture's day is
 * drawn, and the instant — read off the picture's own `data-` geometry,
 * independently of the app's mapping. The day is the one starting `from`,
 * or the picture's first.
 */
const spot = (page, picture, f, from) =>
	page.locator(`${picture} svg.svg`).evaluate(
		(svg, [f, from]) => {
			const day = from ? svg.querySelector(`[data-lo][data-from="${from}"]`) : svg.querySelector('[data-lo]');
			const num = (n) => Number(day.getAttribute(n));
			const box = svg.getBoundingClientRect();
			const view = svg.viewBox.baseVal;
			const user = num('data-lo') + f * (num('data-hi') - num('data-lo'));
			const label = svg
				.querySelector(`.day-label[data-day="${day.getAttribute('data-day')}"]`)
				.getBoundingClientRect();
			const t0 = Date.parse(day.getAttribute('data-from'));
			const t1 = Date.parse(day.getAttribute('data-to'));
			return {
				x: box.left + ((user - view.x) * box.width) / view.width,
				// Level with the day's date label — inside its slot — or, on a
				// one-day band, below the title lying over its top (da-13bc).
				y: from ? label.top + label.height / 2 : box.top + box.height * 0.6,
				at: t0 + f * (t1 - t0)
			};
		},
		[f, from]
	);

/** The time pill's text, without its ×; `null` when there is none. */
const pillText = async (page) => {
	const pills = page.locator(`${PILL}.time`);
	return (await pills.count()) ? pills.evaluate((p) => p.querySelector('.label').textContent) : null;
};

/** `-t HH:MM` / `-t YYYY-MM-DDTHH:MM` on 2026-09-10 (UTC) as an instant. */
const pillInstant = (text) => {
	const [, day, hm] = /^-t (?:(\d{4}-\d\d-\d\d)T)?(\d\d:\d\d)$/.exec(text);
	return Date.parse(`${day ?? '2026-09-10'}T${hm}:00Z`);
};

/** The screen x of each cursor line inside `scope`. */
const lineXs = (page, scope) =>
	page.locator(`${scope} line.cursor`).evaluateAll((ls) =>
		ls.map((l) => {
			const r = l.getBoundingClientRect();
			return { x: r.left + r.width / 2, stroke: l.getAttribute('stroke'), dash: l.getAttribute('stroke-dasharray') };
		})
	);

/**
 * Type a line into the bottom bar and wait for its receipt.
 *
 * The receipt is the first line of a document section in the result area
 * since da-xfa4, so there is none at all before the first command — where
 * the old bar kept an empty span.
 */
const receiptNow = () => document.querySelector('.prompt-bar .doc .receipt')?.textContent ?? '';

async function barRun(page, line) {
	const before = await page.evaluate(receiptNow);
	await page.locator('.prompt-bar input').fill(line);
	await page.locator('.prompt-bar input').press('Enter');
	await page.waitForFunction(
		`(${receiptNow})() !== ${JSON.stringify(before)} && (${receiptNow})() !== ''`
	);
	return page.locator('.prompt-bar .doc .receipt').innerText();
}

describe('the time cursor', () => {
	test('a click on today: a pill, a line, the nowview centred on it; a line logs at it and clears it; × clears it too', async () => {
		await withDay(async (app) => {
			const { page } = app;
			await app.spyOnRunner();
			const at = await spot(page, TODAY, 0.5);
			await page.mouse.click(at.x, at.y);

			const text = await pillText(page);
			assert.match(text ?? '', /^-t \d\d:\d\d$/, `the pill: ${text}`);
			const cursor = pillInstant(text);
			assert.ok(Math.abs(cursor - at.at) <= 60_000, `${text} for ${new Date(at.at).toISOString()}`);

			const [line] = await lineXs(page, TODAY);
			assert.ok(line && Math.abs(line.x - at.x) <= 1, `the line at ${line?.x}, the click at ${at.x}`);
			assert.deepEqual([line.stroke, line.dash], ['#7c9cff', '3 3']);

			const iso = new Date(cursor).toISOString();
			await page.waitForFunction(
				(iso) => {
					const argv = window.__datumSeen.filter((a) => a[0] === 'nowview').at(-1);
					return argv?.[argv.indexOf('--at') + 1] === iso;
				},
				iso
			);
			// The nowview's own line is its centre, on its own axis.
			await page.locator(`${NOWVIEW} line.cursor`).waitFor({ state: 'attached' });
			const centre = await page
				.locator(`${NOWVIEW} svg.svg`)
				.evaluate((svg) => {
					const day = svg.querySelector('[data-lo]');
					const mid = (Number(day.getAttribute('data-lo')) + Number(day.getAttribute('data-hi'))) / 2;
					return [mid, Number(svg.querySelector('line.cursor').getAttribute('x1'))];
				});
			assert.ok(Math.abs(centre[0] - centre[1]) < 0.5, `nowview's line at ${centre[1]}, its centre ${centre[0]}`);

			// A line from the bar is logged at the cursor, then the (unlocked)
			// pill clears on success — and the cursor everywhere with it.
			assert.match(await barRun(page, 'occur tea'), /^CREATE /);
			const [tea] = (await app.entries()).filter((e) => e.field === 'tea');
			assert.equal(Date.parse(tea.occur_time.utc), cursor, JSON.stringify(tea.occur_time));
			const tail = await app.paneTable('tail');
			const row = tail.rows.find((r) => r.includes('tea'));
			assert.ok(row?.[tail.keys.indexOf('time')].startsWith(text.slice(3)), JSON.stringify(row));
			assert.equal(await pillText(page), null);
			await page.waitForFunction(() => {
				const argv = window.__datumSeen.filter((a) => a[0] === 'nowview').at(-1);
				return argv && !argv.includes('--at');
			});
			assert.equal(await page.locator('line.cursor').count(), 0);

			// Click again: a press on a fresh pill also clears the cursor everywhere.
			const at2 = await spot(page, TODAY, 0.5);
			await page.mouse.click(at2.x, at2.y);
			await pillText(page);
			await page.locator(`${PILL} button.remove`).click();
			assert.equal(await page.locator(PILL).count(), 0);
			await page.waitForFunction(() => {
				const argv = window.__datumSeen.filter((a) => a[0] === 'nowview').at(-1);
				return argv && !argv.includes('--at');
			});
			assert.equal(await page.locator('line.cursor').count(), 0);
		});
	});

	// Off UTC, the nowview's centre label is still the pill's time: under WASI
	// `chrono::Local` is UTC, and a 12:26 cursor was labelled 10:26 (da-y6qj).
	test("Europe/Berlin: the nowview's centre label is the pill's time", async () => {
		await withDay(
			async ({ page }) => {
				const at = await spot(page, TODAY, 0.52);
				await page.mouse.click(at.x, at.y);
				const text = await pillText(page);
				assert.match(text ?? '', /^-t \d\d:\d\d$/, `the pill: ${text}`);
				// On screen: the labels sit in the plot's translated group, the
				// cursor line on the svg itself.
				const label = await page.waitForFunction(
					(scope) => {
						const svg = document.querySelector(`${scope} svg.svg`);
						const mid = (el) => {
							const r = el?.getBoundingClientRect();
							return r ? r.left + r.width / 2 : NaN;
						};
						const x = mid(svg?.querySelector('line.cursor'));
						const ticks = [...(svg?.querySelectorAll('text.tick-label') ?? [])];
						const centre = ticks.find((t) => Math.abs(mid(t) - x) < 1);
						return centre?.textContent;
					},
					NOWVIEW
				);
				assert.equal(await label.jsonValue(), text.slice(3));
			},
			{ timezone: 'Europe/Berlin' }
		);
	});

	test("a click on yesterday's band: the dated pill, and a line logged with it lands on yesterday", async () => {
		await withDay(async (app) => {
			const { page } = app;
			await page.evaluate(() => (document.querySelector('.cell.dayview').style.flex = '0 0 400px'));
			await page
				.locator(`${PAST} [data-lo][data-from="2026-09-09T00:00:00.000Z"]`)
				.waitFor({ state: 'attached' });
			const at = await spot(page, PAST, 0.25, '2026-09-09T00:00:00.000Z');
			await page.mouse.click(at.x, at.y);

			const text = await pillText(page);
			assert.match(text ?? '', /^-t 2026-09-09T\d\d:\d\d$/, `the pill: ${text}`);
			assert.ok(Math.abs(pillInstant(text) - at.at) <= 60_000, `${text} for ${new Date(at.at).toISOString()}`);
			assert.equal((await lineXs(page, PAST)).length, 1);
			assert.equal((await lineXs(page, TODAY)).length, 0, 'a line on a band whose day is not the cursor’s');

			assert.match(await barRun(page, 'occur x'), /^CREATE /);
			const [x] = (await app.entries()).filter((e) => e.field === 'x');
			assert.equal(Date.parse(x.occur_time.utc), pillInstant(text));
		});
	});

	test("Enter on a focused mark: the cursor is the mark's own time", async () => {
		await withDay(async ({ page }) => {
			const marks = page.locator(`${TODAY} [data-state]`);
			await marks.first().focus();
			await page.keyboard.press('Enter');
			assert.equal(await pillText(page), '-t 09:00');
			await page.keyboard.press('Tab');
			await page.keyboard.press('Enter');
			assert.equal(await pillText(page), '-t 10:00');
			assert.equal(await page.locator(`${PILL}.time`).count(), 1, 'one time pill, not two');
		});
	});

	test('a drag on the notch, a drag across the band and a scroll of the stack set no cursor', async () => {
		await withDay(async ({ page }) => {
			await page.evaluate(() => (document.querySelector('.cell.dayview').style.flex = '0 0 400px'));
			await page.locator(`${PAST} svg.svg`).waitFor();

			const at = await spot(page, TODAY, 0.5);
			await page.mouse.move(at.x, at.y);
			await page.mouse.down();
			await page.mouse.move(at.x + 40, at.y, { steps: 4 });
			await page.mouse.up();

			const past = await page.locator(`${PAST} svg.svg`).boundingBox();
			await page.mouse.move(past.x + past.width / 2, past.y + past.height - 10);
			await page.mouse.wheel(0, -200);

			// Last: a taller day is less past.
			const grip = await page.locator('.day-grip').boundingBox();
			await page.mouse.move(grip.x + grip.width / 2, grip.y + grip.height / 2);
			await page.mouse.down();
			await page.mouse.move(grip.x + grip.width / 2, grip.y - 20, { steps: 4 });
			await page.mouse.up();
			assert.equal(await pillText(page), null);

			// The same band, clicked: the harness would have seen one.
			const again = await spot(page, TODAY, 0.5);
			await page.mouse.click(again.x, again.y);
			assert.match((await pillText(page)) ?? '', /^-t \d\d:\d\d$/);
		});
	});

	test('390×844: a scroll over the band sets no cursor, a tap does', async () => {
		await withDay(
			async ({ page }) => {
				const at = await spot(page, TODAY, 0.5);
				const cdp = await page.context().newCDPSession(page);
				await cdp.send('Input.synthesizeScrollGesture', {
					x: Math.round(at.x),
					y: Math.round(at.y),
					yDistance: -120,
					gestureSourceType: 'touch'
				});
				assert.equal(await pillText(page), null);

				await page.touchscreen.tap(at.x, at.y);
				const text = await pillText(page);
				assert.match(text ?? '', /^-t \d\d:\d\d$/, `the pill: ${text}`);
				assert.ok(Math.abs(pillInstant(text) - at.at) <= 60_000, `${text} for ${new Date(at.at).toISOString()}`);
			},
			{ device: devices['iPhone 13'], viewport: { width: 390, height: 844 } }
		);
	});
});
