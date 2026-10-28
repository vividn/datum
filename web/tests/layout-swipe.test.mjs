// The swipe, whole (da-kcmp): whose a gesture is, and what it looks like.
//
// vividn, on the deployed build: "let's add smooth swipe animations when
// swiping between tabs, and also allow swiping between tabs at the root
// level (so between layouts). Some widgets allow scrolling horizontally or
// vertically, so they should get precedence, but when they reach the end
// then allow swiping. Or always allow swiping when swiping in the margins
// or along the frame."
//
// So there are four claims to hold, and each is a test below: the ROOT
// swipes between layouts; a SCROLLER keeps the gesture only while it can
// still go the finger's way; the INNERMOST frame that can move takes it
// and a frame on its last tab lets it through; and the content MOVES with
// the finger and slides the rest of the way on release.
//
// Every case is a real touch through CDP (`swipe`, support/layout.mjs) on
// Playwright's iPhone 12 profile, and every one that swipes over a widget
// pins that widget's scroll first (`scrollX`): the rule under test is
// about where a scroller IS, so a test that left it wherever the widget
// happened to draw it would be asserting about the widget.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, withApp } from './support/app.mjs';
import {
	PHONE,
	activeTab,
	addTab,
	atRest,
	bar,
	placeBar,
	fill,
	manage,
	maximizeButton,
	pane,
	pastTheWindow,
	scrollX,
	settled,
	swipeOver,
	titles,
	widgetPane
} from './support/layout.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

// ONE phone app for the tests that start from `main` alone or from a draft
// of it (da-6b3dj, sharedApp in support/app.mjs): `solo` puts `main` back
// after a swipe moved its tabs. What still opens an app of its own starts
// with a second layout open (`twoLayouts`, which `solo` would close), or on
// another screen or with reduced motion.
const phone = sharedApp(PHONE);

/** This device's open layouts: `main` in front, the built-in `tail` behind it. */
const twoLayouts = (context) =>
	context.addInitScript(
		(json) => localStorage.setItem('datum.layout.root.v1', json),
		JSON.stringify({ names: ['main', 'tail'], active: 0 })
	);

/**
 * The layout in front, as the root bar says it — less the orange dot it
 * wears while the copy differs from its save, which a swipe between tabs
 * is enough to earn.
 */
const shown = async (page) =>
	(await page.locator('.root-bar [role="tab"][aria-selected="true"]').innerText()).replace(/\s*•\s*$/, '');

/**
 * Report every transform the strip in the content box `locator` takes
 * from now on.
 *
 * The motion is over in 150ms, so a test that reads the transform after
 * the swipe is a test of how fast the runner is. The box is watched
 * instead — its tab in front takes an inline transform while the box is
 * `moving` (da-24fgb, da-7etnf), and none otherwise — and what it took is
 * read once the tab has landed.
 */
const watch = (locator) =>
	locator.evaluate((el) => {
		window.__transforms = [];
		window.__boxStyles = new Set([el.style.cssText]);
		const front = () => el.querySelector(':scope > .tab-body:not(.behind)')?.style.transform ?? '';
		new MutationObserver(() => {
			window.__transforms.push(el.classList.contains('moving') ? front() : '');
			window.__boxStyles.add(el.style.cssText);
		}).observe(el, {
			attributes: true,
			attributeFilter: ['style', 'class'],
			subtree: true
		});
	});
const moved = async (page) => (await page.evaluate(() => window.__transforms ?? [])).filter(Boolean);

/**
 * The CLI widget, scrolling sideways: a `tail` of a sixty-character
 * field name printed into it.
 *
 * The finger has to start over a scroller for this to be a test of
 * anything. A widget with nothing to scroll was always the root's — it
 * is a widget whose body is WIDER than its box that made the root's
 * swipe dead on a phone, since every pane's body scrolls and a picture
 * arriving is briefly wider than the box it is drawn for.
 */
async function wideRepl(app) {
	await app.submit(`occur ${'x'.repeat(60)}`);
	await app.submit('tail');
	const body = pane(app.page, 'repl').locator('.body:not(.behind .body)');
	assert.ok(
		await body.evaluate((el) => el.scrollWidth > el.clientWidth),
		'the CLI widget does not scroll sideways — this case needs a wider line'
	);
	return body;
}

describe('the root swipes between layouts', () => {
	test('a swipe over the page moves to the next layout, and back', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				assert.deepEqual(await page.locator('.root-bar [role="tab"]').allInnerTexts(), ['main', 'tail']);
				// Over a widget, not the page's margin: the pane under the
				// finger has one tab and no swipe of its own, so the gesture is
				// the root's — and its body is at its end, so it has nowhere to
				// take the finger and yields.
				const body = await wideRepl(app);
				await scrollX(body, true);
				await swipeOver(page, body, -120, 0);
				await until(() => shown(page), (name) => name === 'tail');
				assert.deepEqual(await titles(page), ['tail'], 'the second layout is not what is on screen');

				const back = page.locator('section.pane .body:not(.behind .body)').first();
				await scrollX(back, false);
				await swipeOver(page, back, 120, 0);
				await until(() => shown(page), (name) => name === 'main');
				assert.deepEqual(await titles(page), ['dayview', 'cli', 'tail', 'fields']);

				// And the page it came back to does not hang off its own edge:
				// the root's content is the page's scroller, and a box moved
				// across it would leave it scrolled sideways.
				assert.deepEqual(
					await page.locator('section.root > .content').evaluate((el) => [el.scrollLeft, el.scrollWidth - el.clientWidth]),
					[0, 0]
				);
			},
			{ ...PHONE, prepare: twoLayouts }
		);
	});

	test('with reduced motion the layout still changes, and nothing is ever moved', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				const body = await wideRepl(app);
				await watch(page.locator('section.root > .content'));
				await scrollX(body, true);
				await swipeOver(page, body, -120, 0);
				await until(() => shown(page), (name) => name === 'tail');
				assert.deepEqual(await moved(page), [], 'the content was transformed for a reader who asked for no motion');
			},
			{ ...PHONE, reducedMotion: 'reduce', prepare: twoLayouts }
		);
	});
});

