// A pane's frame (da-jdop, da-jgrn, da-om9v, da-13bc, da-9vtw): the title
// on the outline — the button that opens the pane's menu — the `+` beside
// it, the row as tall as its words, the gaps, the scrollbars. Driven
// through the built app in a real browser and measured in screen
// coordinates, since the left bar is turned round.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { atEachScreen, inWorker, sharedApp, startApp, stopApp, withApp } from './support/app.mjs';
import {
	DESKTOP,
	PHONE,
	activeTab,
	addTab,
	boxOf,
	enterLayoutMode,
	placeBar,
	pane
} from './support/layout.mjs';

before(startApp);
after(stopApp);

// ONE desktop app (da-b7sfp): nothing here reads a row, and `solo` puts
// the layout back to `main` between tests — and the screen back to its size
// after an `atEachScreen` (da-6b3dj). What starts on another screen or
// under the other pointer, or from a worker of its own, opens its own app.
const shared = sharedApp(DESKTOP);

// The bar, after vividn's first look (da-jgrn): with one tab no tab strip at
// all — the title sits on the frame's outline and the `+` is a badge right
// after it; with more, the strip, and the badge right after the last tab.
describe('the title on the frame', () => {
	/** Does `inner` straddle the top edge of `outer`? */
	const straddlesTop = (inner, outer) => inner.y < outer.y && inner.y + inner.height > outer.y;

	test('one tab: no tab strip, the title on the outline, the + right after it, on the keyboard', async () => {
		await shared.solo(async ({ page }) => {
			// The `+` is layout mode's (da-vvcxc).
			await enterLayoutMode(page);
			const tail = pane(page, 'tail');
			assert.equal(await tail.locator('[role="tablist"]').count(), 0);
			const [frame, h2, add] = await Promise.all([boxOf(tail), boxOf(tail.locator('h2')), boxOf(tail.locator('button.add'))]);
			assert.ok(straddlesTop(h2, frame), `the title ${JSON.stringify(h2)} is not on the outline of ${JSON.stringify(frame)}`);
			assert.ok(straddlesTop(add, frame), `the badge ${JSON.stringify(add)} is not on the outline`);
			const apart = add.x - (h2.x + h2.width);
			assert.ok(apart >= 0 && apart <= 8, `the badge is ${apart}px from the title`);
			assert.equal(add.height, h2.height, `the badge is ${add.width}x${add.height}, the title ${h2.height} tall`);
			// The title IS the menu's button (da-9vtw), and it fills the box.
			const title = await boxOf(tail.locator('button.title'));
			assert.deepEqual([title.width, title.height], [h2.width, h2.height], 'the button does not fill the title');
			await tail.locator('button.title').focus();
			await page.keyboard.press('Enter');
			await page.locator('[role="menu"]').waitFor();
			await page.keyboard.press('Escape');
		});
	});

	test('two tabs: the strip, and the + right after the last tab', async () => {
		await shared.solo(async ({ page }) => {
			await addTab(page, 'tail', 'aliases');
			const tail = pane(page, 'tail');
			const tabs = tail.locator('[role="tablist"] [role="tab"]');
			assert.equal(await tabs.count(), 2);
			const [frame, last, more] = await Promise.all([boxOf(tail), boxOf(tabs.last()), boxOf(tail.locator('button.add'))]);
			assert.ok(straddlesTop(last, frame), 'the tabs are not on the outline');
			const apart = more.x - (last.x + last.width);
			assert.ok(apart >= 0 && apart <= 8, `the badge is ${apart}px from the last tab`);
			// No h2 to carry the widget's own title, so it is nowhere: not a
			// stray line at the top of the content, under the tabs (da-13bc).
			await tail.locator('[role="tab"]', { hasText: 'tail' }).click();
			const shown = await tail.locator('> .content').evaluate((el) =>
				[...el.querySelectorAll('.hosted-title')].filter((s) => getComputedStyle(s).display !== 'none').length
			);
			assert.equal(shown, 0, "the tail's title shows in its content");
		});
	});

	// A widget's OWN title controls — the custom pane's ✎ and the editor it
	// opens — are moved into the frame's h2, and a tabbed pane has none
	// (da-ndjo): they showed as stray text at the top of the content, and
	// then as nothing at all, which left a tabbed custom pane's command
	// with nowhere it could be written. The bar keeps a box for them after
	// the strip.
	test('two tabs: the active widget’s own controls are on the bar, and work there', async () => {
		await shared.solo(async ({ page }) => {
			await addTab(page, 'custom', 'aliases');
			const custom = pane(page, 'custom');
			await custom.locator('[role="tab"]', { hasText: 'custom' }).click();
			const own = custom.locator('> .titlebar .own');
			await own.locator('button.edit').waitFor();
			assert.equal(await custom.locator('> .content button.edit').count(), 0, 'the ✎ is in the content');

			await own.locator('button.edit').click();
			const line = own.locator('input.command');
			await line.fill('tail -n 1');
			await line.press('Enter');
			await own.filter({ hasText: 'tail -n 1' }).waitFor();

			// The strip still carries the names, and the widget's own box
			// does not say the same word again beside it.
			assert.equal(await activeTab(page, 'custom'), 'custom');
			const tabs = custom.locator('> .titlebar [role="tab"]');
			assert.deepEqual(await tabs.allInnerTexts(), ['custom', 'aliases']);
		});
	});

	// A widget with no controls of its own is named by the FRAME: the name
	// it was passing through (`CliPane`'s `spec.title`) was the same word,
	// and it silenced the frame's own — the modified dot among it.
	test('one tab: a widget with no controls of its own leaves the frame to name it', async () => {
		await shared.solo(async ({ page }) => {
			const tail = pane(page, 'tail');
			assert.equal(await tail.locator('> .titlebar h2').innerText(), 'tail');
			assert.equal(await tail.locator('.hosted-title').count(), 0, 'the tail pane hosts a title');
		});
	});

	test('on a vertical bar the badge follows the title along it', async () => {
		await shared.solo(async ({ page }) => {
			await enterLayoutMode(page);
			const tail = pane(page, 'tail');
			await placeBar(page, 'tail', 'right');
			let [h2, more] = await Promise.all([boxOf(tail.locator('h2')), boxOf(tail.locator('button.add'))]);
			let apart = more.y - (h2.y + h2.height);
			assert.ok(apart >= 0 && apart <= 8, `right bar, reading down: the badge is ${apart}px below the title`);
			await placeBar(page, 'tail', 'left');
			[h2, more] = await Promise.all([boxOf(tail.locator('h2')), boxOf(tail.locator('button.add'))]);
			apart = h2.y - (more.y + more.height);
			assert.ok(apart >= 0 && apart <= 8, `left bar, reading up: the badge is ${apart}px above the title`);
		});
	});
});

