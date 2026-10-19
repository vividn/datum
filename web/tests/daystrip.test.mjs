// The day strip: a stack of days, one picture each, stitched by the web.
//
// What a reader is doing with this is a comparison — is today like
// yesterday, like last Tuesday — so the assertions are about the two
// things a comparison needs and nothing else: that the days above today
// are the DAYS they claim to be, and that they are drawn against the same
// x-axis today is drawn against. A test that counted `<img>` elements
// before and after would pass on a stack of five copies of today
// (da-cgyh), so every day here is read by its date stamp, and the
// alignment is read off the gridlines rather than eyeballed.
//
// Since da-iu9i the past is ONE PICTURE PER DAY again — the snappiness of
// a band that lands on its own — and what makes the column read as one
// picture is the WEB's work: the same argv for every band, the strip's own
// gridlines across the whole stack at the x `--show json` reports, one
// background, and days that touch across a 1px divider. So the stitching
// is asserted directly: the strip's 12:00 line lands on each band's own
// 12:00 gridline, and on the x the CLI says it should.
//
// The date stamps and the hour ruler are the strip's too now, drawn beside
// and under the bands rather than inside them — so a day is a stamp in the
// strip's own column, and the ruler is one row, once, under the bottom band.
//
// The jagged line is the third thing: it is a claim about what is
// currently ON SCREEN — the day above the divider is not the newest one —
// so it is asserted against the stack's scroll position, driven both ways.
//
// Since da-zb0d the strip has no handle for its own height: its BOX says
// how tall it is, and how tall it is says how much past it SHOWS — which
// since da-bewc is not how much it HOLDS. The stack keeps a fortnight of
// past above the box so that there is somewhere to scroll to; `heldFor`
// is that arithmetic, `onScreen` is what the box shows of it, and the
// scrolling itself is driven by a real wheel and a real finger. What a
// reader sets on the strip itself — the day height, the width of the date
// column, whether today is frozen at the bottom — is a prop in and a
// `change` event out, which is asserted from the outside: what the strip's
// owner heard, and the picture's own SVG `height`, not a class name that
// claims to mean it. The strip remembers nothing, and that is asserted too.
//
// The day height — its follow and its hold, the handle, the freeze — and
// the strip's pin and its row on a phone are `daystrip-height.test.mjs`, a
// file of its own for node's per-file budget (da-4j2hw); the helpers both
// read are `support/daystrip.mjs`.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import {
	sharedApp,
	startApp,
	stopApp,
	seedLines,
	todayAt
} from './support/app.mjs';
import { choose, chooseRoot } from './support/layout.mjs';
import {
	SAVE_OPTIONS,
	settledRoom,
	labelFor,
	colorOf,
	band,
	stamps,
	gridlines,
	svgHeight,
	metrics,
	pastHeight,
	setBox,
	settledAt,
	boxOf,
	owed,
	FORTNIGHT,
	heldFor,
	onScreen,
	sizeFor,
	sizeWhole,
	waitForDays,
	valueOf,
	stripSettings,
	heard,
	say,
	wheelOverPast,
	dragOverPast,
	scrollToDay,
	scrolledFrom,
	viewsOf,
	todayBand,
	stackScroll,
	jagged,
	PHONE
} from './support/daystrip.mjs';

before(startApp);
after(stopApp);

// ONE APP per set of options for most of this file (da-6b3dj, sharedApp in
// support/app.mjs). A test that only sizes the box, writes the tree (`say`)
// or logs rows is `solo`: the box is set before anything is measured, and
// `solo` restores `main` and empties the database. A test that moves the
// strip ITSELF — the handle, the keyboard, the freeze toggle, a zoom — sets
// a session option on the widget, which `solo` does not put back, so it
// takes a ROOM, whose strip is a new widget — as does a test that scrolls
// a stack the test before it scrolled, since `solo` keeps `main`'s strip and
// its scroll position. Which is why the helpers (`support/daystrip.mjs`)
// read and write the strip of the root tab in FRONT. A moved clock is put back by the next
// test's `solo` or `room`, and the one reload is the last test on its app
// (da-d0tfm). What still opens an app of its own has a screen no other test
// shares, or a `prepare`.
const shared = sharedApp();
const clocked = sharedApp({ clock: todayAt(12) });
/** A screen tall enough for sixteen of the shortest days above today. */
const desk = sharedApp({ viewport: { width: 1280, height: 1400 } });
const phone = sharedApp(PHONE);