describe('a scroller keeps the gesture until its end', () => {
	/**
	 * A column of [repl | a pane whose middle tab is a CLI widget of its own].
	 *
	 * The CLI widget is the wide line: the tail's table wraps a long field name
	 * rather than overflowing (`overflow-wrap: anywhere`, TailTable.svelte),
	 * where a `tail` PRINTED into the scrollback is as wide as it is.
	 */
	const threeTabs = () => ({
		version: 1,
		name: 'main',
		root: {
			id: 'root',
			active: 0,
			bar,
			tabs: [
				{
					id: 'root-tab',
					content: {
						id: 'col',
						direction: 'column',
						ratios: [0.5, 0.5],
						children: [
							widgetPane('tail', 'tail'),
							{
								id: 'wide',
								active: 1,
								bar,
								tabs: [
									{ id: 'wide-aliases', content: { id: 'wide-aliases-w', kind: 'aliases', settings: {} } },
									{ id: 'wide-repl', content: { id: 'wide-repl-w', kind: 'cli', settings: {} } },
									{ id: 'wide-custom', content: { id: 'wide-custom-w', kind: 'custom', settings: {} } }
								]
							}
						]
					}
				}
			]
		}
	});

	test('at its start it takes a swipe left and yields one right; at its end it yields both', async () => {
		await phone.solo(
			async (app) => {
				const { page } = app;
				// A field whose name is sixty characters, listed: the line the
				// pane has to scroll to show.
				await app.submit(`occur ${'x'.repeat(60)}`);
				await app.submit('tail');
				const body = pane(page, 'wide').locator('.body:not(.behind .body)');
				assert.ok(
					await body.evaluate((el) => el.scrollWidth > el.clientWidth),
					'the CLI widget does not scroll sideways — this case needs a wider line'
				);

				// At its start, a finger going left has somewhere to take it.
				await scrollX(body, false);
				await swipeOver(page, body, -120, 0, { x: 0.8, y: 0.5 });
				await atRest(pane(page, 'wide').locator('> .content'));
				assert.equal(await activeTab(page, 'wide'), 'cli', 'a swipe over a scroller with room changed the tab');

				// The other way it has not: the line is already at its start,
				// so the frame takes the gesture and the tab before it arrives.
				await scrollX(body, false);
				await swipeOver(page, body, 120, 0, { x: 0.2, y: 0.5 });
				await until(() => activeTab(page, 'wide'), (t) => t === 'aliases');

				// And at its end, a swipe left is the frame's too.
				await pane(page, 'wide').locator('[role="tab"]', { hasText: 'cli' }).click();
				await scrollX(body, true);
				await swipeOver(page, body, -120, 0, { x: 0.8, y: 0.5 });
				await until(() => activeTab(page, 'wide'), (t) => t === 'custom');
				assert.deepEqual(app.errors, []);
			},
			JSON.stringify(threeTabs())
		);
	});
});

describe('the innermost frame that can move takes the gesture', () => {
	test("a tabbed pane moves its own tab; on its last it lets the root move the layout", async () => {
		await withApp(
			async ({ page }) => {
				const body = pane(page, 'side').locator('.body:not(.behind .body)');
				await scrollX(body, true);
				await swipeOver(page, body, -120, 0);
				await until(() => activeTab(page, 'side'), (t) => t === 'nowview');
				assert.equal(await shown(page), 'main', 'the swipe moved the layout as well as the tab');

				// On the pane's LAST tab there is nowhere for it to go, so the
				// same swipe reaches the root and moves the whole layout.
				await pane(page, 'side').locator('[role="tab"]', { hasText: 'custom' }).click();
				const last = pane(page, 'side').locator('.body:not(.behind .body)');
				await scrollX(last, true);
				await swipeOver(page, last, -120, 0);
				await until(() => shown(page), (name) => name === 'tail');

				// And the pane it left is as it was: the root moved, not it.
				const back = page.locator('section.pane .body:not(.behind .body)').first();
				await scrollX(back, false);
				await swipeOver(page, back, 120, 0);
				await until(() => shown(page), (name) => name === 'main');
				assert.equal(await activeTab(page, 'side'), 'custom');
			},
			{ ...PHONE, prepare: twoLayouts }
		);
	});
});

