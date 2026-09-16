// The borders (da-jdop, da-jgrn, da-2a5y, da-zh2o): dragged, nudged,
// throttled, and down to a pane's frame and no further — driven through
// the built app in a real browser, the redraws counted through the seam.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import {
	CHROME,
	DESKTOP,
	along,
	boxOf,
	canvasWidth,
	dragFor,
	drawnRuns,
	draft,
	grab,
	pane,
	settled,
	widgetPane
} from './support/layout.mjs';

before(startApp);
after(stopApp);

describe('a border', () => {
	test('dragged 100px moves the ratio and redraws the picture at the new width', async () => {
		await withApp(async ({ page }) => {
			const border = page.locator('.split.row > [role="separator"]');
			const repl = pane(page, 'repl');
			const before = await repl.boundingBox();
			const drawn = await canvasWidth(page, 'nowview');
			const box = await border.boundingBox();
			const x = box.x + box.width / 2;
			const y = box.y + box.height / 2;
			await page.mouse.move(x, y);
			await page.mouse.down();
			for (let i = 1; i <= 5; i++) await page.mouse.move(x + i * 20, y);
			await page.mouse.up();
			await page.waitForFunction(
				([id, was]) =>
					Math.abs(document.querySelector(`section[data-pane="${id}"]`).getBoundingClientRect().width - was - 100) <= 2,
				['repl', before.width]
			);
			// The picture is re-asked at the narrower box, not squeezed into it.
			await page.waitForFunction(
				(was) => document.querySelector('section[data-pane="nowview"] svg.svg')?.width.baseVal.value < was,
				drawn
			);
			const img = pane(page, 'nowview').locator('svg.svg');
			const shown = await img.evaluate((el) => ({ natural: el.width.baseVal.value, box: el.getBoundingClientRect().width }));
			assert.ok(Math.abs(shown.natural - shown.box) <= 1, `scaled: ${JSON.stringify(shown)}`);
		}, DESKTOP);
	});

	// The kinds have no floor (da-zh2o): the side column, a holder over
	// nowview and the tail, drags down to its panes' outlines and no further.
	test('drags the side column down to its panes\' outlines and no further', async () => {
		await withApp(async ({ page }) => {
			const border = page.locator('.split.row > [role="separator"]');
			const box = await border.boundingBox();
			const x = box.x + box.width / 2;
			const y = box.y + box.height / 2;
			await page.mouse.move(x, y);
			await page.mouse.down();
			for (let i = 1; i <= 20; i++) await page.mouse.move(x + i * 80, y);
			await page.mouse.up();
			await settled(page);
			const side = await pane(page, 'side').boundingBox();
			assert.ok(Math.abs(side.width - CHROME.width) <= 1, `the side column was dragged to ${side.width}px`);
		}, DESKTOP);
	});
});

/** A drawn pane's content box, whole pixels down, as `paneArgv` asks for it. */
const contentWidth = (page, id) =>
	pane(page, id)
		.locator('.body')
		.evaluate((el) => {
			const s = getComputedStyle(el);
			return Math.floor(el.clientWidth - parseFloat(s.paddingLeft) - parseFloat(s.paddingRight));
		});