describe('the day strip', () => {
	test('the box says how many days: today, and the past that fits above it, a picture each', async () => {
		await shared.solo(async (app) => {
			const nowMs = await app.page.evaluate(() => Date.now());

			// A short box: today, and the day or two above it that fit.
			await setBox(app, 300);
			const few = await owed(app);
			assert.ok(few >= 1, `300px holds today and nothing else: ${few} days`);
			await waitForDays(app, heldFor(few));
			const short = await onScreen(app);

			// A tall box: more of them.
			await setBox(app, 600);
			const many = await owed(app);
			assert.ok(many > few, `600px holds no more days than 300px: ${many} vs ${few}`);
			await waitForDays(app, heldFor(many));

			// The box is what SHOWS them: the taller one has more of the
			// past on screen, and neither shows more than it has room for.
			const tall = await onScreen(app);
			assert.ok(tall.length > short.length, `600px shows no more days than 300px: ${tall} vs ${short}`);
			assert.ok(tall.length <= many, `600px shows ${tall.length} days in room for ${many}`);

			// ONE PICTURE PER DAY (da-iu9i), and each one claims one day.
			const past = app.page.locator('.stack .band');
			assert.equal(await past.count(), heldFor(many), 'the past is not a picture a day');
			assert.deepEqual(
				await past.evaluateAll((els) => els.map((b) => b.dataset.days)),
				Array.from({ length: heldFor(many) }, () => '1')
			);

			// And each day is the day it claims: the days immediately before
			// today, oldest at the top, today's stamp last.
			assert.deepEqual(
				await stamps(app),
				Array.from({ length: heldFor(many) + 1 }, (_, i) => labelFor(nowMs, heldFor(many) - i)),
				'the strip stamped the wrong days'
			);

			// Today's band is still the live one: an entry logged now lands
			// in it on the epoch tick, and not in the past.
			await app.submit('switch mood happy -t -5');
			const happy = colorOf('happy');
			await app.page.waitForFunction(
				(c) =>
					document
						.querySelector('.band[data-days-ago="0"] svg.svg')
						.outerHTML.includes(c),
				happy
			);
			await app.idle();
			assert.ok(!(await band(app, 1)).includes(happy), 'yesterday drew an entry logged today');

			// Shrunk back, the days that no longer fit go off screen — the
			// box is a height, not a ratchet.
			await setBox(app, 300);
			await waitForDays(app, heldFor(few));
			assert.deepEqual(await onScreen(app), short);

			// A box exactly today's band, seam and ruler still holds the past
			// above it, to scroll up into (da-2hd21).
			await sizeFor(app, 0);
			assert.equal(await app.page.locator('.stack .band').count(), FORTNIGHT);
		});
	});

	// The bead's own case (da-iu9i): five days of the reader's data, and
	// five separate `dayview` runs of ONE day each — the snappiness is
	// that a band lands on its own, not that the past arrives at once.
	test('five days is five one-day dayviews, each asked bands-only', async () => {
		await clocked.solo(
			async (app) => {
				await seedLines(app, [...[1, 2, 3, 4].map((back) => `occur coffee -d -${back}`), 'occur coffee']);
				await sizeWhole(app, 5);
				await app.idle();

				const seen = await app.spyOnRunner();
				// A fresh day height is a fresh set of pictures: no argv is
				// served from the cache, so what runs is what a first draw runs.
				const was = await valueOf(app);
				await say(app, { dayHeight: was + 7 });
				await app.page.waitForFunction(
					(want) =>
						Number(document.querySelector('button.day-grip')?.getAttribute('aria-valuenow')) === want,
					was + 7
				);
				await waitForDays(app, heldFor(5));
				// The minute tick re-runs today and the geometry (da-r61i), so
				// a window across :00 counted today twice (da-6ehq). The clock
				// is pinned below; the tick's own re-check is driven here, in
				// the window, to show the pin holds — see «one more day».
				await app.page.evaluate(async () => {
					document.dispatchEvent(new Event('visibilitychange'));
					await new Promise((resolve) => requestAnimationFrame(() => resolve()));
				});
				await app.idle();

				const views = (await seen()).filter((argv) => argv[0] === 'dayview');
				const pictures = views.filter((argv) => !argv.includes('json'));
				const flag = (argv, name) => argv[argv.indexOf(name) + 1];
				// Every picture is one day, drawn bands-only: no date column,
				// no ruler, no outer margin — the web draws all three.
				for (const argv of pictures) {
					assert.equal(flag(argv, '--days'), '1', `not one day: ${argv.join(' ')}`);
					assert.equal(flag(argv, '--label-width'), '0', `a date column: ${argv.join(' ')}`);
					assert.equal(flag(argv, '--time-axis-height'), '0', `a ruler: ${argv.join(' ')}`);
					assert.equal(flag(argv, '--margin'), '0', `an outer margin: ${argv.join(' ')}`);
				}
				// The days on the stack, and today, each exactly once.
				const ends = pictures
					.map((argv) => (argv.includes('--end-date') ? Number(flag(argv, '--end-date')) : 0))
					.sort((a, b) => b - a);
				assert.deepEqual(
					ends,
					Array.from({ length: heldFor(5) + 1 }, (_, i) => (i === 0 ? 0 : -i)),
					'not one picture per day'
				);
				// And ONE geometry run for all of them: the gridlines, the
				// ruler and the stamps come off a single `--show json`.
				assert.equal(views.length - pictures.length, 1, 'the geometry was asked for more than once');
			}
		);
	});

	test('the strip draws the date column and the ruler, and its gridlines run through every band', async () => {
		await shared.solo(async (app) => {
			await sizeWhole(app, 5);
			const nowMs = await app.page.evaluate(() => Date.now());

			// Five stamps above today's, in order, in the strip's own column
			// — not inside any picture, which is asked for with no date
			// column at all.
			const shown = await onScreen(app);
			assert.ok(shown.length >= 5, `five whole days do not show: ${shown}`);
			assert.deepEqual(
				(await stamps(app)).slice(-6),
				Array.from({ length: 6 }, (_, i) => labelFor(nowMs, 5 - i))
			);
			assert.ok(!(await band(app, 1)).includes('date-label'), 'a picture carried date labels');
			assert.ok(!(await band(app, 0)).includes('tick-label'), 'a picture carried the ruler');

			// The ruler is one row, once, under the bottom band — and it
			// names hours.
			const ruler = app.page.locator('.strip .axis svg.ruler');
			assert.equal(await ruler.count(), 1, 'the ruler is not drawn exactly once');
			const named = await ruler.locator('text.tick-label').allTextContents();
			assert.ok(named.length >= 4, `the ruler named ${named.length} hours`);
			// A whole day ends on midnight, so the ruler closes on it, as
			// the CLI's axis does (da-zyfbl).
			assert.equal(named.at(-1), '00', `the ruler does not close on midnight: ${named}`);
			const day = named.slice(0, -1);
			assert.deepEqual([...day].sort(), day, 'the ruler runs backwards');

			// The stitching. The strip's own gridlines sit at the x the CLI
			// reports for them, and land on each band's own 12:00 line.
			const geometry = await app.page.evaluate(async () => {
				const grip = document.querySelector('button.day-grip');
				const width = Math.round(document.querySelector('.band').getBoundingClientRect().width);
				const result = await window.datum.runQuiet([
					'dayview', '--width', String(width),
					'--day-height', grip.getAttribute('aria-valuenow'),
					'--margin', '0', '--label-width', '0', '--time-axis-height', '0',
					'--days', '1', '--show', 'json'
				]);
				return JSON.parse(result.stdout).hours;
			});
			const drawn = await app.page
				.locator('svg.gridlines line.gridline')
				.evaluateAll((els) => els.map((l) => Number(l.getAttribute('x1'))));
			assert.equal(drawn.length, geometry.length, 'the strip drew a different number of gridlines');
			for (const [i, hour] of geometry.entries())
				assert.ok(
					Math.abs(drawn[i] - hour.x) <= 1,
					`the strip's ${hour.hour}:00 line is at ${drawn[i]}, the CLI says ${hour.x}`
				);

			// On the screen, in EVERY band: the strip's noon line lands on
			// the noon gridline datum drew inside each picture.
			const noon = await app.page.evaluate(() => {
				const web = document
					.querySelector('svg.gridlines line.gridline[data-hour="12"]')
					.getBoundingClientRect().x;
				const bands = [...document.querySelectorAll('.band')].map((b) => {
					const lines = [...b.querySelectorAll('svg.svg line.gridline')];
					return {
						daysAgo: b.dataset.daysAgo,
						x: lines[12]?.getBoundingClientRect().x ?? null
					};
				});
				return { web, bands };
			});
			assert.ok(noon.bands.length >= 6, `only ${noon.bands.length} bands drawn`);
			for (const { daysAgo, x } of noon.bands)
				assert.ok(
					x !== null && Math.abs(x - noon.web) <= 1,
					`the band ${daysAgo} days back draws noon at ${x}, the strip at ${noon.web}`
				);
		});
	});

	test('the stack holds a fortnight above the box, whether or not the box holds whole days', async () => {
		await shared.solo(async (app) => {
			// Room for three whole days. The box is three days tall — and
			// the stack is a fortnight, which is what there is to scroll to.
			// It used to be three days, to the pixel, which left the wheel
			// and the finger nothing to move (da-bewc).
			await sizeWhole(app, 3);
			const m = await metrics(app);
			const whole = await stackScroll(app);
			assert.equal(svgHeight(await band(app, 1)), m.band, 'datum drew a band another height');
			assert.equal(whole.client, pastHeight(m, 3), `the stack is not three days tall: ${JSON.stringify(whole)}`);
			assert.equal(whole.total, pastHeight(m, FORTNIGHT));
			assert.ok(
				whole.total - whole.client > 200,
				`three whole days of box leave nothing to scroll: ${JSON.stringify(whole)}`
			);

			// Half a day less and the top one is partly revealed: the same
			// fortnight behind a shorter window on it.
			await sizeFor(app, 3);
			const part = await stackScroll(app);
			assert.equal(part.total, pastHeight(m, FORTNIGHT));
			assert.ok(part.client < whole.client, `the shorter box is not shorter: ${JSON.stringify(part)}`);
		});
	});

	test('a wheel over the past scrolls back through the days, today staying put', async () => {
		await shared.solo(async (app) => {
			// A box three days tall: a date stamp fits in it whole, so there
			// is something legible to arrive at.
			await sizeFor(app, 3);
			const rest = await stackScroll(app);
			const before = await onScreen(app);
			assert.ok(before.length > 0, `no day is legible in the box: ${JSON.stringify(rest)}`);
			const today = await todayBand(app);

			// One turn of the wheel, upward. The stack goes back in time —
			// scrollTop falls, because the oldest day is at the top — and it
			// goes a long way: before da-bewc the whole range was the 38px
			// of the one day the box did not quite show.
			await wheelOverPast(app, -600);
			await app.page.waitForFunction(
				(was) => document.querySelector('.stack').scrollTop <= was - 200,
				rest.top
			);
			const moved = await stackScroll(app);
			assert.ok(
				rest.top - moved.top >= 200,
				`the wheel moved the stack ${rest.top - moved.top}px: ${JSON.stringify(moved)}`
			);

			// And a day OLDER than any that was on screen now is — the
			// point of scrolling, as against merely moving.
			const after = await onScreen(app);
			assert.ok(
				after.some((daysAgo) => daysAgo > Math.max(...before)),
				`nothing older came into view: ${after} after ${before}`
			);

			// Today is pinned through all of it: it is not in the scroller.
			assert.deepEqual(await todayBand(app), today, "today's band moved");

			// Nothing else took the gesture: neither the pane's own body
			// (da-mbz2) nor the page.
			const stole = await app.page.evaluate(() => {
				const body = document.querySelector('.strip').closest('.body');
				return { pane: body.scrollTop, page: window.scrollY };
			});
			assert.deepEqual(stole, { pane: 0, page: 0 });
		});
	});

	test('a finger over the past scrolls it on a phone, and the page does not move', async () => {
		// The phone as it comes (390x640, da-hkpd): the box the default
		// layout gives the strip, which is the box vividn could not scroll.
		await phone.solo(
			async (app) => {
				// As it comes: a strip nobody has touched holds a past to
				// scroll to (da-2hd21), where until then it showed exactly one
				// day and had nothing above today. ANCHORED before the scroll
				// is read: `anchor` puts the newest back against the divider a
				// beat after the bands land, so a `scrollTop` read before that
				// is 0 — and `<= 0 - 200` is a wait that cannot end.
				await app.page.waitForFunction(() => {
					const s = document.querySelector('.stack');
					return s && s.scrollTop + s.clientHeight >= s.scrollHeight - 1 && s.scrollTop > 0;
				});
				await app.idle();
				const rest = await stackScroll(app);
				assert.ok(
					rest.total - rest.client >= 200,
					`the phone's strip holds nothing to scroll: ${JSON.stringify(rest)}`
				);

				// A finger pulled DOWN the stack is back in time — today with
				// it, unfrozen as the phone's strip comes (da-2hd21).
				await dragOverPast(app, 300);
				await app.page.waitForFunction(
					(was) => document.querySelector('.stack').scrollTop <= was - 200,
					rest.top
				);
				const moved = await stackScroll(app);
				assert.ok(
					rest.top - moved.top >= 200,
					`the finger moved the stack ${rest.top - moved.top}px: ${JSON.stringify(moved)}`
				);
				assert.equal(await app.page.evaluate(() => window.scrollY), 0, 'the page scrolled instead');
			}
		);
	});

	test('one more day draws the day that was gained, and nothing else', async () => {
		// The shortest day, so a box past the fortnight fits on a screen:
		// inside the fortnight the stack holds the same days however the
		// box moves, and no argv changes, so nothing is re-drawn.
		//
		// This is a count of what ran, so the OTHER thing that runs a
		// `dayview` has to be held still: the minute tick (da-r61i) re-runs
		// today's band and the geometry — two `dayview`s with no
		// `--end-date` — on every wall-clock minute boundary, and a window
		// that happened to straddle :00 counted those two as well. That is
		// what reddened main's run 296 (da-y7r9); it is a clock, so the
		// clock is pinned, as this file's other clock-sensitive tests pin
		// theirs. A `Date.now()` that never changes minute never ticks.
		await settledRoom(desk, 'gained',
			async (app) => {
				await say(app, { dayHeight: 40 });
				await app.page.waitForFunction(
					() => Number(document.querySelector('button.day-grip')?.getAttribute('aria-valuenow')) === 40
				);
				await sizeFor(app, 15);
				await app.idle();
				const seen = await app.spyOnRunner();

				await sizeFor(app, 16);
				await app.idle();

				// The pin, proved rather than assumed: `clock.ts` re-checks
				// the minute on `visibilitychange` — the very `tick()` its
				// timer calls, and the path the flake was reproduced through
				// — so drive it here, inside the window. Under the pin it
				// finds the same minute and draws nothing.
				await app.page.evaluate(async () => {
					document.dispatchEvent(new Event('visibilitychange'));
					// Past Svelte's flush, so a tick that DID fire would have
					// started its runs before `idle` below looks for them.
					await new Promise((resolve) => requestAnimationFrame(() => resolve()));
				});
				await app.idle();

				// Every dayview the strip ran for the taller box: the ONE day
				// it gained. Every other day, and today, and the geometry, are
				// the same argv as before and came out of the cache.
				const flag = (argv, name) => (argv.includes(name) ? argv[argv.indexOf(name) + 1] : null);
				const views = (await seen())
					.filter((argv) => argv[0] === 'dayview')
					.map((argv) => [flag(argv, '--days'), flag(argv, '--end-date')]);
				assert.deepEqual(views, [['1', '-16']]);
			}
		);
	});

	// da-dtwz. A new epoch re-draws the days the reader can SEE and lets
	// the rest keep the picture they have — a fortnight of bands is a
	// fortnight of CLI processes in front of whatever is typed next. What
	// that left open is the day the reader scrolls BACK to: the next beat
	// is the next command, so the entry just logged for ten days ago was
	// not in the band, and would not be until something else ran.
	test('a stale band scrolled into view is re-drawn, and it alone', async () => {
		await clocked.solo(async (app) => {
			// A box a hair SHORT of one whole day, so the stack's viewport
			// holds the band scrolled to it and not a pixel of the one above
			// — what runs below is then a count of that one band.
			const m = await metrics(app);
			await setBox(app, m.band + m.seam + m.axis + m.pitch - 2 + m.chrome);
			await waitForDays(app, heldFor(1));
			await app.idle();

			await app.submit('occur coffee -d -10');
			await app.idle();

			// Off screen, so it still holds the picture drawn before the
			// entry existed — which is the whole point of keeping it.
			assert.ok(!(await band(app, 10)).includes('point-bg'), 'day 10 was re-drawn off screen');

			const seen = await app.spyOnRunner();
			await scrollToDay(app, 10);

			// Within a beat of coming into view, the dot is there.
			await app.page.waitForFunction(() =>
				document.querySelector('.band[data-days-ago="10"] svg.svg')?.outerHTML.includes('point-bg')
			);
			await app.idle();

			// And exactly the band that came into view was asked for: not
			// the fortnight above it, and not the geometry, which is at this
			// epoch already.
			assert.deepEqual(await viewsOf(seen), [10]);
		});
	});

	test('a scroll over bands already at this epoch asks for nothing', async () => {
		await settledRoom(clocked, 'epoch', async (app) => {
			// The stack drawn whole HERE, at a day height of its own (da-qs6re):
			// the room's `empty` bumps the epoch, and under load that can land
			// after the strip's own draw, leaving the days off screen at the
			// epoch before — which a scroll to them rightly asks for again.
			const was = await valueOf(app);
			await say(app, { dayHeight: was + 7 });
			await app.page.waitForFunction(
				(want) => Number(document.querySelector('button.day-grip')?.getAttribute('aria-valuenow')) === want,
				was + 7
			);
			await sizeWhole(app, 2);
			await app.idle();

			// Nothing has run since the stack was drawn, so every band in it
			// is at the epoch on screen — and scrolling is just scrolling.
			const seen = await app.spyOnRunner();
			await app.page.locator('.stack').evaluate((stack) => {
				window.__scrolls = 0;
				stack.addEventListener('scroll', () => window.__scrolls++);
			});
			const rest = await stackScroll(app);
			await wheelOverPast(app, -600);
			const scrolls = await scrolledFrom(app, rest.top, 0);
			const moved = await stackScroll(app);
			await wheelOverPast(app, 600);
			await scrolledFrom(app, moved.top, scrolls);
			await app.idle();

			// The wheel really did turn — an assertion about what a scroll
			// does not ask for is worth nothing if nothing scrolled.
			assert.ok(moved.top < rest.top, `the wheel moved the stack ${rest.top - moved.top}px`);

			assert.deepEqual(await viewsOf(seen), []);
		});
	});

	// da-vw4ow. vividn: «when reaching the top of the scrollable dayview, more
	// days should be added automatically (perhaps even before to avoid
	// jarring? or at least placeholders that are quickly filled in with
	// data)». So: a fortnight more within a box of the top, as placeholders
	// no run is spent on until they are seen, and not a pixel of movement
	// under the reader — down to the first entry, and no further.
	test('nearing the top adds older days, drawn only once seen, and stops at the first entry', async () => {
		await settledRoom(clocked, 'grow', async (app) => {
			await sizeWhole(app, 2);
			await app.submit('occur coffee -d -40');
			await app.idle();
			await waitForDays(app, FORTNIGHT);
			const m = await metrics(app);
			const pastRows = () => app.page.locator('.stack .row').count();
			const topOf = (back) =>
				app.page.locator(`.band[data-days-ago="${back}"]`).evaluate((el) => el.getBoundingClientRect().top);
			/** The past days with any pixel in the stack's box — what the strip draws. */
			const inBox = () =>
				app.page.locator('.stack').evaluate((stack) => {
					const box = stack.getBoundingClientRect();
					return [...stack.querySelectorAll('.band')]
						.filter((band) => {
							const at = band.getBoundingClientRect();
							return at.bottom > box.top && at.top < box.bottom;
						})
						.map((band) => Number(band.dataset.daysAgo));
				});

			// Half a box from the top: the oldest band is on screen and the
			// top is not reached yet. Where it lands is read in the same beat,
			// before the scroll event that grows the stack.
			const seen = await app.spyOnRunner();
			const at = await app.page.locator('.stack').evaluate((stack) => {
				stack.scrollTop = stack.clientHeight / 2;
				return stack.querySelector('.band[data-days-ago="14"]').getBoundingClientRect().top;
			});
			await app.page.waitForFunction((n) => document.querySelectorAll('.stack .row').length > n, FORTNIGHT);
			await app.idle();
			assert.equal(await pastRows(), 2 * FORTNIGHT, 'a fortnight more was not added');
			const scroll = await stackScroll(app);
			assert.equal(scroll.total, pastHeight(m, 2 * FORTNIGHT), 'the placeholders are not full days');
			// Added ABOVE, with the place kept: the old top band is where the
			// reader left it, and there is room over it to scroll into.
			assert.ok(
				scroll.top >= pastHeight(m, FORTNIGHT),
				`the stack was not re-anchored: ${JSON.stringify(scroll)}`
			);
			assert.ok(Math.abs((await topOf(FORTNIGHT)) - at) <= 1, 'the band in view moved');

			// No day that was added has been drawn: none is on screen.
			const ran = await seen();
			assert.ok(
				!(await viewsOf(async () => ran)).some((back) => back > FORTNIGHT),
				'an added day ran off screen'
			);
			assert.equal(ran.filter((argv) => argv[0] === 'head').length, 1, 'the first entry was not asked once');
			assert.equal(await app.page.locator('.band[data-days-ago="20"] svg.svg').count(), 0);

			// Scrolled to, it is drawn — once.
			const later = await app.spyOnRunner();
			// Half a band past day 20's top, so which days are in the box is
			// no question of a pixel: the bottom of 20, 19, the top of 18.
			await app.page.locator('.stack').evaluate((stack, half) => {
				const row = stack.querySelector('.band[data-days-ago="20"]').closest('.row');
				stack.scrollTop += row.getBoundingClientRect().top - stack.getBoundingClientRect().top + half;
			}, m.pitch / 2);
			await app.page.locator('.band[data-days-ago="20"] svg.svg').waitFor();
			await app.idle();
			const drawn = await viewsOf(later);
			assert.equal(drawn.filter((back) => back === 20).length, 1, `day 20 ran ${drawn}`);
			const visible = await inBox();
			assert.ok(
				drawn.every((back) => visible.includes(back)),
				`off-screen days ran: ${drawn} with ${visible} on screen`
			);

			// And to the top, again and again: the stack ends at the first
			// entry's day, 40 back, and says so.
			await app.page.waitForFunction(
				() => {
					const stack = document.querySelector('.stack');
					if (stack.querySelector('.first')) return true;
					stack.scrollTop = 0;
				},
				null,
				{ polling: 100 }
			);
			await app.idle();
			assert.equal(await pastRows(), 40, 'the stack does not end at the first entry');
			assert.deepEqual(
				(await later()).filter((argv) => argv[0] === 'head'),
				[],
				'the first entry was asked again at the same epoch'
			);
			const first = await app.page.evaluate(() => {
				const at = new Date(Date.now() - 40 * 86_400_000);
				const pad = (n) => String(n).padStart(2, '0');
				return `${at.getFullYear()}-${pad(at.getMonth() + 1)}-${pad(at.getDate())}`;
			});
			assert.equal((await app.page.locator('.stack .first').innerText()).trim(), `first entry · ${first}`);
		});
	});

});

