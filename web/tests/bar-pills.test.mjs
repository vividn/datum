// The bar's time-cursor pills, and its mark and clock on a phone.
// One of 3 files split from bar.test.mjs by topic, so each stays well inside
// test.mjs's 300 s file cap (da-5rpcr); the helpers they share are
// support/bar.mjs.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { devices } from 'playwright';
import { atEachScreen, sharedApp, startApp, stopApp, withApp, seedLines } from './support/app.mjs';
import { withBar } from './support/layout.mjs';
import { DESKTOP, DAY, BAR, repl, barHeight } from './support/bar.mjs';

before(startApp);
after(stopApp);

const desktop = repl(sharedApp(DESKTOP));

const day = sharedApp(DAY);

/**
 * Wait until the pills have stopped moving.
 *
 * Two lags to outlast, and neither is a keystroke: the mirror's width is
 * measured by a resize observer, and `.pills` transitions its `left` so a
 * pill left behind by a ghost is seen going back. Measured before both, a
 * pill is still standing where the previous line put it.
 */
const settlePills = (page) =>
	page.evaluate(
		() =>
			new Promise((done) => {
				let last = '';
				let same = 0;
				const tick = () => {
					const now = [...document.querySelectorAll('.prompt-bar .pill')]
						.map((p) => Math.round(p.getBoundingClientRect().left))
						.join(',');
					same = now === last ? same + 1 : 0;
					last = now;
					if (same > 4) return done();
					requestAnimationFrame(tick);
				};
				tick();
			})
	);

/** Where a pill's left edge is, and where the `❯` ends. */
const pillAt = (page) =>
	page.evaluate(() => ({
		pill: document.querySelector('.prompt-bar .pill')?.getBoundingClientRect().left,
		caret: document.querySelector('.prompt-bar .line > .prompt').getBoundingClientRect().right
	}));

/** Track `coffee` and get its button onto the widget bar, as ghost.test.mjs does. */
async function withCoffee(app) {
	// Typed, not quiet: a write is what refreshes the bar, a read does not (da-fprk0).
	await seedLines(app, ['setup coffee --verb occur']);
	await app.page.locator('.bar [data-field="coffee"]').waitFor();
}

const setPill = (page, pill) =>
	page.evaluate((p) => window.datum.bar.setPills([p]), { id: 'p', kind: 'time', ...pill });

