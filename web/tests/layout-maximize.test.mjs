// One pane, filling the page — the maximize badge (da-00am).
//
// vividn, on the deployed build: «Next to the + icon on the pane frame,
// let's add a little maximize icon, that then maximizes the pane, leaving
// the layout and coming back should unmaximize the pane, the maximize
// shouldn't be too sticky in general.»
//
// So there are two halves to hold the app to, and both are here. The first
// is that maximizing SHOWS the pane alone, filling the root's content —
// including a pane drawn inside a nested layout, which fills the page and
// not the box it was in, because a maximize into a small box is not one.
//
// The second is that it is VIEW STATE and nothing else
// (`$lib/layout/maximize.ts`): the tree is not touched, so no draft is
// written, no layout wears the modified dot, the ratios come back exactly,
// and the state is gone on a reload, on leaving the layout and coming back,
// and whenever the pane it names is no longer there. That is what «not too
// sticky» means, and it is the half a reader would only notice by losing
// their layout to it.
//
// Driven through the built app in a real browser, like the other `layout-*`
// files, and reading what a reader sees: the panes on screen, their boxes,
// the badge's own label.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import {
	DESKTOP,
	addTab,
	bar,
	chooseBar,
	chooser,
	exactly,
	fill,
	manage,
	maximizeButton,
	pane,
	settled,
	titles,
	toggleRootLock,
	widgetPane
} from './support/layout.mjs';

before(startApp);
after(stopApp);

/** The desktop default (defaults.ts), as the panes are titled. */
const SIX = ['dayview', 'repl', 'nowview', 'tail', 'custom', 'fields'];

/** Maximize or restore `id`, and wait for the page to settle on it. */
async function press(page, id) {
	await maximizeButton(page, id).click();
	await settled(page);
}

/** Every border's ratio, in DOM order: what «the tree did not change» reads as. */
const ratios = (page) =>
	page.locator('[role="separator"]').evaluateAll((all) => all.map((el) => el.getAttribute('aria-valuenow')));

/** The `day` layout's draft, as stored — null when nothing has been edited. */
const draftOf = (page) => page.evaluate(() => localStorage.getItem('datum.layout.draft.v1.day'));

/** How many root tabs wear the orange modified dot. */
const dots = (page) => page.locator('.root-bar [role="tab"].modified').count();

const boxOf = async (locator) => {
	const box = await locator.boundingBox();
	assert.ok(box, 'nothing to measure');
	return box;
};

/** Two boxes the same, to within a bar's row — the slack a frame's title has. */
function sameBox(got, want, what) {
	for (const side of ['x', 'y', 'width', 'height'])
		assert.ok(
			Math.abs(got[side] - want[side]) <= 16,
			`${what}: ${side} is ${got[side].toFixed(1)}, not ${want[side].toFixed(1)}`
		);
}

/** Whether a maximize is in force at all, as the badges report it. */
const pressed = (page) => page.locator('button.maximize[aria-pressed="true"]').count();