// ── the hour window (da-ukcf) ────────────────────────────────────────
//
// Two fingers over the strip, or ctrl+wheel on a desk, zoom it onto an
// hour range; the `⊖` in the strip's corner gives the whole day back
// (da-8rqb9 — it was a double tap until then). What makes it a
// STRIP's zoom rather than a picture's is that every band moves together
// — they share the x-axis, and a stack whose days were windowed one at a
// time would not be a stack — so the assertion on every gesture is that
// the bands' transforms are all the FIRST one's, and that the date stamps
// beside them have not moved at all.
//
// Read off the screen rather than off the setting wherever it can be: how
// many hours the ruler and the gridlines actually show is the thing a
// reader sees, and the window is only how the strip says it. The setting
// is asserted too, because it is what a saved layout keeps.

/** How many hour gridlines land inside the band — the hours on screen. */
const hoursShown = ({ page }) =>
	page.locator('.strip').evaluate((strip) => {
		const band = strip.querySelector('.band').getBoundingClientRect();
		return [...strip.querySelectorAll('.gridlines .gridline')].filter((line) => {
			const x = Number(line.getAttribute('x1'));
			return x >= -0.5 && x <= band.width + 0.5;
		}).length;
	});

/** The hours the RULER names, in order — the numbers under the bottom band. */
const rulerLabels = ({ page }) =>
	page.locator('.ruler .tick-label').allTextContents();

