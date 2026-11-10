// `rested` (support/layout.mjs, da-otqfq): the web tests' one wait for a page
// at rest. Each condition on a page that holds it false for a while and then
// true, and the helper waits; and the #1891 case, an element attached with its
// intro not yet started, which a read of its animations alone passed early.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { chromium } from 'playwright';
import { PAST_THE_WINDOW_MS, rested } from './support/layout.mjs';

let browser;
before(async () => (browser = await chromium.launch()));
after(() => browser?.close());

async function onPage(body) {
	const page = await browser.newPage();
	try {
		await page.setContent('<!DOCTYPE html><html><body style="margin:0"><div id="box" style="width:40px;height:40px"></div></body></html>');
		await body(page, page.locator('#box'));
	} finally {
		await page.close();
	}
}

const top = (box) => box.evaluate((el) => el.getBoundingClientRect().top);

test('rested waits out a running animation', () =>
	onPage(async (page, box) => {
		await box.evaluate((el) => el.animate([{ opacity: 0 }, { opacity: 1 }], 600));
		const t0 = Date.now();
		await rested(page, box);
		assert.ok(Date.now() - t0 >= 500, `resolved ${Date.now() - t0} ms into a 600 ms animation`);
		assert.equal(await page.evaluate(() => document.getAnimations().length), 0);
	}));

test('rested waits for a box a frame callback moves, with no animation to read', () =>
	onPage(async (page, box) => {
		await box.evaluate((el) => {
			const t0 = performance.now();
			const step = () => {
				const k = Math.min(1, (performance.now() - t0) / 600);
				el.style.transform = `translateY(${200 * k}px)`;
				if (k < 1) requestAnimationFrame(step);
			};
			requestAnimationFrame(step);
		});
		await rested(page, box);
		assert.equal(await top(box), 200);
	}));

// #1891: the list was attached below the screen and its slide began frames
// later, so getAnimations() read nothing animating while it stood below.
// Three frames late here, past a round trip from node.
test('rested does not resolve on an element whose intro is not yet created', () =>
	onPage(async (page, box) => {
		await box.evaluate((el) => {
			el.style.transform = 'translateY(300px)';
			const late = (n, then) => (n ? requestAnimationFrame(() => late(n - 1, then)) : then());
			late(3, () => {
				el.style.transform = '';
				el.animate([{ transform: 'translateY(300px)' }, { transform: 'none' }], 400);
			});
		});
		await rested(page, box);
		assert.equal(await top(box), 0);
		assert.equal(await box.evaluate((el) => el.getAnimations().length), 0);
	}));

// #1889: a widget placed its handles in a ResizeObserver, which a frame
// delivers after the layout a read forces; this one places on the frame after,
// as a widget deferring its draw does. Nothing animates and, with no locator,
// no box is watched: the frames alone wait for it.
test('rested reads after a ResizeObserver has placed what it places', () =>
	onPage(async (page, box) => {
		await box.evaluate((el) => {
			new ResizeObserver(([e]) => requestAnimationFrame(() => (document.body.dataset.placed = e.contentRect.width))).observe(el);
		});
		await rested(page);
		await box.evaluate((el) => (el.style.width = '80px'));
		await rested(page);
		assert.equal(await page.evaluate(() => document.body.dataset.placed), '80');
	}));

test('rested with `tapped` waits out the window after a tab tap', () =>
	onPage(async (page, box) => {
		const t0 = Date.now();
		await rested(page, box, { tapped: true });
		assert.ok(Date.now() - t0 >= PAST_THE_WINDOW_MS, `resolved ${Date.now() - t0} ms after the tap`);
	}));

test('rested with `app` waits for the CLI to go idle', () =>
	onPage(async (page, box) => {
		await page.evaluate(() => {
			window.__datumInflight = 1;
			setTimeout(() => (window.__datumInflight = 0), 600);
		});
		const app = { idle: () => page.waitForFunction(() => window.__datumInflight === 0) };
		await rested(page, box, { app });
		assert.equal(await page.evaluate(() => window.__datumInflight), 0);
	}));

test('rested names the condition that never held', () =>
	onPage(async (page, box) => {
		await box.evaluate((el) => el.animate([{ opacity: 0 }, { opacity: 1 }], { duration: 300, iterations: Infinity }));
		const err = await rested(page, box, { timeout: 1000 }).catch((e) => e);
		assert.match(String(err), /not at rest after 1000 ms: animating/);
		await page.evaluate(() => document.getAnimations().forEach((a) => a.cancel()));
		const app = { idle: () => Promise.reject(new Error('never idle')) };
		await assert.rejects(rested(page, box, { app }), /the CLI never went idle/);
	}));
