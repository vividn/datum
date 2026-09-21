// The layout engine (da-jdop) at the page's root: the default layout, the
// draft, the phone's column and its swipe, holders, the lock — driven
// through the built app in a real browser.
//
// Every assertion here is about what is ON SCREEN — titles, widths, tab
// labels — never "more panes than before": a delta-blind probe passes
// against a page that grew the wrong pane.
//
// The phone cases use Playwright's iPhone 12 profile whole (a touch
// pointer, a device pixel ratio, overlay scrollbars) and drive the swipe
// through `swipe` (support/layout.mjs), a real touch through CDP. The rest
// of the swipe — the motion, the root's layouts, what claims a gesture —
// is `layout-swipe.test.mjs`.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import {
	DESKTOP,
	PHONE,
	activeTab,
	addTab,
	atRest,
	bar,
	boxOf,
	chooseBar,
	draft,
	fill,
	manage,
	manageButton,
	menuButton,
	menuLines,
	overStrip,
	pane,
	settled,
	swipe,
	titles,
	toggleRootLock,
	widgetPane
} from './support/layout.mjs';

before(startApp);
after(stopApp);

describe('the default layout', () => {
	test('shows the strip, the REPL, nowview, tail and the widget bar in panes', async () => {
		await withApp(async ({ page }) => {
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'nowview', 'tail', 'custom', 'fields']);
		}, DESKTOP);
	});

	// The root column's ratios are [0.2, 0.7, 0.1] with the content-sized
	// widget bar holding the 0.1, so the growing cells' flex-grow summed
	// to 0.9 — and flexbox hands out only that fraction of the free
	// space, which left a tenth of it empty under the bar (da-6twf).
	test('fills its column: the last cell reaches the bottom of the split', async () => {
		await withApp(async ({ page }) => {
			const { last, split } = await page.evaluate(() => {
				const el = document.querySelector('[data-split="main"]');
				return {
					last: [...el.querySelectorAll(':scope > .cell')].at(-1).getBoundingClientRect().bottom,
					split: el.getBoundingClientRect().bottom
				};
			});
			assert.ok(Math.abs(split - last) <= 1, `the column ends at ${last}px in a split that ends at ${split}px`);
		}, DESKTOP);
	});

	test('a command through the REPL still reaches tail and nowview', async () => {
		await withApp(async (app) => {
			await app.submit('switch mood happy');
			await app.page.waitForFunction(
				() =>
					document.querySelector('section[data-pane="tail"] .body:not(.behind .body)')?.textContent.includes('mood') &&
					(document.querySelector('section[data-pane="nowview"] svg.svg')?.outerHTML ?? '').includes(
						'data-field="mood"'
					)
			);
		}, DESKTOP);
	});
});

describe('the draft', () => {
	test('is back after a reload, and a layout with no draft is the default', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await manage(page, 'tail', 'split: a new pane to the right');
			await fill(page, 'nowview');
			await chooseBar(page, 'strip', 'left');
			const arranged = await titles(page);
			await app.reload();
			assert.deepEqual(await titles(page), arranged);
			assert.equal(
				await pane(page, 'strip').locator('h2').evaluate((el) => getComputedStyle(el).writingMode),
				'vertical-rl'
			);
		}, DESKTOP);
	});

	test('a blocked localStorage renders the default with no error', async () => {
		await withApp(
			async (app) => {
				assert.deepEqual(await titles(app.page), ['dayview', 'repl', 'nowview', 'tail', 'custom', 'fields']);
				// Nothing remembers an unlock here, so `day` opens locked as a
				// first open does (da-gshh) — and unlocks for the visit.
				await toggleRootLock(app.page);
				await addTab(app.page, 'tail', 'aliases');
				assert.equal(await activeTab(app.page, 'tail'), 'aliases');
				assert.deepEqual(app.errors, []);
			},
			{
				...DESKTOP,
				prepare: (context) =>
					context.addInitScript(() => {
						Object.defineProperty(window, 'localStorage', {
							get() {
								throw new Error('localStorage is blocked');
							}
						});
					})
			}
		);
	});
});