/** Every band's transform, as the browser resolves it. */
const bandTransforms = ({ page }) =>
	page
		.locator('.band svg.svg')
		.evaluateAll((svgs) => svgs.map((svg) => getComputedStyle(svg).transform));

/** Where each date stamp's box begins, rounded to the pixel. */
const stampXs = ({ page }) =>
	page
		.locator('.strip .stamp')
		.evaluateAll((els) => els.map((el) => Math.round(el.getBoundingClientRect().x)));

/** The window the strip's owner has heard. */
const windowHeard = async (app) => [
	await heard(app, 'hourFrom'),
	await heard(app, 'hourTo')
];

/**
 * Wait until the window the owner has heard satisfies `expr`, which is
 * written about `w` — `w[1] - w[0] < 24`.
 */
const untilWindow = ({ page }, expr) =>
	page.waitForFunction(
		`(() => { const s = (${stripSettings})(); const w = [s.hourFrom, s.hourTo]; return ${expr}; })()`
	);

/** Today's band, whose box is every band's box along x. */
const bandBox = ({ page }) => page.locator('.band[data-days-ago="0"]').boundingBox();

/** What each `dayview` was asked for as its `--width`. */
const askedWidths = (argvs) =>
	argvs
		.filter((argv) => argv[0] === 'dayview')
		.map((argv) => Number(argv[argv.indexOf('--width') + 1]));