describe('the frame always swipes', () => {
	test('a touch on a nested pane’s inset moves its tab, over a scroller mid-scroll', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				// A layout of two panes, put in a tab of its own beside another:
				// the frame it draws insets its panes from it (da-9puz), and
				// that inset is the margin vividn's rule is about.
				await page.evaluate((l) => window.datum.layouts.saveLayout(l), {
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
									children: [widgetPane('m-tail', 'tail'), widgetPane('m-aliases', 'aliases')]
								}
							}
						]
					}
				});
				await app.submit(`occur ${'x'.repeat(60)}`);
				await manage(page, 'repl', 'split: a new pane below');
				await fill(page, 'mine');
				const frame = page.locator('section.pane[data-title="mine"]');
				await frame.waitFor();
				const id = await frame.getAttribute('data-pane');
				await addTab(page, id, 'aliases');
				await pane(page, id).locator('[role="tab"]', { hasText: 'mine' }).click();
				await settled(page);

				// The widget inside is a scroller, and mid-scroll: it would keep
				// a gesture that started over it. The finger does not.
				const inner = pane(page, 'm-tail').locator('.body:not(.behind .body)');
				await inner.evaluate((el) => (el.scrollLeft = Math.floor(el.scrollWidth / 2)));
				const content = pane(page, id).locator('> .content');
				await swipeOver(page, content, -120, 0, { x: 0.01, y: 0.5 });
				await until(() => activeTab(page, id), (t) => t === 'aliases');
				assert.deepEqual(app.errors, []);
			},
			{ ...PHONE, viewport: { width: 500, height: 900 }, prepare: twoLayouts }
		);
	});
});

