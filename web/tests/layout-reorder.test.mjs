// Dragging a root tab along the top bar reorders the root tabs (da-w19ny).
// vividn: «Let's allow dragging to reorder root level tabs».
//
// A mouse picks a tab up by moving it; a finger by holding it, because a
// sideways finger on the phone's strip is a scroll. Either way the tab in
// front stays in front, a reload keeps the order, and a drag never also
// switches or opens the menu. The finger is a real touch through CDP.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP, PHONE, boxOf } from './support/layout.mjs';
import { swipeNoWait } from './support/press.mjs';

before(startApp);
after(stopApp);

/**
 * This device's open list `names`, with `active` in front — written over
 * the harness's `main` once, on the first load, so a reload reads what the
 * page left there.
 */
const opened = (names, active) => ({
	prepare: (context) =>
		context.addInitScript((json) => {
			if (sessionStorage.getItem('reorder.seeded')) return;
			sessionStorage.setItem('reorder.seeded', '1');
			localStorage.setItem('datum.layout.root.v1', json);
		}, JSON.stringify({ names, active }))
});

const tab = (page, name) => page.locator('.root-bar [role="tab"]', { hasText: new RegExp(`^\\s*${name}\\s*•?\\s*$`) });
const rootTabs = (page) =>
	page.locator('.root-bar [role="tab"]').evaluateAll((all) => all.map((b) => b.textContent.replace('•', '').trim()));
const shown = (page) =>
	page.locator('.root-bar [role="tab"][aria-selected="true"]').evaluate((b) => b.textContent.replace('•', '').trim());
/** Waits for `name` to be the tab in front: a switch lands a frame after the tap or click resolves (da-qwdm0). */
const front = (page, name) => tab(page, name).and(page.locator('[aria-selected="true"]')).waitFor();
const marked = (page) =>
	page.evaluate(() =>
		[...document.querySelectorAll('.root-bar [data-drop]')].map((t) => [t.textContent.replace('•', '').trim(), t.dataset.drop])
	);
const menuOpen = (page) => page.locator('[role="menu"]').count();

/** A mouse press on `from`'s middle, carried in steps to (x, y), and held there. */
async function carry(page, from, x, y) {
	const box = await boxOf(from);
	await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
	await page.mouse.down();
	const [x0, y0] = [box.x + box.width / 2, box.y + box.height / 2];
	for (let i = 1; i <= 8; i++) await page.mouse.move(x0 + ((x - x0) * i) / 8, y0 + ((y - y0) * i) / 8);
}

const THREE = opened(['main', 'tail', 'repl'], 1);

describe('a root tab dragged on a desktop', () => {
	test('the third dropped before the first moves there, the tab in front stays in front, and a reload keeps it', async () => {
		await withApp(
			async (app) => {
				const { page, errors } = app;
				assert.deepEqual(await rootTabs(page), ['main', 'tail', 'repl']);
				assert.equal(await shown(page), 'tail');
				const first = await boxOf(tab(page, 'main'));
				await page.evaluate(() =>
					addEventListener('gotpointercapture', (e) => (window.__captured = e.target.textContent.trim()), true)
				);
				await carry(page, tab(page, 'repl'), first.x + 3, first.y + first.height / 2);
				// The drop position is drawn: the line on `main`'s leading side.
				assert.deepEqual(await marked(page), [['main', 'before']]);
				// And the tab holds the pointer, so a release outside the window still lands.
				assert.equal(await page.evaluate(() => window.__captured), 'repl');
				await page.mouse.up();
				assert.deepEqual(await rootTabs(page), ['repl', 'main', 'tail']);
				assert.equal(await shown(page), 'tail');
				assert.equal(await menuOpen(page), 0, 'the drag opened a menu');
				assert.deepEqual(await marked(page), []);
				await app.reload();
				assert.deepEqual(await rootTabs(page), ['repl', 'main', 'tail']);
				assert.equal(await shown(page), 'tail');
				assert.deepEqual(errors, []);
			},
			{ ...DESKTOP, ...THREE }
		);
	});

	test('a plain click still switches; dragging the tab in front opens no menu; Escape puts a drag back', async () => {
		await withApp(
			async ({ page, errors }) => {
				await tab(page, 'main').click();
				await front(page, 'main');
				assert.equal(await shown(page), 'main');
				assert.deepEqual(await rootTabs(page), ['main', 'tail', 'repl']);

				// The tab in front is its menu's button: dragged, it moves and opens nothing.
				const last = await boxOf(tab(page, 'repl'));
				await carry(page, tab(page, 'main'), last.x + last.width - 3, last.y + last.height / 2);
				assert.deepEqual(await marked(page), [['repl', 'after']]);
				await page.mouse.up();
				assert.deepEqual(await rootTabs(page), ['tail', 'repl', 'main']);
				assert.equal(await shown(page), 'main');
				assert.equal(await menuOpen(page), 0, 'dragging the tab in front opened its menu');

				// Escape: nothing moves, nothing switches, and the line goes.
				const first = await boxOf(tab(page, 'tail'));
				await carry(page, tab(page, 'repl'), first.x + 3, first.y + first.height / 2);
				assert.deepEqual(await marked(page), [['tail', 'before']]);
				await page.keyboard.press('Escape');
				assert.deepEqual(await marked(page), []);
				await page.mouse.up();
				assert.deepEqual(await rootTabs(page), ['tail', 'repl', 'main']);
				assert.equal(await shown(page), 'main');
				assert.deepEqual(errors, []);
			},
			{ ...DESKTOP, ...THREE }
		);
	});
});