/** How wide a band is DRAWN — the picture's own width, in its own units. */
const drawnWidth = ({ page }) =>
	page.locator('.band[data-days-ago="0"] svg.svg').evaluate((svg) => svg.width.baseVal.value);

/**
 * Two fingers over `box`, `from` px apart, drawn to `to` px apart.
 *
 * Through CDP, as every other touch in this suite: Playwright's
 * touchscreen only taps, and a pinch made of mouse events would be a test
 * of the wrong handler. `midway` is called with the fingers still down —
 * the gesture's preview is only on screen until they lift.
 */
async function pinch(page, box, from, to, midway) {
	const y = box.y + box.height / 2;
	const x = box.x + box.width / 2;
	const cdp = await page.context().newCDPSession(page);
	const both = (apart) => [
		{ id: 1, x: x - apart / 2, y },
		{ id: 2, x: x + apart / 2, y }
	];
	const at = (type, touchPoints) => cdp.send('Input.dispatchTouchEvent', { type, touchPoints });
	try {
		await at('touchStart', [both(from)[0]]);
		await at('touchStart', both(from));
		for (let i = 1; i <= 6; i++) await at('touchMove', both(from + ((to - from) * i) / 6));
		await midway?.();
		await at('touchEnd', []);
	} finally {
		await cdp.detach();
	}
}

