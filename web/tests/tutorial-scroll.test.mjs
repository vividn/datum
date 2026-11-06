// Each game lesson's tab is taller than the screen and read top to bottom (da-zb1dr, vividn, 2026-11-05: «we keep
// a small intro text and then the nowview, game … and collect widget …, and then we can have a more in depth
// discussion below the collect widget of what just happened. … The next button would then be on the very
// bottom»). The one hard constraint: the game asks for a field and it is tapped in collect, so the game and
// collect are both in sight while a round runs, once the intro is scrolled past, on an iPhone SE too.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { devices } from 'playwright';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { settled, startSwitch } from './support/tutorial.mjs';

before(startApp);
after(stopApp);

const GAMES = ['moment', 'on-off', 'switch'];
const NEXT = { moment: 'on/off', 'on-off': 'change', switch: 'The input bar' };
/** What a lesson's tab says; the change lesson's layout is named `switch`. */
const tabLabel = (name) => (name === 'switch' ? 'change' : name);

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
		return {
			cut: { about: cut('about'), discuss: cut('discuss') },
			aboutNext: pane('about').querySelectorAll('button.next').length,
			screen: box(scroller),
			scrollTop: scroller.scrollTop,
			scrollable: scroller.scrollHeight - scroller.clientHeight,
			about: box(pane('about')),
			now: box(pane('now')),
			game: box(pane('lesson').querySelector('.game')),
			collect: box(pane('fields')),
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

const inside = (r, s) => r.top >= s.top - 0.5 && r.bottom <= s.bottom + 0.5 && r.left >= s.left - 0.5 && r.right <= s.right + 0.5;

for (const [screen, device] of [
	['iPhone SE (320×568)', { device: devices['iPhone SE'] }],
	['390×844 phone', { device: devices['iPhone 12'], viewport: { width: 390, height: 844 } }]
]) {
	test(`on a ${screen} each game's game and collect are in sight together past its intro, and the discussion and Next under them`, async () => {
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
				for (const tab of GAMES) {
					await page.locator('.root-bar [role="tab"]', { hasText: tabLabel(tab) }).click();
					await page.locator(`[data-pane="${tab}-lesson"] .game`).waitFor();
					await scrollTo(page, 0);
					await page.locator(`[data-pane="${tab}-fields"] .fields button[data-field]`).first().waitFor();

					const rest = await measure(page, tab);

					assert.ok(rest.scrollable > 0, `${tab}: the tab is taller than the screen`);
					const order = ['about', 'now', 'game', 'collect', 'discuss'];
					for (const [a, b] of order.slice(1).map((b, i) => [order[i], b]))
						assert.ok(rest[a].bottom <= rest[b].top + 0.5, `${tab}: ${a} (bottom ${rest[a].bottom}) above ${b} (top ${rest[b].top})`);

					// Past the intro: the game and collect wholly in sight, before and while a round runs.
					await scrollTo(page, rest.scrollTop + rest.about.bottom - rest.screen.top);
					const past = await measure(page, tab);
					// And the nowview over them, what is watched while tapping (vividn, 2026-10-22).
					assert.ok(inside(past.now, past.screen), `${tab}: the nowview is in sight past the intro`);
					assert.ok(inside(past.game, past.screen), `${tab}: the game is in sight past the intro`);
					assert.ok(inside(past.collect, past.screen), `${tab}: collect is in sight past the intro`);
					if (tab === 'switch') await startSwitch(page);
					else await page.getByRole('button', { name: 'Start', exact: true }).click();
					await settled(page, `[data-pane="${tab}-lesson"]`);
					const during = await measure(page, tab);
					assert.ok(inside(during.now, during.screen), `${tab}: the nowview is in sight while a round runs`);
					assert.ok(inside(during.game, during.screen), `${tab}: the game is in sight while a round runs`);
					assert.ok(inside(during.collect, during.screen), `${tab}: collect is in sight while a round runs`);
					await app.clock.set(app.clock.at() + 1500);
					const ending = page.getByRole('button', { name: 'End now', exact: true });
					await page.waitForFunction((pane) => parseFloat(document.querySelector(`${pane} .game .go .spent`)?.style.inlineSize) > 0, `[data-pane="${tab}-lesson"]`);
					await ending.click();
					await page.getByRole('button', { name: 'Reset', exact: true }).click();
					await settled(page, `[data-pane="${tab}-lesson"]`);

					// At the very bottom, the discussion, and Next its last thing, in sight.
					assert.equal(rest.aboutNext, 0, `${tab}: no Next in the intro`);
					assert.deepEqual(rest.cut, { about: 0, discuss: 0 }, `${tab}: the intro and the discussion are drawn whole, nothing cut off`);
					await scrollTo(page, past.scrollable);
					const end = await measure(page, tab);
					assert.ok(end.next, `${tab}: the discussion ends on Next`);
					assert.ok(end.next.top >= end.collect.bottom, `${tab}: Next is under collect`);
					assert.ok(inside(end.next, end.screen), `${tab}: Next is in sight at the bottom`);
					const button = page.locator(`[data-pane="${tab}-discuss"]`).getByRole('button', { name: `Next: ${NEXT[tab]}`, exact: true });
					assert.equal(await button.count(), 1, `${tab}: Next names the next lesson`);
				}
				assert.deepEqual(app.errors, []);
			},
			{ ...device, query: '?db=tutorial' }
		);
	});
}