describe('on a phone', () => {
	test('the mobile default is one column with the bar on screen, tail, nowview and custom as tabs', async () => {
		await withApp(async ({ page }) => {
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'tail', 'fields']);
			const boxes = await page.evaluate(() =>
				[...document.querySelectorAll('section.pane')].map((p) => {
					const b = p.getBoundingClientRect();
					return { x: Math.round(b.x), w: Math.round(b.width) };
				})
			);
			assert.equal(new Set(boxes.map((b) => `${b.x}:${b.w}`)).size, 1, `not a column: ${JSON.stringify(boxes)}`);
			assert.deepEqual(await pane(page, 'side').locator('[role="tab"]').allInnerTexts(), ['tail', 'nowview', 'custom']);
			const add = await page.locator('.bar button.add-button').boundingBox();
			const screen = await page.evaluate(() => document.documentElement.clientHeight);
			assert.ok(add.y >= 0 && add.y + add.height <= screen, `the + is off-screen: ${JSON.stringify(add)}`);
		}, PHONE);
	});

	test('a swipe moves between tabs, and never over a scroll', async () => {
		await withApp(async (app) => {
			const { page } = app;
			// Scrolled to the top of the screen first: on a 664px phone the
			// tabbed pane sits mostly behind the pinned widget bar until the
			// column is scrolled, and a finger there is on the bar. And from
			// the middle of the content, not its top edge: the tabs straddle
			// the frame's outline and hang into the content (da-jgrn), and
			// Chromium snaps a touch a few pixels under a tab onto the tab.
			const content = pane(page, 'side').locator('.content');
			await content.evaluate((el) => el.scrollIntoView({ block: 'start' }));
			let box = await content.boundingBox();
			const mid = box.y + box.height / 2;
			// Each swipe is read once the content is back at rest (`atRest`):
			// the swap waits for the slide now (da-kcmp), so the same barrier
			// that shows these two landing is what shows the last one, over a
			// scroll, did not.
			await swipe(page, box.x + box.width * 0.8, mid, -120, 0);
			await atRest(content);
			assert.equal(await activeTab(page, 'side'), 'nowview', 'a swipe left did not move to the next tab');
			// And back, from the tab after it rather than from the nowview:
			// the band declares the sideways gesture its own (`data-pans`,
			// da-kcmp), so a finger drawn across the picture never moves a
			// tab — which `layout-swipe.test.mjs` is where it is asserted.
			await pane(page, 'side').locator('[role="tab"]', { hasText: 'custom' }).click();
			await swipe(page, box.x + box.width * 0.2, mid, 120, 0);
			await atRest(content);
			assert.equal(await activeTab(page, 'side'), 'nowview', 'a swipe right did not move back');

			// A pane whose content scrolls along the bar's axis keeps the
			// gesture. A `tail` of a long-named field is the wide line now:
			// the action echo is drawn as a section that ellipsizes instead
			// of overflowing (da-xfa4), and a table still is a table.
			await app.submit(`occur ${'x'.repeat(60)}`);
			await app.submit('tail');
			await addTab(page, 'repl', 'aliases');
			await pane(page, 'repl').locator('[role="tab"]', { hasText: 'repl' }).click();
			const body = pane(page, 'repl').locator('.body:not(.behind .body)');
			assert.ok(
				await body.evaluate((el) => el.scrollWidth > el.clientWidth),
				'the REPL body does not scroll sideways — this case needs a wider line'
			);
			await body.evaluate((el) => el.scrollIntoView({ block: 'start' }));
			box = await body.boundingBox();
			await swipe(page, box.x + box.width * 0.8, box.y + box.height / 2, -120, 0);
			await atRest(pane(page, 'repl').locator('> .content'));
			assert.equal(await activeTab(page, 'repl'), 'repl', 'a swipe over a sideways scroll changed the tab');
		}, PHONE);
	});
});

// The three review threads folded in (da-jgrn): Copilot on #167 and #163.
describe('review follow-ups', () => {
	test('a malformed `fields --json` does not throw, and the bar keeps its last good listing', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.submit('occur coffee');
			await page.locator('.bar [data-field="coffee"]').waitFor();
			await page.evaluate(() => {
				const previous = window.datum.setRunner((argv, stdin) =>
					argv[0] === 'fields'
						? Promise.resolve({ argv, code: 0, stdout: '{"cut off', stderr: '' })
						: previous(argv, stdin)
				);
			});
			await app.submit('occur tea');
			await app.idle();
			assert.deepEqual(app.errors, []);
			assert.equal(await page.locator('.bar [data-field="coffee"]').count(), 1);
		}, DESKTOP);
	});
});

