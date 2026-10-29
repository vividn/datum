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
import { startApp, stopApp, withApp, sharedApp } from './support/app.mjs';

before(startApp);
after(stopApp);

// 15:00 UTC: a coffee at 14:05 and a 30-minute block 14:00–14:30 are that
// afternoon, in today's band of the day strip.
const NOW = Date.UTC(2026, 8, 10, 15, 0, 0);
const BAND = '.band[data-days-ago="0"]';
const DOT = `${BAND} circle[data-field="coffee"]`;
const BLOCK = `${BAND} rect[data-field="work"]`;
const TIP = `${BAND} .tip`;
const LABELS = { coffee: '14:05 coffee', work: '14:00 work: on [30m]' };

// ONE app for the tests on the default screen (sharedApp in support/app.mjs),
// each in a room of its own so no test meets the widgets the last one moved.
// A test that says how its app is to be opened opens its own.
// 900px tall: at Playwright's 720 the bar's open output (da-mwp4m) leaves
// the strip at its shortest day, where today gives way from the top and
// the band over it takes the press.
const shared = sharedApp({ viewport: { width: 1280, height: 900 }, clock: NOW });

async function withDay(fn, options) {
	const body = async (app) => {
		for (const argv of [
			['occur', 'coffee', '-t', '1405'],
			['start', 'work', '30m', '-t', '1430']
		]) {
			const logged = await app.run(argv);
			assert.equal(logged.code, 0, logged.stderr);
		}
		await app.redraw();
		await app.page.locator(BLOCK).waitFor();
		await fn(app);
	};
	if (!options) return shared.fresh(body);
	await withApp(body, { ...options, clock: NOW });
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

	// da-qietq: the disc the pointer lands on is never painted — the dot
	// is its symbol alone, drawn the disc's full size, so no ring shows.
	test('a dot is its coloured symbol alone, with no ring around it', async () => {
		await withDay(async ({ page }) => {
			const { opacity, disc, drawn } = await page.locator(DOT).evaluate((dot) => ({
				opacity: getComputedStyle(dot).fillOpacity,
				disc: dot.getAttribute('r'),
				drawn: dot.nextElementSibling?.matches('circle.point-fg') && dot.nextElementSibling.getAttribute('r')
			}));
			assert.equal(opacity, '0', 'the disc behind the dot is painted');
			assert.equal(drawn, disc, 'the dot is smaller than its disc');
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
				assert.equal(shown.rest, 'work: on [30m]');
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

	// A tap selects the entry, whose label is then its name alone (da-k16pi).
	test('a tap shows the label and a tap elsewhere hides it', async () => {
		await withDay(
			async ({ page }) => {
				await page.locator(DOT).tap();
				assert.equal(await page.locator(TIP).innerText(), 'coffee');
				// Anywhere but the strip: a pane's own body. Not its title —
				// that is the button that opens the pane's menu now (da-9vtw).
				await page.locator('section.pane[data-title="cli"] .body').tap();
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
				assert.match(await page.locator(TIP).innerText(), /work: on \[30m\]$/);
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
			await app.redraw();
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
			assert.ok(names.includes('work: on [30m]'), JSON.stringify(names));
		});
	});

	// The past is a picture a day (da-iu9i): a mark two days back, in its
	// own band, answers as today's do.
	test('a mark in the past picture shows its time and what it is', async () => {
		await withDay(async (app) => {
			const logged = await app.run(['occur', 'coffee', '-d', '-2', '-t', '0930']);
			assert.equal(logged.code, 0, logged.stderr);
			await app.redraw();
			// Scrolled to, as a reader would: a day held from before the write
			// is checked once it is on screen (`refreshVisible`), and only an
			// ask still in flight as the coffee landed drew it there before.
			await app.page.locator('.band[data-days-ago="2"]').scrollIntoViewIfNeeded();
			const dot = app.page.locator('.band[data-days-ago="2"] circle[data-field="coffee"]');
			await dot.hover();
			assert.equal(await app.page.locator('.band[data-days-ago="2"] .tip').innerText(), '09:30 coffee');
		});
	});
});

describe('the SVG alone', () => {
	// The `.html` datum writes is this picture in a bare page (see
	// `wrap_dayview_svg_in_html`): no app, no script. A mark's `<title>`
	// is still its accessible name, which is what a browser shows on hover.
	test("a mark's title is its accessible name with no script", async () => {
		// An app of its own (`{}`): `setContent` replaces the page it would share.
		await withDay(async ({ page, run }) => {
			const drawn = await run(['dayview']);
			await page.setContent(`<!DOCTYPE html><html><body>${drawn.stdout}</body></html>`);
			for (const [selector, name] of [
				['circle[data-field="coffee"]', 'coffee'],
				['rect[data-field="work"]', 'work: on [30m]']
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
		}, {});
	});
});
