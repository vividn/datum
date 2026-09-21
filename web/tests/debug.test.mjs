// The debug widget, and the bar's feel as two live knobs (da-o2wm8).
//
// vividn, 2026-09-21: "A db named DEBUG, which has a layout preloaded into it
// this DEBUG widget. It only gets loaded for the DEBUG db. […] The DEBUG
// widget is only available from the DEBUG db, and is one of the premade
// widgets."
//
// There are three claims here and each is measured on the page:
//
// 1. the GATE — `?db=DEBUG` opens onto a layout that already holds the
//    widget, and no other database offers the kind in its chooser or draws
//    the knobs in a pane that names it;
// 2. the KNOBS — the ranges, the default beside each value, the copyable
//    pair, reset, and that both survive a reload;
// 3. the INSTRUMENT — moving a knob changes where the VERY NEXT release
//    comes to rest, with no reload. That is the whole point of the bead:
//    "the comparison he is making is between two feels a few seconds
//    apart", and a tool that needed a reload would answer nothing.
//
// The numbers themselves are `detent.test.mjs`'s, in node, where `rest`
// is a pure function of the tuning it is handed. What cannot be asked
// there is whether the LIVE pair reaches a real release, and that is 3.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { dragBar, timedDrag } from './support/bar.mjs';
import { choose } from './support/layout.mjs';

before(startApp);
after(stopApp);

/** The debug database, and the layout it carries. Both `defaults.ts`'s. */
const DEBUG = { query: '?db=DEBUG' };

/**
 * The same, on a screen tall enough that a long throw is not clamped.
 *
 * The one claim that needs ROOM: at `TAU = 1200` a release glides twenty
 * times as far as at 60, and a bar that runs out of page has stopped
 * saying anything about the number.
 */
const WIDE_DEBUG = { ...DEBUG, viewport: { width: 1280, height: 900 } };

/**
 * The widget itself, by the class it draws — never by a pane's title.
 *
 * The shipped layout is ONE widget, and a one-widget layout is lifted into
 * its root tab (`inline`, shell.ts): the pane that draws it is `shell` and
 * its `data-title` is whichever root tab is in front. So neither the pane
 * id nor the title names this widget, and `.debug-widget` does.
 */
const PANE = '.debug-widget';

/** Every pane on the page, by name — said out loud before anything is measured. */
const paneNames = (page) =>
	page.evaluate(() => [...document.querySelectorAll('section[data-pane]')].map((s) => s.dataset.pane));

/** The root bar's tabs: this device's open layouts, by name. */
const rootTabs = (page) => page.locator('.root-bar [role="tab"]').allInnerTexts();

/** How far the log is open, in px: the number the bar's whole travel is a slice of. */
const logPx = (page) =>
	page.evaluate(() =>
		Math.round(
			parseFloat(getComputedStyle(document.querySelector('.shell')).getPropertyValue('--log-height')) || 0
		)
	);

/**
 * Wait until the bar has stopped moving: five reads of ALL of it the same.
 *
 * `--shown` as well as `--log-height`, because below the notch the log is
 * 0 the whole way and only the output section is opening — so a watch on
 * the log alone returns mid-glide, and the next drag starts from wherever
 * the bar happened to be. That is how a release measured 10px instead of
 * 20: the apparatus was moving, not the rule.
 */
const settled = (page) =>
	page.evaluate(
		() =>
			new Promise((done) => {
				let last = '';
				let same = 0;
				const tick = () => {
					const style = getComputedStyle(document.querySelector('.shell'));
					const px = (name) => Math.round(parseFloat(style.getPropertyValue(name)) || 0);
					const now = `${px('--shown')}/${px('--log-height')}/${px('--rolled')}`;
					same = now === last ? same + 1 : 0;
					last = now;
					if (same > 4) return done();
					requestAnimationFrame(tick);
				};
				tick();
			})
	);