// vividn's fourth look (da-om9v): "the widget titles bleed too much into
// other frames". The row is its words and a 1px outline in the frame's
// colour, and the half of it outside the pane fits the gap: nothing on
// a pane's bar lies inside a neighbour's border box, whichever edge the
// bars are on. Measured in screen coordinates, since the left bar is
// turned round.
describe('the title row, as tall as its words', () => {
	/** What sits on a pane's bar, or over its corner, that could reach a neighbour. */
	const ON_BAR =
		':scope > .titlebar :is(h2, .own, [role="tab"], button.maximize, button.add, button.close), [role="slider"], .nowview-steps button';

	/** Every piece of `ON_BAR` whose DRAWING is inside another pane's border box.
	 *
	 * The drawing, not the element's box (da-wyofs): a badge offers a
	 * finger `--badge` — 24px where the pointer is coarse — and draws
	 * `--mark` inside it. The 4px a finger's box has over the mark on
	 * either side is not on the screen, so it cannot lie over anything;
	 * what a reader sees lying over a neighbour is the mark. So each
	 * element is measured as the box it PAINTS: its own, unless it paints
	 * with a `::before` inside itself, in which case that one, centred.
	 */
	const bleeds = (page) =>
		page.evaluate((selector) => {
			const panes = [...document.querySelectorAll('section.pane')];
			const rect = (el) => el.getBoundingClientRect();
			const drawn = (el) => {
				const r = rect(el);
				const mark = getComputedStyle(el, '::before');
				// No `::before`, or one that paints no outline: the element
				// is its own drawing, as it was before da-wyofs.
				if (mark.content === 'none' || !(parseFloat(mark.borderTopWidth) > 0)) return r;
				const box = (size, ...edges) => parseFloat(mark[size]) + edges.reduce((n, e) => n + parseFloat(mark[e]), 0);
				const ix = (r.width - box('width', 'borderLeftWidth', 'borderRightWidth')) / 2;
				const iy = (r.height - box('height', 'borderTopWidth', 'borderBottomWidth')) / 2;
				return { left: r.left + ix, right: r.right - ix, top: r.top + iy, bottom: r.bottom - iy };
			};
			// A hair of slack for the arithmetic above: a title that STRADDLES
			// its own outline stops exactly on the neighbour's, and half the
			// numbers here are computed rather than measured.
			const E = 0.25;
			const into = (a, b) =>
				a.left < b.right - E && b.left + E < a.right && a.top < b.bottom - E && b.top + E < a.bottom;
			return panes.flatMap((p) =>
				[...p.querySelectorAll(selector)]
					.filter((el) => el.getClientRects().length > 0)
					.flatMap((el) =>
						panes
							.filter((q) => q !== p && !q.contains(el) && into(drawn(el), rect(q)))
							.map((q) => `${p.dataset.title}'s ${el.tagName.toLowerCase()} "${el.textContent.trim()}" in ${q.dataset.title}`)
					)
			);
		}, ON_BAR);

	// Eight cases, two apps: an edge is `layout.apply`, which the page
	// answers in a frame, and the screen is a resize. Eight boots for them
	// was seven seconds of `dayview` (da-x6ea).
	//
	// AND UNDER BOTH POINTERS (da-wyofs). This test already ran at 390x844
	// — and stayed green while a phone drew every pane's title and badges
	// 3.5px over the pane above, which is vividn's «overlap with other
	// panes». `pointer: coarse` is a property of the browser CONTEXT, not
	// of the viewport, so resizing a desktop app to a phone's screen
	// resizes everything except the one media query that was wrong. A
	// coarse app is a second app; `PHONE` is it.
	for (const [pointer, where] of [['a cursor', { viewport: { width: 1280, height: 800 } }], ['a finger', PHONE]])
		test(`under ${pointer}, no title, tab or … lies over a neighbour, whatever edge every bar is on`, async (t) => {
			await withApp(async ({ page }) => {
				const screens = pointer === 'a finger' ? [page.viewportSize()] : [{ width: 1280, height: 800 }, { width: 390, height: 844 }];
				await atEachScreen(t, page, screens, async () => {
					assert.ok((await page.locator('section.pane').count()) >= 3, 'fewer than three panes');
					for (const edge of ['top', 'bottom', 'left', 'right']) {
						// Every pane's bar at once (the menu's own path is tested above).
						await page.evaluate((edge) =>
							window.datum.layout.apply((tree) => {
								const copy = structuredClone(tree);
								const walk = (pane) => {
									pane.bar = { ...pane.bar, edge };
									for (const tab of pane.tabs) if ('children' in tab.content) tab.content.children.forEach(walk);
								};
								walk(copy.root);
								return copy;
							}),
							edge
						);
						await page.locator(`section.pane.edge-${edge}`).first().waitFor();
						const astray = await page.locator('section.pane').evaluateAll(
							(all, edge) => all.filter((p) => !p.classList.contains(`edge-${edge}`)).map((p) => p.dataset.title),
							edge
						);
						assert.deepEqual(astray, [], `not on the ${edge}`);
						assert.deepEqual(await bleeds(page), [], `on the ${edge}`);
					}
				});
			}, where);
		});

	// vividn, 2026-09-22: «I like how the pane titles are compact on desktop —
	// they are too padded and wide on mobile and overlap with other panes.»
	// The title's box was `--badge`, so a phone got a 24px title with the
	// same 12px words in it and the eight pixels landed as padding above
	// and below them. It is `--mark` now — one value under either pointer —
	// and the finger's box that the menu's button needs is the `.titlebox`
	// around it (da-wyofs). Both halves, because either alone is a
	// regression: the words back to compact but the button off the touch
	// floor, or the floor kept by growing the title again.
	test('the title draws the same height under a cursor and under a finger, and its button keeps the touch floor', async () => {
		const read = async (open) => {
			let out;
			await open(async ({ page }) => {
				// `repl`, which both trees carry with a title of its own:
				// the phone's `main` drops `nowview` and `custom` and tabs
				// `tail` into a pane whose heading is the tab in front.
				out = await pane(page, 'repl').evaluate((p) => {
					const h2 = p.querySelector(':scope > .titlebar h2');
					const button = p.querySelector(':scope > .titlebar button.title');
					const s = getComputedStyle(h2);
					return {
						coarse: matchMedia('(pointer: coarse)').matches,
						drawn: h2.getBoundingClientRect().height,
						font: s.fontSize,
						padding: [s.paddingTop, s.paddingBottom],
						button: button.getBoundingClientRect().height
					};
				});
			});
			return out;
		};
		const [fine, coarse] = [await read(shared.solo), await read((fn) => withApp(fn, PHONE))];
		assert.equal(fine.coarse, false);
		assert.equal(coarse.coarse, true);
		assert.equal(coarse.drawn, fine.drawn, `the title is ${fine.drawn}px under a cursor and ${coarse.drawn}px under a finger`);
		assert.equal(coarse.font, fine.font, `the words are ${fine.font} under a cursor and ${coarse.font} under a finger`);
		// The padding was always nothing; what grew was the box round it.
		for (const [what, seen] of [['a cursor', fine], ['a finger', coarse]])
			assert.deepEqual(seen.padding, ['0px', '0px'], `under ${what} the title is padded ${seen.padding}`);
		assert.equal(fine.button, 16, `the cursor's title button is ${fine.button}px tall`);
		assert.equal(coarse.button, 24, `the finger's title button is ${coarse.button}px tall`);
	});

	test('the title is its words and a 1px outline in the frame’s colour; its cross padding is nothing on a left bar', async () => {
		await shared.solo(async ({ page }) => {
			const tail = pane(page, 'tail');
			const seen = () =>
				tail.evaluate((p) => {
					const h2 = p.querySelector(':scope > .titlebar h2');
					const s = getComputedStyle(h2);
					const b = h2.getBoundingClientRect();
					return {
						font: s.fontSize,
						rem: parseFloat(getComputedStyle(document.documentElement).fontSize),
						box: [b.width, b.height],
						border: [s.borderTopWidth, s.borderRightWidth, s.borderBottomWidth, s.borderLeftWidth],
						colours: [s.borderTopColor, s.borderRightColor, s.borderBottomColor, s.borderLeftColor],
						frame: getComputedStyle(p).borderTopColor,
						padding: { top: s.paddingTop, bottom: s.paddingBottom, left: s.paddingLeft, right: s.paddingRight }
					};
				});
			const top = await seen();
			assert.equal(parseFloat(top.font), 0.75 * top.rem);
			assert.ok(top.box[1] <= 18, `the title is ${top.box[1]}px tall`);
			assert.deepEqual(top.border, ['1px', '1px', '1px', '1px']);
			assert.deepEqual(new Set(top.colours), new Set([top.frame]), `the outline is ${top.colours}, the frame ${top.frame}`);
			assert.deepEqual([top.padding.top, top.padding.bottom], ['0px', '0px']);

			await placeBar(page, 'tail', 'left');
			const left = await seen();
			assert.ok(left.box[0] <= 18, `the left bar's title is ${left.box[0]}px across`);
			for (const side of ['left', 'right']) assert.ok(parseFloat(left.padding[side]) <= 1, `padding-${side} ${left.padding[side]}`);
		});
	});

	test('the custom pane’s ✎ and its editor fit inside the row, uncut', async () => {
		await shared.solo(async ({ page }) => {
			const custom = page.locator('section.pane[data-title="custom"]');
			const fits = () =>
				custom.evaluate((p) => {
					const h2 = p.querySelector(':scope > .titlebar h2');
					return { box: h2.getBoundingClientRect().height, inner: h2.clientHeight, needs: h2.scrollHeight };
				});
			const shown = await fits();
			await custom.locator('button.edit').click();
			await custom.locator('input.command').waitFor();
			const editing = await fits();
			for (const [what, seen] of [['shown', shown], ['editing', editing]]) {
				assert.ok(seen.box <= 18, `${what}: the title is ${seen.box}px tall`);
				assert.ok(seen.needs <= seen.inner, `${what}: the title needs ${seen.needs}px and has ${seen.inner}`);
			}
		});
	});

	test('alarmed, the title’s outline turns orange with the frame', async () => {
		// A sync access handle that never answers: the database is held in
		// memory, and the REPL's frame is alarmed (see storage.test.mjs).
		const NEVER = `FileSystemFileHandle.prototype.createSyncAccessHandle = () => new Promise(() => {});`;
		await withApp(
			async ({ page }) => {
				const frame = page.locator('section.pane.alarm');
				await frame.first().waitFor();
				const colours = await frame.first().evaluate((p) => {
					const s = getComputedStyle(p.querySelector(':scope > .titlebar h2'));
					return [s.borderTopColor, s.borderRightColor, s.borderBottomColor, s.borderLeftColor];
				});
				assert.deepEqual(new Set(colours), new Set(['rgb(232, 130, 12)']));
			},
			{ ...DESKTOP, prepare: inWorker(NEVER) }
		);
	});
});

