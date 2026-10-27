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
import { sharedApp, startApp, stopApp, withApp } from './support/app.mjs';
import {
	DESKTOP,
	MINE,
	PHONE,
	addTab,
	placeBar,
	chooser,
	enterLayoutMode,
	exactly,
	fill,
	leaveLayoutMode,
	manage,
	maximizeButton,
	pane,
	settled,
	titles
} from './support/layout.mjs';

before(startApp);
after(stopApp);

// ONE APP per screen (da-6b3dj, sharedApp in support/app.mjs): `solo` puts
// back a `main` a test edited (`restoreMain`), and the shell's `restore` drops
// any maximize left in force with it. The one that still opens an app of
// its own reloads.
const shared = sharedApp(DESKTOP);
const phone = sharedApp(PHONE);

/** The desktop default (defaults.ts), as the panes are titled. */
const SIX = ['dayview', 'repl', 'nowview', 'tail', 'custom', 'fields'];

/** Maximize or restore `id`, and wait for the page to settle on it — in normal mode, where the badge maximizes. */
async function press(page, id) {
	await leaveLayoutMode(page);
	await maximizeButton(page, id).click();
	await settled(page);
}

/** Every split's ratios, as the tree holds them: what «the tree did not change» reads as. */
const ratios = (page) =>
	page.evaluate(() => JSON.stringify(window.datum.layout.current(), (key, value) => (key === 'ratios' ? value : value)).match(/"ratios":\[[^\]]*\]/g));