/**
 * Put the bar exactly ON the notch, whatever the tuning.
 *
 * Shut it first (a long drag down ends below every edge and is obeyed),
 * then nudge up: BELOW the notch there is nowhere to rest, so the
 * gesture's own direction decides and a nudge up always lands on the
 * notch — under any `DETENT`, `0` included. That independence is what
 * makes it a starting line rather than part of what is being measured.
 */
async function toNotch(page) {
	await dragBar(page, 400);
	await settled(page);
	await dragBar(page, -20);
	await settled(page);
	assert.equal(await logPx(page), 0, 'the bar did not come back to the notch');
}

/** Release 20px above the notch and report where it came to rest. */
async function throwUp(page) {
	await dragBar(page, -20);
	await settled(page);
	return logPx(page);
}

/**
 * Release, and report how long the bar went on moving and where it stopped.
 *
 * The GLIDE's own length is the second half of the trap: `glide` sets the
 * duration so the curve leaves at the finger's speed, `3 * distance /
 * speed` — and a free glide's distance IS `speed * TAU`, so that is
 * `3 * TAU` whatever the speed turned out to be. A ceiling frozen at
 * `3 * 320` would cut it short and the bar would arrive early, which is a
 * feel and not a position: nothing about where it STOPS would say so.
 */
const glided = (page) =>
	page.evaluate(
		() =>
			new Promise((done) => {
				const style = getComputedStyle(document.querySelector('.shell'));
				const read = () => {
					const px = (name) => Math.round(parseFloat(style.getPropertyValue(name)) || 0);
					return `${px('--shown')}/${px('--log-height')}/${px('--rolled')}`;
				};
				const started = performance.now();
				let moved = started;
				let last = '';
				let same = 0;
				const tick = () => {
					const now = read();
					if (now === last) same += 1;
					else {
						same = 0;
						moved = performance.now();
					}
					last = now;
					if (same > 4)
						return done({
							ms: Math.round(moved - started),
							at: Math.round(parseFloat(style.getPropertyValue('--log-height')) || 0)
						});
					requestAnimationFrame(tick);
				};
				tick();
			})
	);

/** The value a knob's number box holds. */
const knob = (page, name) => page.locator(`${PANE} input[aria-label="${name}"]`).inputValue();

/** Type `value` into a knob's number box, as a reader does. */
async function setKnob(page, name, value) {
	const box = page.locator(`${PANE} input[aria-label="${name}"]`);
	await box.fill(String(value));
	await box.blur();
	await page.waitForFunction(
		([pane, n, v]) => document.querySelector(`${pane} .pair`)?.textContent.includes(`${n}=${v}`),
		[PANE, name, String(value)]
	);
}