// vividn's first look (da-jgrn): dragging a border was jittery, because every
// drawn pane re-ran its CLI command at every size it passed through. His
// second (da-2a5y): "have the widgets update some when dragging things
// around, rather than only when you let go, maybe just have a debounce
// baked in". So a drag redraws a drawn pane at most every quarter second
// (`THROTTLE_MS`, resizing.ts) — never at pointer rate — the drop flushes
// the newest box at once, and a burst of nudges is one redraw. Counted
// through the seam, never eyeballed, with the denominator printed.
describe('resizing', () => {
	test('a border dragged 200px over a second redraws a drawn pane 2–6 times, and once more at the drop', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const seen = await app.spyOnRunner();
			const border = page.locator('.split.row > [role="separator"]');
			const took = await dragFor(page, border, 200, 0, 1000);
			const during = drawnRuns(await seen());
			console.log(`  drag: ${took}ms, nowview ran ${during.nowview}x during it, dayview ${during.dayview}x`);
			assert.ok(during.nowview >= 2 && during.nowview <= 6, `nowview ran ${during.nowview} times during a ${took}ms drag`);
			// The strip spans the page above this split: its box did not move,
			// so it has nothing to redraw — and redraws nothing.
			assert.equal(during.dayview, 0, `the strip redrew ${during.dayview}x for a border that did not touch it`);
			// Meanwhile the picture is still up, scaled to its box — never blank.
			assert.ok((await canvasWidth(page, 'nowview')) > 0, 'the picture went away during the drag');

			await page.mouse.up();
			const dropped = Date.now();
			// The drop is the leading edge: the newest box is asked for at
			// once, not a throttle period later — unless the last tick had it
			// already, in which case there is nothing left to ask.
			await page.waitForFunction(
				([before, at]) =>
					window.__datumSeen.filter((a) => a[0] === 'nowview').length > before || Date.now() - at > 200,
				[during.nowview, dropped]
			);
			await app.idle();
			const after = drawnRuns(await seen());
			const atDrop = after.nowview - during.nowview;
			console.log(`  drop: nowview ran ${atDrop}x, asked ${Date.now() - dropped}ms after it`);
			assert.ok(atDrop <= 1, `nowview ran ${atDrop} times at the drop`);
			assert.equal(after.dayview, 0);
			// And the picture is the box's size: asked for at the final width,
			// not the old one stretched to it.
			assert.equal(await canvasWidth(page, 'nowview'), await contentWidth(page, 'nowview'));
			const img = pane(page, 'nowview').locator('svg.svg');
			const shown = await img.evaluate((el) => ({ natural: el.width.baseVal.value, box: el.getBoundingClientRect().width }));
			assert.ok(Math.abs(shown.natural - shown.box) <= 1, `scaled: ${JSON.stringify(shown)}`);
		}, DESKTOP);
	});

	test('ten arrow nudges in 300ms are one redraw', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const seen = await app.spyOnRunner();
			const border = page.locator('.split.row > [role="separator"]');
			await border.focus();
			// 30ms apart, inside one `SETTLE_MS` burst: the pacing is the
			// input, not a wait. The one redraw is the burst's trailing edge,
			// and nothing is pushed after it, so once it is asked for and has
			// answered there is no second one coming.
			for (let i = 0; i < 10; i++) {
				await page.keyboard.press('ArrowRight');
				await page.waitForTimeout(30);
			}
			await page.waitForFunction(() => window.__datumSeen.some((a) => a[0] === 'nowview'));
			await app.idle();
			const runs = drawnRuns(await seen());
			assert.equal(runs.nowview, 1, `nowview ran ${runs.nowview} times for ten nudges`);
			assert.equal(runs.dayview, 0, `the strip, untouched by this border, ran ${runs.dayview} bands`);
			assert.equal(await canvasWidth(page, 'nowview'), await contentWidth(page, 'nowview'));
		}, DESKTOP);
	});


	test('a border is grabbed right beside the badges, which hang half over the gap', async () => {
		await withApp(async ({ page }) => {
			// The tail's badges are centred on its top outline, so half of each
			// hangs into the gap above — the column border between nowview and
			// tail — and no further: 8px of a 16px badge in an 8px gap (da-om9v).
			const border = pane(page, 'side').locator('.split.column > [role="separator"]').first();
			const badge = pane(page, 'tail').locator('.titlebar button.add');
			const [gap, more, tailBefore] = await Promise.all([boxOf(border), boxOf(badge), boxOf(pane(page, 'tail'))]);
			const middle = more.y + more.height / 2;
			assert.ok(Math.abs(middle - (tailBefore.y + 0.5)) <= 1, `the badge's middle (${middle}) is not on the outline (${tailBefore.y})`);
			assert.ok(Math.abs(more.y - (gap.y + gap.height - 8)) <= 1, `the badge's top (${more.y}) is not 8px above the gap's end (${gap.y + gap.height})`);
			// The gap the border is grabbed by is the one past the badges
			// beside the TABS — the `+`, and whatever a later bead puts next
			// to it. The maximize is not among them any more: it sits at the
			// bar's far corner (da-jlgp), so the free run between the two is
			// where the border is bare. Read the badges, take the far edge of
			// the run that starts at the title, and check the point is clear
			// of every badge before pressing it.
			const [past, clear] = await pane(page, 'tail').evaluate((section) => {
				const badges = [...section.querySelectorAll(':scope > .titlebar .along button.badge')];
				const beside = Math.max(
					...badges.filter((b) => !b.classList.contains('maximize')).map((b) => b.getBoundingClientRect().right)
				);
				const on = (at) => badges.some((b) => b.getBoundingClientRect().left <= at && at <= b.getBoundingClientRect().right);
				return [beside, !on(beside + 6)];
			});
			assert.ok(clear, `the point past the tabs' badges (${past + 6}) is on a badge`);
			const x = past + 6;
			const y = gap.y + gap.height / 2;
			await page.mouse.move(x, y);
			await page.mouse.down();
			for (let i = 1; i <= 5; i++) await page.mouse.move(x, y - i * 6);
			await page.mouse.up();
			// The tail grows upward — by less than the 30px if nowview is at
			// its floor, but it grows: this was a drag, not a press on the badge.
			await page.waitForFunction(
				([was]) => document.querySelector('section[data-pane="tail"]').getBoundingClientRect().height > was + 8,
				[tailBefore.height]
			);
			assert.equal(await page.locator('[role="menu"]').count(), 0, 'the drag opened the menu instead');
		}, DESKTOP);
	});
});