describe('a root tab dragged on a phone', () => {
	test('a hold picks the third up and it drops before the first; a reload keeps it', async () => {
		await withApp(
			async (app) => {
				const { page, errors } = app;
				const from = await boxOf(tab(page, 'repl'));
				const first = await boxOf(tab(page, 'main'));
				const [x, y] = [from.x + from.width / 2, from.y + from.height / 2];
				const cdp = await page.context().newCDPSession(page);
				await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x, y }] });
				await page.waitForFunction(() => !!document.querySelector('.root-bar [data-dragging]'));
				for (let i = 1; i <= 8; i++)
					await cdp.send('Input.dispatchTouchEvent', {
						type: 'touchMove',
						touchPoints: [{ x: x + (first.x + 3 - x) * (i / 8), y }]
					});
				assert.deepEqual(await marked(page), [['main', 'before']]);
				await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
				await cdp.detach();
				assert.deepEqual(await rootTabs(page), ['repl', 'main', 'tail']);
				assert.equal(await shown(page), 'tail');
				assert.equal(await menuOpen(page), 0, 'the drag opened a menu');
				await app.reload();
				assert.deepEqual(await rootTabs(page), ['repl', 'main', 'tail']);
				assert.equal(await shown(page), 'tail');
				assert.deepEqual(errors, []);
			},
			{ ...PHONE, ...THREE }
		);
	});

	test('a sideways swipe on the strip scrolls it and reorders nothing; a tap still switches', async () => {
		const names = ['main', 'tail', 'repl', 'nowview', 'custom', 'command', 'stats', 'fields', 'aliases', 'help'];
		await withApp(
			async ({ page, errors }) => {
				const strip = page.locator('.root-bar .tabs');
				const scroll = () => strip.evaluate((el) => el.scrollLeft);
				assert.ok(await strip.evaluate((el) => el.scrollWidth > el.clientWidth), 'the strip does not overflow');
				assert.equal(await scroll(), 0);
				const box = await boxOf(strip);
				// Waited out to its `scrollend`: the swipe leaves the strip flinging, and a
				// tap on a strip still flinging only stops it — no click (da-9i1wu).
				await strip.evaluate((el) => el.addEventListener('scrollend', () => (el.swiped = true), { once: true }));
				// With no hold to beat (da-o504v): a first move the runner sent after 450ms
				// found the tab picked up, and the strip did not scroll. A press
				// on a root tab arms that one timer, reorder's `HOLD_MS`, and no other.
				await swipeNoWait(page, box.x + box.width - 20, box.y + box.height / 2, -150, 0);
				await page.waitForFunction(() => document.querySelector('.root-bar .tabs').swiped);
				assert.ok((await scroll()) > 0, 'the swipe did not scroll the strip');
				assert.deepEqual(await rootTabs(page), names);
				assert.equal(await shown(page), 'main');
				await tab(page, 'tail').tap();
				await front(page, 'tail');
				assert.equal(await shown(page), 'tail');
				assert.deepEqual(await rootTabs(page), names);
				assert.deepEqual(errors, []);
			},
			{ ...PHONE, ...opened(names, 0) }
		);
	});
});
