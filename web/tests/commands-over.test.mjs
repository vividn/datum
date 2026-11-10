// The `❯` as a button (da-xw655): pressed, it turns a quarter, inverts, and
// slides the command widget up over the output section; it closes on the
// `❯` again, on Escape, on a run (which the output then shows) and on a
// choice that fills the line.
//
// vividn, 2026-10-27: «the ❯ become a button, so it has a circle around it.
// When it is clicked (or activated, explained below), it rotates 90 degress
// (with animation please), inverts colors, (so that the button is filled in
// and the chevron is empty), and the command widget slides up overtop the
// output section». And at 17:39, of closing it: «yes all of those … For
// commands that run, then it should close the command widget to display the
// command that just ran and it's output in the outputbar».

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, withApp } from './support/app.mjs';
import { dragOn } from './support/bar.mjs';
import { rested, swipe, withBar } from './support/layout.mjs';
import { reach } from './support/commands.mjs';

before(startApp);
after(stopApp);

const BAR = '.prompt-bar';
const CHEVRON = `${BAR} button.prompt`;
const OVER = `${BAR} #bar-commands`;

const shared = sharedApp({ viewport: { width: 1280, height: 800 }, prepare: withBar });
const onPage = async (fn) => {
	const app = await shared();
	await app.page.locator(`${BAR} input`).fill('');
	await app.home();
	await app.empty();
	if (await app.page.locator(OVER).count()) await app.page.locator(CHEVRON).click();
	await app.page.locator(OVER).waitFor({ state: 'detached' });
	return fn(app);
};

/**
 * Wait for the commands to rest where they slid to, read frame by frame.
 * Svelte's intro first runs a placeholder animation of no length and starts
 * the real slide only when that one finishes, so a wait read at once can
 * find the placeholder alone, finished, and the buttons still below where
 * they rest (da-28k9h).
 */
const slid = (page) =>
	page.locator(OVER).evaluate(async (el) => {
		for (;;) {
			await new Promise(requestAnimationFrame);
			const moving = el.getAnimations({ subtree: true }).filter((a) => a.playState !== 'finished');
			if (!moving.length) return;
			await Promise.all(moving.map((a) => a.finished));
		}
	});

/** What the button says to assistive tech, and how it is drawn. */
const chevron = (page) =>
	page.locator(CHEVRON).evaluate((el) => {
		const style = getComputedStyle(el);
		const accent = getComputedStyle(document.documentElement).getPropertyValue('--accent').trim();
		const probe = document.createElement('i');
		probe.style.color = accent;
		document.body.append(probe);
		const accentRgb = getComputedStyle(probe).color;
		probe.remove();
		return {
			label: el.getAttribute('aria-label'),
			expanded: el.getAttribute('aria-expanded'),
			controls: el.getAttribute('aria-controls'),
			transform: style.transform,
			filled: style.backgroundColor === accentRgb,
			radius: style.borderRadius,
			size: el.getBoundingClientRect().height
		};
	});