// vividn's third look (da-zh2o): "let widgets get a lot smaller, until the
// display is gone and just a frame bar remains". Every kind a border can
// be dragged beside (the widget bar sizes itself, and the border beside
// it is no handle) shrinks to its frame, down a column AND across a row:
// the section is exactly the frame, and nothing of the widget is painted
// past it. The title lies OVER the content (da-13bc), so what is left of
// the content box is the inside half of the title's row — 8px down a
// column under a top bar, 0px across a row.
describe('a widget pane shrinks to its frame', () => {
	for (const kind of ['daystrip', 'repl', 'nowview', 'tail', 'custom', 'stats']) {
		for (const direction of ['column', 'row']) {
			const axis = direction === 'row' ? 'width' : 'height';
			test(`${kind}, across a ${direction}: only its frame is left, ${CHROME[axis]}px ${axis}`, async () => {
				await withApp(
					async ({ page }) => {
						const at = await grab(page, page.locator(`.split.${direction} > [role="separator"]`));
						for (let i = 1; i <= 20; i++) {
							await page.mouse.move(direction === 'row' ? at.x - i * 80 : at.x, direction === 'row' ? at.y : at.y - i * 60);
						}
						await page.mouse.up();
						await settled(page);
						const seen = await pane(page, 'a').evaluate((section, row) => {
							const content = section.querySelector(':scope > .content');
							const s = getComputedStyle(content);
							const b = section.getBoundingClientRect();
							// Halfway into the border gap past the frame, at every layer: nothing of the widget is there.
							const past = row ? [b.right + 4, b.top + b.height / 2] : [b.left + b.width / 2, b.bottom + 4];
							return {
								spills: document.elementsFromPoint(...past).some((el) => section.contains(el)),
								section: b[row ? 'width' : 'height'],
								content: row
									? content.clientWidth - parseFloat(s.paddingLeft) - parseFloat(s.paddingRight)
									: content.clientHeight - parseFloat(s.paddingTop) - parseFloat(s.paddingBottom)
							};
						}, direction === 'row');
						assert.ok(Math.abs(seen.content - (CHROME[axis] - 2)) <= 1, `${kind}: the content box is ${seen.content}px ${axis}, not the title's inside half`);
						assert.ok(!seen.spills, `${kind}: the widget paints outside its frame`);
						assert.ok(Math.abs(seen.section - CHROME[axis]) <= 1, `${kind}: the frame is ${seen.section}px ${axis}, not ${CHROME[axis]}`);
					},
					{ ...DESKTOP, prepare: (context) => context.addInitScript(draft, JSON.stringify(along(direction, widgetPane('a', kind), widgetPane('b', 'repl')))) }
				);
			});
		}
	}
});
