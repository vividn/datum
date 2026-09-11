// Hovering a drawn view (da-s5da): every mark in the dayview and nowview
// says what it is — the `<title>` the CLI writes — and the page adds the
// time: a dot's own, or the one under the pointer on a block. Inline, the
// `<title>` is gone and its text is the mark's `data-title` (da-8jb8).
//
// Delta-blind on purpose: every label is held to the text of the mark it
// names and to the time the geometry puts under the pointer, never to "a
// tooltip appeared".

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { devices } from 'playwright';
import { startApp, stopApp, withApp, workerClock } from './support/app.mjs';

before(startApp);
after(stopApp);

// 15:00 UTC: a coffee at 14:05 and a 30-minute block 14:00–14:30 are that
// afternoon, in today's band of the day strip.
const NOW = Date.UTC(2026, 8, 10, 15, 0, 0);
const BAND = '.band[data-days-ago="0"]';
const DOT = `${BAND} circle[data-field="coffee"]`;
const BLOCK = `${BAND} rect[data-field="work"]`;
const TIP = `${BAND} .tip`;
const LABELS = { coffee: '14:05 coffee', work: '14:00 work: start 14:00–14:30' };

async function withDay(fn, options = {}) {
	const clock = workerClock(NOW);
	await withApp(
		async (app) => {
			for (const argv of [
				['occur', 'coffee', '-t', '1405'],
				['start', 'work', '30m', '-t', '1430']
			]) {
				const logged = await app.run(argv);
				assert.equal(logged.code, 0, logged.stderr);
			}
			await app.reload();
			await app.page.locator(BLOCK).waitFor();
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

/** The tooltip's `HH:MM` as minutes of the day, and the rest of its text. */
async function tip(page) {
	const [, h, m, rest] = /^(\d\d):(\d\d) (.*)$/.exec(await page.locator(TIP).innerText());
	return { minutes: Number(h) * 60 + Number(m), rest };
}

describe('hovering the day strip', () => {
	test('a dot shows its own time and what it is', async () => {
		await withDay(async ({ page }) => {
			await page.locator(DOT).hover();
			assert.equal(await page.locator(TIP).innerText(), LABELS.coffee);
			assert.equal(
				await page.locator(DOT).getAttribute('aria-describedby'),
				await page.locator(TIP).getAttribute('id')
			);
		});
	});

	test('a block shows the time under the pointer, then goes away off it', async () => {
		await withDay(async ({ page }) => {
			const block = page.locator(BLOCK);
			const { width, height } = await block.boundingBox();
			// A fifth of the way in and halfway: 14:06 and 14:15, each
			// within the minute the pointer's pixel spans.
			for (const [f, want] of [
				[0.2, 14 * 60 + 6],
				[0.5, 14 * 60 + 15]
			]) {
				await block.hover({ position: { x: width * f, y: height / 2 } });
				const shown = await tip(page);
				assert.equal(shown.rest, 'work: start 14:00–14:30');
				assert.ok(Math.abs(shown.minutes - want) <= 1, `${f}: ${JSON.stringify(shown)}`);
			}

			// Empty space in the same band — 03:00, no mark — shows nothing.
			const band = await page.locator(`${BAND} svg.svg`).boundingBox();
			const { y } = await block.boundingBox();
			await page.mouse.move(band.x + band.width * 0.15, y + height / 2);
			assert.ok(await page.locator(TIP).isHidden(), 'a label over empty space');

			await block.hover();
			await page.mouse.move(1, 1);
			assert.ok(await page.locator(TIP).isHidden(), 'the label stayed after the pointer left');
		});
	});

	test('the keyboard reaches each mark, and focus shows its label', async () => {
		await withDay(async ({ page }) => {
			const marks = page.locator(`${BAND} [data-state]`);
			assert.equal(await marks.count(), 2);
			await marks.first().focus();
			for (const i of [0, 1]) {
				if (i) await page.keyboard.press('Tab');
				const field = await page.evaluate(() => document.activeElement.getAttribute('data-field'));
				assert.equal(field, await marks.nth(i).getAttribute('data-field'), 'Tab did not reach the next mark');
				assert.equal(await page.locator(TIP).innerText(), LABELS[field]);
			}
		});
	});

	test('a tap shows the label and a tap elsewhere hides it', async () => {
		await withDay(
			async ({ page }) => {
				await page.locator(DOT).tap();
				assert.equal(await page.locator(TIP).innerText(), LABELS.coffee);
				await page.locator('section.pane h2').first().tap();
				assert.ok(await page.locator(TIP).isHidden());
			},
			{ device: devices['iPhone 13'] }
		);
	});

	test('at 390px the label stays inside the viewport', async () => {
		await withDay(
			async ({ page }) => {
				const block = page.locator(BLOCK);
				const { width, height } = await block.boundingBox();
				await block.hover({ position: { x: width - 1, y: height / 2 } });
				const box = await page.locator(TIP).boundingBox();
				assert.ok(box.x >= 0 && box.x + box.width <= 390, JSON.stringify(box));
				assert.match(await page.locator(TIP).innerText(), /work: start 14:00–14:30$/);
			},
			{ viewport: { width: 390, height: 844 } }
		);
	});

	// A `<title>` inline is the browser's own tooltip, drawn over the page's
	// (da-8jb8). Playwright cannot see a native tooltip, so the assertion is
	// that no picture has a `<title>` left — and that every mark keeps its
	// text as `data-title` and its accessible name as an equal `aria-label`.
	test('no picture keeps a <title>; every mark has data-title and an aria-label', async () => {
		await withDay(async (app) => {
			// Inside nowview's window, so its picture has a mark too.
			const logged = await app.run(['occur', 'coffee', '-t', '1455']);
			assert.equal(logged.code, 0, logged.stderr);
			await app.reload();
			await app.page.locator('section.pane[data-title="nowview"] [data-state]').first().waitFor();
			const pictures = await app.page.locator('.picture').evaluateAll((ps) =>
				ps.map((p) => ({
					titles: p.querySelectorAll('svg title').length,
					marks: [...p.querySelectorAll('[data-state]')].map((m) => [
						m.getAttribute('data-title'),
						m.getAttribute('aria-label')
					])
				}))
			);
			assert.ok(pictures.length >= 2, `${pictures.length} pictures`);
			for (const { titles, marks } of pictures) {
				assert.equal(titles, 0);
				for (const [title, label] of marks) {
					assert.ok(title, 'a mark with no data-title');
					assert.equal(label, title);
				}
			}
			const names = pictures.flatMap((p) => p.marks.map(([title]) => title));
			assert.ok(names.includes('work: start 14:00–14:30'), JSON.stringify(names));
		});
	});

	// The past is one picture of many days (da-hnyb): a mark two days back,
	// in the middle of it, answers as today's do.
	test('a mark in the past picture shows its time and what it is', async () => {
		await withDay(async (app) => {
			const logged = await app.run(['occur', 'coffee', '-d', '-2', '-t', '0930']);
			assert.equal(logged.code, 0, logged.stderr);
			await app.reload();
			await app.page.evaluate(() => (document.querySelector('.cell.dayview').style.flex = '0 0 400px'));
			const dot = app.page.locator('.band[data-days-ago="1"] circle[data-field="coffee"]');
			await dot.hover();
			assert.equal(await app.page.locator('.band[data-days-ago="1"] .tip').innerText(), '09:30 coffee');
		});
	});
});

describe('the SVG alone', () => {
	// The `.html` datum writes is this picture in a bare page (see
	// `wrap_dayview_svg_in_html`): no app, no script. A mark's `<title>`
	// is still its accessible name, which is what a browser shows on hover.
	test("a mark's title is its accessible name with no script", async () => {
		await withDay(async ({ page, run }) => {
			const drawn = await run(['dayview']);
			await page.setContent(`<!DOCTYPE html><html><body>${drawn.stdout}</body></html>`);
			for (const [selector, name] of [
				['circle[data-field="coffee"]', 'coffee'],
				['rect[data-field="work"]', 'work: start 14:00–14:30']
			]) {
				const client = await page.context().newCDPSession(page);
				const { nodeId } = await client.send('DOM.querySelector', {
					nodeId: (await client.send('DOM.getDocument')).root.nodeId,
					selector
				});
				const { nodes } = await client.send('Accessibility.getPartialAXTree', {
					nodeId,
					fetchRelatives: false
				});
				assert.equal(nodes[0].name?.value, name);
			}
		});
	});
});
