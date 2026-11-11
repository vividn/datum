// Each game lesson's tab is taller than the screen and read top to bottom (da-zb1dr, vividn, 2026-11-05: «we keep
// a small intro text and then the nowview, game … and collect widget …, and then we can have a more in depth
// discussion below the collect widget of what just happened. … The next button would then be on the very
// bottom»). The game asks for a field and it is tapped in collect, so the game and collect are both in sight while a
// round runs. The intro and the discussion are their text's height, and the intro, the nowview, the game and
// collect fill the screen, the shell's and not the window's, the bar's output section open or shut (da-83y6w,
// vividn, 2026-11-09: «the intro block is much too big … make sure the complete game and collect widget are on
// screen without having to scroll … the followup text widget can also clamp to its text size»).

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { devices } from 'playwright';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { settled, startSwitch, lessonTab } from './support/tutorial.mjs';
import { leaveWidget, modifyWidget } from './support/layout.mjs';

before(startApp);
after(stopApp);

const GAMES = ['moment', 'on-off', 'change'];
/** The next lesson's heading, which Next names: its title, not its tab's word (da-je8ju). */
const NEXT = { moment: 'Turn the TV off', 'on-off': 'Change of scenery', change: 'The input bar' };

/** The rects that matter, against the page's scroller: the part of the screen the tab is drawn in. */
const measure = (page, tab) =>
	page.evaluate((tab) => {
		const box = (el) => el.getBoundingClientRect().toJSON();
		const pane = (part) => document.querySelector(`[data-pane="${tab}-${part}"]`);
		const scroller = document.querySelector('section.holder.root > .content');
		const next = pane('discuss').querySelector('button.next');
		/** How much of a pane's text is cut off, a scroll away inside it. */
		const cut = (part) => {
			const body = pane(part).querySelector('.body');
			return body.scrollHeight - body.clientHeight;
		};
		/** How much taller a pane is than its text, the words and the button under them, and a line of it. */
		const spare = (part) => {
			const text = pane(part).querySelector('.tutorial');
			return { by: box(pane(part)).height - box(text).height, line: parseFloat(getComputedStyle(text).lineHeight) };
		};
		return {
			cut: { about: cut('about'), discuss: cut('discuss') },
			spare: { about: spare('about'), discuss: spare('discuss') },
			aboutNext: pane('about').querySelectorAll('button.next').length,
			screen: box(scroller),
			bar: box(document.querySelector('.prompt-bar')),
			scrollTop: scroller.scrollTop,
			scrollable: scroller.scrollHeight - scroller.clientHeight,
			about: box(pane('about')),
			now: box(pane('now')),
			lesson: box(pane('lesson')),
			game: box(pane('lesson').querySelector('.game')),
			collect: box(pane('fields')),
			fields: [...pane('fields').querySelectorAll('.fields button[data-field]')].map(box),
			rows: [...pane('fields').querySelectorAll('.fields .spot.shelf[data-row]')].map((s) => Number(s.dataset.row)),
			discuss: box(pane('discuss')),
			next: next && box(next)
		};
	}, tab);

/** Scroll the page's scroller to `top`, and let its panes settle. */
const scrollTo = (page, top) =>
	page.evaluate(async (top) => {
		document.querySelector('section.holder.root > .content').scrollTop = top;
		await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
	}, top);

/**
 * Record what the page's own scripts do that would move its scroller: a write to its scroll, a `scrollTo` or
 * `scrollBy` of it, a `scrollIntoView` or a `focus()` without `preventScroll` of something in it, and a `hashchange`,
 * which jumps to its target with none of them (da-nj5qa). Playwright scrolls from a world of its own, which none of
 * these see. Nor do they see the page's CSS scroll-snap, the browser's scroll anchoring or an `autofocus`, which move
 * the scroller with no call. Returns what was seen since, and stops.
 */
const watchPageScrolls = async (page) => {
	await page.evaluate(() => {
		const scroller = document.querySelector('section.holder.root > .content');
		const calls = [];
		const undo = [];
		const wrap = (proto, name, moves) => {
			const was = Object.getOwnPropertyDescriptor(proto, name);
			const key = was.set ? 'set' : 'value';
			const fn = was[key];
			Object.defineProperty(proto, name, {
				...was,
				[key](...args) {
					if (moves(this, ...args)) calls.push(`${name} of ${this.outerHTML?.slice(0, 60)}`);
					return fn.apply(this, args);
				}
			});
			undo.push(() => Object.defineProperty(proto, name, was));
		};
		const itself = (el) => el === scroller;
		const within = (el) => scroller.contains(el);
		wrap(Element.prototype, 'scrollTop', itself);
		wrap(Element.prototype, 'scrollTo', itself);
		wrap(Element.prototype, 'scroll', itself);
		wrap(Element.prototype, 'scrollBy', itself);
		wrap(Element.prototype, 'scrollIntoView', within);
		for (const proto of [HTMLElement.prototype, SVGElement.prototype]) wrap(proto, 'focus', (el, options) => within(el) && !options?.preventScroll);
		const hash = () => calls.push(`hashchange to ${location.hash}`);
		window.addEventListener('hashchange', hash);
		undo.push(() => window.removeEventListener('hashchange', hash));
		window.__pageScrolls = () => (undo.forEach((u) => u()), calls);
	});
	return () => page.evaluate(() => window.__pageScrolls());
};