// The margins, after vividn's first look (da-jgrn): the gap between panes
// from 12px to 8px — the floor a finger needs on a border — and the body's
// padding a third off, 0.5rem 0.6rem → 0.33rem 0.4rem. Asserted from the
// computed styles, on a desktop and on a phone.
describe('the margins', () => {
	test('the gap is 8px and the body padding 0.33rem 0.4rem', async (t) => {
		await shared.solo(async ({ page }) => {
			await atEachScreen(t, page, [{ width: 1280, height: 900 }, { width: 390, height: 844 }], async () => {
				const seen = await page.evaluate(() => {
					// The gap is the same in either mode: a handle in layout mode, static outside it.
					const border = document.querySelector('.split .border');
					const b = border.getBoundingClientRect();
					const body = getComputedStyle(document.querySelector('section.pane .body'));
					return {
						gap: Math.min(b.width, b.height),
						padding: [body.paddingTop, body.paddingRight, body.paddingBottom, body.paddingLeft].map(parseFloat)
					};
				});
				assert.equal(seen.gap, 8);
				const rem = await page.evaluate(() => parseFloat(getComputedStyle(document.documentElement).fontSize));
				const near = (a, b) => Math.abs(a - b) < 0.05;
				assert.ok(near(seen.padding[0], 0.33 * rem) && near(seen.padding[2], 0.33 * rem), `top/bottom ${seen.padding}`);
				assert.ok(near(seen.padding[1], 0.4 * rem) && near(seen.padding[3], 0.4 * rem), `left/right ${seen.padding}`);
			});
		});
	});
});

