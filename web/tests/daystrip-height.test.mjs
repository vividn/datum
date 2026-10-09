// The day strip's own height and its pin: a day height that follows its box
// until a reader touches the handle and holds after, the handle on both
// axes and the keyboard, the freeze toggle, a strip that writes nothing
// down, the jagged line, a midnight, and its row on a phone. Split from
// `daystrip.test.mjs` for node's per-file budget (da-4j2hw), whose header
// says what the strip is asserted against and how its tests share an app;
// the helpers both read are `support/daystrip.mjs`.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import {
	atEachScreen,
	sharedApp,
	startApp,
	stopApp,
	withApp,
	todayAt
} from './support/app.mjs';
import {
	NOWVIEW,
	NARROW,
	settledRoom,
	labelFor,
	band,
	stamps,
	gridlines,
	metrics,
	grown,
	boxTo,
	following,
	MIN_DAY,
	MAX_DAY,
	settledAt,
	boxOf,
	owedAt,
	heldFor,
	heldAt,
	sizeFor,
	waitForDays,
	valueOf,
	widthOf,
	heard,
	heardIs,
	say,
	waitForHeights,
	dragBy,
	todayBand,
	stackScroll,
	jagged,
	storeKeys
} from './support/daystrip.mjs';

before(startApp);
after(stopApp);

const shared = sharedApp();
const clocked = sharedApp({ clock: todayAt(12) });
const narrow = sharedApp(NARROW);