describe('the DEBUG database', () => {
	test('opens onto a layout that already holds the debug widget', async () => {
		await withApp(async (app) => {
			const { page } = app;
			// SAY WHAT IS THERE and fail on the name, not on its absence: a
			// probe that measures a missing widget reads as "the feature is
			// not there" whatever the reason.
			const tabs = await rootTabs(page);
			assert.ok(tabs.includes('debug'), `the root bar has no debug tab: ${tabs.join(', ')}`);
			assert.equal(
				await page.locator(PANE).count(),
				1,
				`the debug widget is not drawn; the panes are ${(await paneNames(page)).join(', ')}`
			);
			await page.locator(`${PANE} .pair`).waitFor();

			// The tree it opened is the DATABASE's, not a built-in: the row
			// is in the data db, carrying the kit that shipped it.
			const rows = JSON.parse((await app.run(['tail', 'layout', '--json', '-n', '10'])).stdout).rows;
			const shipped = rows.filter((r) => r.data?.kit_id === 'debug');
			assert.equal(shipped.length, 1, `the DEBUG db carries ${shipped.length} debug layouts`);
			assert.equal(shipped[0].data.name, 'debug');
			assert.equal(shipped[0].slug, 'layout:debug');
		}, DEBUG);
	});

	test('installs that layout once, not once per load', async () => {
		await withApp(async (app) => {
			await app.page.locator(`${PANE} .pair`).waitFor();
			for (const round of [1, 2]) {
				await app.reload();
				await app.page.locator(`${PANE} .pair`).waitFor();
				const rows = JSON.parse((await app.run(['tail', 'layout', '--json', '-n', '10'])).stdout).rows;
				assert.equal(
					rows.filter((r) => r.data?.kit_id === 'debug').length,
					1,
					`reload ${round} installed the layout again`
				);
			}
		}, DEBUG);
	});

	// The gate is the whole reason vividn changed the design — "That seems
	// safer" — so it is asserted from the reader's own door: the chooser.
	test('is the only database whose chooser offers the kind', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const offered = async () => {
				await choose(page, 'tail', 'Change Widget');
				await page.locator('dialog.chooser').waitFor();
				const kinds = await page
					.locator('dialog.chooser [role="group"][aria-label="Widgets"] button')
					.allInnerTexts();
				await page.keyboard.press('Escape');
				await page.locator('dialog.chooser').waitFor({ state: 'detached' });
				return kinds;
			};

			const here = await offered();
			assert.ok(here.includes('tail'), `the chooser offered nothing familiar: ${here.join(', ')}`);
			assert.ok(!here.includes('debug'), `the default database offered debug: ${here.join(', ')}`);

			// The SAME page, the same menu, one query away: the gate is the
			// database and nothing else about the device or the layout.
			await page.goto(`${page.url()}?db=DEBUG`);
			await page.locator('.root-bar [role="tab"]').first().waitFor();
			await page.locator('.root-bar [role="tab"]', { hasText: 'day' }).click();
			const inDebug = await offered();
			assert.ok(inDebug.includes('debug'), `DEBUG did not offer it either: ${inDebug.join(', ')}`);
			assert.deepEqual(app.errors, []);
		});
	});

	// ⚠ A saved layout is the DEVICE's — it lives in `_internal` and opens
	// in every database this device visits — so a layout naming the debug
	// widget WILL be opened somewhere the chooser would not have offered
	// it. An empty pane and a crash were the other two possible answers;
	// neither is one a reader can act on, so the widget says where its
	// contents live.
	test('a saved layout holding the widget says where it lives, elsewhere', async () => {
		await withApp(async (app) => {
			const root = {
				id: 'mine',
				tabs: [{ id: 'mine-tab', content: { id: 'mine-widget', kind: 'debug', settings: {} } }],
				active: 0,
				bar: { edge: 'top', align: 'start' }
			};
			const saved = await app.run([
				'--db',
				'_internal',
				'occur',
				'layout',
				'--id',
				'%name',
				'-k',
				'name="mine"',
				'-k',
				'version=1',
				'-k',
				`root=${JSON.stringify(root)}`
			]);
			assert.equal(saved.code, 0, `could not save the layout: ${saved.stderr}`);

			await app.page.goto(`${app.page.url().replace(/\/$/, '')}/mine`);
			await app.page.locator('.prompt-bar input:not([disabled])').waitFor();
			await app.page.locator(PANE).waitFor();
			assert.match(
				await app.page.locator(PANE).innerText(),
				/the debug widgets live in the DEBUG database/,
				'a debug widget outside DEBUG drew something other than its notice'
			);
			assert.equal(await app.page.locator(`${PANE} .pair`).count(), 0, 'the knobs were drawn anyway');
			assert.deepEqual(app.errors, []);
		});
	});
});