describe('the pills ride with the text', () => {
	test('a pill sits just after the ❯ when the box is empty, and moves right as the line grows', async () => {
		await desktop(async (app) => {
			const { page } = app;
			await setPill(page, { text: '-t 14:30' });
			await page.locator(`${BAR} .pill`).waitFor();

			const empty = await pillAt(page);
			// "a little bit of space between the > and the pill so that it is
			// easy to click into the box" — and not so much that it reads as
			// belonging to something else.
			assert.ok(
				empty.pill - empty.caret > 4 && empty.pill - empty.caret < 40,
				`the pill is ${empty.pill - empty.caret}px from the ❯`
			);

			const input = page.locator(`${BAR} input`);
			await input.fill('occur coffee beans=ethiopian');
			await page.waitForFunction(
				(was) => document.querySelector('.prompt-bar .pill').getBoundingClientRect().left > was + 20,
				empty.pill
			);

			// However long the line gets it goes OFF THE RIGHT EDGE with it,
			// and the box scrolls right to reach it. vividn, 2026-09-18: "They
			// should be to right, and just go off the right edge of the screen.
			// The user can scroll to the right when input text gets long, so it
			// should be equivalently present in this rightwards scroll." This
			// replaces the claim that stood here — that the pill never leaves
			// the box, which is what parked it at the box's right edge.
			await input.fill('occur coffee ' + 'x'.repeat(400));
			await app.idle();
			await settlePills(page);
			const long = await page.evaluate(() => {
				const pill = document.querySelector('.prompt-bar .pill').getBoundingClientRect();
				const mirror = document.querySelector('.prompt-bar .mirror').getBoundingClientRect();
				const port = document.querySelector('.prompt-bar .field');
				return {
					pill: pill.left,
					text: mirror.right,
					reach: port.scrollWidth - port.clientWidth
				};
			});
			// Still after the text, wherever the text has got to, and with
			// somewhere to scroll to it.
			assert.ok(long.pill >= long.text - 1, `the pill at ${long.pill} is over the text ending at ${long.text}`);
			assert.ok(long.reach > 0, 'a line 400 characters long left the run with nothing to scroll');

			// And a scroll to the end of the run brings it whole into the box.
			await page.locator(`${BAR} .field`).evaluate((el) => (el.scrollLeft = el.scrollWidth));
			await page.waitForFunction(() => {
				const pill = document.querySelector('.prompt-bar .pill').getBoundingClientRect();
				const box = document.querySelector('.prompt-bar .field').getBoundingClientRect();
				return pill.left >= box.left - 1 && pill.right <= box.right + 1;
			});
			assert.deepEqual(app.errors, []);
		});
	});

	test('a press spends its unlocked pill beside the ghost; a locked one stays and comes back left', async () => {
		await day.solo(async (app) => {
			const { page } = app;
			await withCoffee(app);
			await setPill(page, { text: '-t 14:30', locked: true });
			await page.locator(`${BAR} .pill`).waitFor();
			const waiting = (await pillAt(page)).pill;
			const placed = await page.evaluate(() => parseFloat(document.querySelector('.prompt-bar .pills').style.left));

			// A press nobody typed — the widget bar's, which is the press that
			// carries the pills (`pressWithPills`). Watched from before it:
			// the ghost can be gone by the time the press lands (da-ehpe5).
			const saw = await app.watchGhosts();
			await app.pressWidget('coffee');
			// The ghost is the command ALONE: the pill is drawn as a pill
			// beside it, so saying it in the text too would say it twice.
			assert.deepEqual([...new Set((await saw('moment coffee')).map((g) => g.line))], ['moment coffee']);
			await page.waitForFunction(
				(was) => window.__datumGhosts.some((g) => g.left > was + 20),
				placed
			);

			// The ghost goes; the locked pill stays and returns leftward, to
			// where it waited.
			await page.waitForFunction(() => !document.querySelector('.prompt-bar .ghost'));
			await page.waitForFunction(
				(was) => Math.abs(document.querySelector('.prompt-bar .pill').getBoundingClientRect().left - was) < 1,
				waiting
			);
			assert.equal(await page.locator(`${BAR} .pill`).count(), 1, 'a locked pill was spent');
			assert.deepEqual(app.errors, []);
			// The day page: `withCoffee` presses the COLLECT widget, which is a
			// pane of the day frame and is not on the repl layout.
		});
	});

	test('a spent pill fades in step with the ghost it went into, though it is spent later', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await withCoffee(app);

			// The ghost starts at the press and the pill is spent only when
			// the command lands (da-jrozq), so a pill that started a fade of
			// its own would lag the ghost by the whole command. Both clocks
			// read in one evaluation, the moment the spent pill is there.
			//
			// And a busy frame, made here rather than hoped for from a loaded
			// runner (da-d7ndh): the main thread held 150ms the moment the
			// spent pill is in the page, so a pill whose fade starts at the
			// next frame it gets falls that far behind, every run.
			await page.evaluate(() => {
				new MutationObserver(() => {
					if (window.__held || !document.querySelector('.prompt-bar .spent')) return;
					window.__held = performance.now();
					while (performance.now() - window.__held < 150);
				}).observe(document.body, { subtree: true, childList: true });
			});
			// Read in the landing's own evaluation: a press landing near
			// GHOST_MS leaves the two milliseconds, less than a second
			// round trip to the page takes on a loaded runner (da-gd2yn).
			const { clocks } = await pressLanded(app);
			assert.ok(clocks, 'the press landed with no spent pill and ghost fading to read');
			assert.ok(Math.abs(clocks.pill - clocks.ghost) < 50, `pill at ${clocks.pill}ms, ghost at ${clocks.ghost}ms`);
			assert.ok(await page.evaluate(() => window.__held), 'the busy frame never came: nothing held the spent pill');
			await page.waitForFunction(() => !document.querySelector('.prompt-bar .spent'));
			assert.deepEqual(app.errors, []);
			// The wall clock: how far into its fade the pill joins is the
			// time the command took, on `Date` (#lib/bar.ts), which a held
			// clock says was none.
		}, { ...DAY, clock: null });
	});

	test('an unlocked pill leaves the live set at once and fades as a spent one', async () => {
		await day.solo(async (app) => {
			const { page } = app;
			await withCoffee(app);

			// Gone from the live pills — so gone from the next command's argv
			// and from the time cursor — and on screen only as a spent one.
			// Counted at the landing itself: the spent pill lives what is left
			// of GHOST_MS after it, which can be milliseconds (da-ohoei).
			const { pills, spent } = await pressLanded(app);
			assert.equal(pills, 0);
			assert.equal(spent, 1);
			await page.waitForFunction(() => !document.querySelector('.prompt-bar .spent'));
			assert.deepEqual(app.errors, []);
			// The day page, as above: the press is the collect widget's.
		});
	});
});