describe('the maximize badge', () => {
	test('fills the root with the one pane, and puts the tree back exactly', async () => {
		await withApp(async ({ page, errors }) => {
			// The box the whole tree has is the box the one pane must take.
			const whole = await boxOf(page.locator('[data-split="main"]'));
			const before = { ratios: await ratios(page), draft: await draftOf(page) };
			assert.deepEqual(await titles(page), SIX);

			await press(page, 'repl');
			assert.deepEqual(await titles(page), ['repl']);
			sameBox(await boxOf(pane(page, 'repl')), whole, 'the maximized repl');

			const badge = maximizeButton(page, 'repl');
			assert.equal(await badge.getAttribute('aria-label'), 'restore the repl pane');
			assert.equal(await badge.getAttribute('aria-pressed'), 'true');

			await press(page, 'repl');
			assert.deepEqual(await titles(page), SIX);
			assert.deepEqual(await ratios(page), before.ratios, 'the ratios moved');
			assert.equal(await draftOf(page), before.draft, 'maximizing wrote a draft');
			assert.equal(await dots(page), 0, 'maximizing marked a layout modified');
			assert.equal(
				await maximizeButton(page, 'repl').getAttribute('aria-label'),
				'maximize the repl pane'
			);
			assert.deepEqual(errors, []);
		}, DESKTOP);
	});

	// «leaving the layout and coming back should unmaximize the pane».
	test('is dropped by opening another layout and coming back', async () => {
		await withApp(async ({ page }) => {
			await press(page, 'repl');
			assert.deepEqual(await titles(page), ['repl']);

			await page.locator('.root-bar button[aria-label="open a layout"]').click();
			await chooser(page).waitFor();
			await chooser(page).locator('button').filter({ hasText: exactly('stats') }).click();
			await chooser(page).waitFor({ state: 'detached' });
			await page.locator('.root-bar [role="tab"][aria-selected="true"]').filter({ hasText: 'stats' }).waitFor();

			await page.locator('.root-bar [role="tab"]').filter({ hasText: exactly('day') }).click();
			await settled(page);
			assert.deepEqual(await titles(page), SIX);
			assert.equal(await pressed(page), 0);
		}, DESKTOP);
	});

	test('is gone after a reload', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await press(page, 'repl');
			assert.deepEqual(await titles(page), ['repl']);

			await app.reload();
			await settled(page);
			assert.deepEqual(await titles(page), SIX);
			assert.equal(await pressed(page), 0);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	// The healing: the state names a pane, and a pane that is not in the
	// tab's tree any more is simply not honoured. Nothing has to notice.
	test('heals when the maximized pane is closed', async () => {
		await withApp(async ({ page, errors }) => {
			await press(page, 'repl');
			await manage(page, 'repl', 'Close pane');
			await settled(page);
			assert.deepEqual(
				await titles(page),
				SIX.filter((t) => t !== 'repl')
			);
			assert.equal(await pressed(page), 0);
			assert.deepEqual(errors, []);
		}, DESKTOP);
	});

	// A maximize into a small box is not a maximize: a pane drawn inside a
	// nested layout tab fills the ROOT, not the layout that holds it.
	test('a pane inside a nested layout fills the root, not the nested box', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const whole = await boxOf(page.locator('[data-split="main"]'));
			await page.evaluate((mine) => window.datum.layouts.saveLayout(mine), MINE);
			await manage(page, 'tail', 'split: a new pane to the right');
			await fill(page, 'mine');
			await pane(page, 'm-a').waitFor();
			await settled(page);

			const inner = await boxOf(pane(page, 'm-a'));
			assert.ok(inner.width < whole.width / 2, `the nested pane is already ${inner.width}px wide`);

			await press(page, 'm-a');
			assert.deepEqual(await titles(page), ['nowview']);
			sameBox(await boxOf(pane(page, 'm-a')), whole, "the nested layout's maximized pane");
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	// A lock keeps the LAYOUT from changing; maximizing changes nothing
	// saved, so it is the one badge that stays.
	test('a locked layout keeps the badge and loses the `+` and the `▾`', async () => {
		await withApp(async ({ page }) => {
			await toggleRootLock(page);
			await settled(page);
			assert.equal(await page.locator('section.pane button.add').count(), 0, 'a locked layout kept its `+`');
			// Both go together (da-bzhq): "Locking hides both the + and this
			// down arrow icon menu."
			assert.equal(await page.locator('section.pane button.manage').count(), 0, 'a locked layout kept its `▾`');

			await press(page, 'repl');
			assert.deepEqual(await titles(page), ['repl']);
			await press(page, 'repl');
			assert.deepEqual(await titles(page), SIX);
		}, DESKTOP);
	});

	// Nothing to fill, and no bar to put it on.
	test('the root and a bare holder draw none', async () => {
		await withApp(async ({ page }) => {
			const stray = await page.evaluate(() =>
				[...document.querySelectorAll('button.maximize')]
					.map((el) => el.closest('section'))
					.filter((s) => s.classList.contains('root') || s.classList.contains('bare'))
					.map((s) => s.dataset.title)
			);
			assert.deepEqual(stray, []);
			// The badge is on the panes that DO draw a bar, all six of them.
			assert.equal(await page.locator('section.pane > .titlebar button.maximize').count(), SIX.length);
		}, DESKTOP);
	});

	// The DOM order is the KEYBOARD's and, since da-bzhq, the drawn order
	// too — no `order` and no auto margin deal a badge out to a corner any
	// more. vividn: «move the maximize button to be before the tab names,
	// rather than on the opposite side of the bar», so: maximize, then the
	// title or the tabs, then `+`, then `▾`.
	test('the maximize comes first, before the title, and the `+` and `▾` after it', async () => {
		await withApp(async ({ page }) => {
			const names = (id) =>
				pane(page, id)
					.locator('> .titlebar > .along')
					.evaluate((el) =>
						[...el.children].map(
							(c) =>
								['titlebox', 'tabs', 'add', 'maximize', 'manage'].find((name) => c.classList.contains(name)) ??
								c.className
						)
					);
			assert.deepEqual(await names('repl'), ['maximize', 'titlebox', 'add', 'manage']);
			// And with a strip in place of the title, the same order.
			await addTab(page, 'repl', 'aliases');
			assert.deepEqual(await names('repl'), ['maximize', 'tabs', 'add', 'manage']);
		}, DESKTOP);
	});
});

// vividn, on the deployed build (da-jlgp): «make the maximize icon smaller so
// it fits in its button, also let's move this button to the other side of
// the tabs, ideally it would be in the corner». The size is still that; the
// PLACE was overruled on 2026-09-21 (da-bzhq) — «move the maximize button
// to be before the tab names» — so the corner cases below read the new rule.
describe('the maximize badge, drawn and placed', () => {
	/** The button's box, its `<svg>`'s, and the box the strokes actually cover. */
	async function drawn(page, id) {
		const button = maximizeButton(page, id);
		const [box, svg, ink] = await Promise.all([
			boxOf(button),
			boxOf(button.locator('svg')),
			button.evaluate((el) => {
				const rects = [...el.querySelectorAll('svg path')].map((p) => p.getBoundingClientRect());
				return {
					x: Math.min(...rects.map((r) => r.left)),
					y: Math.min(...rects.map((r) => r.top)),
					right: Math.max(...rects.map((r) => r.right)),
					bottom: Math.max(...rects.map((r) => r.bottom))
				};
			})
		]);
		return { box, svg, ink };
	}

	/** `inner` does not hang out of `outer` on any side. */
	function inside(inner, outer, what) {
		const [left, top, right, bottom] = [
			inner.x,
			inner.y,
			inner.right ?? inner.x + inner.width,
			inner.bottom ?? inner.y + inner.height
		];
		assert.ok(left >= outer.x - 0.5, `${what}: ${left.toFixed(1)} is left of the button's ${outer.x.toFixed(1)}`);
		assert.ok(top >= outer.y - 0.5, `${what}: ${top.toFixed(1)} is above the button's ${outer.y.toFixed(1)}`);
		const [r, b] = [outer.x + outer.width, outer.y + outer.height];
		assert.ok(right <= r + 0.5, `${what}: ${right.toFixed(1)} is right of the button's ${r.toFixed(1)}`);
		assert.ok(bottom <= b + 0.5, `${what}: ${bottom.toFixed(1)} is below the button's ${b.toFixed(1)}`);
	}

	// The glyph was a fixed 16px square in a 16px button with a 1px
	// outline, so it hung a pixel out of every side.
	test('the glyph fits inside its button, on a horizontal bar and on a vertical one', async () => {
		await withApp(async ({ page }) => {
			for (const edge of ['top', 'right', 'left', 'bottom']) {
				if (edge !== 'top') {
					await chooseBar(page, 'repl', edge);
					await settled(page);
				}
				const { box, svg, ink } = await drawn(page, 'repl');
				assert.ok(box.width >= 16 && box.height >= 16, `the ${edge} bar's button is ${box.width}x${box.height}`);
				inside(svg, box, `the ${edge} bar's glyph box`);
				inside(ink, box, `the ${edge} bar's strokes`);
				// Smaller than the button, which is what «fits» asks for.
				assert.ok(svg.width < box.width, `the ${edge} bar's glyph is ${svg.width}px in a ${box.width}px button`);
			}
		}, DESKTOP);
	});

	/**
	 * Where each of the bar's items sits ALONG the bar, reading the way the
	 * bar reads: the distance from the end the alignment calls `start`.
	 * Measured in screen coordinates and turned round on a left-hand bar,
	 * which is drawn upside down so that it reads bottom-to-top
	 * (PaneFrame.svelte, `rotate(180deg)`).
	 */
	async function along(page, id, flip) {
		const [row, max, title, add] = await Promise.all([
			boxOf(pane(page, id).locator('> .titlebar > .along')),
			boxOf(maximizeButton(page, id)),
			boxOf(pane(page, id).locator('> .titlebar .titlebox')),
			boxOf(pane(page, id).locator('> .titlebar button.add'))
		]);
		// The bar runs along its longer side, whichever edge it is on.
		const axis = row.width >= row.height ? 'x' : 'y';
		const size = axis === 'x' ? 'width' : 'height';
		const lead = (b) => (flip ? row[axis] + row[size] - (b[axis] + b[size]) : b[axis] - row[axis]);
		return { max: lead(max), title: lead(title), add: lead(add) };
	}

	// vividn, 2026-09-21 (da-fphjw), on the chrome da-bzhq had just landed:
	// «let's push the maximize button into the corner more, and it can be
	// a bit smaller as well».
	//
	// Two measurements, and the FIRST of them is a floor rather than a
	// preference: «a bit smaller» is about the MARK, and a fingertip is a
	// fixed size, so the glyph shrinks inside a button that does not. The
	// button is left on the 16px/24px floor `reachable.test.mjs` holds
	// every badge to, and the corner is read on a rotated bar as well as a
	// top one, `start` being the same corner to the alignment either way.
	for (const edge of ['top', 'left']) {
		test(`on a ${edge} bar it sits in the corner the bar starts at, and its glyph is smaller than its button by 4px a side`, async () => {
			await withApp(async ({ page }) => {
				if (edge !== 'top') {
					await chooseBar(page, 'repl', edge);
					await settled(page);
				}
				const flip = edge === 'left';

				// INTO THE CORNER: the row is held 0.5rem off the rounded
				// corners and the maximize gives 6px of that back, so it
				// starts within a couple of pixels of the pane's own corner.
				const start = await along(page, 'repl', flip);
				assert.ok(start.max <= 3, `the badge starts ${start.max.toFixed(1)}px from the corner`);
				assert.ok(start.max >= -0.5, `the badge starts ${start.max.toFixed(1)}px OUTSIDE the corner`);
				// And only it: the title, which used to lead the row, keeps the
				// clearance the maximize gave up.
				assert.ok(start.title - start.max >= 6, 'the whole row moved, not the badge');

				// A BIT SMALLER, and the BUTTON is not what shrank: the glyph
				// is four pixels in from the border box on every side, and the
				// button is still on its floor.
				const { box, svg } = await drawn(page, 'repl');
				for (const side of ['width', 'height'])
					assert.ok(
						Math.abs(box[side] - svg[side] - 8) <= 0.5,
						`the glyph is ${svg[side].toFixed(1)} in a ${box[side].toFixed(1)}px button`
					);
				assert.ok(box.width >= 16 && box.height >= 16, `the hit area shrank to ${box.width}x${box.height}`);
			}, DESKTOP);
		});
	}

	// The other end of the row is untouched: at `align: end` the last badge
	// keeps the 0.5rem that holds it off the rounded corner there.
	test('the badges at the row\'s other end keep their clearance', async () => {
		await withApp(async ({ page }) => {
			await chooseBar(page, 'repl', 'end');
			await settled(page);
			const trail = await page.evaluate(() => {
				const row = document.querySelector('section[data-pane="repl"] > .titlebar > .along');
				const last = row.lastElementChild.getBoundingClientRect();
				const at = row.getBoundingClientRect();
				return at.right - last.right;
			});
			assert.ok(trail >= 6, `the last badge is ${trail.toFixed(1)}px from the row's end`);
		}, DESKTOP);
	});

	// vividn, 2026-09-21 (da-bzhq): «move the maximize button to be before
	// the tab names, rather than on the opposite side of the bar». So it is
	// the FIRST thing along the bar at either alignment and on a rotated
	// bar too — which is the old far-corner rule (da-jlgp) withdrawn.
	for (const edge of ['top', 'left']) {
		test(`on a ${edge} bar the badge is before the tab names, at either alignment`, async () => {
			await withApp(async ({ page }) => {
				if (edge !== 'top') {
					await chooseBar(page, 'repl', edge);
					await settled(page);
				}

				const flip = edge === 'left';

				const start = await along(page, 'repl', flip);
				assert.ok(start.max < start.title, `align start: the badge (${start.max.toFixed(1)}) is not before the title (${start.title.toFixed(1)})`);
				assert.ok(start.title < start.add, 'align start: the `+` did not stay after the title');

				await chooseBar(page, 'repl', 'end');
				await settled(page);
				const done = await along(page, 'repl', flip);
				assert.ok(done.max < done.title, `align end: the badge (${done.max.toFixed(1)}) is not before the title (${done.title.toFixed(1)})`);
				assert.ok(done.title < done.add, 'align end: the `+` did not stay after the title');

				// Pressing it there still maximizes: the place is all that moved.
				await press(page, 'repl');
				assert.deepEqual(await titles(page), ['repl']);
			}, DESKTOP);
		});
	}
});

/** Two panes side by side, so a nested layout has an INSIDE to maximize out of. */
const MINE = {
	version: 1,
	name: 'mine',
	root: {
		id: 'm',
		active: 0,
		bar,
		tabs: [
			{
				id: 'm-tab',
				content: {
					id: 'm-split',
					direction: 'row',
					ratios: [0.5, 0.5],
					children: [widgetPane('m-a', 'nowview'), widgetPane('m-b', 'fields')]
				}
			}
		]
	}
};
