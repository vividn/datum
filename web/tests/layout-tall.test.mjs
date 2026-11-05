// A layout taller than the screen (da-py6pl). vividn, 2026-11-03: «allow a
// toggle that lets widgets expand vertically beyond a single screen (that
// you can scroll down). … when you resize widgets in the vertical direction
// it just pushes other widgets down. You can also expand the bottom-most
// widget down by dragging down a bit and holding there.»
//
// The harness's `main` (support/desk.mjs): the column `main` of strip /
// middle (cli | the column `aside` of nowview / tail / custom) / bar.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp } from './support/app.mjs';
import { DESKTOP, enterLayoutMode, leaveLayoutMode, rootMenuButton, settled, switchItem } from './support/layout.mjs';

before(startApp);
after(stopApp);

const shared = sharedApp(DESKTOP);
const TALL = 'Taller than the screen';

/** The root tab's switch, pressed in layout mode; the menu closed again. */
async function toggleTall(page, on) {
	await enterLayoutMode(page);
	await rootMenuButton(page).click();
	await switchItem(page, TALL, !on).click();
	await switchItem(page, TALL, on).waitFor();
	await page.keyboard.press('Escape');
	await page.locator('[role="menu"]').waitFor({ state: 'detached' });
}

/** Each cell of a split, top and height, measured in the layout's own scroll (so a scroll moves nothing). */
const cellsOf = (page, id) =>
	page.evaluate((id) => {
		const scroller = document.querySelector('.root > .content');
		const top = scroller.getBoundingClientRect().top - scroller.scrollTop;
		return [...document.querySelectorAll(`[data-split="${id}"] > .cell`)].map((c) => {
			const r = c.getBoundingClientRect();
			return { top: r.top - top, height: r.height };
		});
	}, id);

/** Whether the page scrolls: the root's scroller holds more than it shows. */
const scrolls = (page) =>
	page.evaluate(() => {
		const s = document.querySelector('.root > .content');
		return s.scrollHeight > s.clientHeight + 1;
	});

/** Drag a split's `index`th border down `dy` pixels, in steps. */
async function dragDown(page, split, index, dy) {
	const box = await page.locator(`[data-split="${split}"] > [role="separator"]`).nth(index).boundingBox();
	const x = box.x + box.width / 2;
	const y = box.y + box.height / 2;
	await page.mouse.move(x, y);
	await page.mouse.down();
	for (let i = 1; i <= 5; i++) await page.mouse.move(x, y + (i * dy) / 5);
	await page.mouse.up();
	await settled(page);
}

const near = (a, b, what) => assert.ok(Math.abs(a - b) <= 2, `${what}: ${a} vs ${b}`);