/** How long the ghost stays, and so a spent pill with it (#lib/bar.ts). */
const GHOST_MS = 1000;

/**
 * Load an unlocked time pill, press the coffee widget and wait for the
 * press to LAND: the pill leaving the live set, which is the moment it is
 * spent (da-jrozq). A spent pill joins the ghost's clock, so a press that
 * lands more than GHOST_MS after it was pressed leaves none to see, and a
 * test waiting for one would time out 30s later reading as a missing
 * feature (da-wjz42). A loaded CI runner took 1032ms once (da-d7ndh): that
 * is the runner, not the bar, so a press that slow is pressed again with a
 * fresh pill — at a time of its own, so no two presses write one entry —
 * and five of them fail, with every timing.
 *
 * Each press waits for nothing to be in flight (da-8ep3q): a press lands
 * after the write's own bar reads, and one pressed at once after the last
 * landed queued its write behind that landing's pane refreshes — 2 to 8 of
 * them in flight at every retry, measured — so on a loaded runner every
 * retry ran slow for the press before it (1028, 1066, 1151ms).
 *
 * Answers the bar's live and spent pill counts, and how far through its
 * fade the spent pill and its ghost each are (null with either gone), read
 * in the landing's own evaluation, since a press landing just under
 * GHOST_MS leaves the spent pill milliseconds to live.
 */
const pressLanded = async (app) => {
	const { page } = app;
	const took = [];
	while (took.length < 5) {
		await setPill(page, { text: `-t 14:3${took.length}` });
		await page.locator(`${BAR} .pill`).waitFor();
		await page.evaluate(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))));
		await app.idle();
		await page.evaluate(() => (window.__pressedAt = performance.now()));
		await app.pressWidget('coffee');
		const landed = await page
			.waitForFunction(
				(bar) =>
					window.datum.bar.pills().length === 0 && {
						took: performance.now() - window.__pressedAt,
						pills: document.querySelectorAll(`${bar} .pill`).length,
						spent: document.querySelectorAll(`${bar} .spent`).length,
						clocks: (() => {
							const [pill, ghost] = ['spent', 'ghost'].map(
								(c) => document.querySelector(`${bar} .${c}`)?.getAnimations()[0]
							);
							if (!pill || !ghost) return null;
							// How far through, in ms: the pill takes the ghost's
							// start time (`withGhost`), so a busy frame cannot
							// put it behind.
							const t = (a) => a.effect.getComputedTiming().progress * 1000;
							return { pill: t(pill), ghost: t(ghost) };
						})()
					},
				BAR
			)
			.then((h) => h.jsonValue());
		took.push(landed.took);
		if (landed.took < GHOST_MS) return landed;
	}
	assert.fail(`no press landed inside the ghost's ${GHOST_MS}ms: ${took.map(Math.round).join(', ')}ms`);
};

/**
 * Wait for the pills to have stopped moving after a line was typed.
 *
 * Two things happen and neither is a promise the test can await: the mirror
 * measures the new text a frame after it is in the box (da-xfa4), and the
 * pills then SLIDE to where it puts them over 120ms — or, on a screen too
 * narrow for them, do not move at all. So the condition is the one that
 * covers both: the mirror says the typed line, and nothing has moved for a
 * quarter of a second.
 */
const settled = async (page, selector, typed) => {
	await page.evaluate(() => delete window.__settle);
	await page.waitForFunction(
		([s, text]) => {
			if (document.querySelector('.prompt-bar .mirror').textContent !== text) return false;
			const left = document.querySelector(s).getBoundingClientRect().left;
			const seen = window.__settle;
			if (!seen || Math.abs(seen.left - left) > 0.5) {
				window.__settle = { left, since: performance.now() };
				return false;
			}
			return performance.now() - seen.since > 250;
		},
		[selector, typed]
	);
};