/** The `main` layout's draft, as stored — null when nothing has been edited. */
const draftOf = (page) => page.evaluate(() => localStorage.getItem('datum.layout.draft.v1.main'));

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
		await shared.solo(async ({ page, errors }) => {
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
		});
	});

	// da-x7xjg: the tree stays mounted under the big pane, every other pane
	// at its own size and hidden, so a restore remounts nothing and asks the
	// CLI nothing of the panes that were never moved.
	test('a restore remounts no pane, and asks the CLI nothing', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const unkept = () => page.evaluate(() => [...document.querySelectorAll('section.pane')].filter((p) => !p.__kept).length);
			await app.idle();
			await page.evaluate(() => document.querySelectorAll('section.pane').forEach((p) => (p.__kept = true)));
			await press(page, 'repl');
			await app.idle();
			assert.equal(await unkept(), 0, 'the maximize mounted a pane');
			const seen = await app.spyOnRunner();
			await press(page, 'repl');
			await app.idle();
			assert.deepEqual(await titles(page), SIX);
			assert.deepEqual((await seen()).map((argv) => argv.join(' ')), [], 'the restore asked the CLI');
			assert.equal(await unkept(), 0, 'the restore mounted a pane again');
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	// «leaving the layout and coming back should unmaximize the pane».
	test('is dropped by opening another layout and coming back', async () => {
		await shared.solo(async ({ page }) => {
			await press(page, 'repl');
			assert.deepEqual(await titles(page), ['repl']);

			await page.locator('.root-bar button[aria-label="open a layout"]').click();
			await chooser(page).waitFor();
			await chooser(page).locator('button').filter({ hasText: exactly('stats') }).click();
			await chooser(page).waitFor({ state: 'detached' });
			await page.locator('.root-bar [role="tab"][aria-selected="true"]').filter({ hasText: 'stats' }).waitFor();

			await page.locator('.root-bar [role="tab"]').filter({ hasText: exactly('main') }).click();
			await settled(page);
			assert.deepEqual(await titles(page), SIX);
			assert.equal(await pressed(page), 0);
		});
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
		await shared.solo(async ({ page, errors }) => {
			await press(page, 'repl');
			await manage(page, 'repl', 'Close pane');
			await settled(page);
			assert.deepEqual(
				await titles(page),
				SIX.filter((t) => t !== 'repl')
			);
			assert.equal(await pressed(page), 0);
			assert.deepEqual(errors, []);
		});
	});

	// A maximize into a small box is not a maximize: a pane drawn inside a
	// nested layout tab fills the ROOT, not the layout that holds it.
	test('a pane inside a nested layout fills the root, not the nested box', async () => {
		await shared.solo(async (app) => {
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
		});
	});

	// Normal mode keeps the LAYOUT from changing (da-vvcxc); maximizing
	// changes nothing saved, so it is the one badge that stays.
	test('normal mode keeps the badge and has no `+` and no `×`', async () => {
		await shared.solo(async ({ page }) => {
			assert.equal(await page.locator('section.pane button.add').count(), 0, 'normal mode has a `+`');
			assert.equal(await page.locator('section.pane button.close').count(), 0, 'normal mode has a `×`');

			await press(page, 'repl');
			assert.deepEqual(await titles(page), ['repl']);
			await press(page, 'repl');
			assert.deepEqual(await titles(page), SIX);
		});
	});

	// Nothing to fill, and no bar to put it on.
	test('the root and a bare holder draw none', async () => {
		await shared.solo(async ({ page }) => {
			const stray = await page.evaluate(() =>
				[...document.querySelectorAll('button.maximize')]
					.map((el) => el.closest('section'))
					.filter((s) => s.classList.contains('root') || s.classList.contains('bare'))
					.map((s) => s.dataset.title)
			);
			assert.deepEqual(stray, []);
			// The badge is on the panes that DO draw a bar, all six of them.
			assert.equal(await page.locator('section.pane > .titlebar button.maximize').count(), SIX.length);
		});
	});

	// The DOM order is the KEYBOARD's and, since da-bzhq, the drawn order
	// too — no `order` and no auto margin deal a badge out to a corner any
	// more. vividn: «move the maximize button to be before the tab names,
	// rather than on the opposite side of the bar», so: maximize (in layout
	// mode, the handle in its place), then the title or the tabs, then `+`
	// — and since da-efr4i the `×` after it, the one destructive badge at
	// the far end from the `+`.
	test('the maximize comes first, before the title, and the `+` and `×` after it', async () => {
		await shared.solo(async ({ page }) => {
			await enterLayoutMode(page);
			const names = (id) =>
				pane(page, id)
					.locator('> .titlebar > .along')
					.evaluate((el) =>
						[...el.children].map(
							(c) =>
								['titlebox', 'tabs', 'add', 'maximize', 'close'].find((name) => c.classList.contains(name)) ??
								c.className
						)
					);
			assert.deepEqual(await names('repl'), ['maximize', 'titlebox', 'add', 'close']);
			// And with a strip in place of the title, the same order.
			await addTab(page, 'repl', 'aliases');
			assert.deepEqual(await names('repl'), ['maximize', 'tabs', 'add', 'close']);
		});
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
		await shared.solo(async ({ page }) => {
			for (const edge of ['top', 'right', 'left', 'bottom']) {
				if (edge !== 'top') {
					await placeBar(page, 'repl', edge);
					await settled(page);
				}
				const { box, svg, ink } = await drawn(page, 'repl');
				assert.ok(box.width >= 16 && box.height >= 16, `the ${edge} bar's button is ${box.width}x${box.height}`);
				inside(svg, box, `the ${edge} bar's glyph box`);
				inside(ink, box, `the ${edge} bar's strokes`);
				// Smaller than the button, which is what «fits» asks for.
				assert.ok(svg.width < box.width, `the ${edge} bar's glyph is ${svg.width}px in a ${box.width}px button`);
			}
		});
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

	/**
	 * How far the button's centre is from the NEAREST corner point of the
	 * pane's outline, on each axis — the outline being the rectangle
	 * through the middle of the 1px border, so half a pixel inside the
	 * border box on every side.
	 *
	 * Read as a distance from the nearest corner rather than from a named
	 * one, because «fully in the corner» is the same claim whichever edge
	 * the bar is on and whichever way it is turned, and a rule written
	 * that way cannot be satisfied by a badge that found some OTHER
	 * corner: the tests below pin which corner separately, by the order
	 * along the row.
	 */
	async function offCorner(page, id) {
		const [pane_, button] = await Promise.all([boxOf(pane(page, id)), boxOf(maximizeButton(page, id))]);
		const at = { x: button.x + button.width / 2, y: button.y + button.height / 2 };
		const xs = [pane_.x + 0.5, pane_.x + pane_.width - 0.5];
		const ys = [pane_.y + 0.5, pane_.y + pane_.height - 0.5];
		return {
			x: Math.min(...xs.map((x) => Math.abs(at.x - x))),
			y: Math.min(...ys.map((y) => Math.abs(at.y - y)))
		};
	}

	// vividn, 2026-09-22 (da-lihui), on what da-fphjw shipped: «yes it is
	// already aligned to this border, but I want it to be fully in the
	// corner so it is aligned with both the horizontal and vertical
	// borders», and «the maximize button didn't actually seem to get
	// smaller […] it can still be a bit smaller than it is (reduce padding
	// some)».
	//
	// So there are two measurements here, and BOTH of them were got wrong
	// once. The badge straddles the border it runs along and now the other
	// one too, which puts its centre on the corner point — as much of it
	// outside the pane as in, on either axis. And what got smaller is the
	// box you can SEE: the hit area is a fingertip, a fixed size, so the
	// drawn box comes off `--mark` while the button stays on the 16px/24px
	// floor `reachable.test.mjs` holds every badge to. Read on a rotated
	// bar as well as a top one, `start` being the same corner to the
	// alignment either way.
	//
	// UNDER BOTH POINTERS since da-wyofs, and that is the point of it.
	// Both measurements below came off `--badge` — which is 16 under a
	// cursor and 24 under a finger — and both have only ever run on
	// `DESKTOP`, so they pinned the drawing at the one size where it
	// already looked right. Under a finger the drawn box had reached 20px
	// and the glyph SIXTEEN, twice what a cursor saw, with this file
	// green throughout. They read `--mark` now: the size of the DRAWING,
	// one value whatever the pointer is.
	for (const [pointer, where, floor] of [['a cursor', shared, 16], ['a finger', phone, 24]])
		for (const edge of ['top', 'left']) {
			test(`under ${pointer}, on a ${edge} bar its centre is on the pane's corner, and the box it draws is smaller than the box it offers a finger`, async () => {
				await where.solo(async ({ page }) => {
					if (edge !== 'top') {
						await placeBar(page, 'repl', edge);
						await settled(page);
					}
					const flip = edge === 'left';
					// The `+` the row is read against is layout mode's (da-vvcxc).
					await enterLayoutMode(page);

					// FULLY IN THE CORNER, on both axes at once.
					const off = await offCorner(page, 'repl');
					assert.ok(off.x <= 1, `the badge's centre is ${off.x.toFixed(1)}px off the corner across the pane`);
					assert.ok(off.y <= 1, `the badge's centre is ${off.y.toFixed(1)}px off the corner down the pane`);

					// And only it moved: the title keeps the clearance that holds
					// the rest of the row off the rounded corner.
					const start = await along(page, 'repl', flip);
					assert.ok(start.title - start.max >= 6, 'the whole row moved, not the badge');

					// A BIT SMALLER, and the HIT AREA is not what shrank. The
					// button paints nothing itself; the box on the screen is the
					// `::before` inside it, two pixels off `--mark` (four until
					// da-33ves: «too small, make a little bigger») — so the
					// glyph, eight pixels off the same mark, has three pixels of
					// padding a side.
					const { box, svg } = await drawn(page, 'repl');
					const seen = await maximizeButton(page, 'repl').evaluate((el) => {
						const own = getComputedStyle(el);
						const drawn_ = getComputedStyle(el, '::before');
						return {
							badge: parseFloat(own.getPropertyValue('--badge')),
							mark: parseFloat(own.getPropertyValue('--mark')),
							width: parseFloat(drawn_.width),
							height: parseFloat(drawn_.height),
							ground: own.backgroundColor,
							ownEdge: own.borderTopColor
						};
					});
					assert.equal(seen.badge, floor, `--badge is ${seen.badge} under ${pointer}`);
					for (const side of ['width', 'height']) {
						assert.ok(
							Math.abs(seen.mark - svg[side] - 8) <= 0.5,
							`the glyph is ${svg[side].toFixed(1)} on a ${seen.mark}px mark`
						);
						assert.ok(
							Math.abs(seen[side] - (seen.mark - 2)) <= 0.5,
							`the drawn box is ${seen[side]}, not ${seen.mark - 2}`
						);
						assert.ok(seen[side] < box[side], `the drawn box is ${seen[side]} in a ${box[side]}px button`);
						assert.ok(seen[side] > svg[side], `the drawn box (${seen[side]}) does not clear the glyph (${svg[side]})`);
					}
					// Nothing is drawn by the button itself, or there would be two
					// boxes on the screen and the smaller one would not be it.
					for (const [what, colour] of [['ground', seen.ground], ['outline', seen.ownEdge]])
						assert.match(colour, /^rgba\(.*,\s*0\)$/, `the button draws its own ${what} (${colour})`);
					assert.ok(
						box.width >= floor && box.height >= floor,
						`the hit area is ${box.width}x${box.height} under ${pointer}, not ${floor}`
					);
				});
			});
		}

	// Acceptance, read straight (da-wyofs): the SAME app under the two
	// pointers, so that «the drawing is one size and the target is two» is
	// asserted as the comparison it is, and not as two numbers written
	// down in two runs. A cursor's reading is the one that was already
	// right, so a regression that moved BOTH pointers together — which is
	// exactly what the single token did — still fails here.
	// THE SAME CLAIM FOR THE WHOLE ROW (da-wyofs, da-efr4i): the maximize
	// (the handle, in layout mode), the `+` and the `×` draw a `::before`
	// inside a button that is the finger's box. One drawn size under either pointer, and no
	// badge drawing on the button itself — the four are one mechanism, so
	// a new badge that misses it is caught here rather than by eye on a
	// phone. And one size AMONG them (da-33ves): the maximize drew 12
	// beside three 14s, and was the one vividn called too small.
	test('every badge on the row draws one box, at one size under either pointer', async () => {
		const read = async (where) => {
			let out;
			await where.solo(async ({ page }) => {
				await enterLayoutMode(page);
				out = await page.evaluate(() => {
					const row = document.querySelector('section[data-pane="repl"] > .titlebar > .along');
					const seen = {};
					for (const [name, sel] of [['maximize', 'button.maximize'], ['+', 'button.add'], ['×', 'button.close']]) {
						const el = row.querySelector(sel);
						if (!el) { seen[name] = null; continue; }
						const own = getComputedStyle(el);
						const before = getComputedStyle(el, '::before');
						seen[name] = {
							button: Math.round(el.getBoundingClientRect().width),
							// The drawn box is the `::before`'s border box, which
							// is its computed `width` under `box-sizing:
							// border-box` (app.css). It was read as that plus 2,
							// which put the three at 16 while they painted 14
							// (counted in pixels, da-33ves).
							drawn: parseFloat(before.width),
							ownEdge: own.borderTopColor,
							ownGround: own.backgroundColor
						};
					}
					return { coarse: matchMedia('(pointer: coarse)').matches, seen };
				});
			});
			return out;
		};
		const [fine, coarse] = [await read(shared), await read(phone)];
		assert.equal(fine.coarse, false);
		assert.equal(coarse.coarse, true);
		for (const name of ['maximize', '+', '×']) {
			const [a, b] = [fine.seen[name], coarse.seen[name]];
			assert.ok(a && b, `the ${name} was not on the row`);
			// The hit area is the one thing that differs, which is what says
			// the two arms really were two pointers.
			assert.equal(a.button, 16, `the ${name}'s cursor hit area is ${a.button}`);
			assert.equal(b.button, 24, `the ${name}'s finger hit area is ${b.button}`);
			assert.equal(
				b.drawn,
				a.drawn,
				`the ${name} draws ${a.drawn} under a cursor and ${b.drawn} under a finger`
			);
			for (const [when, seen] of [['under a cursor', a], ['under a finger', b]]) {
				assert.match(seen.ownEdge, /^rgba\(.*,\s*0\)$/, `${when} the ${name} draws on its button (edge ${seen.ownEdge})`);
				assert.match(seen.ownGround, /^rgba\(.*,\s*0\)$/, `${when} the ${name} grounds its button (${seen.ownGround})`);
			}
		}
		// And all three draw the SAME box as each other, which is what
		// «one mechanism» means and what the eye actually reads on the row.
		const sizes = ['maximize', '+', '×'].map((n) => fine.seen[n].drawn);
		assert.equal(new Set(sizes).size, 1, `the row's badges draw ${sizes.join(', ')}`);
	});

	test('the drawing is the same under a cursor and under a finger; the target is not', async () => {
		const read = async (where) => {
			let out;
			await where.solo(async ({ page }) => {
				const { box, svg } = await drawn(page, 'repl');
				const seen = await maximizeButton(page, 'repl').evaluate((el) => {
					const d = getComputedStyle(el, '::before');
					return { coarse: matchMedia('(pointer: coarse)').matches, drawn: parseFloat(d.width) };
				});
				out = { ...seen, button: box.width, glyph: svg.width };
			});
			return out;
		};
		const [fine, coarse] = [await read(shared), await read(phone)];
		assert.equal(fine.coarse, false);
		assert.equal(coarse.coarse, true);
		assert.equal(coarse.drawn, fine.drawn, `the drawn box is ${fine.drawn} under a cursor and ${coarse.drawn} under a finger`);
		assert.equal(coarse.glyph, fine.glyph, `the glyph is ${fine.glyph} under a cursor and ${coarse.glyph} under a finger`);
		assert.equal(fine.button, 16, `the cursor's hit area is ${fine.button}`);
		assert.equal(coarse.button, 24, `the finger's hit area is ${coarse.button}`);
	});

	// Pointed at, the badge still draws ONE box. `.control.badge`'s hover
	// rule (app.css) colours the BUTTON's border, and the button's border
	// is the one that must never appear — a hover that brought it back
	// would put a second, larger outline round the smaller one, at exactly
	// the moment a reader is looking straight at it. What wins is Svelte's
	// scope class, repeated to outrank a global rule; that is a property of
	// the compiler rather than of this file, so it is asserted here.
	// EVERY BADGE, since da-wyofs put the `+` on the same mechanism: what
	// `.control.badge:hover` (app.css) colours is the BUTTON's border, and
	// the button's border is the one that must never appear. A scoped
	// selector wins that argument only by carrying more weight than a
	// global one, which is arithmetic about the compiler and about a file
	// this one does not own, so it is asserted rather than trusted. The `+`
	// and the `×` are layout mode's (da-vvcxc), so they are read there.
	// The `×` (da-efr4i) is the last. It arrived on the row while
	// da-wyofs was in flight and the two rebased past each other cleanly,
	// which is exactly how a badge comes to draw on its BUTTON while its
	// neighbours draw inside theirs — invisible under a cursor, where the
	// two boxes are the same 16px, and a 24px drawn box beside two 16px
	// ones under a finger. Measured before it was fixed.
	for (const [what, selector] of [
		['maximize', 'button.maximize'],
		['+', 'button.add'],
		['×', 'button.close']
	])
		test(`pointing at the ${what} moves the drawn edge, and does not give the button one`, async () => {
			await shared.solo(async ({ page }) => {
				if (what !== 'maximize') await enterLayoutMode(page);
				const badge = pane(page, 'repl').locator(`> .titlebar ${selector}`);
				const look = () =>
					badge.evaluate((el) => {
						const own = getComputedStyle(el);
						return {
							hovered: el.matches(':hover'),
							ground: own.backgroundColor,
							ownEdge: own.borderTopColor,
							drawnEdge: getComputedStyle(el, '::before').borderTopColor
						};
					});

				const rest = await look();
				await badge.hover();
				await page.waitForFunction(
					(selector) => document.querySelector(`section[data-pane="repl"] > .titlebar ${selector}`).matches(':hover'),
					selector
				);
				const on = await look();

				assert.ok(on.hovered && !rest.hovered, 'the hover never landed');
				assert.notEqual(on.drawnEdge, rest.drawnEdge, 'the drawn edge did not answer the pointer');
				for (const [when, seen] of [['at rest', rest], ['pointed at', on]]) {
					assert.match(seen.ground, /^rgba\(.*,\s*0\)$/, `${when}: the button draws its own ground (${seen.ground})`);
					assert.match(seen.ownEdge, /^rgba\(.*,\s*0\)$/, `${when}: the button draws its own outline (${seen.ownEdge})`);
				}
			});
		});

	// The other end of the row is untouched: at `align: end` the last badge
	// keeps the 0.5rem that holds it off the rounded corner there.
	test('the badges at the row\'s other end keep their clearance', async () => {
		await shared.solo(async ({ page }) => {
			await placeBar(page, 'repl', 'end');
			await enterLayoutMode(page);
			await settled(page);
			const trail = await page.evaluate(() => {
				const row = document.querySelector('section[data-pane="repl"] > .titlebar > .along');
				const last = row.lastElementChild.getBoundingClientRect();
				const at = row.getBoundingClientRect();
				return at.right - last.right;
			});
			assert.ok(trail >= 6, `the last badge is ${trail.toFixed(1)}px from the row's end`);
		});
	});

	// vividn, 2026-09-21 (da-bzhq): «move the maximize button to be before
	// the tab names, rather than on the opposite side of the bar». So it is
	// the FIRST thing along the bar at either alignment and on a rotated
	// bar too — which is the old far-corner rule (da-jlgp) withdrawn.
	for (const edge of ['top', 'left']) {
		test(`on a ${edge} bar the badge is before the tab names, at either alignment`, async () => {
			await shared.solo(async ({ page }) => {
				if (edge !== 'top') {
					await placeBar(page, 'repl', edge);
					await settled(page);
				}

				const flip = edge === 'left';

				await enterLayoutMode(page);
				const start = await along(page, 'repl', flip);
				assert.ok(start.max < start.title, `align start: the badge (${start.max.toFixed(1)}) is not before the title (${start.title.toFixed(1)})`);
				assert.ok(start.title < start.add, 'align start: the `+` did not stay after the title');

				await placeBar(page, 'repl', 'end');
				await settled(page);
				const done = await along(page, 'repl', flip);
				assert.ok(done.max < done.title, `align end: the badge (${done.max.toFixed(1)}) is not before the title (${done.title.toFixed(1)})`);
				assert.ok(done.title < done.add, 'align end: the `+` did not stay after the title');

				// Pressing it there still maximizes: the place is all that moved.
				await leaveLayoutMode(page);
				await press(page, 'repl');
				assert.deepEqual(await titles(page), ['repl']);
			});
		});
	}
});