/**
 * Two taps in the same place, quickly.
 *
 * It was the gesture for the whole day back until da-8rqb9, and is kept
 * here to assert that it no longer is: the strip's reset had to leave the
 * double tap because da-pzzoz had given the same two taps to the inspect
 * and told them apart by POSITION, which a dense day has no ground for.
 */
async function doubleTap(page, box) {
	const point = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
	const cdp = await page.context().newCDPSession(page);
	// The taps carry their own clock: an event every 60ms, so the two
	// presses land 120ms apart however loaded the runner is. Left to the
	// round trips it was the runner's: on a loaded CI box the second press
	// came more than DOUBLE_MS (350) after the first, and the double tap
	// was two single taps (da-bwvuu).
	const t0 = Date.now() / 1000;
	const at = (type, touchPoints, i) =>
		cdp.send('Input.dispatchTouchEvent', { type, touchPoints, timestamp: t0 + i * 0.06 });
	try {
		for (const i of [0, 2]) {
			await at('touchStart', [point], i);
			await at('touchEnd', [], i + 1);
		}
	} finally {
		await cdp.detach();
	}
}

/** The zoom reset in the strip's corner (da-8rqb9). */
const UNZOOM = '.strip button.unzoom';

/** Today's band — the one at the bottom, whose picture is the day so far. */
const TODAY = '.band[data-days-ago="0"]';