/** Every live pill on the bar, as `setPills` would have them. */
const setPills = (page, texts) =>
	page.evaluate(
		(all) => window.datum.bar.setPills(all.map((text, i) => ({ id: `p${i}`, kind: 'time', text }))),
		texts
	);

/**
 * What the reader can SEE of the first live pill — read off the page, and
 * the kind's colour resolved through a probe rather than written here, so
 * the colour stays in the one place the component keeps it.
 */
const pillLook = (page) =>
	page.evaluate(() => {
		const pill = document.querySelector('.prompt-bar .pill');
		const style = getComputedStyle(pill);
		const probe = document.createElement('span');
		probe.style.color = style.getPropertyValue('--pill');
		document.body.append(probe);
		const kind = getComputedStyle(probe).color;
		probe.remove();
		const buttons = [...pill.querySelectorAll('button')];
		return {
			background: style.backgroundColor,
			kind,
			size: style.fontSize,
			inputSize: getComputedStyle(document.querySelector('.prompt-bar input')).fontSize,
			text: pill.textContent,
			labels: buttons.map((b) => b.getAttribute('aria-label')),
			lockIsSvg: !!pill.querySelector('button.lock > svg'),
			// The × is a MARK inside the remove button, never a target of its own.
			xIsOwnButton: buttons.some((b) => b.textContent.trim() === '×'),
			nested: buttons.some((b) => b.querySelector('button')),
			height: pill.getBoundingClientRect().height,
			ring: style.boxShadow
		};
	});