describe('the motion', () => {
	test('the content follows the finger, slides on, and comes back to rest', async () => {
		await phone.solo(async ({ page }) => {
			const content = pane(page, 'side').locator('> .content');
			const body = pane(page, 'side').locator('.body:not(.behind .body)');
			const rows = await pane(page, 'side').locator('tbody tr').count();
			await scrollX(body, true);
			await watch(content);
			await swipeOver(page, body, -120, 0);
			await until(() => activeTab(page, 'side'), (t) => t === 'nowview');
			const seen = await moved(page);
			assert.ok(seen.length > 1, `the content never moved: ${JSON.stringify(seen)}`);
			assert.ok(
				seen.some((t) => /translateX\(-\d/.test(t)),
				`nothing followed the finger the way it went: ${JSON.stringify(seen)}`
			);
			// Moved on the tab bodies alone: a style on the content box is
			// inherited by both layouts, and restyled all of them on every
			// move of the finger (da-7etnf).
			const box = await page.evaluate(() => [...window.__boxStyles]);
			assert.equal(box.length, 1, `the content box's own style changed as it moved: ${JSON.stringify(box)}`);
			await atRest(content);
			// And the tail behind it counted its rows in the same box it had:
			// a transform moves what is drawn, never what is laid out, and the
			// tail asks for as many rows as its box holds (da-zfkx).
			await pane(page, 'side').locator('[role="tab"]', { hasText: 'tail' }).click();
			await until(() => pane(page, 'side').locator('tbody tr').count(), (n) => n === rows);
		});
	});

	test('a swipe too short moves nothing, and the tab does not move', async () => {
		await phone.solo(async ({ page }) => {
			const content = pane(page, 'side').locator('> .content');
			const body = pane(page, 'side').locator('.body:not(.behind .body)');
			await scrollX(body, true);
			await watch(content);
			// Past COMMIT_PX, so the frame claims it; short of SWIPE_PX, so it
			// is a touch and the strip stays where it is (da-24fgb).
			await swipeOver(page, body, -35, 0);
			await atRest(content);
			assert.deepEqual(await moved(page), [], 'a drag shorter than a swipe moved the strip');
			assert.equal(await activeTab(page, 'side'), 'tail');
			// The control: the same drag a little longer does move it.
			await swipeOver(page, body, -60, 0);
			await until(() => activeTab(page, 'side'), (t) => t === 'nowview');
			assert.ok((await moved(page)).length > 0, 'a swipe past SWIPE_PX never moved the strip');
		});
	});
});

describe('the strip: never a blank between two tabs', () => {
	// vividn, 2026-09-23: "swiping between tabs or widgets still has an
	// awkward nomans land between them, the swiping should go immediately
	// between the two widgets without the blank space" (da-24fgb). The claim
	// is about FRAMES, so it is read from frames: a sampler on
	// requestAnimationFrame measures, every frame the box draws, how much of
	// its extent along the swipe nothing visible covers. A synthetic drag's
	// moves arrive in one burst and draw nothing between them, so the drag
	// here waits two frames after every move, and the test asserts that
	// mid-motion frames WERE drawn before it believes any of them.

	/** Start sampling `box`: the gap along `axis` that the visible `drawn` leave. */
	const sample = (box, drawn, axis) =>
		box.evaluate(
			(el, [drawn, axis]) => {
				const frames = (window.__frames = []);
				const span = (r) => (axis === 'x' ? [r.left, r.right] : [r.top, r.bottom]);
				// How far a box is drawn from where it is laid out.
				const shift = (d) => {
					const m = new DOMMatrix(d ? getComputedStyle(d).transform : 'none');
					return axis === 'x' ? m.m41 : m.m42;
				};
				const tick = () => {
					if (window.__frames !== frames) return;
					// The window is where the box is LAID OUT: the box itself
					// may be what moves (it was, before the strip).
					const [lo, hi] = span(el.getBoundingClientRect()).map((v) => v - shift(el));
					const spans = [...el.querySelectorAll(drawn)]
						.filter((d) => getComputedStyle(d).visibility === 'visible' && !d.closest('.tab-body.kept.behind:not(.beside)'))
						.map((d) => span(d.getBoundingClientRect()))
						.map(([a, b]) => [Math.max(a, lo), Math.min(b, hi)])
						.filter(([a, b]) => b > a)
						.sort((p, q) => p[0] - q[0]);
					let covered = 0;
					let reach = lo;
					for (const [a, b] of spans) {
						if (b > reach) covered += b - Math.max(a, reach);
						reach = Math.max(reach, b);
					}
					// How far the tab in front is drawn from home, whichever box
					// carries the transform: the strip's, or the box's own.
					const at = shift(el) + shift(el.querySelector(':scope > .tab-body:not(.behind)'));
					frames.push({ moving: el.classList.contains('moving'), at, gap: hi - lo - covered });
					requestAnimationFrame(tick);
				};
				requestAnimationFrame(tick);
			},
			[drawn, axis]
		);
	const sampled = (page) =>
		page.evaluate(() => {
			const frames = window.__frames;
			window.__frames = null;
			return frames;
		});

	/** A finger that lets the page draw a frame after every move. */
	async function drag(page, locator, dx, dy) {
		await locator.evaluate((el) => el.scrollIntoView({ block: 'nearest' }));
		const box = await locator.boundingBox();
		const x = box.x + box.width / 2;
		const y = box.y + box.height / 2;
		const frame = () => page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
		const cdp = await page.context().newCDPSession(page);
		await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x, y }] });
		for (let i = 1; i <= 12; i++) {
			await cdp.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: [{ x: x + (dx * i) / 12, y: y + (dy * i) / 12 }] });
			await frame();
		}
		await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
		await cdp.detach();
	}

	/**
	 * Swipe over `over` by (dx, dy) until `landed`, and hold every frame of
	 * the content box `content` to the rest's gap: while the strip moves,
	 * nothing is uncovered that was not uncovered with one tab standing
	 * still.
	 */
	async function unbroken(page, content, drawn, label, landed, dx, dy, over) {
		await sample(content, drawn, dx ? 'x' : 'y');
		await drag(page, over, dx, dy);
		await until(landed, Boolean);
		await atRest(content);
		await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
		const frames = await sampled(page);
		const rest = Math.max(...frames.filter((f) => !f.moving).map((f) => f.gap));
		const moving = frames.filter((f) => f.moving && Math.abs(f.at) >= 20);
		assert.ok(moving.length >= 3, `${label}: too few frames drawn mid-motion to judge: ${JSON.stringify(frames)}`);
		const blank = frames.filter((f) => f.moving && f.gap > rest + 1);
		assert.deepEqual(blank, [], `${label}: frames with a blank the tabs at rest do not leave (${rest}px)`);
	}

	/** The side pane's body in front: the finger's start. */
	const front = (page) => pane(page, 'side').locator('.body:not(.behind .body)');
	/** Swipe the side pane from tab `from` and wait for `to`. */
	const side = (page, from, to, dx, dy) =>
		unbroken(
			page,
			pane(page, 'side').locator('> .content'),
			'.body',
			`${from} → ${to}`,
			async () => (await activeTab(page, 'side')) === to,
			dx,
			dy,
			front(page)
		);

	test('side to side, both ways: some tab covers the box in every frame', async () => {
		await phone.solo(async ({ page }) => {
			await scrollX(front(page), true);
			await side(page, 'tail', 'nowview', -150, 0);
			await pane(page, 'side').locator('[role="tab"]', { hasText: 'custom' }).click();
			await scrollX(front(page), false);
			await side(page, 'custom', 'nowview', 150, 0);
		});
	});

	test('up and down, under a bar on the left: the same', async () => {
		await phone.solo(async ({ page }) => {
			await placeBar(page, 'side', 'left');
			await side(page, 'tail', 'nowview', 0, -150);
			await pane(page, 'side').locator('[role="tab"]', { hasText: 'custom' }).click();
			await side(page, 'custom', 'nowview', 0, 150);
		});
	});

	test('the root brings the next layout on beside the one it leaves', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				// The next layout is not mounted until a gesture brings it on.
				assert.deepEqual(await titles(page), ['dayview', 'cli', 'tail', 'fields']);
				const body = await wideRepl(app);
				await scrollX(body, true);
				await unbroken(
					page,
					page.locator('section.root > .content'),
					'.body',
					'main → tail',
					async () => (await shown(page)) === 'tail',
					-150,
					0,
					body
				);
				// Kept, clipped and refusing (da-tajsm, da-p77nk): a return shows it
				// drawn, and nothing in it is seen, pressed, focused or read meanwhile.
				const left = page.locator('section.root > .content > .tab-body.behind');
				assert.deepEqual(
					await left.evaluateAll((els) =>
						els.map((el) => {
							const button = el.querySelector('button');
							button?.focus();
							const box = el.getBoundingClientRect();
							const hit = document.elementFromPoint(box.x + box.width / 2, box.y + box.height / 2);
							return {
								clip: getComputedStyle(el).clipPath,
								hidden: el.getAttribute('aria-hidden'),
								focused: button !== null && document.activeElement === button,
								hit: el.contains(hit),
								titles: [...el.querySelectorAll('section.pane')].map((p) => p.dataset.title)
							};
						})
					),
					[{ clip: 'inset(50%)', hidden: 'true', focused: false, hit: false, titles: ['dayview', 'cli', 'tail', 'fields'] }],
					'the layout left behind is kept, clipped, and takes no focus or press'
				);
			},
			{ ...PHONE, prepare: twoLayouts }
		);
	});

	test('a return to a layout once opened asks the CLI nothing and builds nothing (da-tajsm)', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				const root = async (name) => {
					await page.locator('.root-bar [role="tab"]', { hasText: name }).click();
					await until(() => shown(page), (shownName) => shownName === name);
					await app.idle();
				};
				await root('tail');
				await root('main');
				const panes = page.locator('section.root > .content > .tab-body:not(.behind) section.pane');
				await panes.evaluateAll((els) => els.forEach((el) => (el.dataset.kept = 'yes')));
				const seen = await app.spyOnRunner();
				await root('tail');
				await root('main');
				assert.deepEqual(await seen(), [], 'a switch with nothing changed ran a command');
				assert.deepEqual(
					await panes.evaluateAll((els) => els.map((el) => el.dataset.kept ?? 'new')),
					['yes', 'yes', 'yes', 'yes'],
					'the layout came back built anew'
				);
			},
			{ ...PHONE, prepare: twoLayouts }
		);
	});

	// da-p77nk: an inherited change reaching a dayview's SVG has Chrome lay
	// it out again, so a switch that flipped `visibility` and `inert` on the
	// layout going behind and the one coming on laid out the whole of `main`
	// twice (63–80 ms on a phone at 4×, from 4 dirty boxes). A layout behind
	// is skipped instead, and everything in it keeps what it inherited in front.
	test('a root switch changes nothing any element of a layout inherits (da-p77nk)', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				const root = async (name) => {
					await page.locator('.root-bar [role="tab"]', { hasText: name }).click();
					await until(() => shown(page), (shownName) => shownName === name);
					await app.idle();
				};
				await root('tail');
				await root('main');
				const mainBody = page.locator('section.root > .content > .tab-body:not(.behind)');
				await mainBody.evaluate((el) => (el.dataset.probe = 'main'));
				const inherited = () =>
					page.locator('section.root > .content > .tab-body[data-probe="main"]').evaluate((el) =>
						[...el.querySelectorAll('*')].map((d) => {
							const style = getComputedStyle(d);
							return `${style.visibility} ${style.pointerEvents} ${d.closest('[inert]') !== null}`;
						})
					);
				const front = await inherited();
				assert.ok(front.length > 100, `too few elements in main to judge: ${front.length}`);
				await root('tail');
				const behind = await inherited();
				const changed = behind.filter((v, i) => v !== front[i]);
				assert.deepEqual([behind.length, changed.length, changed.slice(0, 3)], [front.length, 0, []], 'main behind inherits otherwise than in front');
				// What `inert` kept out, the keyboard included: Tab and Shift+Tab
				// pass over main, and still go everywhere else.
				const stops = [];
				for (const key of [...Array(40).fill('Tab'), ...Array(40).fill('Shift+Tab')]) {
					await page.keyboard.press(key);
					stops.push(
						await page.evaluate(() =>
							document.activeElement?.closest('[data-probe="main"]') ? 'main' : (document.activeElement?.tagName ?? 'none')
						)
					);
				}
				assert.ok(!stops.includes('main'), `a key stopped in main behind: ${stops.join(' ')}`);
				assert.ok(new Set(stops).size > 2, `the keys went nowhere: ${stops.join(' ')}`);
				await root('main');
				assert.deepEqual(await inherited(), front, 'main back in front inherits otherwise than before');
				// And focus inside main as it goes behind by a switch that moves no
				// focus (a swipe, a script) is let go, as `inert` let it go.
				const held = await page.evaluate(() => {
					const button = document.querySelector('[data-probe="main"] button');
					button?.focus();
					return button !== null && document.activeElement === button;
				});
				assert.ok(held, 'no control in main took focus to begin with');
				await page.evaluate(() =>
					[...document.querySelectorAll('.root-bar [role="tab"]')].find((t) => t.textContent.includes('tail')).click()
				);
				await until(() => shown(page), (shownName) => shownName === 'tail');
				assert.equal(
					await page.evaluate(() => document.activeElement?.closest('[data-probe="main"]') !== null && document.activeElement !== null),
					false,
					'focus stayed in main behind'
				);
			},
			{ ...PHONE, prepare: twoLayouts }
		);
	});

	// da-obvkq: a class toggled over a `> *` rule has Chrome restyle every
	// element under the box that wears it, and `grown` went on `main` at
	// every switch to a single-pane root: about 50 ms of each on a phone at
	// 4×, booked as Layout. Chrome's trace names such an invalidation.
	test('a root switch restyles no layout whole (da-obvkq)', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				const root = async (name) => {
					await page.locator('.root-bar [role="tab"]', { hasText: name }).click();
					await until(() => shown(page), (shownName) => shownName === name);
					await app.idle();
				};
				await root('tail');
				await root('main');
				const cdp = await page.context().newCDPSession(page);
				const events = [];
				cdp.on('Tracing.dataCollected', (e) => events.push(...e.value));
				await cdp.send('Tracing.start', {
					categories: 'disabled-by-default-devtools.timeline.invalidationTracking',
					transferMode: 'ReportEvents'
				});
				await root('tail');
				await root('main');
				const done = new Promise((resolve) => cdp.once('Tracing.tracingComplete', resolve));
				await cdp.send('Tracing.end');
				await done;
				await cdp.detach();
				const invalidations = events.filter((e) => e.name === 'StyleInvalidatorInvalidationTracking').map((e) => e.args.data);
				assert.ok(invalidations.length > 0, 'the trace held no style invalidation at all');
				const whole = invalidations.filter((d) => /subtree/.test(d.reason) && /tab-body/.test(d.nodeName));
				assert.deepEqual(whole.map((d) => d.nodeName), [], 'a switch restyled everything under a tab');
			},
			{ ...PHONE, prepare: twoLayouts }
		);
	});

	// da-1acrc: the tab in front was `display: contents`, so going behind
	// built the layout's boxes again. As a box it must still let the page
	// scroll to the split's far edge and the content's padding past it,
	// which an absolute box in front lost (638 → 630 at 560×420).
	test('the layout in front is a box, and the page scrolls past its split by the padding (da-1acrc)', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await page.setViewportSize({ width: 300, height: 300 });
				for (const name of ['tail', 'main']) {
					await page.locator('.root-bar [role="tab"]', { hasText: name }).click();
					await until(() => shown(page), (shownName) => shownName === name);
				}
				await app.idle();
				const seen = await page.evaluate(() => {
					const content = document.querySelector('section.root > .content');
					const front = content.querySelector(':scope > .tab-body:not(.behind)');
					const split = front.querySelector(':scope > .split').getBoundingClientRect();
					const box = content.getBoundingClientRect();
					const pad = getComputedStyle(content);
					return {
						display: getComputedStyle(front).display,
						client: [content.clientWidth, content.clientHeight],
						scroll: [content.scrollWidth, content.scrollHeight],
						reach: [
							split.right - box.left - content.clientLeft + content.scrollLeft + parseFloat(pad.paddingRight),
							split.bottom - box.top - content.clientTop + content.scrollTop + parseFloat(pad.paddingBottom)
						]
					};
				});
				assert.equal(seen.display, 'flex', 'the tab in front is not a box');
				assert.ok(
					seen.scroll.some((s, i) => s > seen.client[i]),
					`main fits ${seen.client.join('x')}, so nothing here overflows: ${seen.scroll.join('x')}`
				);
				assert.ok(
					seen.scroll.every((s, i) => Math.abs(s - seen.reach[i]) <= 1),
					`the page scrolls ${seen.scroll.join('x')}, the split and its padding reach ${seen.reach.join('x')}`
				);
			},
			{ ...PHONE, prepare: twoLayouts }
		);
	});

	// vremya on #1188: a run cannot be stopped once started, so a layout
	// behind that refreshed the moment a write landed kept the next line
	// typed waiting. It refreshes once every command has answered and a
	// beat has gone by.
	test('a layout behind takes a write once the page has gone quiet, and comes back with it', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				const root = async (name) => {
					await page.locator('.root-bar [role="tab"]', { hasText: name }).click();
					await until(() => shown(page), (shownName) => shownName === name);
					await app.idle();
				};
				await root('tail');
				await root('main');
				const kept = page.locator('section.root > .content > .tab-body.behind');
				const hidden = () => kept.evaluate((el) => el.textContent.includes('coffee'));
				await app.submit('occur coffee');
				await app.idle();
				assert.equal(await hidden(), false, 'the layout behind refreshed with the write');
				await until(hidden, (has) => has);
				const seen = await app.spyOnRunner();
				await root('tail');
				assert.deepEqual(await seen(), [], 'the return ran a command');
			},
			{ ...PHONE, prepare: twoLayouts }
		);
	});
});