describe('the ❯ opens the commands over the output', () => {
	test('a press turns it, fills it and slides the commands over the output; a second takes them away', async () => {
		await onPage(async ({ page, errors }) => {
			const shut = await chevron(page);
			assert.equal(shut.label, 'Commands');
			assert.equal(shut.expanded, 'false');
			assert.equal(shut.filled, false, 'the shut ❯ is filled');
			assert.equal(shut.transform, 'none');
			assert.equal(shut.radius, '50%');
			assert.ok(shut.size >= 24, `the ❯ is ${shut.size}px`);

			await page.locator(CHEVRON).click();
			await page.locator(`${OVER} .bar.commands button[data-command="undo"]`).waitFor();
			await slid(page);
			await page.locator(CHEVRON).evaluate((el) => Promise.all(el.getAnimations().map((a) => a.finished)));
			const open = await chevron(page);
			assert.equal(open.expanded, 'true');
			assert.equal(open.controls, 'bar-commands');
			assert.equal(open.filled, true, 'the open ❯ is not filled');
			// A quarter turn back, so the ❯ points up: matrix(cos −90°, sin −90°, −sin −90°, cos −90°, 0, 0).
			const [a, b, c, d] = open.transform.match(/-?[\d.e-]+/g).map(Number);
			assert.deepEqual([a, b, c, d].map((n) => Math.round(n)), [0, -1, 1, 0], `the open ❯ points up: ${open.transform}`);

			// Over the output section exactly: its box, on the input's top edge.
			const boxes = await page.evaluate(() => {
				const at = (s) => document.querySelector(`.prompt-bar ${s}`).getBoundingClientRect();
				const over = at('#bar-commands');
				const output = at('.section.output');
				const input = at('.section.input');
				return { over: [over.top, over.bottom, over.width], output: [output.top, output.bottom, output.width], input: input.top };
			});
			assert.ok(Math.abs(boxes.over[1] - boxes.input) < 1, `the commands end at ${boxes.over[1]}, the input starts at ${boxes.input}`);
			assert.ok(Math.abs(boxes.over[0] - boxes.output[0]) < 1.5, `the commands start at ${boxes.over[0]}, the output at ${boxes.output[0]}`);
			assert.ok(Math.abs(boxes.over[2] - boxes.output[2]) < 1, `the commands are ${boxes.over[2]}px wide, the output ${boxes.output[2]}px`);

			await page.locator(CHEVRON).click();
			await page.locator(OVER).waitFor({ state: 'detached' });
			const again = await chevron(page);
			assert.equal(again.expanded, 'false');
			assert.notEqual(await page.evaluate(() => document.activeElement?.tagName), 'INPUT', 'the ❯ took the caret');
			assert.deepEqual(errors, []);
		});
	});

	test('the keyboard: Enter and Space press it, Escape closes it after a subcommand list is backed out of', async () => {
		await onPage(async ({ page, errors }) => {
			await page.locator(CHEVRON).focus();
			await page.keyboard.press('Enter');
			await page.locator(OVER).waitFor();
			await page.keyboard.press('Space');
			await page.locator(OVER).waitFor({ state: 'detached' });
			await page.keyboard.press('Space');
			await page.locator(OVER).waitFor();

			// `db` opens its subcommands: the first Escape backs out of them
			// and leaves the commands up, the second takes them away.
			await page.locator(`${OVER} button[data-command="db"]`).click();
			await page.locator(`${OVER} button[data-back]`).waitFor();
			await page.keyboard.press('Escape');
			await page.locator(`${OVER} button[data-back]`).waitFor({ state: 'detached' });
			// A close the Escape set off would slide them out: wait out whatever moves.
			await rested(page, page.locator(BAR));
			assert.equal(await page.locator(OVER).count(), 1, 'one Escape backed out and closed too');
			await page.keyboard.press('Escape');
			await page.locator(OVER).waitFor({ state: 'detached' });
			assert.equal((await chevron(page)).expanded, 'false');
			assert.deepEqual(errors, []);
		});
	});

	test('a run closes them and the output shows it', async () => {
		await onPage(async ({ page, errors }) => {
			await page.locator(CHEVRON).click();
			await page.locator(OVER).waitFor();
			await page.locator(`${BAR} input`).fill('moment coffee');
			await page.locator(`${BAR} input`).press('Enter');
			await page.locator(OVER).waitFor({ state: 'detached' });
			await page.locator(`${BAR} .section.output .group:last-child`, { hasText: 'coffee' }).waitFor();
			assert.equal((await chevron(page)).expanded, 'false');
			assert.deepEqual(errors, []);
		});
	});

	test('choosing a command fills the line and closes them', async () => {
		await onPage(async ({ page, errors }) => {
			await page.locator(CHEVRON).click();
			// Opening write is no choice: they stay up for it (da-fg4w4).
			await (await reach(page.locator(`${OVER} .bar.commands`), 'moment')).click();
			await page.locator(OVER).waitFor({ state: 'detached' });
			assert.equal(await page.locator(`${BAR} input`).inputValue(), 'moment ');
			assert.deepEqual(errors, []);
		});
	});

	// A hold fills the bare command too, but to show its flags: it leaves the
	// commands up, and the flags tapped there compose on the line (canary on
	// PR #1281).
	test('holding a command keeps them up, its flags composing on the line', async () => {
		await onPage(async ({ page, errors }) => {
			await page.locator(CHEVRON).click();
			// Where it comes to rest: a press on a button still sliding under it is no hold.
			await page.locator(`${OVER} button[data-command="undo"]`).waitFor();
			await slid(page);
			const box = await (await reach(page.locator(`${OVER} .bar.commands`), 'moment')).boundingBox();
			await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
			await page.mouse.down();
			await page.locator(`${OVER} button[data-flag]`).first().waitFor();
			await page.mouse.up();
			await page.locator(`${OVER} button[data-flag="moment -q"]`).click();
			await page.waitForFunction(() => document.querySelector('.prompt-bar input').value === 'moment -q ');
			await page.locator(`${OVER} button[data-flag="moment -t"]`).click();
			await page.waitForFunction(() => document.querySelector('.prompt-bar input').value === 'moment -q -t ');
			assert.equal(await page.locator(OVER).count(), 1, 'the flags closed the commands');
			assert.deepEqual(errors, []);
		});
	});

	// vividn, 2026-10-27: «pills and filters activate the command widget to
	// open, so you can easily see the available commands for a given filter
	// set»; 17:39: «all pills and filters open it, clearing the last pill
	// closes it» (da-lnirx).
	test('a pill opens them, a second keeps them, and clearing the last closes them', async () => {
		await onPage(async ({ page, errors }) => {
			const set = (all) => page.evaluate((all) => window.datum.bar.setPills(all), all);
			const drop = (id) => page.evaluate((id) => window.datum.bar.removePill(id), id);
			const open = async () => {
				await page.locator(OVER).waitFor();
				assert.equal((await chevron(page)).expanded, 'true');
			};
			const TIME = { id: 'when', kind: 'time', text: '-t 14:30' };
			const NOTE = { id: 'c', kind: 'note', text: '-c two' };

			await set([TIME]);
			await open();
			await set([TIME, NOTE]);
			await open();
			await drop('when');
			await rested(page, page.locator(BAR));
			await open();
			// The commands follow the pills that are left.
			await page.locator(`${OVER} button[data-command="moment"]`).waitFor();
			await drop('c');
			await page.locator(OVER).waitFor({ state: 'detached' });
			assert.equal((await chevron(page)).expanded, 'false');

			// Closed with a pill loaded (Escape: with a pill up the ❯ is the ×,
			// da-gf5xx), a pill rewritten in place does not open them again; one
			// added does.
			await set([TIME]);
			await open();
			await page.keyboard.press('Escape');
			await page.locator(OVER).waitFor({ state: 'detached' });
			await set([{ ...TIME, text: '-t 15:00' }]);
			// An open it set off would slide them in: wait out whatever moves.
			await rested(page, page.locator(BAR));
			assert.equal(await page.locator(OVER).count(), 0, 'a moved pill opened the commands');
			await set([{ ...TIME, text: '-t 15:00' }, NOTE]);
			await open();
			await set([]);
			await page.locator(OVER).waitFor({ state: 'detached' });
			assert.deepEqual(errors, []);
		});
	});

	test('with no motion asked for, it turns and slides without animating', async () => {
		await onPage(async ({ page, errors }) => {
			await page.emulateMedia({ reducedMotion: 'reduce' });
			try {
				await page.locator(CHEVRON).click();
				await page.locator(OVER).waitFor();
				const moving = await page.evaluate(() => ({
					turn: getComputedStyle(document.querySelector('.prompt-bar button.prompt')).transitionDuration,
					slides: document.querySelector('.prompt-bar #bar-commands').getAnimations().length
				}));
				assert.equal(moving.turn, '0s');
				assert.equal(moving.slides, 0);
				await page.locator(CHEVRON).click();
				await page.locator(OVER).waitFor({ state: 'detached' });
			} finally {
				await page.emulateMedia({ reducedMotion: null });
			}
			assert.deepEqual(errors, []);
		});
	});
});

