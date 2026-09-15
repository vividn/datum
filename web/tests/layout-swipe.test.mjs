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
import { startApp, stopApp, withApp } from './support/app.mjs';
import {
	PHONE,
	activeTab,
	addTab,
	atRest,
	bar,
	draft,
	fill,
	choose,
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

/** This device's open layouts: `day` in front, the built-in `tail` behind it. */
const twoLayouts = (context) =>
	context.addInitScript(
		(json) => localStorage.setItem('datum.layout.root.v1', json),
		JSON.stringify({ names: ['day', 'tail'], active: 0 })
	);

/**
 * The layout in front, as the root bar says it — less the orange dot it
 * wears while the copy differs from its save, which a swipe between tabs
 * is enough to earn.
 */
const shown = async (page) =>
	(await page.locator('.root-bar [role="tab"][aria-selected="true"]').innerText()).replace(/\s*•\s*$/, '');

/**
 * Report every transform `locator` takes from now on.
 *
 * The motion is over in 150ms twice, so a test that reads the transform
 * after the swipe is a test of how fast the runner is. The inline style is
 * watched instead, and what it took is read once the tab has landed.
 */
const watch = (locator) =>
	locator.evaluate((el) => {
		window.__transforms = [];
		new MutationObserver(() => window.__transforms.push(el.style.transform)).observe(el, {
			attributes: true,
			attributeFilter: ['style']
		});
	});
const moved = async (page) => (await page.evaluate(() => window.__transforms ?? [])).filter(Boolean);

describe('the root swipes between layouts', () => {
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
		const body = pane(app.page, 'repl').locator('.body');
		assert.ok(
			await body.evaluate((el) => el.scrollWidth > el.clientWidth),
			'the REPL does not scroll sideways — this case needs a wider line'
		);
		return body;
	}

	test('a swipe over the page moves to the next layout, and back', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				assert.deepEqual(await page.locator('.root-bar [role="tab"]').allInnerTexts(), ['day', 'tail']);
				// Over a widget, not the page's margin: the pane under the
				// finger has one tab and no swipe of its own, so the gesture is
				// the root's — and its body is at its end, so it has nowhere to
				// take the finger and yields.
				const body = await wideRepl(app);
				await scrollX(body, true);
				await swipeOver(page, body, -120, 0);
				await until(() => shown(page), (name) => name === 'tail');
				assert.deepEqual(await titles(page), ['tail'], 'the second layout is not what is on screen');

				const back = page.locator('section.pane .body').first();
				await scrollX(back, false);
				await swipeOver(page, back, 120, 0);
				await until(() => shown(page), (name) => name === 'day');
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
		name: 'day',
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
		await withApp(
			async (app) => {
				const { page } = app;
				// A field whose name is sixty characters, listed: the line the
				// pane has to scroll to show.
				await app.submit(`occur ${'x'.repeat(60)}`);
				await app.submit('tail');
				const body = pane(page, 'wide').locator('.body');
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
			{ ...PHONE, prepare: (context) => context.addInitScript(draft, JSON.stringify(threeTabs())) }
		);
	});
});

describe('the innermost frame that can move takes the gesture', () => {
	test("a tabbed pane moves its own tab; on its last it lets the root move the layout", async () => {
		await withApp(
			async ({ page }) => {
				const body = pane(page, 'side').locator('.body');
				await scrollX(body, true);
				await swipeOver(page, body, -120, 0);
				await until(() => activeTab(page, 'side'), (t) => t === 'nowview');
				assert.equal(await shown(page), 'day', 'the swipe moved the layout as well as the tab');

				// On the pane's LAST tab there is nowhere for it to go, so the
				// same swipe reaches the root and moves the whole layout.
				await pane(page, 'side').locator('[role="tab"]', { hasText: 'custom' }).click();
				const last = pane(page, 'side').locator('.body');
				await scrollX(last, true);
				await swipeOver(page, last, -120, 0);
				await until(() => shown(page), (name) => name === 'tail');

				// And the pane it left is as it was: the root moved, not it.
				const back = page.locator('section.pane .body').first();
				await scrollX(back, false);
				await swipeOver(page, back, 120, 0);
				await until(() => shown(page), (name) => name === 'day');
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
				await choose(page, 'repl', 'split: a new pane below');
				await fill(page, 'mine');
				const frame = page.locator('section.pane[data-title="mine"]');
				await frame.waitFor();
				const id = await frame.getAttribute('data-pane');
				await addTab(page, id, 'aliases');
				await pane(page, id).locator('[role="tab"]', { hasText: 'mine' }).click();
				await settled(page);

				// The widget inside is a scroller, and mid-scroll: it would keep
				// a gesture that started over it. The finger does not.
				const inner = pane(page, 'm-tail').locator('.body');
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
		await withApp(async ({ page }) => {
			const content = pane(page, 'side').locator('> .content');
			const body = pane(page, 'side').locator('.body');
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
		}, PHONE);
	});

	test('a swipe too short comes back, and the tab does not move', async () => {
		await withApp(async ({ page }) => {
			const content = pane(page, 'side').locator('> .content');
			const body = pane(page, 'side').locator('.body');
			await scrollX(body, true);
			await watch(content);
			await swipeOver(page, body, -20, 0);
			assert.ok((await moved(page)).length > 0, 'a claimed gesture moved nothing while the finger was down');
			await atRest(content);
			assert.equal(await activeTab(page, 'side'), 'tail');
		}, PHONE);
	});
});

describe('a widget that takes the axis itself', () => {
	test('a one-finger drag across the nowview band leaves the tab alone', async () => {
		await withApp(async ({ page }) => {
			await pane(page, 'side').locator('[role="tab"]', { hasText: 'nowview' }).click();
			const band = pane(page, 'side').locator('.nowview');
			await band.locator('svg.svg').waitFor();
			assert.equal(await band.getAttribute('data-pans'), 'x');
			const content = pane(page, 'side').locator('> .content');
			await swipeOver(page, band, -120, 0);
			await atRest(content);
			assert.equal(await activeTab(page, 'side'), 'nowview', 'a drag across the band moved the tab');
			await swipeOver(page, band, 120, 0);
			await atRest(content);
			assert.equal(await activeTab(page, 'side'), 'nowview', 'a drag back across the band moved the tab');
		}, PHONE);
	});
});
