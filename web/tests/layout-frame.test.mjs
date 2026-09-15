// A pane's frame (da-jdop, da-jgrn, da-om9v, da-13bc, da-9vtw): the title
// on the outline — the button that opens the pane's menu — the `+` beside
// it, the row as tall as its words, the gaps, the scrollbars. Driven
// through the built app in a real browser and measured in screen
// coordinates, since the left bar is turned round.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { inWorker, startApp, stopApp, withApp } from './support/app.mjs';
import {
	DESKTOP,
	addTab,
	boxOf,
	chooseBar,
	pane
} from './support/layout.mjs';

before(startApp);
after(stopApp);

// The bar, after vividn's first look (da-jgrn): with one tab no tab strip at
// all — the title sits on the frame's outline and the `+` is a badge right
// after it; with more, the strip, and the badge right after the last tab.
describe('the title on the frame', () => {
	/** Does `inner` straddle the top edge of `outer`? */
	const straddlesTop = (inner, outer) => inner.y < outer.y && inner.y + inner.height > outer.y;

	test('one tab: no tab strip, the title on the outline, the + right after it, on the keyboard', async () => {
		await withApp(async ({ page }) => {
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
		}, DESKTOP);
	});

	test('two tabs: the strip, and the + right after the last tab', async () => {
		await withApp(async ({ page }) => {
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
		}, DESKTOP);
	});

	test('on a vertical bar the badge follows the title along it', async () => {
		await withApp(async ({ page }) => {
			const tail = pane(page, 'tail');
			await chooseBar(page, 'tail', 'right');
			let [h2, more] = await Promise.all([boxOf(tail.locator('h2')), boxOf(tail.locator('button.add'))]);
			let apart = more.y - (h2.y + h2.height);
			assert.ok(apart >= 0 && apart <= 8, `right bar, reading down: the badge is ${apart}px below the title`);
			await chooseBar(page, 'tail', 'left');
			[h2, more] = await Promise.all([boxOf(tail.locator('h2')), boxOf(tail.locator('button.add'))]);
			apart = h2.y - (more.y + more.height);
			assert.ok(apart >= 0 && apart <= 8, `left bar, reading up: the badge is ${apart}px above the title`);
		}, DESKTOP);
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
	const ON_BAR = ':scope > .titlebar :is(h2, [role="tab"], button.add), [role="slider"], .nowview-steps button';

	/** Every piece of `ON_BAR` inside another pane's border box. */
	const bleeds = (page) =>
		page.evaluate((selector) => {
			const panes = [...document.querySelectorAll('section.pane')];
			const rect = (el) => el.getBoundingClientRect();
			const into = (a, b) => a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom;
			return panes.flatMap((p) =>
				[...p.querySelectorAll(selector)]
					.filter((el) => el.getClientRects().length > 0)
					.flatMap((el) =>
						panes
							.filter((q) => q !== p && !q.contains(el) && into(rect(el), rect(q)))
							.map((q) => `${p.dataset.title}'s ${el.tagName.toLowerCase()} "${el.textContent.trim()}" in ${q.dataset.title}`)
					)
			);
		}, ON_BAR);

	for (const [name, viewport] of [
		['1280x800', { width: 1280, height: 800 }],
		['390x844', { width: 390, height: 844 }]
	]) {
		for (const edge of ['top', 'bottom', 'left', 'right']) {
			test(`no title, tab or … lies over a neighbour, every bar on the ${edge}, at ${name}`, async () => {
				await withApp(async ({ page }) => {
					assert.ok((await page.locator('section.pane').count()) >= 3, 'fewer than three panes');
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
					assert.deepEqual(await bleeds(page), []);
				}, { viewport });
			});
		}
	}

	test('the title is its words and a 1px outline in the frame’s colour; its cross padding is nothing on a left bar', async () => {
		await withApp(async ({ page }) => {
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

			await chooseBar(page, 'tail', 'left');
			const left = await seen();
			assert.ok(left.box[0] <= 18, `the left bar's title is ${left.box[0]}px across`);
			for (const side of ['left', 'right']) assert.ok(parseFloat(left.padding[side]) <= 1, `padding-${side} ${left.padding[side]}`);
		}, DESKTOP);
	});

	test('the custom pane’s ✎ and its editor fit inside the row, uncut', async () => {
		await withApp(async ({ page }) => {
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
		}, DESKTOP);
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
	for (const [name, viewport] of [
		['1280', { width: 1280, height: 900 }],
		['390', { width: 390, height: 844 }]
	]) {
		test(`the gap is 8px and the body padding 0.33rem 0.4rem at ${name}`, async () => {
			await withApp(async ({ page }) => {
				const seen = await page.evaluate(() => {
					const border = document.querySelector('.split .border:not(.static)');
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
			}, { viewport });
		});
	}
});

// The scrollbars (da-2a5y): "The built in ones look terrible with the
// design, they should be more very tiny, invisible when not in use, and
// match the frame color." One rule for every scroll container, in app.css.
describe('scrollbars', () => {
	for (const [name, viewport] of [
		['1280', { width: 1280, height: 900 }],
		['390', { width: 390, height: 844 }]
	]) {
		test(`are thin on a pane's body and on the root scroller at ${name}`, async () => {
			await withApp(async ({ page }) => {
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
			}, { viewport });
		});
	}
});

// vividn's third look (da-13bc): "The pane tile currently still pushes the
// contents of the pane down, just overlay it directly on the frame (please
// center on the frame line)" — and, from the floor, "when dayview has the
// label on the left and right, the minimum is no longer the day height.
// but slightly larger". Measured in screen coordinates, on all four edges
// (the left-hand bar is rotated), never "less than before".
describe('the title on the line', () => {
	for (const edge of ['top', 'bottom', 'left', 'right']) {
		test(`bar on the ${edge}: the content has no padding and the title is centred on the outline`, async () => {
			await withApp(async ({ page }) => {
				if (edge !== 'top') await chooseBar(page, 'tail', edge);
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
			}, DESKTOP);
		});
	}

	test("a title on the page's top edge is whole, and the panes start where the page's margin ends", async () => {
		await withApp(async ({ page }) => {
			const seen = await page.evaluate(() => {
				const main = document.querySelector('main');
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
		}, DESKTOP);
	});

});