// vividn, on the deployed build (da-tnv3): "We need to make the pills much
// more prominent, they are almost invisible right now. Solid background
// color, click anywhere except lock to dismiss… increase their font size
// to be the same as the input bar… Let's use a flat lock icon, mono
// colour." Every claim below is measured on the page: the fill is compared
// with the kind's own colour and the size with the input's own size, so
// neither number is written twice.
describe('the pills are prominent', () => {
	test('solid in its kind’s colour, at the input’s size, with an SVG lock and no × of its own', async () => {
		await desktop(async (app) => {
			const { page } = app;
			await setPill(page, { text: '-t 14:30' });
			await page.locator(`${BAR} .pill`).waitFor();

			const look = await pillLook(page);
			assert.notEqual(look.background, 'rgba(0, 0, 0, 0)', 'the pill is not filled');
			assert.equal(look.background, look.kind, `filled ${look.background}, its kind is ${look.kind}`);
			assert.equal(look.size, look.inputSize, `the pill at ${look.size}, the input at ${look.inputSize}`);

			// No emoji anywhere in what the pill says, and the lock is drawn.
			assert.equal(look.text.replace('×', ''), '-t 14:30');
			assert.ok(look.lockIsSvg, 'the lock is not an svg');
			assert.ok(!look.xIsOwnButton, 'the × is a button of its own');
			assert.ok(!look.nested, 'a button inside a button');
			assert.deepEqual(look.labels, ['remove -t 14:30', 'lock -t 14:30']);
			assert.deepEqual(app.errors, []);
		});
	});

	test('a press anywhere but the lock removes it — text, ×, padding and Enter alike', async () => {
		await desktop(async (app) => {
			const { page } = app;
			const seen = await app.spyOnRunner();
			const live = () => page.evaluate(() => window.datum.bar.pills().length);

			// The lock is the one part that is not the remove button: it flips
			// `aria-pressed`, marks the pill, and the pill stays.
			await setPill(page, { text: '-t 14:30' });
			await page.locator(`${BAR} .pill button.lock`).click();
			assert.equal(
				await page.locator(`${BAR} .pill button.lock`).getAttribute('aria-pressed'),
				'true'
			);
			assert.equal(await live(), 1, 'the lock removed the pill');
			const locked = await pillLook(page);
			assert.notEqual(locked.ring, 'none', 'a locked pill reads no differently');

			// A locked pill still carries into the argv a submitted line builds.
			await app.submit('tail');
			assert.ok(
				(await seen()).some((argv) => argv.join(' ').includes('-t 14:30')),
				'the pill never reached the argv'
			);

			// Everywhere else on it is one press: the text, the × beside it,
			// the padding under the pointer, and Enter on the focused button.
			for (const target of ['.pill .label', '.pill .x', '.pill button.remove']) {
				await setPill(page, { text: '-t 14:30' });
				await page.locator(`${BAR} ${target}`).waitFor();
				await page.locator(`${BAR} ${target}`).click();
				await page.waitForFunction(() => window.datum.bar.pills().length === 0);
				assert.equal(await page.locator(`${BAR} .pill`).count(), 0, `${target} did not remove it`);
			}
			await setPill(page, { text: '-t 14:30' });
			await page.locator(`${BAR} .pill button.remove`).press('Enter');
			await page.waitForFunction(() => window.datum.bar.pills().length === 0);

			// And gone from the argv with it.
			const before = (await seen()).length;
			await app.submit('tail');
			assert.ok(
				!(await seen())
					.slice(before)
					.some((argv) => argv.join(' ').includes('-t 14:30')),
				'a removed pill still reached the argv'
			);
			assert.deepEqual(app.errors, []);
		});
	});

	test('three pills do not grow the bar', async (t) => {
		await desktop(async (app) => {
			const { page } = app;
			await atEachScreen(t, page, [{ width: 1280, height: 900 }, { width: 390, height: 640 }], async () => {
				const bare = await barHeight(page);
				await setPills(page, ['-t 14:30', '-d yesterday', '30m']);
				await page.locator(`${BAR} .pill`).first().waitFor();
				await app.idle();
				assert.equal(await barHeight(page), bare, 'the pills grew the bar');

				// The trap the larger font makes (da-xfa4's mirror): the pills
				// still ride AFTER what is typed, not over it — or, when the
				// line has outgrown the box, stand at its right edge, which is
				// the clamp Prompt.svelte documents.
				//
				// Three pills are wider than a 390px phone's whole row — 435px
				// of them against a 390px screen — so there the second case is
				// the only one available and not every pill CAN be on screen.
				// What is asked of it is the bound that is achievable: the set
				// reaches leftward to the screen's edge and no further, so the
				// leftmost pill is always reachable. (A set wider than the
				// screen still runs off the right; showing all of it would
				// want a scroll or a wrap, which no flow of the app asks for
				// today — the pair a `retime` makes is two, and that pair is
				// held whole by the test above.) Riding after the `❯` is the
				// FIRST case's claim, made where the pills fit.
				await page.locator(`${BAR} input`).fill('occur coffee');
				await settled(page, `${BAR} .pills`, 'occur coffee');
				const after = await page.evaluate(() => {
					const at = (s) => document.querySelector(`.prompt-bar ${s}`).getBoundingClientRect();
					const pills = at('.pills');
					return {
						pill: at('.pill').left,
						caret: at('.line > .prompt').right,
						text: at('.mirror').right,
						pillsRight: pills.right,
						pillsWidth: pills.width,
						box: at('.field').right,
						boxWidth: at('.field').width,
						screen: document.documentElement.clientWidth
					};
				});
				const fits = after.pillsWidth <= after.boxWidth;
				if (fits)
					assert.ok(after.pill > after.caret, `the pill at ${after.pill}, the ❯ ends at ${after.caret}`);
				else
					assert.ok(
						after.pill >= -0.5,
						`the pills reach to ${after.pill}, past the screen's left edge`
					);
				assert.ok(
					after.pill >= after.text - 1 || after.pillsRight >= after.box - 1,
					`the pill at ${after.pill} is over the typed text, which ends at ${after.text}`
				);
				assert.equal(await barHeight(page), bare, 'the pills grew the bar');
				assert.deepEqual(app.errors, []);
				// Cleared, so the next screen starts from a bare bar.
				await setPills(page, []);
				await page.locator(`${BAR} input`).fill('');
				await page.locator(`${BAR} .pill`).waitFor({ state: 'detached' });
			});
		});
	});
});