const inside = (r, s) => r.top >= s.top - 0.5 && r.bottom <= s.bottom + 0.5 && r.left >= s.left - 0.5 && r.right <= s.right + 0.5;

for (const [screen, device] of [
	['1280×900 desktop', { viewport: { width: 1280, height: 900 } }],
	['iPhone SE (375×667)', { device: devices['iPhone SE (3rd gen)'] }]
]) {
	test(`on a ${screen}, output open and shut, each game's intro and discussion are their text's height, and its game and collect in sight at once`, async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				// The switch game starts from its cue, by collect's own picker, which at a phone's width can leave «garden»
				// out of reach (da-u846n): a walk that starts elsewhere, seeded before its tab is first drawn.
				await page.evaluate(() => {
					const { house } = window.datum.tutorial;
					let seed = 1;
					while (house.scene(seed)[0].state === 'garden') seed++;
					house.round.seed = seed;
				});
				for (const output of ['open', 'shut']) {
					// Every screen starts with the output section open; PageDown on the box shuts it.
					if (output === 'shut') await page.locator('.prompt-bar input').press('PageDown');
					for (const tab of GAMES) {
						const at = `${tab}, output ${output}`;
						await lessonTab(page, tab).click();
						await page.locator(`[data-pane="${tab}-lesson"] .game`).waitFor();
						await scrollTo(page, 0);
						await page.locator(`[data-pane="${tab}-fields"] .fields button[data-field]`).first().waitFor();

						const rest = await measure(page, tab);
						assert.ok(rest.screen.bottom <= rest.bar.top + 0.5, `${at}: the screen (bottom ${rest.screen.bottom}) ends over the bar (${rest.bar.top})`);
						assert.ok(rest.scrollable > 0, `${at}: the tab is taller than the screen`);
						const order = ['about', 'now', 'game', 'collect', 'discuss'];
						for (const [a, b] of order.slice(1).map((b, i) => [order[i], b]))
							assert.ok(rest[a].bottom <= rest[b].top + 0.5, `${at}: ${a} (bottom ${rest[a].bottom}) above ${b} (top ${rest[b].top})`);
						assert.equal(rest.aboutNext, 0, `${at}: no Next in the intro`);
						assert.deepEqual(rest.cut, { about: 0, discuss: 0 }, `${at}: the intro and the discussion are drawn whole, nothing cut off`);
						for (const part of ['about', 'discuss']) {
							const { by, line } = rest.spare[part];
							assert.ok(by <= line, `${at}: the ${part} pane is ${by}px taller than its text, more than a line (${line}px)`);
						}
						// As it opens: the nowview, the game and every one of collect's fields at once, with no scroll.
						for (const part of ['now', 'game'])
							assert.ok(inside(rest[part], rest.screen), `${at}: the ${part} is in sight as the tab opens`);
						const fields = await page.evaluate((t) => window.datum.tutorial.lessons.find((l) => l.name === t).fields.length, tab);
						assert.equal(rest.fields.length, fields, `${at}: collect draws each of the game's fields`);
						for (const [i, field] of rest.fields.entries())
							assert.ok(inside(field, rest.screen), `${at}: collect's field ${i} (bottom ${field.bottom}) is in sight, the screen's bottom ${rest.screen.bottom}`);
						// Collect two thirds of the 0.4 of the nowview, the game and collect it had (da-je8ju, vividn, 2026-11-10:
						// «goes to 2/3 of its current height»), and its zoom three rows (vividn: «be 3 rows»): a game's fields
						// span the band, so its rows at three are the top, the middle and the bottom; at two or four they are not.
						const share = rest.collect.height / (rest.now.height + rest.lesson.height + rest.collect.height);
						assert.ok(Math.abs(share - (2 / 3) * 0.4) < 0.01, `${at}: collect is ${share} of the nowview, game and collect`);
						// The change game's one field, room, stands in one row whatever the count, so it cannot show three.
						if (tab !== 'change') assert.deepEqual([...new Set(rest.rows)].sort(), [0, 1, 2], `${at}: collect's fields stand in three rows`);

						if (output === 'open') {
							// Past the intro: the game and collect wholly in sight, before and while a round runs.
							await scrollTo(page, rest.scrollTop + rest.about.bottom - rest.screen.top);
							const past = await measure(page, tab);
							// And the nowview over them, what is watched while tapping (vividn, 2026-10-22).
							assert.ok(inside(past.now, past.screen), `${at}: the nowview is in sight past the intro`);
							assert.ok(inside(past.game, past.screen), `${at}: the game is in sight past the intro`);
							assert.ok(inside(past.collect, past.screen), `${at}: collect is in sight past the intro`);
							const scrolls = await watchPageScrolls(page);
							if (tab === 'change') await startSwitch(page);
							else await page.getByRole('button', { name: 'Start', exact: true }).click();
							await settled(page, `[data-pane="${tab}-lesson"]`);
							// The page moved nothing as the round began, so where the reader left it is where a reader sees it. A
							// click Playwright retries (a gliding spot over its target, under load) scrolls the target to the
							// screen's end, center or start first, and the page with it, which a reader's tap does not (da-cnvfw).
							assert.deepEqual(await scrolls(), [], `${at}: the page scrolls nothing itself as a round begins`);
							await scrollTo(page, past.scrollTop);
							const during = await measure(page, tab);
							assert.ok(inside(during.now, during.screen), `${at}: the nowview is in sight while a round runs`);
							assert.ok(inside(during.game, during.screen), `${at}: the game is in sight while a round runs`);
							assert.ok(inside(during.collect, during.screen), `${at}: collect is in sight while a round runs`);
							await app.clock.set(app.clock.at() + 1500);
							const ending = page.getByRole('button', { name: 'End now', exact: true });
							await page.waitForFunction((pane) => parseFloat(document.querySelector(`${pane} .game .go .spent`)?.style.inlineSize) > 0, `[data-pane="${tab}-lesson"]`);
							await ending.click();
							await page.getByRole('button', { name: 'Reset', exact: true }).click();
							await settled(page, `[data-pane="${tab}-lesson"]`);
						}

						// At the very bottom, the discussion, and Next its last thing, in sight.
						await scrollTo(page, rest.scrollable);
						const end = await measure(page, tab);
						assert.ok(end.next, `${at}: the discussion ends on Next`);
						assert.ok(end.next.top >= end.collect.bottom, `${at}: Next is under collect`);
						assert.ok(inside(end.next, end.screen), `${at}: Next is in sight at the bottom`);
						const button = page.locator(`[data-pane="${tab}-discuss"]`).getByRole('button', { name: `Next: ${NEXT[tab]}`, exact: true });
						assert.equal(await button.count(), 1, `${at}: Next names the next lesson`);
					}
				}
				assert.deepEqual(app.errors, []);
			},
			{ ...device, query: '?db=tutorial' }
		);
	});
}