describe('the day strip zooms onto an hour range', () => {
	test('a pinch windows every band at once, and the stamps stay where they are', async () => {
		await settledRoom(phone, 'pinch', async (app) => {
			const { page } = app;
			// More than one band to move TOGETHER, which is the whole claim:
			// a day follows its widget until the reader shrinks it
			// (da-r0ydu), so a phone's strip shows exactly one day until one
			// is set. The assertion below counts them, and would pass
			// vacuously on a stack of one.
			await say(app, { dayHeight: 40 });
			await settledAt(app, 40);
			await app.idle();
			const box = await bandBox(app);
			const stamps = await stampXs(app);
			// At rest a band is drawn at the band's own width: the whole day.
			const full = await drawnWidth(app);
			assert.equal(await hoursShown(app), 25, 'the strip does not start on the whole day');

			// Fingers drawn three times as far apart: a third of the day.
			const seen = await app.spyOnRunner();
			await pinch(page, box, 100, 300, async () => {
				// Under the gesture the pictures on screen are MAPPED, not
				// asked for again — that is what makes a pinch free.
				assert.deepEqual(askedWidths(await seen()), []);
				const during = await bandTransforms(app);
				assert.ok(during.length > 1, `only ${during.length} band on screen`);
				assert.deepEqual(during, during.map(() => during[0]), 'the bands moved apart');
				assert.equal(await hoursShown(app), 9, 'the pinch did not narrow the window');
			});

			// The release asks for the window — the same eight hours, drawn
			// at its own width now, which is three days' worth of pixels.
			await untilWindow(app, 'w[1] - w[0] < 24');
			assert.deepEqual(await windowHeard(app), [8, 16]);
			await app.idle();
			assert.deepEqual(
				[...new Set(askedWidths(await seen()))],
				[full * 3],
				'the bands were not re-drawn at the window’s width'
			);

			// And the whole strip is on those hours: one transform for every
			// band, nine hours on screen, and a ruler that names them.
			const after = await bandTransforms(app);
			assert.deepEqual(after, after.map(() => after[0]), 'the bands moved apart');
			assert.equal(await hoursShown(app), 9);
			assert.deepEqual(
				await rulerLabels(app),
				['08', '09', '10', '11', '12', '13', '14', '15', '16'],
				'the ruler does not name the hours the window holds'
			);

			// The stamps are beside the pictures, not in them: nothing the
			// window does moves them.
			assert.deepEqual(await stampXs(app), stamps, 'the date stamps moved');
			assert.deepEqual(app.errors, []);
		});
	});

	test('the ⊖ gives the whole day back, and a double tap no longer does', async () => {
		await settledRoom(phone, 'unzoom', async (app) => {
			await app.idle();
			const box = await bandBox(app);

			// At rest there is nothing to come back from, so there is no
			// button either — the `.jagged` line's rule.
			assert.equal(await app.page.locator(UNZOOM).count(), 0, 'the ⊖ is there at rest');

			await pinch(app.page, box, 100, 300);
			await untilWindow(app, 'w[1] - w[0] < 24');
			await app.idle();
			const windowed = await windowHeard(app);

			// The gesture that used to mean this (da-ukcf) means nothing to
			// the window now: it is the inspect's, everywhere (da-8rqb9).
			await doubleTap(app.page, box);
			await app.idle();
			assert.deepEqual(await windowHeard(app), windowed, 'a double tap still resets the zoom');

			await app.page.locator(UNZOOM).click();
			await untilWindow(app, 'w[0] === 0 && w[1] === 24');
			await app.idle();
			assert.equal(await hoursShown(app), 25);
			assert.equal(await app.page.locator(UNZOOM).count(), 0, 'the ⊖ outlived the window');
			assert.deepEqual(app.errors, []);
		});
	});

	// ⛑ THE REASON FOR THE BUTTON, asserted on the shape that produced it.
	//
	// da-pzzoz told the inspect's double tap from the zoom reset's by
	// POSITION: on a mark, inspect; on the picture's ground, the whole day
	// back. vividn's own sentence is what breaks it — «when you have enough
	// data, like me, everywhere has data under it». So the reader with the
	// most data is the one whose reset goes missing, and a test on an
	// EMPTY strip would never see it: there, every tap is on ground.
	//
	// The day here is a day with data ALL ALONG IT — measured, not
	// assumed: four fields switching through the day, and every point
	// sampled on a field's own lane is on a mark, for the whole width. On
	// such a day a double tap aimed at the picture lands on data and
	// INSPECTS, which is asserted below; the way back has to be somewhere
	// a picture is not, and the `⊖` is in the date column, which no
	// picture is ever drawn in.
	test('on a day whose lanes are full, a double tap inspects — and the ⊖ still gives the day back', async () => {
		await settledRoom(phone, 'dense', async (app) => {
			const { page } = app;
			// Late in the day, so the last switch has five hours to run: the
			// seeds are read against the clock, and the band re-drawn at it
			// by the seed's last line, which goes through the bar. The next
			// test's room puts the clock back.
			await app.clock.set(todayAt(23));
			// `work` is centred on the band's middle by its spec, where the
			// double tap lands, rather than wherever its name hashes to.
			await seedLines(app, [
				'setup work --verb switch --y 0.5',
				...['mood', 'work', 'place', 'posture'].flatMap((field, lane) =>
					[0, 6, 12, 18].map(
						(hour) => `switch ${field} s${lane}${hour} -t ${String(hour).padStart(2, '0')}:00`
					)
				)
			]);
			await page.locator(`${TODAY} svg.svg`).waitFor();
			await app.idle();

			// Every row of the band that has a mark on it at all has one at
			// every hour of the day: that is «everywhere has data under it»
			// in the direction a finger travels.
			const lanes = await page.locator(TODAY).evaluate((band) => {
				const box = band.getBoundingClientRect();
				const on = (x, y) => !!document.elementFromPoint(x, y)?.closest('[data-id]');
				const rows = [];
				for (let j = 1; j < 30; j++) {
					const y = box.top + (box.height * j) / 30;
					const xs = [...Array(19).keys()].map((i) => box.left + (box.width * (i + 1)) / 20);
					const marks = xs.filter((x) => on(x, y)).length;
					if (marks) rows.push({ y: Math.round(y - box.top), marks, of: xs.length });
				}
				return rows;
			});
			assert.ok(lanes.length >= 4, `only ${lanes.length} rows of the band carry data`);
			assert.deepEqual(
				lanes.filter((row) => row.marks !== row.of),
				[],
				'a lane has a gap in it: this is not the dense case'
			);

			const box = await bandBox(app);
			await pinch(page, box, 100, 300);
			await untilWindow(app, 'w[1] - w[0] < 24');
			await app.idle();
			const windowed = await windowHeard(app);

			// So the double tap the reset used to share is spoken for: aimed
			// at the picture it finds a mark and inspects it. The entry is
			// read at the point the tap will land, on the ZOOMED picture,
			// rather than assumed from the one before it.
			const mark = await page.locator(TODAY).evaluate((band) => {
				const b = band.getBoundingClientRect();
				const at = document.elementFromPoint(b.left + b.width / 2, b.top + b.height / 2);
				return at?.closest('[data-id]')?.getAttribute('data-id') ?? null;
			});
			assert.ok(mark, 'the middle of a full day is still ground');

			await doubleTap(page, box);
			await page.waitForFunction(
				`window.datum.bar.pills().some((p) => p.text === ${JSON.stringify(mark)})`
			);
			assert.deepEqual(await windowHeard(app), windowed, 'the double tap threw the window away');

			// And the way back is reachable from exactly there — asked the
			// way a finger asks. Every point of the button's box hit-tests
			// to the button, and none of them to a mark: not the middle
			// alone, because a fingertip is the whole 26px and the picture
			// beside it is three bands wide while zoomed.
			//
			// HIT-TESTING and not the marks' boxes: a zoomed picture hangs
			// off both ends of the band that clips it, and a clipped mark's
			// `getBoundingClientRect` still reports the whole of it — so
			// «no mark overlaps the button» is false by that measure and
			// true to the finger (the band's `overflow` is what decides).
			const missed = await page.locator(UNZOOM).evaluate((el) => {
				const r = el.getBoundingClientRect();
				const away = [];
				for (let i = 0; i <= 4; i++)
					for (let j = 0; j <= 4; j++) {
						const x = r.left + 1 + ((r.width - 2) * i) / 4;
						const y = r.top + 1 + ((r.height - 2) * j) / 4;
						const at = document.elementFromPoint(x, y);
						if (!el.contains(at))
							away.push(`${Math.round(x - r.left)},${Math.round(y - r.top)}: ${at?.tagName}${at?.closest('[data-id]') ? ' (a mark)' : ''}`);
					}
				return away;
			});
			assert.deepEqual(missed, [], 'the ⊖ does not have its own hit area');

			await page.locator(UNZOOM).click();
			await untilWindow(app, 'w[0] === 0 && w[1] === 24');
			await app.idle();
			assert.equal(await hoursShown(app), 25);
			assert.deepEqual(app.errors, []);
		});
	});

	// vividn, 2026-10-19: «it looks blank when scrolling to the right when
	// already at now» (da-og8a2). The grid and the ruler ran on past now;
	// what was missing was a state recorded there, cut at the now-line.
	test('zoomed past now, the grid runs on and a recorded meeting is drawn; a running state stops at now', async () => {
		await settledRoom(phone, 'future', async (app) => {
			const { page } = app;
			await app.clock.set(todayAt(12));
			await seedLines(app, ['start work -t 09:00', 'start meeting -t 14:00', 'end meeting -t 15:00']);
			await say(app, { hourFrom: 11, hourTo: 16 });
			await untilWindow(app, 'w[0] === 11');
			await page.locator(`${TODAY} rect.block[data-field="meeting"]`).waitFor();
			await app.idle();

			const box = await bandBox(app);
			const right = box.x + box.width;
			const now = (await page.locator(`${TODAY} line.current-time-line`).boundingBox()).x;
			const block = (field) => page.locator(`${TODAY} rect.block[data-field="${field}"]`).boundingBox();
			const meeting = await block('meeting');
			const work = await block('work');
			assert.ok(now > box.x && now < right, `now is off the band: ${now} in ${box.x}–${right}`);
			assert.ok(
				meeting.x > now && meeting.x + meeting.width < right,
				`the meeting is not past now on the band: ${JSON.stringify(meeting)}, now ${now}`
			);
			assert.ok(Math.abs(work.x + work.width - now) < 1.5, `work runs past now: ${JSON.stringify(work)}, now ${now}`);
			const hoursPastNow = await page
				.locator('.strip .gridlines .gridline')
				.evaluateAll((lines, [from, to]) =>
					lines.map((l) => l.getBoundingClientRect().x).filter((x) => x > from + 1 && x < to)
				, [now, right]);
			assert.ok(hoursPastNow.length >= 3, `the grid past now: ${hoursPastNow}`);
		});
	});

	test('the ⊖ is a fingertip, in the one column no picture is drawn in', async () => {
		await settledRoom(phone, 'fingertip', async (app) => {
			const { page } = app;
			await app.idle();
			const box = await bandBox(app);
			await pinch(page, box, 100, 300);
			await untilWindow(app, 'w[1] - w[0] < 24');
			await app.idle();

			const seen = await page.locator(UNZOOM).evaluate((el) => {
				const r = el.getBoundingClientRect();
				const strip = el.closest('.strip').getBoundingClientRect();
				const spine = el.closest('.strip').querySelector('.spine').getBoundingClientRect();
				return {
					width: r.width,
					height: r.height,
					// In the strip's bottom corner, and inside its box.
					fromBottom: Math.round(strip.bottom - r.bottom),
					inside: r.left >= strip.left - 0.5 && r.right <= strip.right + 0.5,
					// The spine is the picture's left edge: every band and the
					// gridlines begin there, so a button left of it is over no
					// picture at any density.
					clearOfThePictures: r.right <= spine.left + 0.5,
					glyph: el.querySelector('svg').getBoundingClientRect().width
				};
			});
			// §12: the box a finger lands on is 24px, whatever the drawing
			// inside it is (the old beta ⓘ's pattern).
			assert.ok(seen.width >= 24 && seen.height >= 24, `the ⊖ is ${seen.width}×${seen.height}`);
			assert.ok(seen.glyph < seen.width, 'the glyph is not smaller than its hit area');
			assert.equal(seen.fromBottom, 0, 'the ⊖ is not in the strip’s bottom corner');
			assert.ok(seen.inside, 'the ⊖ hangs off the strip');
			assert.ok(seen.clearOfThePictures, 'the ⊖ is over a picture');

			// And clear of the control over that column's edge: the `+`, at
			// the top of today (da-2hd21).
			const boxOf = (sel) => page.locator(sel).evaluate((el) => el.getBoundingClientRect().toJSON());
			const [reset, grip] = [await boxOf(UNZOOM), await boxOf('.strip button.day-grip')];
			assert.ok(
				reset.right <= grip.left + 0.5 || reset.top >= grip.bottom - 0.5,
				`the ⊖ (${reset.top}–${reset.bottom}) sits under the + (${grip.top}–${grip.bottom})`
			);
			assert.deepEqual(app.errors, []);
		});
	});

	test('ctrl+wheel zooms, shift+wheel pans, and the ⊖ is the way back', async () => {
		await settledRoom(desk, 'wheel',
			async (app) => {
				const { page } = app;
				await app.idle();
				const box = await bandBox(app);
				const middle = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
				await page.mouse.move(middle.x, middle.y);

				// A bare wheel is the stack scrolling back through the days,
				// not a zoom — the strip's oldest reflex, and still true.
				await page.mouse.wheel(0, -100);
				await app.idle();
				assert.deepEqual(await windowHeard(app), [0, 24]);

				// ctrl+wheel: a notch is half a doubling, so the day comes in
				// by √2 about the pointer — which is the middle, so the window
				// stays centred on noon.
				await page.keyboard.down('Control');
				await page.mouse.wheel(0, -100);
				await untilWindow(app, 'w[1] - w[0] < 24');
				await page.keyboard.up('Control');
				const zoom = await windowHeard(app);
				assert.ok(
					Math.abs(zoom[1] - zoom[0] - 24 / Math.SQRT2) < 0.05,
					`a notch is not half a doubling: ${zoom}`
				);
				assert.ok(Math.abs((zoom[0] + zoom[1]) / 2 - 12) < 0.05, `the zoom drifted: ${zoom}`);
				const shown = await hoursShown(app);
				assert.ok(shown < 25, `ctrl+wheel showed ${shown} hours of the day`);

				// shift+wheel: the same window, later in the day. Down and
				// right are later, so the picture moves against the wheel.
				await page.keyboard.down('Shift');
				await page.mouse.wheel(0, 200);
				await untilWindow(app, `w[0] > ${zoom[0] + 0.5}`);
				await page.keyboard.up('Shift');
				const panned = await windowHeard(app);
				assert.ok(
					Math.abs(panned[1] - panned[0] - (zoom[1] - zoom[0])) < 0.05,
					`the pan changed the span: ${zoom} → ${panned}`
				);
				const panShown = await hoursShown(app);
				assert.ok(
					Math.abs(panShown - shown) <= 1,
					`the pan changed how much is on screen: ${shown} → ${panShown}`
				);

				// Every band is still on the one axis — once the strip is at
				// rest. The zoom re-asked every band at the window's width and
				// they arrive one CLI process apart; until a band's own answer
				// lands it is still the pre-zoom picture, stretched by √2 to
				// show the same hours (`mapFor` in $lib/DayStrip.svelte). That
				// is the gesture's preview carrying on, not the bands moving
				// apart, so the one axis is what the SETTLED strip draws
				// (da-vi1j — the pinch above waits the same way).
				await app.idle();
				const after = await bandTransforms(app);
				assert.deepEqual(after, after.map(() => after[0]), 'the bands moved apart');

				// The button in the corner: the whole day.
				await page.locator(UNZOOM).click();
				await untilWindow(app, 'w[0] === 0 && w[1] === 24');
				await app.idle();
				assert.equal(await hoursShown(app), 25);
				assert.deepEqual(app.errors, []);
			}
		);
	});

	test('the window is a setting: this session’s, saved with the layout when asked, reset with the widget', async () => {
		// `main` itself, saved and reloaded: the last test on `desk`, since
		// what it leaves behind is a boot no other test would want to share.
		await desk.solo(
			async (app) => {
				const { page } = app;
				await app.idle();
				const box = await bandBox(app);

				await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
				await page.keyboard.down('Control');
				await page.mouse.wheel(0, -200);
				await untilWindow(app, 'w[1] - w[0] < 13');
				await page.keyboard.up('Control');
				const zoomed = await windowHeard(app);
				await app.idle();
				const shown = await hoursShown(app);

				// The zoom is a setting the widget moved ITSELF, so since
				// da-vhwq it is this session's and the layout has not heard of
				// it: «Save options with layout» is what hands it over, and
				// `Save layout` then writes it down.
				await choose(page, 'strip', SAVE_OPTIONS);
				await chooseRoot(page, 'Save layout');
				await app.reload();
				await app.idle();

				// The strip comes back on the hours it was left on, and it is
				// DRAWING them: the setting alone would pass on a strip that
				// had forgotten how to window.
				assert.deepEqual(await windowHeard(app), zoomed);
				assert.equal(await hoursShown(app), shown);
				const bands = await bandTransforms(app);
				assert.deepEqual(bands, bands.map(() => bands[0]));
				assert.notEqual(
					bands[0],
					'matrix(1, 0, 0, 1, 0, 0)',
					'the restored strip draws the whole day'
				);

				// And `Reset widget` is the other way back to the whole day:
				// the window is one of the strip's declared defaults, so the
				// act that puts a widget back to them puts the window back.
				await choose(page, 'strip', 'Reset widget');
				await untilWindow(app, 'w[0] === 0 && w[1] === 24');
				await app.idle();
				assert.equal(await hoursShown(app), 25);
				assert.deepEqual(app.errors, []);
			}
		);
	});
});