describe('two pills on a phone', () => {
	// vividn's own flow: a tail row, a dayview tap, one tap on `retime` — which
	// puts a hid pill AND a time pill in the prompt at once. On this bead's
	// row they no longer fit the box, and where they go was settled twice.
	//
	// vremya asked, and I built, pills that reach LEFTWARD to stay on the
	// screen. vividn overruled it (2026-09-18): "About pills overlapping the
	// clock, this is not correct behavior. They should never go over the >
	// at all. They should be to right, and just go off the right edge of the
	// screen. The user can scroll to the right when input text gets long, so
	// it should be equivalently present in this rightwards scroll."
	//
	// So the claim is HIS, and it is the one this file holds now: the `❯` is
	// a wall the pills never cross, they ride after the typed text, and they
	// come back with it when a long line is scrolled right. Going off the
	// right edge is allowed — it is what he asked for — so nothing here
	// asserts they are all on the screen.
	const TWO = [
		{ id: 'h', kind: 'hid', text: '35mtf', locked: true },
		{ id: 't', kind: 'time', text: '-t 14:30', locked: true }
	];

	const settle = settlePills;

	/** Every pill's left edge, the scrollport's box, and where the `❯` ends. */
	const where = (page) =>
		page.evaluate(() => {
			const port = document.querySelector('.prompt-bar .field');
			const box = port.getBoundingClientRect();
			return {
				pills: [...document.querySelectorAll('.prompt-bar .pill')].map((p) =>
					Math.round(p.getBoundingClientRect().left)
				),
				ends: [...document.querySelectorAll('.prompt-bar .pill')].map((p) =>
					Math.round(p.getBoundingClientRect().right)
				),
				// The one wall: nothing of the run is PAINTED outside this,
				// whatever the run's own layout says, because the port clips.
				port: { left: Math.round(box.left), right: Math.round(box.right) },
				left: port.scrollLeft,
				reach: port.scrollWidth - port.clientWidth,
				caret: Math.round(document.querySelector('.prompt-bar .line > .prompt').getBoundingClientRect().right)
			};
		});

	for (const width of [360, 390]) {
		test(`at ${width} the pills stay right of the \u276f, typed line or not`, async (t) => {
			await withApp(async (app) => { // its own: one screen per test
				const { page } = app;
				await page.evaluate((p) => window.datum.bar.setPills(p), TWO);
				await page.locator(`${BAR} .pill`).nth(1).waitFor();
				await settle(page);

				const empty = await where(page);
				for (const left of empty.pills)
					assert.ok(left >= empty.caret, `a pill starts at ${left}, left of the \u276f at ${empty.caret}`);

				// A line long enough to push them: they follow the text, and
				// still never reach the `\u276f`.
				await page.locator(`${BAR} input`).fill('occur coffee beans=ethiopian roast=dark');
				await settle(page);
				const typed = await where(page);
				for (const left of typed.pills)
					assert.ok(left >= typed.caret, `typing put a pill at ${left}, left of the \u276f at ${typed.caret}`);
				assert.ok(
					typed.pills[0] > empty.pills[0],
					`the pills did not follow the text: ${empty.pills[0]} then ${typed.pills[0]}`
				);

				// And scrolling that line right brings them back onto the screen
				// — "equivalently present in this rightwards scroll". The
				// SCROLLPORT is what the `\u276f` is walled off by: the run may
				// lie left of it, and what lies left of it is clipped away.
				assert.ok(
					typed.reach > 0,
					'the pills are off the edge with nothing left to scroll to them'
				);
				assert.ok(
					typed.port.left >= typed.caret,
					`the box starts at ${typed.port.left}, over the \u276f at ${typed.caret}`
				);
				await page.locator(`${BAR} .field`).evaluate((el) => (el.scrollLeft = el.scrollWidth));
				await settle(page);
				const scrolled = await where(page);
				assert.ok(
					scrolled.pills[0] < typed.pills[0],
					`scrolling right did not bring the pills back: ${typed.pills[0]} then ${scrolled.pills[0]}`
				);
				// The END of the run is inside the port, which is what makes the
				// last pill reachable — the pair together is wider than a
				// 360px phone's box, so no scroll can hold both of them at
				// once and asking for that would be asking for the impossible.
				const end = scrolled.ends[scrolled.ends.length - 1];
				assert.ok(
					end <= scrolled.port.right + 1,
					`the last pill ends at ${end}, past the box's ${scrolled.port.right}`
				);
				assert.equal(scrolled.left, scrolled.reach, 'the run did not scroll to its end');
				assert.deepEqual(app.errors, []);
			}, { viewport: { width, height: 720 }, device: { isMobile: true, hasTouch: true }, prepare: withBar });
		});
	}
});