// The three rows are the zoom's default (da-je8ju): a person's own zoom, set in modify mode (da-xi4up), wins, and over a
// reload, where the box is fitted again.
test('a game’s collect opens at three rows, and a zoom moved by hand stands over a reload', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			const room = '[data-pane="moment-fields"] .fields .room';
			const shelved = () => page.locator(`${room} .spot.shelf[data-row]`).evaluateAll((ss) => [...new Set(ss.map((s) => Number(s.dataset.row)))].sort());
			const scale = () => page.locator(room).evaluate((el) => Number(getComputedStyle(el).getPropertyValue('--zoom')));
			await lessonTab(page, 'moment').click();
			await page.locator(`${room} .spot.shelf`).first().waitFor();
			await app.idle();
			assert.deepEqual(await shelved(), [0, 1, 2], 'three rows as it opens');
			// With the bar's output shut the screen, and collect with it, is taller: at zoom 1 its box holds four rows
			// (a second size, where three is the fit's and not the box's), and the fit grows the rows to keep three.
			const open = await scale();
			const tall = (await page.locator(room).boundingBox()).height;
			await page.locator('.prompt-bar input').press('PageDown');
			await page.waitForFunction(([sel, was]) => document.querySelector(sel).getBoundingClientRect().height > was + 10, [room, tall]);
			await app.idle();
			assert.deepEqual(await shelved(), [0, 1, 2], 'three rows with the output shut');
			const fitted = await scale();
			assert.ok(fitted > open, `the zoom grew with the box, ${open} to ${fitted}`);
			// In the widget's modify mode, where alone the rows zoom (da-xi4up), kept by its ✓.
			await modifyWidget(page, 'moment-fields');
			const box = await page.locator(room).boundingBox();
			await page.mouse.move(box.x + box.width * 0.75, box.y + box.height / 2);
			await page.keyboard.down('Control');
			await page.mouse.wheel(0, 150);
			await page.keyboard.up('Control');
			await leaveWidget(page, 'moment-fields', 'apply');
			const held = await scale();
			assert.ok(held < fitted, `the zoom ${fitted} became ${held}`);
			assert.ok((await shelved()).length > 3, 'more, smaller rows');
			await app.reload();
			await page.locator(`${room} .spot.shelf`).first().waitFor();
			await app.idle();
			assert.equal(await scale(), held, 'a reload keeps the zoom moved by hand');
			assert.deepEqual(app.errors, []);
		},
		{ viewport: { width: 1280, height: 900 }, query: '?db=tutorial' }
	);
});