describe('the two knobs', () => {
	test('read their defaults, their ranges, and the copyable pair', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await page.locator(`${PANE} .pair`).waitFor();

			assert.equal(await knob(page, 'TAU'), '320');
			assert.equal(await knob(page, 'DETENT'), '24');
			assert.equal(await page.locator(`${PANE} .pair`).innerText(), 'TAU=320 DETENT=24');
			assert.match(await page.locator(`${PANE}`).innerText(), /default 320/);
			assert.match(await page.locator(`${PANE}`).innerText(), /default 24/);

			// Wide enough that the ends are boring, and `DETENT = 0` reachable.
			const range = (name, attr) =>
				page.locator(`${PANE} input[aria-label="${name} slider"]`).getAttribute(attr);
			assert.deepEqual([await range('TAU', 'min'), await range('TAU', 'max')], ['60', '1200']);
			assert.deepEqual([await range('DETENT', 'min'), await range('DETENT', 'max')], ['0', '80']);
		}, DEBUG);
	});

	test('keep what was set across a reload, and reset returns to 320 / 24', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await page.locator(`${PANE} .pair`).waitFor();
			await setKnob(page, 'TAU', 900);
			await setKnob(page, 'DETENT', 0);
			assert.equal(await page.locator(`${PANE} .pair`).innerText(), 'TAU=900 DETENT=0');

			await app.reload();
			await page.locator(`${PANE} .pair`).waitFor();
			assert.equal(
				await page.locator(`${PANE} .pair`).innerText(),
				'TAU=900 DETENT=0',
				'the tuning did not survive the reload'
			);

			await page.locator(`${PANE} button`, { hasText: 'reset to defaults' }).click();
			await page.waitForFunction(
				(pane) => document.querySelector(`${pane} .pair`)?.textContent === 'TAU=320 DETENT=24',
				PANE
			);
			// And reset means UNSET: nothing stored, so a fresh install is
			// the constants exactly.
			assert.equal(
				await page.evaluate(() => localStorage.getItem('datum.bar.tuning.v1')),
				null,
				'reset left an override behind'
			);
		}, DEBUG);
	});

	// A pair another build wrote, or one edited by hand: brought inside the
	// ranges on the way IN (canary on PR #69). The ranges live beside the
	// clamp, so the box a reader sees can never hold a value outside its
	// own min and max.
	test('a stored pair from outside the ranges is brought inside them', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await page.locator(`${PANE} .pair`).waitFor();
				assert.equal(await page.locator(`${PANE} .pair`).innerText(), 'TAU=1200 DETENT=0');
				assert.equal(await knob(page, 'TAU'), '1200');
				assert.equal(await knob(page, 'DETENT'), '0');
				assert.deepEqual(app.errors, []);
			},
			{
				...DEBUG,
				prepare: (context) =>
					context.addInitScript(() => {
						try {
							localStorage.setItem('datum.bar.tuning.v1', JSON.stringify({ tau: 99999, detent: -5 }));
						} catch {
							/* blocked: the defaults, which is the other test */
						}
					})
			}
		);
	});

	test('works for the visit when the store is blocked', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await page.locator(`${PANE} .pair`).waitFor();
				assert.equal(await page.locator(`${PANE} .pair`).innerText(), 'TAU=320 DETENT=24');
				await setKnob(page, 'TAU', 600);
				assert.equal(await page.locator(`${PANE} .pair`).innerText(), 'TAU=600 DETENT=24');
				assert.deepEqual(app.errors, [], `the blocked store threw at the page: ${app.errors[0]}`);
			},
			{
				...DEBUG,
				prepare: (context) =>
					context.addInitScript(() => {
						Object.defineProperty(window, 'localStorage', {
							configurable: true,
							get() {
								throw new DOMException('storage is blocked', 'SecurityError');
							}
						});
					})
			}
		);
	});
});

