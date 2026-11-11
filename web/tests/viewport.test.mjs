// The shell fits the VISIBLE viewport, whatever told it the size (da-31ts).
//
// vividn, 2026-09-17: an installed PWA cut its bottom bar off on first load,
// «it goes away when rotating and coming back». Nothing here can launch an
// installed PWA or raise a real keyboard, so this file does not claim the
// bug is fixed — it holds the MECHANISM: the shell is not sized in viewport
// units, and it follows the visual viewport on each event a phone sends
// when that changes (its own `resize`, `pageshow`, `orientationchange`),
// including a change that sends the window no `resize` at all, which is
// what an on-screen keyboard is on iOS. The phone is the real check.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { chromium, devices, webkit } from 'playwright';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const PHONE = { width: 390, height: 640 };

/**
 * The phone the bug was seen on is a Pixel 8 Pro running Brave, installed
 * (Android Chromium): chromium is asked as a mobile device, webkit, the
 * second engine, as an iPhone (da-atwj6's bounce was seen on one), both at
 * a phone's size.
 */
const onPhone = (engine) => ({
	engine,
	viewport: PHONE,
	device: engine === chromium ? { isMobile: true, hasTouch: true } : devices['iPhone 13']
});

/** The bottoms of the shell and of the bottom bar, and the visible height. */
const edges = (page) =>
	page.evaluate(() => ({
		shell: Math.round(document.querySelector('.shell').getBoundingClientRect().bottom),
		bar: Math.round(document.querySelector('.prompt-bar').getBoundingClientRect().bottom),
		visible: Math.round(visualViewport.height)
	}));

/** Wait until both bottoms sit on the visible viewport's bottom, `height`. */
async function fitsTo(page, height) {
	await until(
		() => edges(page),
		(e) => e.visible === height && e.shell === height && e.bar === height
	);
}

/**
 * Make the visual viewport `height` tall without resizing the window — a
 * keyboard, or a launch that settled late — and send `event` on `target`.
 */
const shrinkVisible = (page, height, target, event) =>
	page.evaluate(
		([height, target, event]) => {
			Object.defineProperty(visualViewport, 'height', { configurable: true, get: () => height });
			(target === 'visualViewport' ? visualViewport : window).dispatchEvent(new Event(event));
		},
		[height, target, event]
	);

/**
 * Report the visual viewport as `height` tall and `offsetTop` down the
 * layout viewport, and send its `scroll`, as iOS does under a rubber-band
 * (the height unchanged, the top moved) or with a keyboard up (both).
 */
const moveVisible = (page, height, offsetTop) =>
	page.evaluate(
		([height, offsetTop]) => {
			Object.defineProperty(visualViewport, 'height', { configurable: true, get: () => height });
			Object.defineProperty(visualViewport, 'offsetTop', { configurable: true, get: () => offsetTop });
			visualViewport.dispatchEvent(new Event('scroll'));
		},
		[height, offsetTop]
	);

/** The tops of the shell and of the bottom bar's foot, after `moveVisible` has been heard. */
const placed = (page) =>
	page.evaluate(
		() =>
			new Promise((done) =>
				requestAnimationFrame(() =>
					done({
						top: Math.round(document.querySelector('.shell').getBoundingClientRect().top),
						bar: Math.round(document.querySelector('.prompt-bar').getBoundingClientRect().bottom)
					})
				)
			)
	);

// CI launches chromium only; a webkit leg that cannot launch is skipped with
// the reason on record (see dbio.test.mjs).
async function launchable(engine) {
	try {
		await (await engine.launch()).close();
		return true;
	} catch {
		return false;
	}
}
const available = new Map(
	await Promise.all([chromium, webkit].map(async (engine) => [engine, await launchable(engine)]))
);
assert.ok([...available.values()].some(Boolean), 'no browser can be launched here — nothing examined');