describe('a layout left in the moment after a write', () => {
	// vremya on #1188: the tab left before the layouts behind caught up was
	// drawn at their older epoch, a change, and its panes ran at it and
	// again as it caught up. Each tail here runs once: `main`'s for the
	// write, and `tail`'s as it comes forward; their boxes tell them apart.
	test('runs nothing more as it goes behind', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				for (const name of ['tail', 'main']) {
					await page.locator('.root-bar [role="tab"]', { hasText: name }).click();
					await until(() => shown(page), (shownName) => shownName === name);
					await app.idle();
				}
				const seen = await app.spyOnRunner();
				await app.submit('occur coffee');
				await page.evaluate(() =>
					[...document.querySelectorAll('.root-bar [role="tab"]')].find((t) => t.textContent.trim() === 'tail').click()
				);
				await until(() => shown(page), (shownName) => shownName === 'tail');
				await app.idle();
				await page.waitForTimeout(1500);
				await app.idle();
				const tails = {};
				for (const argv of await seen())
					if (argv[0] === 'tail' && argv.includes('--json')) {
						const n = argv[argv.indexOf('-n') + 1];
						tails[n] = (tails[n] ?? 0) + 1;
					}
				assert.equal(Object.keys(tails).length, 2, `not two tails: ${JSON.stringify(tails)}`);
				assert.deepEqual(Object.values(tails), [1, 1], `a tail ran more than once: ${JSON.stringify(tails)}`);
			},
			{ ...PHONE, prepare: twoLayouts }
		);
	});
});