// vividn, 2026-11-07 (da-gf5xx): «I want the ^ to then unfold into an x,
// same weight and style as the chevron. clicking it then clears any input
// text and pills. Similarly when you start typing text into the input bar
// without the command window open, the > should unfold into an x
// (horizontally this time and keeping its dark background) and then also
// clear the input when clicked. Then we can remove the clearing x on the
// right side». The chevron is the ×'s two strokes, each dashed to its first
// half; the × runs them their whole length (8√2, 11.32).
describe('with input on the line, the ❯ is the ×', () => {
	const BOX = `${BAR} input`;
	const NOTE = { id: 'c', kind: 'note', text: '-c two' };

	/** The button once it has come to rest: what it says, how it is drawn, how much of each stroke shows, and where the focus is. */
	const face = (page) =>
		page.locator(CHEVRON).evaluate(async (el) => {
			for (let moving = el.getAnimations({ subtree: true }); moving.length; moving = el.getAnimations({ subtree: true }))
				await Promise.all(moving.map((a) => a.finished));
			const style = getComputedStyle(el);
			const [a, b] = style.transform === 'none' ? [1, 0] : style.transform.match(/-?[\d.e-]+/g).map(Number);
			return {
				label: el.getAttribute('aria-label'),
				title: el.getAttribute('title'),
				act: el.dataset.act ?? null,
				expanded: el.getAttribute('aria-expanded'),
				up: Math.round(a) === 0 && Math.round(b) === -1,
				filled: style.backgroundColor !== 'rgba(0, 0, 0, 0)',
				strokes: [...el.querySelectorAll('path')].map((p) => parseFloat(getComputedStyle(p).strokeDasharray)),
				focus: document.activeElement?.tagName
			};
		});
	const CHEVRON_FACE = { label: 'Commands', title: null, act: null, strokes: [5.66, 5.66] };
	const CROSS = { label: 'clear the input', title: 'clear the input', act: 'fill', strokes: [11.32, 11.32] };
	const line = (page) => page.evaluate(() => ({ text: document.querySelector('.prompt-bar input').value, pills: window.datum.bar.pills().length }));
	const pick = (f, keys) => Object.fromEntries(keys.map((k) => [k, f[k]]));
	const KEYS = ['label', 'title', 'act', 'strokes'];

	test('the commands up with a pill: the ^ unfolds, filled, and a tap clears the pill and the text, the focus staying in the box', async () => {
		await onPage(async ({ page, errors }) => {
			await page.locator(BOX).focus();
			await page.evaluate((pill) => window.datum.bar.setPills([pill]), NOTE);
			await page.locator(OVER).waitFor();
			await page.locator(BOX).fill('occur tea');
			const cross = await face(page);
			assert.deepEqual(pick(cross, KEYS), CROSS);
			assert.deepEqual([cross.up, cross.filled, cross.expanded], [true, true, 'true'], 'the × is not on the turned, filled button');
			assert.equal(await page.locator(`${BAR} .clear`).count(), 0, 'the × on the right is still drawn');

			await page.locator(CHEVRON).click();
			assert.deepEqual(await line(page), { text: '', pills: 0 });
			// The last pill gone closes the commands (da-lnirx), so it folds and turns back.
			await page.locator(OVER).waitFor({ state: 'detached' });
			const after = await face(page);
			assert.deepEqual(pick(after, KEYS), CHEVRON_FACE);
			assert.equal(after.focus, 'INPUT', 'the × took the focus from the box');
			assert.deepEqual(errors, []);
		});
	});

	test('the commands up with text alone: a tap clears it and folds back to the ^, the commands staying up', async () => {
		await onPage(async ({ page, errors }) => {
			await page.locator(CHEVRON).click();
			await page.locator(OVER).waitFor();
			assert.deepEqual(pick(await face(page), KEYS), CHEVRON_FACE, 'the commands up over an empty line drew the ×');
			await page.locator(BOX).fill('occur');
			assert.deepEqual(pick(await face(page), KEYS), CROSS);
			await page.locator(CHEVRON).click();
			assert.deepEqual(await line(page), { text: '', pills: 0 });
			const after = await face(page);
			assert.deepEqual(pick(after, KEYS), CHEVRON_FACE);
			assert.deepEqual([after.up, after.filled, after.expanded], [true, true, 'true'], 'the commands went with the text');
			assert.equal(await page.locator(OVER).count(), 1);
			assert.deepEqual(errors, []);
		});
	});

	test('the commands down with text typed: the ❯ unfolds on its own ground, and a tap clears the line, the focus staying in the box', async () => {
		await onPage(async ({ page, errors }) => {
			await page.locator(BOX).fill('occur tea');
			const cross = await face(page);
			assert.deepEqual(pick(cross, KEYS), CROSS);
			assert.deepEqual([cross.up, cross.filled, cross.expanded], [false, false, 'false'], 'the closed × turned or filled');
			assert.equal(await page.locator(`${BAR} .clear`).count(), 0, 'the × on the right is still drawn');
			await page.locator(CHEVRON).click();
			assert.deepEqual(await line(page), { text: '', pills: 0 });
			const after = await face(page);
			assert.deepEqual(pick(after, KEYS), CHEVRON_FACE);
			assert.equal(after.focus, 'INPUT', 'the × took the focus from the box');
			assert.equal(await page.locator(OVER).count(), 0, 'the × opened the commands');
			assert.deepEqual(errors, []);
		});
	});

	// A pill alone, the commands put away: the ❯ is what opens them again.
	test('the commands down with a pill alone: it is the ❯, and a tap opens them', async () => {
		await onPage(async ({ page, errors }) => {
			await page.evaluate((pill) => window.datum.bar.setPills([pill]), NOTE);
			await page.locator(OVER).waitFor();
			await page.keyboard.press('Escape');
			await page.locator(OVER).waitFor({ state: 'detached' });
			assert.deepEqual(pick(await face(page), KEYS), CHEVRON_FACE);
			await page.locator(CHEVRON).click();
			await page.locator(OVER).waitFor();
			assert.equal(await page.evaluate(() => window.datum.bar.pills().length), 1, 'the ❯ cleared the pill');
			await page.evaluate(() => window.datum.bar.setPills([]));
			await page.locator(OVER).waitFor({ state: 'detached' });
			assert.deepEqual(errors, []);
		});
	});

	test('the strokes unfold after the turn, and with no motion asked for they swap at once', async () => {
		await onPage(async ({ page, errors }) => {
			const timing = () =>
				page.locator(`${CHEVRON} path`).first().evaluate((p) => {
					const style = getComputedStyle(p);
					return [style.transitionDuration, style.transitionDelay];
				});
			await page.evaluate((pill) => window.datum.bar.setPills([pill]), NOTE);
			await page.locator(OVER).waitFor();
			assert.deepEqual(await timing(), ['0.16s', '0.16s'], 'the × does not wait for the turn');
			await page.emulateMedia({ reducedMotion: 'reduce' });
			try {
				assert.equal((await timing())[0], '0s');
			} finally {
				await page.emulateMedia({ reducedMotion: null });
			}
			await page.evaluate(() => window.datum.bar.setPills([]));
			await page.locator(OVER).waitFor({ state: 'detached' });
			assert.deepEqual(errors, []);
		});
	});
});