describe('a layout taller than the screen', () => {
	test('a border dragged down 200px grows the pane above by 200px and pushes the ones below down; the page scrolls', async () => {
		await shared.solo(async ({ page }) => {
			await toggleTall(page, true);
			assert.equal(await scrolls(page), false, 'on, the layout still fits until it is grown');
			const before = await cellsOf(page, 'main');
			await dragDown(page, 'main', 0, 200);
			const after = await cellsOf(page, 'main');
			near(after[0].height, before[0].height + 200, 'the strip');
			near(after[1].top, before[1].top + 200, 'the middle moved down');
			near(after[1].height, before[1].height, 'the middle kept its height');
			near(after[2].top, before[2].top + 200, 'the bar moved down');
			near(after[2].height, before[2].height, 'the bar kept its height');
			assert.equal(await scrolls(page), true);
		});
	});

	test('a nested border pushes everything below it down, the cells holding it growing with it', async () => {
		await shared.solo(async ({ page }) => {
			await toggleTall(page, true);
			const [main, aside] = [await cellsOf(page, 'main'), await cellsOf(page, 'aside')];
			await dragDown(page, 'aside', 0, 100);
			const [main2, aside2] = [await cellsOf(page, 'main'), await cellsOf(page, 'aside')];
			near(aside2[0].height, aside[0].height + 100, 'nowview');
			near(aside2[1].top, aside[1].top + 100, 'the tail moved down');
			near(aside2[1].height, aside[1].height, 'the tail kept its height');
			near(main2[0].height, main[0].height, 'the strip above kept its height');
			near(main2[1].height, main[1].height + 100, 'the middle grew with it');
			near(main2[2].top, main[2].top + 100, 'the bar moved down');
		});
	});

	test("held at the screen's bottom, the foot keeps growing the last pane, and the page scrolls with it", async () => {
		await shared.solo(async ({ page }) => {
			await toggleTall(page, true);
			const foot = page.locator('.border.foot');
			const box = await foot.boundingBox();
			const bottom = await page.evaluate(() => document.querySelector('.root > .content').getBoundingClientRect().bottom);
			const x = box.x + box.width / 2;
			const start = (await cellsOf(page, 'main'))[2].height;
			await page.mouse.move(x, box.y + box.height / 2);
			await page.mouse.down();
			await page.mouse.move(x, bottom - 4, { steps: 3 });
			await page.waitForTimeout(400);
			const first = (await cellsOf(page, 'main'))[2].height;
			await page.waitForTimeout(400);
			const second = (await cellsOf(page, 'main'))[2].height;
			await page.mouse.up();
			await settled(page);
			assert.ok(first > start + 50, `it grew while held: ${start} → ${first}`);
			assert.ok(second > first + 50, `and went on growing: ${first} → ${second}`);
			const s = await page.evaluate(() => {
				const s = document.querySelector('.root > .content');
				return { top: s.scrollTop, max: s.scrollHeight - s.clientHeight };
			});
			assert.ok(s.top > 50 && s.max - s.top <= 50, `the page scrolled with it: ${JSON.stringify(s)}`);
		});
	});

	test('the arrows push the same way: on a border, and on the foot, which a keyboard reaches', async () => {
		await shared.solo(async ({ page }) => {
			await toggleTall(page, true);
			const before = await cellsOf(page, 'main');
			await page.locator('[data-split="main"] > [role="separator"]').first().focus();
			await page.keyboard.press('ArrowDown');
			await settled(page);
			const after = await cellsOf(page, 'main');
			const step = after[0].height - before[0].height;
			assert.ok(step > 5, `the strip grew by ${step}px`);
			near(after[1].height, before[1].height, 'the middle kept its height');
			near(after[2].top, before[2].top + step, 'the bar moved down as far');
			await page.locator('.border.foot').focus();
			await page.keyboard.press('ArrowDown');
			await settled(page);
			const last = await cellsOf(page, 'main');
			near(last[2].height, after[2].height + step, 'the foot grew the bar as far');
			near(last[0].height, after[0].height, 'the strip kept its height');
		});
	});

	test('off, the same drag takes the height from the pane below, as ever', async () => {
		await shared.solo(async ({ page }) => {
			await enterLayoutMode(page);
			assert.equal(await page.locator('.border.foot').count(), 0, 'no foot');
			const before = await cellsOf(page, 'main');
			await dragDown(page, 'main', 0, 200);
			const after = await cellsOf(page, 'main');
			near(after[0].height, before[0].height + 200, 'the strip');
			near(after[1].height, before[1].height - 200, 'the middle gave it up');
			near(after[2].top, before[2].top, 'the bar stayed');
			assert.equal(await scrolls(page), false);
		});
	});

	test("saved, it is the layout's root pane's `tall`, which a kit's [[layout]] sets, and opened it is the tab's again", async () => {
		await shared.solo(async ({ page }) => {
			await toggleTall(page, true);
			await dragDown(page, 'main', 0, 200);
			const { saved, back } = await page.evaluate(() => {
				const { extract, inline, layoutTabs } = window.datum.shell;
				const saved = extract(layoutTabs(window.datum.layout.current()).find((t) => t.layout === 'main'));
				return { saved: saved.root.tall, back: inline(saved, new Set()).tall };
			});
			assert.ok(saved > 1, `saved ${saved} screens`);
			assert.equal(back, saved);
		});
	});

	test('it survives a reload, and turned off fits back into one screen, each pane keeping its share', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await toggleTall(page, true);
			await dragDown(page, 'main', 0, 200);
			await leaveLayoutMode(page);
			await app.reload();
			assert.equal(await scrolls(page), true, 'still taller than the screen after a reload');
			const tall = await cellsOf(page, 'main');
			const share = (cells) => cells.map((c) => c.height / cells.reduce((a, b) => a + b.height, 0));
			await toggleTall(page, false);
			await settled(page);
			assert.equal(await scrolls(page), false, 'off, it fits one screen');
			const fit = share(await cellsOf(page, 'main'));
			share(tall).forEach((s, i) => assert.ok(Math.abs(s - fit[i]) < 0.01, `cell ${i}: ${s} vs ${fit[i]}`));
		});
	});
});