describe("the day strip's day height and its pin", () => {
	// ⛔ FOLLOW UNTIL TOUCHED, THEN HOLD — and back onto the derived value
	// RE-ARMS the follow (da-r0ydu). vividn, 2026-09-21: «make the dayview
	// widget internal options default to a day height that is equal to the
	// height of the widget … I don't know if it should stay locked to that,
	// my gut says no, so that if the user pulls down the dayview it
	// expands.»
	//
	// The two states are one property of the DATA and not a flag beside
	// it: the layout says nothing about the day height, so the strip
	// derives one from its box; the handle puts a number where the
	// layout's silence was, so the box stops moving it; and the handle put
	// back onto what the box would have chosen leaves nothing held again
	// (`overlay`, layout/session.ts). So every leg below asserts the PIXELS
	// and what the OWNER holds — a day that happened to land at the right
	// height with a number still held is the bug this rule prevents.
	test('follows its widget until touched, holds after, and follows again when let go', async () => {
		await settledRoom(shared, 'follows', async (app) => {
			const box = await boxOf(app);
			assert.equal(await heard(app), undefined, 'a fresh strip holds a day height');
			assert.equal(await valueOf(app), await following(app), 'one day does not fill the widget');

			// FOLLOWS: a taller pane is a taller DAY, and still nothing held.
			// The box is the element's the moment the cell is styled; the
			// strip hears of it a settle later (layout/resizing.ts), so the
			// wait is for the handle to arrive at what the box already says.
			await boxTo(app, box + 60);
			await settledAt(app, await following(app));
			assert.equal(await valueOf(app), (await following(app)), 'the day did not follow the widget');
			assert.equal(await heard(app), undefined, 'following the box left something held');

			// TOUCHED: the handle's value is the reader's, and the owner
			// hears it — up is taller (da-hsba).
			const grow = 30;
			const mine = (await valueOf(app)) + grow;
			await dragBy(app, 'button.day-grip', 0, -grow);
			assert.equal(await valueOf(app), mine);
			await heardIs(app, mine);

			// HOLDS: the pane is resized again and the day does not move —
			// which is the whole of «then it stays». Waited out rather than
			// waited for: the claim is that nothing happens, so the settle
			// has to have come and gone before it is worth making.
			await boxTo(app, box + 120);
			const wouldBe = await following(app);
			assert.notEqual(wouldBe, mine, 'the resize asked for the height already held');
			await app.page.waitForTimeout(3 * 150);
			await app.idle();
			assert.equal(await valueOf(app), mine, 'a resize moved a day height the reader had set');
			assert.equal(await heard(app), mine);

			// RE-ARMS: dragged onto the height the box would have chosen,
			// nothing is held — the same absence a fresh strip has.
			await dragBy(app, 'button.day-grip', 0, -(wouldBe - mine));
			assert.equal(await valueOf(app), wouldBe);
			await heardIs(app, undefined);

			// And it follows again, which is the assertion that the absence
			// MEANS something rather than merely being tidy.
			//
			// ⚑ +150 and not +180: at the default viewport +180 leaves the
			// strip exactly 255px of content, which derives exactly
			// `MAX_DAY_HEIGHT`. Asserting there asks nothing — the CLAMP
			// would hold the number up even if the follow were broken — and
			// it sits on the boundary besides. +150 derives 210, inside the
			// range, so the number can only come from the box.
			await boxTo(app, box + 150);
			const free = await following(app);
			assert.ok(free > MIN_DAY && free < MAX_DAY, `the last leg derives ${free}, which is clamped`);
			await settledAt(app, free);
			assert.equal(await valueOf(app), free);
			// And the PICTURES were drawn at it — datum's own `height`, not
			// two readers on this side agreeing with each other. If
			// `following` were wrong the bands would not come back at this
			// size (canary minimax-m27 on PR #73).
			await waitForHeights(app, free);
			assert.equal(await heard(app), undefined);
		});
	});

	// ⛔ ONLY THE HEIGHT AXIS DECIDES THE HOLD (da-r0ydu, canary kimi-k3 on
	// PR #73). The handle carries BOTH sizes on every drop and every arrow
	// key, so a purely SIDEWAYS drag reaches `setSizes` with the height it
	// already had. On a box that derives exactly the held value that used
	// to read as «dragged back onto the derived value» and re-armed the
	// follow — losing a height the reader set, on an axis they never
	// touched, invisibly until the next pane resize moved their day.
	//
	// The set-up is the whole point: the strip is HELD at exactly what its
	// box would derive, which is the one state where the two are
	// indistinguishable by value and only the MOVE tells them apart.
	test('a sideways drag keeps a held day height, even when it equals the derived one', async () => {
		await settledRoom(shared, 'sideways-hold', async (app) => {
			// Held at the derived value: `say` writes the tree, so the day
			// is the reader's — and it is the number the box would pick.
			//
			// ⚑ `say` is `layout.apply`, which is `layout.update` and NOT
			// `inline` — so `RETIRED_SETTINGS` does not run over it, and this
			// set-up holds even on the day a box derives exactly the retired
			// 85. A reader who later routes this through `inline` has to
			// make the number here something else.
			const derived = await following(app);
			await say(app, { dayHeight: derived });
			await settledAt(app, derived);
			assert.equal(await heard(app), derived, 'the set-up did not hold a day height');

			// SIDEWAYS ONLY: the label column widens, the height is not touched.
			const wide = await widthOf(app);
			await dragBy(app, 'button.day-grip', 40, 0);
			assert.ok((await widthOf(app)) > wide, 'the sideways drag moved nothing');

			// The height is still the reader's, and the box still says so.
			assert.equal(await valueOf(app), derived);
			assert.equal(await heard(app), derived, 'a sideways drag re-armed the follow');

			// And it really is HELD: a resize leaves it where it was, which
			// is the thing the lost hold would have cost.
			await boxTo(app, (await boxOf(app)) + 60);
			await app.page.waitForTimeout(3 * 150);
			await app.idle();
			assert.equal(await valueOf(app), derived, 'the box moved a day the reader had set');
			assert.equal(await heard(app), derived);
		});
	});

	// ⛔ THE FLOOR CAN BE HELD, even in a box that derives it (canary
	// minimax-m27 on PR #73, who read the code right and drew the wrong
	// conclusion from it).
	//
	// Their observation is correct: in a pane already deriving 40, a drag
	// onto 40 re-arms the follow, because held-at-X and following-at-X are
	// the SAME DATA — the absence is the state (da-r0ydu), which is what
	// the bead asked for instead of a flag. What does not follow is that
	// the state is unreachable. A height is held by the MOVE that sets it,
	// and it SURVIVES the box afterwards — so the floor is held from a box
	// that derives something else, and it stays held when the box shrinks
	// onto it. That is the whole of «then it stays», and this is the leg
	// of it that the edge case appeared to deny.
	test('the floor, held from a taller box, stays held when the box shrinks onto it', async () => {
		await settledRoom(shared, 'floor', async (app) => {
			const box = await boxOf(app);

			// A box that derives something well clear of the floor, so the
			// drag onto the floor is unambiguously a MOVE.
			await boxTo(app, box + 120);
			const tall = await following(app);
			assert.ok(tall > 40 + 20, `the tall box derives ${tall}, too near the floor to prove anything`);
			await settledAt(app, tall);

			// Dragged to the floor: down is shorter, and far enough to clamp.
			await dragBy(app, 'button.day-grip', 0, 400);
			assert.equal(await valueOf(app), 40, 'the drag did not reach the floor');
			await heardIs(app, 40);

			// Now shrink the box until it derives the floor itself — the
			// state the finding says cannot be held.
			await boxTo(app, 68);
			await app.page.waitForTimeout(3 * 150);
			await app.idle();
			assert.equal(await following(app), 40, 'the small box does not derive the floor');
			assert.equal(await valueOf(app), 40);
			assert.equal(await heard(app), 40, 'the box deriving the floor un-held it');

			// And it is HELD, not merely equal: the box grows and the day
			// does not go with it.
			await boxTo(app, box + 120);
			await app.page.waitForTimeout(3 * 150);
			await app.idle();
			assert.equal(await valueOf(app), 40, 'a taller box moved a floor the reader had set');
			assert.equal(await heard(app), 40);
		});
	});

	// A held height the strip cannot draw (canary deepseek-v4-pro on PR #73).
	// `DEFAULT_DAY_HEIGHT` is no longer «a height to fall back on» — since
	// da-r0ydu it is only the stand-in before the box is measured — so an
	// unusable setting must not resolve to it. A number outside the range
	// is CLAMPED into the range; something that is no number at all holds
	// nothing, and the strip follows, which is the state an unusable
	// setting should leave a reader in.
	test('a held day height the strip cannot draw is clamped, and a non-number follows', async () => {
		await shared.solo(async (app) => {
			const floor = 40;
			await say(app, { dayHeight: 0 });
			await settledAt(app, floor);
			assert.equal(await valueOf(app), floor, 'a held 0 did not land on the floor');

			await say(app, { dayHeight: 4000 });
			await settledAt(app, 240);
			assert.equal(await valueOf(app), 240, 'a held 4000 did not land on the ceiling');

			// Not a number at all: nothing is held, so the box decides again.
			await say(app, { dayHeight: null });
			await settledAt(app, await following(app));
			assert.equal(await valueOf(app), await following(app), 'a held null did not go back to following');
		});
	});

	test('the day height comes in as a prop and goes out as a change', async () => {
		await settledRoom(shared, 'prop', async (app) => {
			// What the default is is the strip's business, not this file's, so
			// it is read off a fresh strip rather than restated here. Nothing
			// is HELD on a fresh strip (da-r0ydu): the day follows its widget
			// until the handle is touched, so the owner's settings are silent
			// about it and the handle still says a number.
			const fallback = await valueOf(app);
			assert.ok(fallback > 0, `the handle says ${fallback}`);
			assert.equal(await heard(app), undefined);

			// The owner said 120 — through the tree, as a saved layout does:
			// the handle says so, and every band is drawn in the box that
			// height gives it.
			await say(app, { dayHeight: 120 });
			await app.page.waitForFunction(
				() => Number(document.querySelector('button.day-grip')?.getAttribute('aria-valuenow')) === 120
			);
			assert.equal(await valueOf(app), 120);
			await sizeFor(app, 2);
			const m = await metrics(app);
			assert.equal(m.band, 120, `the day box is not the 120 day height: ${m.band}`);
			await waitForHeights(app, m.band);

			// The handle moves it — up is taller (da-hsba) — and the owner
			// hears the new value: the strip does not keep it to itself.
			const grow = 30;
			await dragBy(app, 'button.day-grip', 0, -grow);
			assert.equal(await valueOf(app), 120 + grow);
			await heardIs(app, 120 + grow);
			// And the bands are re-drawn, not stretched: the picture datum
			// wrote is the new size. Waited for: the render is on the drop.
			await waitForHeights(app, grown(m, grow).band);
		});
	});

	test('the handle re-draws every day at the new height, and the days stay on one x-axis', async () => {
		await settledRoom(shared, 'redraw', async (app) => {
			// A day height set first, and not the derived one: this test
			// drags 40px each way from it TWICE, and the shortest day the
			// strip may draw is 40 (da-r0ydu — a day that follows its widget
			// is whatever the pane happens to be, which on a 1280x720 screen
			// is within one drag of that floor).
			await say(app, { dayHeight: 120 });
			await settledAt(app, 120);
			// Three, so that two are still there once the days are taller:
			// the box does not grow with them, so fewer fit.
			await sizeFor(app, 3);
			const m = await metrics(app);
			const was = await valueOf(app);
			const grow = 40;

			// UP is taller (da-hsba): the pointer moves 40px up, and every
			// day is drawn 40px taller — today's, and each of the past's.
			await dragBy(app, 'button.day-grip', 0, -grow);
			assert.equal(await valueOf(app), was + grow);
			const up = grown(m, grow);
			assert.equal(owedAt(up), 2, 'the taller days do not leave two in the box');
			await waitForHeights(app, up.band);
			await waitForDays(app, heldAt(up));

			// And the thing the stack is for still holds at the new height:
			// the same hour is the same column in every band.
			const today = gridlines(await band(app, 0));
			assert.ok(today.length >= 4, `today's band drew ${today.length} gridlines`);
			assert.deepEqual(gridlines(await band(app, 1)), today, 'the past is on a different x-axis after a resize');

			// And DOWN is shorter, by the same 40, from the same start —
			// and more days fit.
			await dragBy(app, 'button.day-grip', 0, grow);
			assert.equal(await valueOf(app), was);
			await waitForHeights(app, m.band);
			await dragBy(app, 'button.day-grip', 0, grow);
			assert.equal(await valueOf(app), was - grow);
			const down = grown(m, -grow);
			await waitForHeights(app, down.band);
			await waitForDays(app, heldAt(down));
		});
	});

	// The second axis of the same handle (da-iu9i): the date column is the
	// strip's own now, so how wide it is is a setting, and the `+` at the
	// 00 mark drags it sideways.
	test('the handle dragged sideways widens the date column, and the bands give up the room', async () => {
		await settledRoom(shared, 'widen', async (app) => {
			await sizeFor(app, 3);
			const was = await widthOf(app);
			const before = await metrics(app);
			const bandWidth = await app.page
				.locator('.band[data-days-ago="0"]')
				.evaluate((el) => el.getBoundingClientRect().width);

			const wider = 30;
			await dragBy(app, 'button.day-grip', wider, 0);
			assert.equal(await widthOf(app), was + wider);
			await heardIs(app, was + wider, 'labelWidth');

			const after = await metrics(app);
			assert.equal(after.label, before.label + wider, 'the date column did not widen');
			// The strip's box did not change, so the bands are narrower by
			// exactly what the column took — and they are re-DRAWN at it,
			// not squeezed: the picture datum wrote is the new width.
			await app.page.waitForFunction(
				(want) =>
					[...document.querySelectorAll('.band svg.svg')].every(
						(svg) => Math.abs(Number(svg.getAttribute('width')) - want) <= 1
					),
				bandWidth - wider
			);
			const now = await app.page
				.locator('.band[data-days-ago="0"]')
				.evaluate((el) => el.getBoundingClientRect().width);
			assert.ok(Math.abs(now - (bandWidth - wider)) <= 1, `the band is ${now}px, not ${bandWidth - wider}`);

			// And the stamps are still legible in the wider column.
			assert.equal((await stamps(app)).at(-1).length, 3, 'the stamp lost lines as the column grew');
		});
	});

	// The «better preview rendering» (vividn, 2026-09-15): the bands follow
	// the finger as scaled pictures, and datum is asked again only once the
	// handle is let go.
	test('the bands scale live under the drag and are re-drawn on the drop', async () => {
		await settledRoom(clocked, 'scale', async (app) => {
			await sizeFor(app, 2);
			const was = await valueOf(app);
			const before = await app.page
				.locator('.band[data-days-ago="0"] svg.svg')
				.evaluate((el) => el.getBoundingClientRect().height);
			const seen = await app.spyOnRunner();

			const box = await app.page.locator('button.day-grip').boundingBox();
			const x = box.x + box.width / 2;
			const y = box.y + box.height / 2;
			await app.page.mouse.move(x, y);
			await app.page.mouse.down();
			await app.page.mouse.move(x, y - 40, { steps: 8 });

			// Mid-drag: the band on the screen is 40px taller although datum
			// has not been asked for anything.
			const during = await app.page
				.locator('.band[data-days-ago="0"] svg.svg')
				.evaluate((el) => el.getBoundingClientRect().height);
			assert.ok(
				Math.abs(during - (before + 40)) <= 1,
				`the preview shows ${during}px where ${before + 40} was asked for`
			);
			assert.equal(
				(await seen()).filter((argv) => argv[0] === 'dayview').length,
				0,
				'the strip re-drew during the drag'
			);

			await app.page.mouse.up();
			assert.equal(await valueOf(app), was + 40);
			await waitForHeights(app, was + 40);
			// And once it lands the picture is its own size again: nothing
			// on the screen is a stretched band.
			const after = await app.page
				.locator('.band[data-days-ago="0"] svg.svg')
				.evaluate((el) => el.getBoundingClientRect().height);
			assert.ok(Math.abs(after - (was + 40)) <= 1, `the drawn band is ${after}px, not ${was + 40}`);
		});
	});

	test('the keyboard works both axes of the handle', async () => {
		await settledRoom(shared, 'keys', async (app) => {
			const tall = await valueOf(app);
			const wide = await widthOf(app);

			await app.page.locator('button.day-grip').focus();
			assert.ok(
				await app.page.evaluate(() => document.activeElement.classList.contains('day-grip')),
				'the handle does not take focus'
			);
			// Up is taller, as it is for the pointer — the two paths are one
			// convention.
			await app.page.keyboard.press('ArrowUp');
			const taller = await valueOf(app);
			assert.ok(taller > tall, `up did not make the day taller: ${taller}`);
			await app.page.keyboard.press('ArrowDown');
			await app.page.keyboard.press('ArrowDown');
			assert.equal(await valueOf(app), 2 * tall - taller);
			assert.equal(await heard(app), 2 * tall - taller);

			// And right is a wider column, left a narrower one.
			await app.page.keyboard.press('ArrowRight');
			const wider = await widthOf(app);
			assert.ok(wider > wide, `right did not widen the date column: ${wider}`);
			await app.page.keyboard.press('ArrowLeft');
			assert.equal(await widthOf(app), wide);
			assert.equal(await heard(app, 'labelWidth'), wide);
		});
	});

	// vividn, 2026-09-15: «at the very left side of the dividing line, put a
	// little icon to freeze today at the bottom».
	test('the freeze toggle takes today out of the pin and puts it back', async () => {
		await settledRoom(shared, 'freeze', async (app) => {
			await sizeFor(app, 3);
			const toggle = app.page.locator('button.freeze');
			assert.equal(await toggle.getAttribute('aria-checked'), 'true', 'today does not start frozen');
			assert.equal(await app.page.locator('.stack .band[data-days-ago="0"]').count(), 0);

			// Frozen: scrolled to the very top of the past, today's band is
			// still against the bottom of the strip.
			await app.page.locator('.stack').evaluate((el) => (el.scrollTop = 0));
			const m = await metrics(app);
			const strip = await app.page.locator('.strip').boundingBox();
			const pinned = await todayBand(app);
			assert.ok(
				Math.abs(pinned.y + pinned.height - (strip.y + strip.height - m.axis)) <= 2,
				`today is not against the ruler: ${JSON.stringify(pinned)} in ${JSON.stringify(strip)}`
			);

			// Off: today is simply the last band in the scroller.
			await toggle.click();
			assert.equal(await toggle.getAttribute('aria-checked'), 'false');
			await heardIs(app, false, 'freezeToday');
			await app.page.locator('.stack .band[data-days-ago="0"]').waitFor();

			// And it scrolls away with the rest.
			await app.page.locator('.stack').evaluate((el) => (el.scrollTop = 0));
			await app.page.waitForFunction(() => {
				const stack = document.querySelector('.stack').getBoundingClientRect();
				const today = document
					.querySelector('.band[data-days-ago="0"]')
					.getBoundingClientRect();
				return today.top >= stack.bottom - 1;
			});

			// Back on, and today is pinned again.
			await toggle.click();
			await heardIs(app, true, 'freezeToday');
			await app.page.waitForFunction(() => !document.querySelector('.stack .band[data-days-ago="0"]'));
			const again = await todayBand(app);
			assert.ok(Math.abs(again.y + again.height - (pinned.y + pinned.height)) <= 2);
		});
	});

	// da-iu9i: «the ↻ goes» — resetting the widget's settings is the pane
	// menu's `Reset widget` (da-7ymx), not a button of the strip's own.
	test('the strip has no reset button', async () => {
		await shared.solo(async (app) => {
			assert.equal(await app.page.locator('.strip button.reset').count(), 0);
			assert.equal(await app.page.locator('section.pane[data-title="dayview"] button.reset').count(), 0);
		});
	});

	// da-mu5op: «dot symbols in dayview and nowview should not be
	// selectable (as text). Right now long pressing or double tapping on a
	// symbol often directly selects the text». A headless browser does no
	// touch selection and a mouse never lands on a glyph (it has
	// `pointer-events="none"`), so what is read is what those gestures
	// would take: a selection over each picture — the strip, and nowview,
	// whose pane has only the SVG's own root style — takes no text at all.
	test("a picture's text is not selectable: a dot's symbol, the ruler, nowview", async () => {
		await shared.solo(async (app) => {
			await app.run(['spec', 'set', 'coffee', '--verb', 'occur', '--glyph', '▲']);
			await app.submit('occur coffee -t -30');
			const glyph = app.page.locator('.band[data-days-ago="0"] text.point-fg', { hasText: '▲' });
			const nowGlyph = app.page.locator(`${NOWVIEW} text.point-fg`, { hasText: '▲' });
			await glyph.waitFor();
			await nowGlyph.waitFor();
			for (const text of [glyph, nowGlyph, app.page.locator('.ruler .tick-label').first()])
				assert.equal(await text.evaluate((t) => getComputedStyle(t).userSelect), 'none');
			/** What selecting everything in `el` takes. */
			const taken = (el) =>
				el.evaluate((el) => {
					getSelection().selectAllChildren(el);
					const text = getSelection().toString();
					getSelection().removeAllRanges();
					return text;
				});
			assert.equal(await taken(app.page.locator('.strip')), '', 'the strip');
			assert.equal(await taken(app.page.locator(`${NOWVIEW} svg:has(> g.plot)`)), '', 'nowview');
		});
	});

	test('the strip writes nothing down, and draws with the store blocked', async () => {
		// Whatever is in the store before the handle moves is what is in it
		// after: the strip has no key of its own, and adds none.
		let fallback;
		await settledRoom(shared, 'nowrite', async (app) => {
			fallback = await valueOf(app);
			await sizeFor(app, 2);
			const before = await storeKeys(app);

			await dragBy(app, 'button.day-grip', 0, -20);
			await app.page.locator('button.day-grip').focus();
			await app.page.keyboard.press('ArrowUp');
			await app.page.locator('button.freeze').click();
			await heardIs(app, false, 'freezeToday');
			// Heard, and drawn: the strip has done what it does with it.
			await app.idle();

			assert.deepEqual(await storeKeys(app), before, 'the strip wrote to localStorage');
		});

		// And a store that throws on the ACCESS — which is what a browser
		// set to block site data actually does — changes nothing: the strip
		// draws, and the handle works for the visit.
		await withApp( // own app: a `prepare` that blocks the store
			async (app) => {
				// Nothing remembers a visit, so it is a first one, on `welcome` (da-u7vz8).
				await app.toCollect();
				await app.page.locator('.band[data-days-ago="0"] svg.svg').waitFor();
				assert.equal(await valueOf(app), fallback);
				await sizeFor(app, 1);
				await app.page.locator('button.day-grip').focus();
				await app.page.keyboard.press('ArrowUp');
				assert.ok((await valueOf(app)) > fallback);
				assert.deepEqual(app.errors, [], 'the page threw with storage blocked');
			},
			{
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

	test('the jagged line says the day above the divider is not the newest', async () => {
		await shared.solo(async (app) => {
			// Deeper than the box shows whole, so there is something to
			// scroll. Nothing to tear yet: yesterday is at the bottom,
			// against the divider.
			await sizeFor(app, 5);

			const box = await stackScroll(app);
			assert.ok(box.total > box.client, `the stack does not scroll: ${JSON.stringify(box)}`);
			assert.equal(await jagged(app), 0, 'the jagged line is up with yesterday in place');

			// Scroll up into the older days: now what sits above today is
			// not the day before it, and the strip says so.
			await app.page.locator('.stack').evaluate((el) => (el.scrollTop = 0));
			await app.page.locator('button.jagged').waitFor();

			// A fine tear, not a coarse one: 201 vertices (da-hnyb), where it
			// once had 26 (da-hsba) and then 101, drawn at the same 6px.
			const tear = await app.page.locator('button.jagged svg').evaluate((svg) => ({
				vertices: svg.querySelector('polyline').getAttribute('points').trim().split(/\s+/).length,
				height: svg.getBoundingClientRect().height
			}));
			assert.equal(tear.vertices, 201);
			assert.equal(tear.height, 6);

			// It is a button, it takes focus, and the keyboard works it.
			await app.page.locator('button.jagged').focus();
			assert.ok(
				await app.page.evaluate(() => document.activeElement.classList.contains('jagged')),
				'the jagged line does not take focus'
			);
			await app.page.locator('button.jagged').press('Enter');
			await app.page.locator('button.jagged').waitFor({ state: 'detached' });

			const back = await stackScroll(app);
			assert.ok(
				back.top + back.client >= back.total - 1,
				`enter did not bring yesterday back: ${JSON.stringify(back)}`
			);
			// The depth is unchanged — the line puts the stack back, it does
			// not throw away the days it holds.
			assert.equal(await app.page.locator('.stack .band').count(), heldFor(5));

			// And the same by pointer, which is the other half of "reachable
			// by keyboard and by touch".
			await app.page.locator('.stack').evaluate((el) => (el.scrollTop = 0));
			await app.page.locator('button.jagged').click();
			await app.page.locator('button.jagged').waitFor({ state: 'detached' });
		});
	});

	test('a re-pin does not take back a scroll the strip has not heard yet', async () => {
		// A scroll's event fires a frame after the scroll. Anything that
		// re-anchors the stack in that frame — a band landing, the minute's
		// redraw of today, here the freeze toggle — used to read the stale
		// «at the bottom» and pin the reader straight back down (da-6m5s:
		// the jagged test's second scroll, lost in CI to a band landing).
		await settledRoom(shared, 'repin', async (app) => {
			await sizeFor(app, 5);
			await app.page.locator('.stack').evaluate((el) => {
				el.scrollTop = 0;
				document.querySelector('button.freeze').click();
			});
			await app.page.locator('button.jagged').waitFor();
			assert.equal((await stackScroll(app)).top, 0, 'the re-pin took the scroll back');
		});
	});

	test('a midnight moves the whole strip on by a day', async () => {
		/** The days `from` back to 0 before `ms`, oldest first, as stamps. */
		const marks = (ms, from) => Array.from({ length: from + 1 }, (_, i) => labelFor(ms, from - i));
		// A room of `clocked`, held at noon, so the fast-forward below lands
		// after midnight and inside the next day rather than on its
		// boundary. The next test's room or solo puts the clock back.
		await settledRoom(clocked, 'midnight',
			async (app) => {
				const noon = app.clock.at();
				// Deep enough that the stack scrolls, so the jagged line has
				// something to say at the end.
				await sizeFor(app, 5);
				assert.deepEqual(await stamps(app), marks(noon, heldFor(5)));

				// Over the boundary. The strip redraws on the frame's epoch,
				// which is the same beat today's band has always refreshed
				// on — so a command is what makes it look at the clock again.
				const after = noon + 16 * 60 * 60 * 1000;
				await app.clock.set(after);
				await app.submit('occur coffee');
				await app.idle();

				// Today is the new day; yesterday is the day that was today;
				// the past is stamped with the days before that.
				assert.deepEqual(await stamps(app), marks(after, heldFor(5)), 'the strip did not follow the new today');
				// The box did not move, so neither did the depth.
				assert.equal(await app.page.locator('.stack .band').count(), heldFor(5));
				// And the rule still reads off the scroll position, not off
				// the date it was first drawn for.
				assert.equal(await jagged(app), 0);
				await app.page.locator('.stack').evaluate((el) => (el.scrollTop = 0));
				await app.page.locator('button.jagged').waitFor();
			}
		);
	});

	// The strip's row is a fixed height now, so it cannot push the thing a
	// phone user logs with off the bottom of the screen (da-hkpd) — as
	// long as that height is one a phone can spare.
	test("the strip's row leaves the widget bar on screen", async () => {
		const screen = NARROW.viewport;
		// First on `narrow`, so the strip's row is the one a 360px boot
		// lays out, which no test before it has sized.
		await narrow.solo(
			async (app) => {
				await app.submit('occur coffee');
				await app.page.locator('.bar [data-field="coffee"]').waitFor();

				const bar = await app.page.locator('.bar [data-field="coffee"]').boundingBox();
				assert.ok(
					bar.y + bar.height <= screen.height,
					`the widget bar is off the bottom: ${JSON.stringify(bar)}`
				);
				// And the row is a box today fits in whole, not one it
				// scrolls inside.
				const m = await metrics(app);
				assert.ok(m.strip >= m.today, `today's band does not fit its row: ${JSON.stringify(m)}`);
			}
		);
	});

	// The strip is full-width, and the fix for horizontal scrollbars over
	// it landed the day before this did (1ceada8) — so every depth the
	// box can hold is swept, at every width the app is read at.
	test('never scrolls the page sideways, at any width or depth', async (t) => {
		// On `narrow`, booted at 360px wide as a phone is.
		await narrow.solo(
			async (app) => {
				const widths = [360, 390, 768, 1280].map((width) => ({ width, height: 800 }));
				await atEachScreen(t, app.page, widths, async ({ width }) => {
					for (const depth of [0, 1, 5]) {
						await sizeFor(app, depth);
						const seen = await app.page.evaluate(() => {
							const el = document.scrollingElement;
							return { scroll: el.scrollWidth, client: el.clientWidth };
						});
						assert.ok(
							seen.scroll <= seen.client,
							`${width}px at ${depth} days deep scrolls sideways: ${seen.scroll} in ${seen.client}`
						);
					}
				});
			}
		);
	});
});