for (const engine of [chromium, webkit]) {
	const skip = !available.get(engine) && `${engine.name()} cannot be launched here`;
	describe(engine.name(), { skip }, () => {
		test('the shell is sized by no viewport unit', async () => {
			await withApp(
				async ({ page }) => {
					const units = await page.evaluate(() =>
						[...document.styleSheets]
							.flatMap((sheet) => [...sheet.cssRules])
							.filter((rule) => /\.(shell|pages|prompt-bar|log)\b/.test(rule.selectorText ?? ''))
							.map((rule) => rule.cssText)
							.filter((text) => /\d[dsl]?v(h|b|max|min)\b/.test(text))
					);
					assert.deepEqual(units, []);
				},
				onPhone(engine)
			);
		});

		test('the shell follows the window shrinking and growing (a URL bar)', async () => {
			await withApp(
				async ({ page }) => {
					await fitsTo(page, 640);
					await page.setViewportSize({ width: 390, height: 560 });
					await fitsTo(page, 560);
					await page.setViewportSize({ width: 390, height: 700 });
					await fitsTo(page, 700);
				},
				onPhone(engine)
			);
		});

		// The measurement that corrects a LAUNCH, which sends no event at all
		// (da-ckw3 owns this one; da-31ts built the cure and left it
		// untested). vividn: a rotation cured it, «so maybe just one size
		// refresh is necessary after the page loads» — that refresh is the
		// `requestAnimationFrame` in `followViewport`. Here the first read of
		// `visualViewport.height` is too TALL, as an installed PWA's is
		// before its system bars settle, and every read after it is the
		// truth. Nothing is dispatched: if that one frame's re-measure were
		// deleted, nothing else in this suite would go red and only an
		// installed phone would ever notice.
		test('a launch that measured too tall is corrected a frame later, with no event', async () => {
			const TALL = 640;
			const TRUE = 400;
			await withApp(
				async ({ page }) => {
					await until(
						() => edges(page),
						(e) => e.shell === TRUE && e.bar === TRUE
					);
					// And it really was the wrong number that was read first:
					// a test whose fake never lied would pass for nothing.
					const reads = await page.evaluate(() => window.__viewportReads);
					assert.equal(reads[0], TALL, `the first read was ${reads[0]}, not the too-tall one`);
					assert.ok(reads.length > 1, 'nothing read the height a second time');
				},
				{
					...onPhone(engine),
					prepare: (context) =>
						context.addInitScript(
							([tall, truth]) => {
								window.__viewportReads = [];
								Object.defineProperty(visualViewport, 'height', {
									configurable: true,
									get() {
										const answer = window.__viewportReads.length === 0 ? tall : truth;
										window.__viewportReads.push(answer);
										return answer;
									}
								});
							},
							[TALL, TRUE]
						)
				}
			);
		});

		// da-atwj6, vividn, 2026-11-11: «the whole app moving down or up when
		// you try to scroll, often in the other direction». No engine here
		// rubber-bands (mobile WebKit takes no wheel and no drag), so the
		// bounce is played as iOS reports it.
		// Only the document: a pane's scroll still chains to a tall layout's
		// own scroller (layout-swipe.test.mjs).
		test('the document never bounces', async () => {
			await withApp(
				async ({ page }) => {
					const behavior = await page.evaluate(() => ({
						html: getComputedStyle(document.documentElement).overscrollBehaviorY,
						body: getComputedStyle(document.body).overscrollBehaviorY
					}));
					assert.deepEqual(behavior, { html: 'none', body: 'none' });
				},
				onPhone(engine)
			);
		});

		test('a bounce moves the visual viewport and not the shell', async () => {
			await withApp(
				async ({ page }) => {
					await fitsTo(page, 640);
					for (const offsetTop of [-60, -20, 20, 60, 0]) {
						await moveVisible(page, 640, offsetTop);
						assert.deepEqual(await placed(page), { top: 0, bar: 640 }, `offsetTop ${offsetTop}`);
					}
				},
				onPhone(engine)
			);
		});

		test('with a keyboard up the bar sits above it, and a bounce there moves it no further', async () => {
			await withApp(
				async ({ page }) => {
					await fitsTo(page, 640);
					// A keyboard 240px tall, the visible part scrolled 120px down.
					await moveVisible(page, 400, 120);
					assert.deepEqual(await placed(page), { top: 120, bar: 520 });
					// Bounced past either end of the room it leaves: held at the end.
					await moveVisible(page, 400, -30);
					assert.deepEqual(await placed(page), { top: 0, bar: 400 });
					await moveVisible(page, 400, 300);
					assert.deepEqual(await placed(page), { top: 240, bar: 640 });
				},
				onPhone(engine)
			);
		});

		for (const [target, event] of [
			['visualViewport', 'resize'],
			['window', 'pageshow'],
			['window', 'orientationchange']
		])
			test(`a visible viewport that shrank without a window resize is re-measured on ${event}`, async () => {
				await withApp(
					async ({ page }) => {
						await fitsTo(page, 640);
						await shrinkVisible(page, 400, target, event);
						await fitsTo(page, 400);
					},
					onPhone(engine)
				);
			});
	});
}