describe('a maximize keeps every layout mounted', () => {
	// da-x7xjg, after vremya on #1188: the maximized pane is the tree's own,
	// laid over it, so a restore shows the layout in front still drawn and
	// mounts nothing, the layouts kept behind it included.
	test('the restore mounts no pane and no layout', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				const bodies = () => page.locator('section.root > .content > .tab-body').count();
				for (const name of ['tail', 'main']) {
					await page.locator('.root-bar [role="tab"]', { hasText: name }).click();
					await until(() => shown(page), (shownName) => shownName === name);
					await app.idle();
				}
				assert.equal(await bodies(), 2, 'tail was not kept behind main');
				const mark = () => page.evaluate(() => document.querySelectorAll('section.pane').forEach((p) => (p.__kept = true)));
				const unkept = () => page.evaluate(() => [...document.querySelectorAll('section.pane')].filter((p) => !p.__kept).length);
				await mark();
				await maximizeButton(page, 'side').click();
				await page.locator('section[data-pane="side"] > .titlebar button.maximize[aria-pressed="true"]').waitFor();
				assert.equal(await bodies(), 2, 'the maximize let the kept layout go');
				await maximizeButton(page, 'side').click();
				await until(() => titles(page), (seen) => seen.join() === 'dayview,cli,tail,fields');
				assert.equal(await bodies(), 2, 'the restore let the kept layout go');
				assert.equal(await unkept(), 0, 'the restore mounted a pane again');
			},
			{ ...PHONE, prepare: twoLayouts }
		);
	});
});