// vividn's second look (da-2a5y): "There are some container widgets, that
// throw the spacing off, they have their own ... button floating there,
// but aren't really necessary." A pane whose one tab holds a split is a
// box that is not there; with two or more tabs it keeps its strip, and its
// menu — on the tab in front (da-9vtw) — leaves splitting to the children.
describe('holders', () => {
	test('a holder with one tab draws no bar and no menu, beside a widget pane that has both', async () => {
		await withApp(async ({ page }) => {
			const side = page.locator('section.holder[data-pane="side"]');
			assert.equal(await side.count(), 1);
			assert.equal(await side.locator('> .titlebar').count(), 0);
			assert.equal(await side.locator('> .titlebar button.title, > .titlebar button.add').count(), 0);
			// The control: the tail, inside it, has its title and its badge.
			const tail = pane(page, 'tail');
			assert.equal(await tail.locator('> .titlebar h2').count(), 1);
			assert.equal(await tail.locator('> .titlebar button.title').count(), 1);
			assert.equal(await tail.locator('> .titlebar button.add').count(), 1);
			// And the holder's box is its split's box: no row above it.
			const [holder, split] = await Promise.all([boxOf(side), boxOf(side.locator('> .content > .tab-body > .split'))]);
			assert.equal(Math.round(holder.y), Math.round(split.y), 'the split does not start where the holder does');
			assert.equal(Math.round(holder.height), Math.round(split.height));
		}, DESKTOP);
	});

	test('a holder with two tabs keeps its strip, and its menu splits the tab, renames it and saves it as a layout', async () => {
		await withApp(
			async ({ page }) => {
				const holder = page.locator('section.holder[data-pane="tabbed"]');
				assert.equal(await holder.count(), 1);
				assert.deepEqual(await holder.locator('> .titlebar [role="tab"]').allInnerTexts(), ['tail | nowview', 'aliases']);
				// Its children's titles hang into room kept under the strip (da-cesj).
				assert.deepEqual(await overStrip(page, 'tabbed'), { titles: 2, overlaps: [] });
				await menuButton(page, 'tabbed').click();
				// The NAME's slice: the split it holds is nobody's layout
				// until Save as layout… is asked.
				assert.deepEqual(await menuLines(page), ['Change Widget', 'Save as layout…']);
				await page.keyboard.press('Escape');
				await page.locator('[role="menu"]').waitFor({ state: 'detached' });
				// The `▾`'s: in a strip, Split splits the tab in front, one
				// level deeper (da-6ji5), and Break out takes it out of the
				// strip.
				await manageButton(page, 'tabbed').click();
				assert.deepEqual(await menuLines(page), [
					'Rename tab…',
					'Split ← ↓ ↑ →',
					'Break out ← ↓ ↑ →',
					'Bar',
					'× Close tab'
				]);
				await page.keyboard.press('Escape');
			},
			{ ...DESKTOP, prepare: (context) => context.addInitScript(draft, JSON.stringify(tabbedHolder())) }
		);
	});

	// Copilot on #172: a root with two tabs must keep a strip, or nothing
	// could switch them. The shell (da-0a5o) draws the page root's tabs as
	// the root bar, so the page root draws none; a saved layout whose root
	// has two tabs is inlined whole under its root tab, and keeps its strip.
	test("a layout's root with two tabs keeps its strip; the page root's strip is the root bar (Copilot on #172)", async () => {
		await withApp(
			async ({ page }) => {
				assert.equal(await page.locator('section.root > .titlebar').count(), 0, 'the page root drew a strip');
				const inner = pane(page, 'root');
				assert.deepEqual(await inner.locator('> .titlebar [role="tab"]').allInnerTexts(), ['repl', 'tail']);
				await inner.locator('> .titlebar [role="tab"]', { hasText: 'tail' }).click();
				assert.deepEqual(await titles(page), ['tail']);
			},
			{ ...DESKTOP, prepare: (context) => context.addInitScript(draft, TWO_ROOT_TABS) }
		);
	});

	test('the root holder draws no bar: its split starts at the top of the frame', async () => {
		await withApp(async ({ page }) => {
			const root = page.locator('section.root');
			assert.equal(await root.locator('> .titlebar').count(), 0);
			const [frame, split] = await Promise.all([boxOf(root), boxOf(root.locator('> .content > .tab-body > .split'))]);
			assert.equal(Math.round(frame.y), Math.round(split.y));
		}, DESKTOP);
	});
});

