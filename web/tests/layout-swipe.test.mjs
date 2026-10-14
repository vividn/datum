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
	pane,
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
 * instead — its tab in front is moved by `--front` while it is `moving`
 * (da-24fgb), and not at all otherwise — and what it took is read once
 * the tab has landed.
 */
const watch = (locator) =>
	locator.evaluate((el) => {
		window.__transforms = [];
		new MutationObserver(() =>
			window.__transforms.push(el.classList.contains('moving') ? el.style.getPropertyValue('--front') : '')
		).observe(el, { attributes: true, attributeFilter: ['style', 'class'] });
	});
const moved = async (page) => (await page.evaluate(() => window.__transforms ?? [])).filter(Boolean);

/**
 * The REPL pane, scrolling sideways: a `tail` of a sixty-character
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
		'the REPL does not scroll sideways — this case needs a wider line'
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
				assert.deepEqual(await titles(page), ['dayview', 'repl', 'tail', 'fields']);

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
	 * A column of [repl | a pane whose middle tab is a REPL of its own].
	 *
	 * The REPL is the wide line: the tail's table wraps a long field name
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
									{ id: 'wide-repl', content: { id: 'wide-repl-w', kind: 'repl', settings: {} } },
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
					'the REPL does not scroll sideways — this case needs a wider line'
				);

				// At its start, a finger going left has somewhere to take it.
				await scrollX(body, false);
				await swipeOver(page, body, -120, 0, { x: 0.8, y: 0.5 });
				await atRest(pane(page, 'wide').locator('> .content'));
				assert.equal(await activeTab(page, 'wide'), 'repl', 'a swipe over a scroller with room changed the tab');

				// The other way it has not: the line is already at its start,
				// so the frame takes the gesture and the tab before it arrives.
				await scrollX(body, false);
				await swipeOver(page, body, 120, 0, { x: 0.2, y: 0.5 });
				await until(() => activeTab(page, 'wide'), (t) => t === 'aliases');

				// And at its end, a swipe left is the frame's too.
				await pane(page, 'wide').locator('[role="tab"]', { hasText: 'repl' }).click();
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
						.filter((d) => getComputedStyle(d).visibility === 'visible')
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
				assert.deepEqual(await titles(page), ['dayview', 'repl', 'tail', 'fields']);
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
				assert.deepEqual(await titles(page), ['tail'], 'the layout left behind is still mounted');
			},
			{ ...PHONE, prepare: twoLayouts }
		);
	});
});

describe('a widget that takes the axis itself', () => {
	test('a one-finger drag across the nowview band leaves the tab alone', async () => {
		await phone.solo(async ({ page }) => {
			await pane(page, 'side').locator('[role="tab"]', { hasText: 'nowview' }).click();
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