// ⛔ THE HALF THAT MAKES IT AN INSTRUMENT. A knob that needed a reload
// would be worthless, because the comparison is between two feels a few
// seconds apart — and a `GLIDE_MAX_MS` frozen at import would have let the
// number move while the bar did not, which is worse than no tool at all.
describe('a knob reaches the very next release', () => {
	test('DETENT decides where the same release comes to rest, with no reload', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await page.locator(`${PANE} .pair`).waitFor();

			// AT THE DEFAULT: a release 20px above the notch is inside the
			// 24px detent, so the notch takes it and the log shuts.
			await toNotch(page);
			assert.equal(await throwUp(page), 0, 'the default detent did not catch a 20px release');

			// NO DETENTS AT ALL — the same gesture, the knob moved between
			// them and nothing else: the bar now rests where it was let go.
			await setKnob(page, 'DETENT', 0);
			await toNotch(page);
			assert.equal(await throwUp(page), 20, 'DETENT = 0 still snapped the release to the notch');

			// And back again, in the same page: the knob is live both ways.
			await setKnob(page, 'DETENT', 24);
			await toNotch(page);
			assert.equal(await throwUp(page), 0, 'raising the detent again did not catch');
			assert.deepEqual(app.errors, []);
		}, DEBUG);
	});

	// ⚑ AND THIS IS WHAT MAKES THE WHOLE THING WORK. The widget lives in
	// the DEBUG database; the BAR it tunes is on every screen. So the pair
	// is the DEVICE's and not that database's — he tunes in DEBUG, switches
	// to his own, and the bar he uses all day carries what he just chose.
	// Scoped to the database he could only ever feel it against a handful
	// of fake entries, which would answer nothing.
	test('the value he set in DEBUG is the value his own database\u2019s bar has', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await page.locator(`${PANE} .pair`).waitFor();
			await setKnob(page, 'DETENT', 0);

			// Out of DEBUG, by the one piece of state that says which
			// database this is (da-vk1x): the query.
			await page.goto(page.url().split('?')[0]);
			await page.locator('.prompt-bar input:not([disabled])').waitFor();
			await page.locator('.root-bar [role="tab"]').first().waitFor();
			assert.equal(await page.locator(PANE).count(), 0, 'the debug widget came along too');

			await toNotch(page);
			assert.equal(
				await throwUp(page),
				20,
				'the bar he uses all day did not carry the DETENT he set in DEBUG'
			);
			assert.deepEqual(app.errors, []);
		}, DEBUG);
	});

	// ⛔ TAU'S OWN TRAP, and it is the one that would have made his answer
	// wrong quietly: `GLIDE_MAX_MS` was `3 * TAU` computed once at import,
	// so a raised TAU moved the number while the glide stayed clamped by a
	// ceiling derived from 320 — the bar would not do what the knob said.
	//
	// A THROW is the only way to ask, because a synthetic drag's moves all
	// carry one stamp and so carry no speed at all: `timedDrag` puts a
	// frame between them. The same push each time, the knob moved between
	// them and nothing else.
	test('TAU decides how far the same throw carries, and how long it glides', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await page.locator(`${PANE} .pair`).waitFor();

			const thrown = async (tau) => {
				await setKnob(page, 'TAU', tau);
				await toNotch(page);
				await timedDrag(page, -60);
				return glided(page);
			};

			const brief = await thrown(60);
			const middling = await thrown(320);
			const long = await thrown(1200);

			// WHERE it stops: `speed * TAU` past the 60px the finger placed.
			const far = `60 → ${brief.at}px, 320 → ${middling.at}px, 1200 → ${long.at}px`;
			assert.ok(brief.at >= 60, `a 60px drag left the log ${brief.at}px tall (${far})`);
			assert.ok(middling.at > brief.at + 40, `TAU 320 did not beat TAU 60 (${far})`);
			assert.ok(long.at > middling.at + 200, `TAU 1200 did not beat TAU 320 (${far})`);

			// And HOW LONG it keeps going, which is the trap itself: a free
			// glide runs `3 * TAU`, so 1200 is about 3.6s — and a ceiling
			// frozen at `3 * 320` could not let it past 960ms however high
			// the number went.
			const took = `60 → ${brief.ms}ms, 320 → ${middling.ms}ms, 1200 → ${long.ms}ms`;
			assert.ok(
				long.ms > 1500,
				`a TAU=1200 throw was over in ${long.ms}ms; a ceiling frozen at 3 × 320 ends every glide by 960 (${took})`
			);
			assert.ok(brief.ms < long.ms / 2, `TAU 60 glided as long as TAU 1200 (${took})`);
			assert.deepEqual(app.errors, []);
		}, WIDE_DEBUG);
	});
});