// vividn, 2026-10-28: «it should intercept swipes rather than going down
// through to the output bar like currently. i.e., swiping up on the command
// pop up bar shouldn't open the cli» (da-m9zo7). And 2026-11-04: «increase
// the size of the buttons in the command pallette. It should be one row,
// horizontally scrollable» (da-qzzox). So the commands are larger circles in
// one row, a sideways swipe or wheel on them scrolls them, the row declares
// `data-pans="x"` so it is never a tab swipe, and the bar and the log under
// them stay where they were; a swipe on the input row still moves the bar.
describe('a swipe on the commands is theirs', () => {
	const PHONE = { viewport: { width: 390, height: 664 }, device: { isMobile: true, hasTouch: true }, prepare: withBar };
	const LIST = `${OVER} .bar.commands`;

	/** How far the bar is open, how tall the log, how far the commands are scrolled, and the root tab in front. */
	const state = (page) =>
		page.evaluate((list) => {
			const style = getComputedStyle(document.querySelector('.shell'));
			const px = (name) => Math.round(parseFloat(style.getPropertyValue(name)) || 0);
			const el = document.querySelector(list);
			return {
				shown: px('--shown'),
				log: px('--log-height'),
				scrolled: Math.round(el?.scrollLeft ?? -1),
				room: el ? el.scrollWidth - el.clientWidth : -1,
				tab: document.querySelector('.root-bar [role="tab"][aria-selected="true"]')?.textContent.replace('•', '').trim()
			};
		}, LIST);

	/**
	 * Wait for the scroll, the bar and the tab to stop moving: the same
	 * reading either side of a rest of the page's, since a scroll is no
	 * animation and moves no box that `rested` reads.
	 */
	const still = async (page) => {
		for (let last = JSON.stringify(await state(page)); ; ) {
			await rested(page);
			const now = JSON.stringify(await state(page));
			if (now === last) return JSON.parse(now);
			last = now;
		}
	};

	/** The circles' rows, whether each is wholly inside the row's box, and whether the last is. */
	const row = (page) =>
		page.locator(LIST).evaluate((bar) => {
			const box = bar.getBoundingClientRect();
			const all = [...bar.querySelectorAll('button.round')].map((b) => b.getBoundingClientRect());
			const inside = (b) => b.left >= box.left - 0.5 && b.right <= box.right + 0.5;
			return {
				circles: all.length,
				lines: new Set(all.map((b) => Math.round(b.top))).size,
				size: Math.min(...all.map((b) => b.height)),
				below: all.every((b) => b.bottom <= box.bottom + 0.5),
				last: inside(all.at(-1)),
				pans: bar.dataset.pans,
				down: bar.scrollHeight > bar.clientHeight + 1
			};
		});

	/**
	 * Up, with every command in them: a pill only a few commands refuse
	 * leaves them flat and long enough to scroll (da-fg4w4), and opens them
	 * itself (da-lnirx).
	 */
	const open = async (page) => {
		await page.evaluate(() => window.datum.bar.setPills([{ id: 'c', kind: 'note', text: '-c two' }]));
		await page.locator(`${LIST} button[data-command="moment"]`).waitFor();
		await slid(page);
	};

	for (const width of [360, 390])
		test(`on a ${width}px phone, the base row is eight larger circles in one row that scrolls sideways and never down`, async () => {
			await withApp(
				async ({ page, errors }) => {
					await page.locator(CHEVRON).click();
					await page.locator(`${LIST} button[data-command="undo"]`).waitFor();
					await slid(page);
					const base = await row(page);
					assert.deepEqual(
						{ ...base, size: base.size >= 44 },
						{ circles: 8, lines: 1, size: true, below: true, last: false, pans: 'x', down: false },
						`the base row: ${JSON.stringify(base)}`
					);
					assert.deepEqual(errors, []);
				},
				{ ...PHONE, viewport: { width, height: 664 } }
			);
		});

	test('on a phone, a sideways swipe scrolls them to the last, in one row, and the bar and the root tab stay; a swipe on the input row still moves the bar', async () => {
		await withApp(async ({ page, errors }) => {
			await open(page);
			const before = await still(page);
			assert.equal(before.scrolled, 0);
			assert.ok(before.room > 100, `the commands have ${before.room}px to scroll`);
			const flat = await row(page);
			assert.equal(flat.lines, 1, `${flat.circles} commands stand in ${flat.lines} rows`);
			assert.equal(flat.last, false, 'the last command is in sight before any scroll: the case is not tested');
			const box = await page.locator(LIST).boundingBox();
			const y = box.y + box.height / 2;

			// Real fingers leftwards, a frame a move: the row scrolls to its end, the bar, the log and the root tab do not move.
			for (let i = 0; i < 12 && !(await row(page)).last; i++) {
				await swipe(page, box.x + box.width - 20, y, -(box.width - 60), 0, 10, 16);
				const now = await still(page);
				assert.deepEqual([now.shown, now.log, now.tab], [before.shown, before.log, before.tab], 'a sideways swipe on the commands moved the bar or the tab');
			}
			const end = await still(page);
			assert.ok((await row(page)).last, `swipes left the commands at ${end.scrolled}px of ${end.room}px, the last out of sight`);

			// And back: the row scrolls back and the tab stays.
			await swipe(page, box.x + 20, y, box.width - 60, 0, 10, 16);
			const back = await still(page);
			assert.ok(back.scrolled < end.scrolled, `a swipe right left the commands at ${back.scrolled}px, from ${end.scrolled}px`);
			assert.equal(back.tab, before.tab, 'a swipe right on the commands swiped the root tab');

			// The bar's grip never sees an upward drag on them, even one the browser does not take.
			await dragOn(page, OVER, -200);
			const dragged = await still(page);
			assert.deepEqual([dragged.shown, dragged.log], [before.shown, before.log], 'a drag on the commands moved the bar');
			assert.equal(await page.locator(OVER).count(), 1);

			// The input row is the bar's, as ever.
			const clock = await page.locator(`${BAR} .stamp`).boundingBox();
			await swipe(page, clock.x + clock.width / 2, clock.y + clock.height / 2, 0, -200, 10);
			const moved = await still(page);
			assert.ok(moved.log > 0, `a swipe up on the input row left the log at ${moved.log}px`);
			assert.deepEqual(errors, []);
		}, PHONE);
	});

	test('a sideways wheel over them scrolls them and leaves the bar', async () => {
		await onPage(async ({ page, errors }) => {
			await page.setViewportSize({ width: 600, height: 800 });
			try {
				await open(page);
				const before = await still(page);
				assert.ok(before.room > 0, 'the commands fit across: the case is not tested');
				const box = await page.locator(OVER).boundingBox();
				await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
				await page.mouse.wheel(60, 0);
				const across = await still(page);
				assert.ok(across.scrolled > before.scrolled, `a wheel left the commands at ${across.scrolled}px`);
				assert.deepEqual([across.shown, across.log], [before.shown, before.log], 'a sideways wheel on the commands moved the bar');
				await page.mouse.wheel(0, 60);
				const down = await still(page);
				assert.deepEqual([down.shown, down.log], [before.shown, before.log], 'a wheel down on the commands moved the bar');
				// Clearing the last pill takes them away.
				await page.evaluate(() => window.datum.bar.setPills([]));
				await page.locator(OVER).waitFor({ state: 'detached' });
			} finally {
				await page.setViewportSize({ width: 1280, height: 800 });
			}
			assert.deepEqual(errors, []);
		});
	});
});
