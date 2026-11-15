// A game's «How to play» (da-op236). vividn, 2026-11-15: «add a "how to play"
// button to each game that starts a tour, explains what will happen and how to
// record the data», «For the on/off tour mention that you are a multitasker»,
// and «How to play can be above start, moving the start button down a little
// bit so that the two buttons are centered vertically».
//
// On each game's tab: the button above Start (the change game's cue), the pair
// centered in their column; pressed from the keyboard, its tour rings the game,
// the collect widget, the nowview and Start (the box again, for change), in
// that order, and at its end no round has begun and nothing is logged.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP, PHONE } from './support/layout.mjs';
import { lessonTab } from './support/tutorial.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

/** The sentence on screen, and whether the ring stands round the drawn element `selector` names. */
const stop = (page, selector) =>
	page.evaluate((selector) => {
		const ring = document.querySelector('.tour .ring')?.getBoundingClientRect();
		const el = [...document.querySelectorAll(selector)].map((e) => e.getBoundingClientRect()).find((r) => r.width > 0);
		const round = !!ring && !!el && [ring.left - el.left, ring.top - el.top, el.right - ring.right, el.bottom - ring.bottom].every((d) => Math.abs(d + 4) < 1);
		return { say: document.querySelector('.tour .say p')?.textContent ?? null, round };
	}, selector);

for (const [what, device] of [
	['a phone', PHONE],
	['a desktop', DESKTOP]
])
	test(`${what}: each game’s «How to play» stands above Start, centered with it, and tours the game, collect, the nowview and Start, starting no round`, async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				for (const tab of ['moment', 'on-off', 'change']) {
					const lesson = await page.evaluate((tab) => window.datum.tutorial.lessons.find((l) => l.name === tab), tab);
					const logged = () =>
						Promise.all(lesson.fields.map((f) => app.run(['tail', f, '--json', '-n', '50']).then((r) => JSON.parse(r.stdout).rows.length))).then((n) =>
							n.reduce((a, b) => a + b, 0)
						);
					const pane = `[data-pane="${tab}-lesson"]`;
					await lessonTab(page, tab).click();
					const how = page.locator(pane).getByRole('button', { name: 'How to play', exact: true });
					await how.waitFor();
					const before = await logged();

					// Above Start (the cue, where the game starts itself), the space over the one the space under the other.
					const under = tab === 'change' ? `${pane} .controls .cue` : `${pane} .controls .go`;
					const [controls, top, bottom] = await Promise.all(
						[`${pane} .controls`, `${pane} .controls .how`, under].map((s) => page.locator(s).evaluate((el) => el.getBoundingClientRect().toJSON()))
					);
					assert.ok(top.bottom <= bottom.top, `${tab}: How to play above ${under}`);
					const over = top.top - controls.top;
					const below = controls.bottom - bottom.bottom;
					assert.ok(over > 0 && Math.abs(over - below) <= 2, `${tab}: the pair centered in its column, ${over} over and ${below} under`);

					// Its words inside the button, and the button inside the box (da-hv9tk): the words ran 17 px past it on a phone.
					const fit = await page.locator(`${pane} .controls .how`).evaluate((el) => {
						const box = el.closest('.game').getBoundingClientRect();
						const r = el.getBoundingClientRect();
						return { past: el.scrollWidth - el.clientWidth, right: r.right, edge: box.right - 1 };
					});
					assert.ok(fit.past <= 0, `${tab}: How to play's words ${fit.past} px past its button`);
					assert.ok(fit.right <= fit.edge, `${tab}: How to play's right ${fit.right} inside the box's ${fit.edge}`);

					// From the keyboard.
					await how.focus();
					await page.keyboard.press('Enter');
					const stops = [`${pane} .stage`, `[data-pane="${tab}-fields"]`, `[data-pane="${tab}-now"]`, tab === 'change' ? `${pane} .stage` : `${pane} .go`];
					for (const [i, at] of stops.entries()) {
						const s = await until(() => stop(page, at), (s) => s.round && !!s.say);
						if (tab === 'on-off' && i === 0) assert.match(s.say, /multitasker/, 'on/off: you are a multitasker');
						const next = page.locator('.tour .say .next');
						assert.equal(await next.innerText(), i === stops.length - 1 ? 'done' : 'next', `${tab}: stop ${i + 1} of ${stops.length}`);
						await next.click();
					}
					await page.locator('.tour').waitFor({ state: 'detached' });

					assert.equal(await page.locator(`${pane} .game`).getAttribute('data-phase'), 'neutral', `${tab}: no round begun`);
					if (tab !== 'change') await page.locator(pane).getByRole('button', { name: 'Start', exact: true }).waitFor();
					assert.equal(await logged(), before, `${tab}: nothing logged`);
				}
				assert.deepEqual(app.errors, []);
			},
			{ ...device, query: '?db=tutorial' }
		);
	});