// da-9puz. `Tab.layout` is the layout boundary: a tab carrying a name is
// one thing with an identity, where a holder made by splitting is
// scaffolding. vividn: "when a layout is selected for a pane, it should
// still get a frame and a label, right now it doesn't, exploding its
// subwidgets into its space" — a saved layout of several panes arrives as
// a split, which made the tab a holder, so it drew no outline, no
// background and (with one tab) no bar, and its panes landed loose among
// the panes around them.
describe('a tab that holds a layout', () => {
	/** A layout of two panes stacked: what a reader saves and then puts in a pane. */
	const TWO = {
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
						direction: 'column',
						ratios: [0.5, 0.5],
						children: [widgetPane('m-a', 'nowview'), widgetPane('m-b', 'stats')]
					}
				}
			]
		}
	};
	/** And a layout that is one widget, which was always a pane and stays one. */
	const ONE = { version: 1, name: 'one', root: widgetPane('o', 'stats') };

	const save = (app, layout) => app.page.evaluate((l) => window.datum.layouts.saveLayout(l), layout);

	/** Put `name` in a pane split off the tail, and wait for its frame. */
	async function put(app, name) {
		await manage(app.page, 'tail', 'split: a new pane to the right');
		await fill(app.page, name);
		await app.page.locator(`section.pane[data-title="${name}"]`).waitFor();
		await settled(app.page);
	}

	const boxes = (locator) => locator.evaluateAll((all) => all.map((el) => el.getBoundingClientRect()).map((b) => ({ x: b.x, y: b.y, right: b.right, bottom: b.bottom })));

	test('a layout of several panes draws a frame, titled with its name — not «split» — and its panes sit inside it', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await save(app, TWO);
			await put(app, 'mine');

			const mine = page.locator('section.pane[data-title="mine"]');
			assert.equal(await mine.count(), 1, 'the layout drew no frame of its own');
			assert.equal(await page.locator('section.holder[data-title="mine"]').count(), 0, 'the layout is still a holder');
			assert.equal(await mine.locator('> .titlebar h2').innerText(), 'mine');
			// A frame, which is an outline and a ground of its own.
			const drawn = await mine.evaluate((el) => {
				const s = getComputedStyle(el);
				return { border: s.borderTopWidth, ground: s.backgroundColor };
			});
			assert.equal(drawn.border, '1px');
			assert.notEqual(drawn.ground, 'rgba(0, 0, 0, 0)', 'the layout has no ground of its own');

			// Its two panes are inside it, and its title is not lain on by
			// theirs: the inset (the gap all round, the whole row on the
			// bar's edge) is what keeps the two outlines two.
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'nowview', 'tail', 'mine', 'nowview', 'stats', 'custom', 'fields']);
			const [frame] = await boxes(mine);
			const inside = await boxes(mine.locator(':scope > .content > .tab-body > .split > .cell section.pane'));
			assert.equal(inside.length, 2);
			for (const p of inside)
				assert.ok(p.x > frame.x && p.y > frame.y && p.right < frame.right && p.bottom < frame.bottom, `a pane of the layout is not inside its frame: ${JSON.stringify(p)} in ${JSON.stringify(frame)}`);
			const [outer] = await boxes(mine.locator('> .titlebar h2'));
			const [first] = await boxes(mine.locator(':scope > .content h2'));
			assert.ok(outer.bottom <= first.y, `the layout's title (${outer.y}–${outer.bottom}) lies on its first pane's (${first.y}–${first.bottom})`);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	test('a layout of one widget is that widget in a normal frame, wearing the layout name', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await save(app, ONE);
			await put(app, 'one');

			const one = page.locator('section.pane[data-title="one"]');
			assert.equal(await one.locator('.stats').count(), 1);
			// One frame between the reader and the widget, not two: the
			// widget's pane IS the layout's tab (shell.ts lifts it).
			assert.equal(await one.locator('section.pane, section.holder').count(), 0, 'a second frame inside the one-widget layout');
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'nowview', 'tail', 'one', 'custom', 'fields']);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	test('a holder made by splitting draws no frame, and the page root none either, though its tab is the `day` layout', async () => {
		await withApp(
			async ({ page }) => {
				// The scaffolding case, unchanged: a pane one of whose tabs is
				// a split that is nobody's layout, beside a widget tab.
				const tail = page.locator('section.holder[data-pane="tabbed"]');
				assert.equal(await tail.count(), 1, 'a holder made by splitting drew a frame');
				assert.deepEqual(await tail.locator('> .titlebar [role="tab"]').allInnerTexts(), ['tail | nowview', 'aliases']);

				// And the root, whose tab carries `day`, is still no box at all:
				// the shell draws its name and its tabs (da-0a5o).
				const root = page.locator('section.root');
				assert.equal(await root.evaluate((el) => el.classList.contains('holder')), true);
				assert.equal(await root.evaluate((el) => getComputedStyle(el).borderTopWidth), '0px');
				assert.equal(await root.locator('> .titlebar').count(), 0);
			},
			{ ...DESKTOP, prepare: (context) => context.addInitScript(draft, JSON.stringify(tabbedHolder())) }
		);
	});

	test('on a 320px phone a layout in a pane still fits inside its frame', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await save(app, TWO);
			await manage(page, 'repl', 'split: a new pane below');
			await fill(page, 'mine');
			await page.locator('section.pane[data-title="mine"]').waitFor();
			await settled(page);
			const mine = page.locator('section.pane[data-title="mine"]');
			const [frame] = await boxes(mine);
			const inside = await boxes(mine.locator(':scope > .content > .tab-body > .split > .cell section.pane'));
			assert.equal(inside.length, 2);
			for (const p of inside)
				assert.ok(p.x >= frame.x && p.right <= frame.right, `a pane of the layout runs out of its frame: ${JSON.stringify(p)} in ${JSON.stringify(frame)}`);
			assert.deepEqual(app.errors, []);
		}, { viewport: { width: 320, height: 640 } });
	});
});