describe('the layout brought on is drawn at the size it lands at', () => {
	// da-466lv: the root's frame is a box of another size for each kind of
	// layout in front — a split's on its frame, a single pane's inside its
	// 1px outline — and the layout a swipe brings on was laid out in the
	// one in front, so every widget that measures its box drew twice: once
	// beside, 2px off, and again as it landed.

	/** The first widget body of the layout in front, or of the one beside it: its laid-out size. */
	const body = (page, which) =>
		page
			.locator(`section.root > .content > .tab-body${which === 'beside' ? '.beside' : ':not(.behind)'} .body`)
			.first()
			.evaluate((el) => [el.getBoundingClientRect().width, el.getBoundingClientRect().height]);

	/** Put a finger on `over`, drag it `dx` and hold it there while `read` runs; then let go. */
	async function holding(page, over, dx, read) {
		await over.evaluate((el) => el.scrollIntoView({ block: 'nearest' }));
		const box = await over.boundingBox();
		const x = box.x + box.width / 2;
		const y = box.y + box.height / 2;
		const cdp = await page.context().newCDPSession(page);
		const at = (type, touchPoints) => cdp.send('Input.dispatchTouchEvent', { type, touchPoints });
		await at('touchStart', [{ x, y }]);
		for (let i = 1; i <= 6; i++) await at('touchMove', [{ x: x + (dx * i) / 6, y }]);
		try {
			return await read();
		} finally {
			await at('touchEnd', []);
			await cdp.detach();
		}
	}

	/** Swipe from the layout in front to `to` over `over`: the body's size beside, and at rest. */
	async function across(page, over, dx, to) {
		const beside = await holding(page, over, dx, async () => {
			await until(() => page.locator('section.root > .content > .tab-body.beside .body').count(), (n) => n > 0);
			return body(page, 'beside');
		});
		await until(() => shown(page), (t) => t === to);
		await atRest(page.locator('section.root > .content'));
		return { beside, rest: await body(page, 'front') };
	}

	const same = ({ beside, rest }, label) =>
		assert.ok(
			beside.every((v, i) => Math.abs(v - rest[i]) < 0.5),
			`${label}: laid out at ${beside.join('x')} beside and ${rest.join('x')} where it lands`
		);

	test('a split root brought on from a single pane, and a single pane from a split', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				const repl = await wideRepl(app);
				await scrollX(repl, true);
				same(await across(page, repl, -120, 'tail'), 'tail, beside main');
				const tail = page.locator('section.root > .content .body').first();
				await scrollX(tail, false);
				same(await across(page, tail, 120, 'main'), 'main, beside tail');
			},
			{ ...PHONE, prepare: twoLayouts }
		);
	});
});

describe('a widget that takes the axis itself', () => {
	test('a one-finger drag across the nowview band leaves the tab alone', async () => {
		await phone.solo(async ({ page }) => {
			await pane(page, 'side').locator('[role="tab"]', { hasText: 'nowview' }).click();
			await pastTheWindow(page);
			// The BAND declares the axis, not the widget: the margin above it
			// is room a swipe may start in (da-rskp).
			const band = pane(page, 'side').locator('.nowview-band');
			await band.locator('svg.svg').waitFor();
			assert.equal(await band.getAttribute('data-pans'), 'x');
			assert.equal(await pane(page, 'side').locator('.nowview').getAttribute('data-pans'), null);
			const content = pane(page, 'side').locator('> .content');
			await swipeOver(page, band, -120, 0);
			await atRest(content);
			assert.equal(await activeTab(page, 'side'), 'nowview', 'a drag across the band moved the tab');
			await swipeOver(page, band, 120, 0);
			await atRest(content);
			assert.equal(await activeTab(page, 'side'), 'nowview', 'a drag back across the band moved the tab');
		});
	});
});