describe('a phone stacks the mark over the clock (da-ovdtt)', () => {
	// vividn, 2026-09-28, from vremya's walk on a Pixel 7: side by side, the
	// mark and the clock took about 40% of the bar, and with `occur tea
	// amount=3 cups=1` composed only `[mount=3] [cups=1]` showed. So on a
	// phone they share one column, the mark over the clock — and the mark
	// stays, because it is the database switcher.
	const PIXEL = { device: devices['Pixel 7'], prepare: withBar };

	/**
	 * The mark, the clock and where the box starts — and where the box
	 * started when the two stood side by side, which is the same page with
	 * the column taken back out of the row (`display: contents`, as a wide
	 * screen has it).
	 */
	const measure = (page) =>
		page.evaluate(async () => {
			const box = (s) => document.querySelector(s).getBoundingClientRect();
			const frame = () => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
			const now = { mark: box('.prompt-bar .brand'), clock: box('.prompt-bar .section.input .stamp'), input: box('.prompt-bar .section.input'), field: box('.prompt-bar input').left };
			const head = document.querySelector('.prompt-bar .head');
			head.style.display = 'contents';
			await frame();
			const before = box('.prompt-bar input').left;
			head.style.display = '';
			await frame();
			return { ...now, before };
		});

	test('at a Pixel 7 the clock sits under the mark, and the box starts further left', async () => {
		await withApp(async (app) => {
			const { mark, clock, input, field, before } = await measure(app.page);
			assert.ok(Math.abs(mark.left - clock.left) <= 1, `the mark starts at ${mark.left}, the clock at ${clock.left}`);
			assert.ok(clock.top >= mark.bottom - 1, `the clock starts at ${clock.top}, above the mark's foot at ${mark.bottom}`);
			for (const [what, b] of [['the mark', mark], ['the clock', clock]])
				assert.ok(b.top >= input.top - 1 && b.bottom <= input.bottom + 1, `${what} is ${b.top}–${b.bottom}, the input row ${input.top}–${input.bottom}`);
			assert.ok(field < before - 40, `the box starts at ${field}, side by side it started at ${before}`);
			assert.deepEqual(app.errors, []);
		}, PIXEL);
	});

	test('at a Pixel 7 a tap on the mark still opens the databases', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await page.locator(`${BAR} .brand`).tap();
			await page.locator('[role="menu"][aria-label="databases"]').waitFor();
			assert.deepEqual(app.errors, []);
		}, PIXEL);
	});

	// The control: a wide screen was side by side and still is.
	test('a desktop is unchanged: the box starts where it did', async () => {
		await desktop(async (app) => {
			const { mark, clock, field, before } = await measure(app.page);
			assert.ok(clock.left >= mark.right - 1, `the clock starts at ${clock.left}, inside the mark ending at ${mark.right}`);
			assert.equal(field, before);
			assert.deepEqual(app.errors, []);
		});
	});
});

describe('the clock', () => {
	// It was UNDER the logo while the bar was two tiers with the mark
	// spanning both (da-xfa4). da-ckw3 put the four things of the input on
	// ONE row, so the claim moves with it: the clock is BESIDE the mark, on
	// the row, and still on the box's own line.
	test('is beside the logo, on the input row’s line', async () => {
		await desktop(async (app) => {
			const { page } = app;
			const boxes = await page.evaluate(() => {
				const rect = (s) => {
					const r = document.querySelector(s).getBoundingClientRect();
					return { top: r.top, bottom: r.bottom, left: r.left, mid: r.top + r.height / 2 };
				};
				return {
					brand: rect('.prompt-bar .brand'),
					stamp: rect('.prompt-bar .section.input .stamp'),
					input: rect('.prompt-bar input'),
					clock: document.querySelector('.prompt-bar .section.input .stamp').textContent
				};
			});
			assert.match(boxes.clock, /^\d\d:\d\d:\d\d$/, `the clock reads ${boxes.clock}`);
			assert.ok(boxes.stamp.left > boxes.brand.left, 'the clock is not past the logo');
			assert.ok(
				Math.abs(boxes.stamp.mid - boxes.brand.mid) < 6,
				`the clock's middle at ${boxes.stamp.mid}, the mark's at ${boxes.brand.mid} — not one row`
			);
			assert.ok(
				Math.abs(boxes.stamp.mid - boxes.input.mid) < 12,
				`the clock's middle at ${boxes.stamp.mid}, the prompt's at ${boxes.input.mid}`
			);
			assert.deepEqual(app.errors, []);
		});
	});
});