// The lock (da-2a5y): "Layouts should have the ability to be locked,
// which prevents the resizing of panes […] no splitting, new tabs, or
// resizing." Where the lock is toggled from is the shell's; here it
// arrives as this device's lock on the `day` tab (per device, never in
// the tree — da-0a5o), and covers the whole tree.
describe('the lock', () => {

	/** The cells of the row split, by width. */
	const cellWidths = (page) =>
		page.evaluate(() => [...document.querySelectorAll('.split.row > .cell')].map((c) => c.getBoundingClientRect().width));

	test('a locked root: no separators, no …, a drag and an arrow move nothing, the tabs still switch', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				assert.deepEqual(await titles(page), ['repl', 'tail']);
				assert.equal(await page.locator('[role="separator"]').count(), 0);
				// The root bar's `+` and `…` (da-0a5o) are above the layout, not under its lock.
				assert.equal(await page.locator('section.root button.add').count(), 0);
				// A menu is left only where a widget has options (da-gvdc): the
				// repl's, which reads `Options…` and nothing else. The tabbed
				// pane shows `tail`, which has none.
				assert.equal(await page.locator('section.root button.title').count(), 1);
				await pane(page, 'repl').locator('button.title').click();
				assert.deepEqual(await menuLines(page), ['Options…']);
				await page.keyboard.press('Escape');
				await page.locator('[role="menu"]').waitFor({ state: 'detached' });
				// The gap is still there, as a static border, and dragging across it moves nothing.
				const gap = page.locator('.split.row > .border.static');
				assert.equal(await gap.count(), 1);
				assert.equal(await gap.evaluate((el) => getComputedStyle(el).cursor), 'default');
				const was = await cellWidths(page);
				const box = await gap.boundingBox();
				const x = box.x + box.width / 2;
				const y = box.y + box.height / 2;
				await page.mouse.move(x, y);
				await page.mouse.down();
				for (let i = 1; i <= 5; i++) await page.mouse.move(x + i * 30, y);
				await page.mouse.up();
				await settled(page);
				assert.deepEqual(await cellWidths(page), was, 'a drag across a locked border moved it');
				await page.evaluate(() => document.body.focus());
				await page.keyboard.press('ArrowRight');
				await settled(page);
				assert.deepEqual(await cellWidths(page), was, 'an arrow moved a locked border');
				// Tabs are not an edit: the tabbed pane still switches.
				await pane(page, 'tabbed').locator('[role="tab"]', { hasText: 'nowview' }).click();
				assert.equal(await activeTab(page, 'tabbed'), 'nowview');
				assert.deepEqual(app.errors, []);

				// Unlocked, the controls are back — the one border, and on each
				// pane a menu and a `+` (the root has none of its own): the
				// title where there is one tab, the tab in front where there
				// are two.
				await page.evaluate(() => localStorage.removeItem('datum.layout.locked.v1.day'));
				await app.reload();
				assert.equal(await page.locator('.split.row > [role="separator"]').count(), 1);
				assert.equal(await pane(page, 'repl').locator('button.title').count(), 1);
				assert.equal(
					await pane(page, 'tabbed').locator('[role="tab"][aria-selected="true"][aria-haspopup="menu"]').count(),
					1
				);
				assert.equal(await page.locator('section.root button.add').count(), 2);
			},
			// The init script runs on every load, the reload included: it
			// writes the draft and the lock once and leaves the unlocked one alone.
			{ ...DESKTOP, prepare: (context) => context.addInitScript(lockedOnce, JSON.stringify(lockable())) }
		);
	});
});