describe('a run of swipes: the window after a switch', () => {
	// vividn, 2026-10-25 (da-d9p5m): «when switching between tabs there
	// should be a certain brief time window where a swipe anywhere on the new
	// tab keeps switching tabs, even if otherwise a widget would have taken
	// that swipe internally» — «this goes for both inner pane tabs and global
	// tabs».
	test('inside the window a swipe over the nowview band moves the tab; after it the band keeps it', async () => {
		await phone.solo(async ({ page }) => {
			const side = pane(page, 'side');
			const content = side.locator('> .content');
			const band = side.locator('.nowview-band');
			const tab = (name) => side.locator('[role="tab"]', { hasText: name }).click();
			await tab('nowview');
			await band.locator('svg.svg').waitFor();
			await tab('tail');
			await pastTheWindow(page);

			// A swipe lands on the nowview, and the next one over its band,
			// which a drag across is otherwise the band's own, moves on.
			const body = side.locator('.body:not(.behind .body)');
			await scrollX(body, true);
			await swipeOver(page, body, -120, 0);
			await until(() => activeTab(page, 'side'), (t) => t === 'nowview');
			const pan = await band.evaluate((el) => el.style.getPropertyValue('--pan'));
			await swipeOver(page, band, -120, 0);
			await until(() => activeTab(page, 'side'), (t) => t === 'custom');
			assert.equal(await band.evaluate((el) => el.style.getPropertyValue('--pan')), pan, 'the band panned under the tab’s swipe');

			// A tap on the tab opens the window as well.
			await tab('nowview');
			await swipeOver(page, band, 120, 0);
			await until(() => activeTab(page, 'side'), (t) => t === 'tail');

			// And past it the band takes the drag, as it always did.
			await tab('nowview');
			await pastTheWindow(page);
			await swipeOver(page, band, -120, 0);
			await atRest(content);
			assert.equal(await activeTab(page, 'side'), 'nowview', 'a drag across the band moved the tab after the window');
		});
	});

	test('a swipe the window passes on ends it: the next one over a scroller is the scroller’s', async () => {
		// da-bgve9: a swipe the frame does not take, with no tab that way, is
		// the reader working in the tab, and the run is over. Under the
		// tail, a pane whose FIRST tab is a CLI widget with a line wider than it.
		const replFirst = {
			version: 1,
			name: 'main',
			root: {
				id: 'root',
				active: 0,
				bar,
				tabs: [
					{
						id: 'root-tab',
						content: {
							id: 'col',
							direction: 'column',
							ratios: [0.5, 0.5],
							children: [
								widgetPane('tail', 'tail'),
								{
									id: 'wide',
									active: 0,
									bar,
									tabs: [
										{ id: 'wide-repl', content: { id: 'wide-repl-w', kind: 'cli', settings: {} } },
										{ id: 'wide-custom', content: { id: 'wide-custom-w', kind: 'custom', settings: {} } }
									]
								}
							]
						}
					}
				]
			}
		};
		await phone.solo(
			async (app) => {
				const { page } = app;
				await app.submit(`occur ${'x'.repeat(60)}`);
				await app.submit('tail');
				const wide = pane(page, 'wide');
				const body = wide.locator('.body:not(.behind .body)');
				const tab = (name) => wide.locator('[role="tab"]', { hasText: name }).click();
				await tab('custom');
				await tab('cli');
				// `cli` is the first tab: a swipe right has no tab to go to.
				await swipeOver(page, body, 120, 0, { x: 0.2, y: 0.5 });
				const [at, most] = await body.evaluate((el) => [el.scrollLeft, el.scrollWidth - el.clientWidth]);
				assert.ok(at < most, 'the CLI widget does not scroll sideways — this case needs a wider line');
				await swipeOver(page, body, -120, 0, { x: 0.8, y: 0.5 });
				await atRest(wide.locator('> .content'));
				assert.equal(await activeTab(page, 'wide'), 'cli', 'the window took a swipe after one it passed on');
				assert.ok((await body.evaluate((el) => el.scrollLeft)) > at, 'the scroller under the finger did not take the swipe');
			},
			JSON.stringify(replFirst)
		);
	});

	test('after a layout switch a swipe over a tabbed pane moves the layout; after the window, the pane’s tab', async () => {
		await withApp(
			async ({ page }) => {
				const root = (name) => page.locator('.root-bar [role="tab"]', { hasText: name }).click();
				const body = () => pane(page, 'side').locator('.body:not(.behind .body)');
				await root('tail');
				await until(() => shown(page), (name) => name === 'tail');
				await root('main');
				await until(() => shown(page), (name) => name === 'main');
				await scrollX(body(), true);
				await swipeOver(page, body(), -120, 0);
				await until(() => shown(page), (name) => name === 'tail');

				await root('main');
				await until(() => shown(page), (name) => name === 'main');
				assert.equal(await activeTab(page, 'side'), 'tail', 'the inner pane moved its tab in the root’s window');
				await pastTheWindow(page);
				await scrollX(body(), true);
				await swipeOver(page, body(), -120, 0);
				await until(() => activeTab(page, 'side'), (t) => t === 'nowview');
				assert.equal(await shown(page), 'main', 'the swipe after the window moved the layout');
			},
			{ ...PHONE, prepare: twoLayouts }
		);
	});
});