// The scrollbars (da-2a5y): "The built in ones look terrible with the
// design, they should be more very tiny, invisible when not in use, and
// match the frame color." One rule for every scroll container, in app.css.
describe('scrollbars', () => {
	test("are thin on a pane's body and on the root scroller", async (t) => {
		await shared.solo(async ({ page }) => {
			await atEachScreen(t, page, [{ width: 1280, height: 900 }, { width: 390, height: 844 }], async () => {
				const seen = await page.evaluate(() => {
					const style = (el) => {
						const s = getComputedStyle(el);
						return { width: s.scrollbarWidth, color: s.scrollbarColor };
					};
					return {
						body: style(document.querySelector('section.pane .body')),
						root: style(document.querySelector('section.root > .content'))
					};
				});
				assert.equal(seen.body.width, 'thin', `the body: ${JSON.stringify(seen.body)}`);
				assert.equal(seen.root.width, 'thin', `the root scroller: ${JSON.stringify(seen.root)}`);
				// No pointer over it: the thumb is transparent.
				assert.match(seen.body.color, /^rgba\(0, 0, 0, 0\) rgba\(0, 0, 0, 0\)$|^transparent transparent$/);
			});
		});
	});
});

// vividn's third look (da-13bc): "The pane tile currently still pushes the
// contents of the pane down, just overlay it directly on the frame (please
// center on the frame line)" — and, from the floor, "when dayview has the
// label on the left and right, the minimum is no longer the day height.
// but slightly larger". Measured in screen coordinates, on all four edges
// (the left-hand bar is rotated), never "less than before".
describe('the title on the line', () => {
	// One app for the four edges: `placeBar` walks the tail pane's bar
	// round the frame, which is what a reader does to it (da-x6ea).
	test('the content has no padding and the title is centred on the outline, on every edge', async (t) => {
		await shared.solo(async ({ page }) => {
			for (const edge of ['top', 'bottom', 'left', 'right']) {
				await t.test(`bar on the ${edge}`, async () => {
					if (edge !== 'top') await placeBar(page, 'tail', edge);
					const tail = pane(page, 'tail');
					const seen = await tail.evaluate((section) => {
						const s = getComputedStyle(section.querySelector(':scope > .content'));
						const f = section.getBoundingClientRect();
						const h = section.querySelector('h2').getBoundingClientRect();
						return {
							padding: [s.paddingTop, s.paddingRight, s.paddingBottom, s.paddingLeft],
							// The 1px outline's own middle, on each edge.
							line: { top: f.top + 0.5, bottom: f.bottom - 0.5, left: f.left + 0.5, right: f.right - 0.5 },
							middle: { x: h.left + h.width / 2, y: h.top + h.height / 2 }
						};
					});
					assert.deepEqual(seen.padding, ['0px', '0px', '0px', '0px']);
					const at = edge === 'top' || edge === 'bottom' ? seen.middle.y : seen.middle.x;
					assert.ok(Math.abs(at - seen.line[edge]) <= 1, `the title's middle is at ${at}, the ${edge} line at ${seen.line[edge]}`);
				});
			}
		});
	});

	test("a title on the page's top edge is whole, and the panes start where the page's margin ends", async () => {
		await shared.solo(async ({ page }) => {
			const seen = await page.evaluate(() => {
				// The page's air is `.frame`'s, not `main`'s (da-ckw3): a padded
				// box cannot be shorter than its padding, and `main` has to be
				// able to reach zero when the bar's log is pulled to its tallest.
				// So the margin a pane starts after is measured where it now is.
				const main = document.querySelector('main > .frame');
				const h2 = document.querySelector('section[data-pane="strip"] h2');
				const box = h2.closest('.titlebox');
				const h = h2.getBoundingClientRect();
				return {
					clip: document.querySelector('section.root > .content').getBoundingClientRect().top,
					title: h.top,
					// The title's topmost row of pixels is the title's, not cut
					// away — the button laid over it (da-9vtw) is the title too.
					hit: box.contains(document.elementFromPoint(h.left + h.width / 2, h.top + 1)),
					pane: document.querySelector('section[data-pane="strip"]').getBoundingClientRect().top,
					margin: main.getBoundingClientRect().top + parseFloat(getComputedStyle(main).paddingTop)
				};
			});
			assert.ok(seen.title >= seen.clip, `the title starts at ${seen.title}, the scroller cuts at ${seen.clip}`);
			assert.ok(seen.hit, 'the top of the title is covered');
			assert.equal(seen.pane, seen.margin);
		});
	});

});