/** A root holder over [repl | a pane whose two tabs are a SPLIT and a widget]. */
function tabbedHolder() {
	return {
		version: 1,
		name: 'day',
		root: {
			id: 'root',
			tabs: [
				{
					id: 'root-tab',
					content: {
						id: 'row',
						direction: 'row',
						ratios: [0.5, 0.5],
						children: [
							widgetPane('repl', 'repl'),
							{
								id: 'tabbed',
								tabs: [
									{
										id: 'tabbed-split',
										content: {
											id: 'inner',
											direction: 'row',
											ratios: [0.5, 0.5],
											children: [widgetPane('inner-tail', 'tail'), widgetPane('inner-nowview', 'nowview')]
										}
									},
									{ id: 'tabbed-aliases', content: { id: 'tabbed-aliases-widget', kind: 'aliases', settings: {} } }
								],
								active: 0,
								bar
							}
						]
					}
				}
			],
			active: 0,
			bar
		}
	};
}

/** A root holder over [repl | a tabbed tail/nowview pane]. */
function lockable() {
	const tab = {
		id: 'root-tab',
		content: {
			id: 'row',
			direction: 'row',
			ratios: [0.5, 0.5],
			children: [
				widgetPane('repl', 'repl'),
				{
					id: 'tabbed',
					tabs: [
						{ id: 'tabbed-tail', content: { id: 'tabbed-tail-widget', kind: 'tail', settings: {} } },
						{ id: 'tabbed-nowview', content: { id: 'tabbed-nowview-widget', kind: 'nowview', settings: {} } }
					],
					active: 0,
					bar
				}
			]
		}
	};
	return { version: 1, name: 'day', root: { id: 'root', tabs: [tab], active: 0, bar } };
}

/** A root pane with two widget tabs, the REPL and the tail. */
const TWO_ROOT_TABS = JSON.stringify({
	version: 1,
	name: 'default',
	root: {
		id: 'root',
		tabs: [
			{ id: 'root-repl', content: { id: 'root-repl-widget', kind: 'repl', settings: {} } },
			{ id: 'root-tail', content: { id: 'root-tail-widget', kind: 'tail', settings: {} } }
		],
		active: 0,
		bar
	}
});

function lockedOnce(json) {
	if (localStorage.getItem('datum.layout.draft.v1.day')) return;
	localStorage.setItem('datum.layout.draft.v1.day', json);
	localStorage.setItem('datum.layout.locked.v1.day', '1');
}
