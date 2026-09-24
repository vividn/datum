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

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { devices } from 'playwright';
import {
	atEachScreen,
	sharedApp,
	startApp,
	stopApp,
	withApp,
	workerClock,
	seedLines,
	todayAt
} from './support/app.mjs';
import { choose, chooseRoot } from './support/layout.mjs';

/** «Save options with layout» (da-vhwq): what hands a widget's own move to the layout. */
const SAVE_OPTIONS = 'Save options with layout';

before(startApp);
after(stopApp);

// ONE APP per set of options for most of this file (da-6b3dj, sharedApp in
// support/app.mjs). A test that only sizes the box, writes the tree (`say`)
// or logs rows is `solo`: the box is set before anything is measured, and
// `solo` restores `day` and empties the database. A test that moves the
// strip ITSELF — the handle, the keyboard, the freeze toggle, a zoom — sets
// a session option on the widget, which `solo` does not put back, so it
// takes a ROOM, whose strip is a new widget — as does a test that scrolls
// a stack the test before it scrolled, since `solo` keeps `day`'s strip and
// its scroll position. Which is why the helpers below read and write the
// strip of the root tab in FRONT. What still opens an
// app of its own has options no other test shares (a viewport, a clock), a
// `prepare`, a moved clock, or a reload.
const shared = sharedApp();
const clocked = sharedApp({ clock: todayAt(12) });

/**
 * A room on `on`, once its strip has heard its box. A room's strip is a new
 * widget, and it hears its box a settle after the room is drawn
 * (layout/resizing.ts) — for ~100 ms it holds a day height from before —
 * where a fresh boot has long since settled. Waited out, as the tests below
 * wait out a settle.
 */
const settledRoom = (on, name, fn) =>
	on.room(name, async (app) => {
		await app.page.waitForTimeout(3 * 150);
		await app.idle();
		return fn(app);
	});

/** datum's 3-line day label, as `format_day_label` builds one. */
const DAYS = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
const MONTHS = [
	'Jan',
	'Feb',
	'Mar',
	'Apr',
	'May',
	'Jun',
	'Jul',
	'Aug',
	'Sep',
	'Oct',
	'Nov',
	'Dec'
];

/**
 * The label a band `daysAgo` before `nowMs` carries.
 *
 * Read in UTC because the browser context is pinned to UTC
 * (support/app.mjs), and off the PAGE's clock rather than node's: the two
 * straddle a midnight for one instant a day, and an expectation built
 * from the wrong one is a test that fails at 00:00 and nowhere else.
 */
function labelFor(nowMs, daysAgo) {
	const day = new Date(nowMs);
	day.setUTCHours(0, 0, 0, 0);
	day.setUTCDate(day.getUTCDate() - daysAgo);
	return [
		DAYS[day.getUTCDay()],
		`${MONTHS[day.getUTCMonth()]} ${String(day.getUTCDate()).padStart(2, '0')}`,
		String(day.getUTCFullYear())
	];
}

/** The colour datum gives a name: the first three bytes of its md5. */
const colorOf = (name) => '#' + createHash('md5').update(name).digest('hex').slice(0, 6);

/**
 * One band's SVG, by how many days back it is: 0 is today's.
 */
async function band({ page }, daysAgo) {
	return page
		.locator(`.band[data-days-ago="${daysAgo}"] svg.svg`)
		.evaluate((svg) => svg.outerHTML);
}

/**
 * Every row's date stamp, top to bottom — the strip's own, in its own
 * column, rather than text inside the picture (da-iu9i).
 */
const stamps = ({ page }) =>
	page
		.locator('.strip .row')
		.evaluateAll((rows) =>
			rows.map((row) => [...row.querySelectorAll('.stamp span')].map((s) => s.textContent))
		);

/** Where every hourly gridline sits along a band's own x-axis. */
const gridlines = (svg) =>
	[...svg.matchAll(/<line class="gridline"[^>]*\sx1="([\d.]+)"/g)].map((m) => Number(m[1]));

/** How tall datum actually drew the picture, off the document it wrote. */
const svgHeight = (svg) => Number(svg.match(/<svg[^>]*\sheight="([\d.]+)"/)[1]);

/**
 * The strip's own arithmetic, read off the element it styles.
 *
 * One day's box, the pitch each further day adds, the ruler's row and the
 * divider are what the past COSTS in strip height, so the tests never
 * restate a constant that lives in $lib/panes.ts. `chrome` is what the
 * page's box spends around the strip — the pane's title bar, borders and
 * padding — which is what separates the height a test sets from the
 * height the strip gets.
 */
const metrics = ({ page }) =>
	page.locator('.strip').evaluate((el) => {
		const style = getComputedStyle(el);
		const px = (name) => parseFloat(style.getPropertyValue(name));
		const box = document.querySelector('.cell.dayview').getBoundingClientRect().height;
		return {
			band: px('--band'),
			pitch: px('--pitch'),
			today: px('--today'),
			label: px('--label'),
			axis: px('--axis'),
			divider: px('--divider'),
			strip: el.clientHeight,
			chrome: box - el.clientHeight
		};
	});

/** How tall `days` whole days of the past are: a pitch each, the divider included. */
const pastHeight = (m, days) => days * m.pitch;

/** The same arithmetic with every day drawn `by` pixels taller. */
const grown = (m, by) => ({
	...m,
	band: m.band + by,
	pitch: m.pitch + by,
	today: m.today + by
});

/**
 * Give the strip's box `px` of height, as its pane does — holding the day
 * height first, if the strip is still following its box (da-r0ydu).
 *
 * The box is the pane's cell in the layout tree (da-jdop): a flex item
 * whose size is its share of the split, so a height alone is ignored
 * and the basis is what has to be set.
 *
 * ⚑ THE PIN IS NOT SCAFFOLDING, IT IS THE OTHER HALF OF THE RULE. A strip
 * nobody has touched draws ONE DAY at whatever height its widget is, so a
 * taller box there is a taller DAY and never more days. Everything below
 * is about the second state — the reader has set a height, it HOLDS, and
 * the box then says how much past fits above it — so these tests enter it
 * the way a reader does. Pinning at the height the strip is already
 * drawing moves no pixel; it only stops the box from moving it. That the
 * two states are these two, and that the second is reached and left by
 * the handle alone, is asserted in «follows its widget until touched».
 */
async function setBox(app, px) {
	// NOTHING HEARD IS THE STRIP STILL FOLLOWING (da-r0ydu): the day height
	// is absent from the owner's settings exactly while it is derived, so
	// `undefined` here is the question «is this strip still following its
	// box?» asked of the one place that can answer it.
	if ((await heard(app)) === undefined) await say(app, { dayHeight: await valueOf(app) });
	await boxTo(app, px);
}

/** The box alone, with nothing held: what «follows its widget» is driven by. */
const boxTo = ({ page }, px) =>
	page.evaluate((h) => {
		document.querySelector('.cell.dayview').style.flex = `0 0 ${h}px`;
	}, px);

/**
 * The day height the box would choose for itself — `dayFilling`, read off
 * the element the strip styles rather than restated (`metrics`).
 *
 * ⚑ CLAMPED, because `dayFilling` is: without this the helper and the
 * strip agree only while the derived height is inside
 * [`MIN_DAY_HEIGHT`, `MAX_DAY_HEIGHT`], and past the ceiling `settledAt`
 * would wait on an `aria-valuenow` the strip will never print — a hang
 * rather than a failure.
 */
const following = async (app) => {
	const m = await metrics(app);
	return Math.min(MAX_DAY, Math.max(MIN_DAY, Math.round(m.strip - m.axis)));
};

/** The strip's own floor and ceiling (`panes.ts`), which `dayFilling` clamps to. */
const MIN_DAY = 40;
const MAX_DAY = 240;

/** Wait until the handle says `want` — the strip has heard its new box. */
const settledAt = ({ page }, want) =>
	page.waitForFunction(
		(want) => Number(document.querySelector('button.day-grip')?.getAttribute('aria-valuenow')) === want,
		want
	);

/** The box the cell has now, which a follow test moves from. */
const boxOf = ({ page }) =>
	page.locator('.cell.dayview').evaluate((el) => Math.round(el.getBoundingClientRect().height));

/**
 * How many days before today the box SHOWS at arithmetic `m`: the room
 * above the divider, in days, counting a partly revealed one.
 */
function owedAt(m) {
	const room = Math.max(0, m.strip - m.axis - m.divider - m.band);
	return room > 0 ? Math.ceil((room - m.band) / m.pitch) + 1 : 0;
}
const owed = async (app) => owedAt(await metrics(app));

/** The scrollback the stack holds below what the box shows — DayStrip's FORTNIGHT. */
const FORTNIGHT = 14;
/** How far back the strip reaches however tall its box — DayStrip's MAX_DEPTH. */
const DEEPEST = 60;

/**
 * How many days the stack HOLDS when the box shows `shown` of them.
 *
 * The two numbers are not one (da-bewc): a stack holding exactly what its
 * box shows overflows it by at most the one partly revealed day, which is
 * a few dozen pixels of scroll and then nothing. So there is always a
 * fortnight of past above the box to scroll back through, and a box that
 * shows more than a fortnight holds what it shows.
 */
const heldFor = (shown) => (shown > 0 ? Math.min(DEEPEST, Math.max(FORTNIGHT, shown)) : 0);
const heldAt = (m) => heldFor(owedAt(m));

/**
 * Which days of the past are ON SCREEN, by how many days back each is.
 *
 * A day is on screen when its whole date stamp is inside the stack's box
 * — the stamp is how a reader tells one day from another, so a day whose
 * stamp is cut is a day they cannot read.
 */
const onScreen = ({ page }) =>
	page.locator('.stack').evaluate((stack) => {
		const box = stack.getBoundingClientRect();
		const seen = [];
		for (const row of stack.querySelectorAll('.row')) {
			const mark = row.querySelector('.stamp').getBoundingClientRect();
			if (mark.top >= box.top - 1 && mark.bottom <= box.bottom + 1)
				seen.push(Number(row.querySelector('.band').dataset.daysAgo));
		}
		return seen.sort((a, b) => a - b);
	});

/**
 * Size the box so the strip shows exactly `days` days before today.
 *
 * Half a pitch short of `days` whole ones, so the top day is only partly
 * revealed: that is what makes the stack scroll, and a scrolling stack is
 * what the jagged line is about. Zero days is a box exactly today's band,
 * the divider and the ruler — a side bar's floor, not a top bar's: a top
 * bar's title lies over 8px of the content, and its floor keeps them off
 * today (Layout.svelte's `chromeOf`, da-13bc). So for zero days the
 * strip's bar goes on the left first.
 */
async function sizeFor(app, days) {
	if (days === 0) {
		await app.page.evaluate(() =>
			window.datum.layout.apply((tree) => {
				const copy = structuredClone(tree);
				const walk = (pane) => {
					if (pane.tabs.some((tab) => tab.content.kind === 'daystrip')) pane.bar = { ...pane.bar, edge: 'left' };
					for (const tab of pane.tabs) if ('children' in tab.content) tab.content.children.forEach(walk);
				};
				walk(copy.root);
				return copy;
			})
		);
		await app.page.locator('section.edge-left .strip').waitFor();
	}
	const m = await metrics(app);
	const floor = m.band + m.divider + m.axis;
	const strip = days === 0 ? floor : floor + pastHeight(m, days) - m.pitch / 2;
	await setBox(app, Math.ceil(strip + m.chrome));
	await waitForDays(app, heldFor(days));
}

/** Size the box so the room above the divider is exactly `days` whole days. */
async function sizeWhole(app, days) {
	const m = await metrics(app);
	await setBox(app, m.band + m.divider + m.axis + pastHeight(m, days) + m.chrome);
	await waitForDays(app, heldFor(days));
}

/**
 * Wait until the stack HOLDS exactly `depth` days before today, and every
 * band on screen has a picture. What the box SHOWS of them is `onScreen`
 * (da-bewc).
 */
const waitForDays = ({ page }, depth) =>
	page.waitForFunction((want) => {
		const past = [...document.querySelectorAll('.stack .band')];
		const all = [...document.querySelectorAll('.band')];
		return (
			past.length === want &&
			all.length === want + 1 &&
			all.every((b) => b.querySelector('svg.svg'))
		);
	}, depth);

/** What the `+` handle currently says the day height is. */
const valueOf = async ({ page }) =>
	Number(await page.locator('button.day-grip').getAttribute('aria-valuenow'));
/** And the width of the date column, its other axis. */
const widthOf = async ({ page }) =>
	Number(await page.locator('button.day-grip').getAttribute('data-label-width'));

/**
 * In the page: the settings the strip widget is DRAWN from — what the
 * strip's owner (the layout, da-jdop) last heard from it, with whatever
 * THIS SESSION holds over it. Since da-vhwq a move of the strip's own
 * lands in the session and reaches the tree only on «Save options with
 * layout», so the tree alone is no longer what the strip was told. The
 * strip of the root tab in FRONT, so a room reads its own (da-6b3dj).
 */
const stripSettings = () => {
	const walk = (pane) => {
		for (const tab of pane.tabs) {
			if ('children' in tab.content) {
				const hit = tab.content.children.map(walk).find(Boolean);
				if (hit) return hit;
			} else if (tab.content.kind === 'daystrip') return window.datum.layout.settings(tab.id);
		}
	};
	const { root } = window.datum.layout.current();
	return walk({ tabs: [root.tabs[root.active]] });
};

const heard = ({ page }, key = 'dayHeight') =>
	page.evaluate(`(${stripSettings})().${key}`);

/** Wait until the tree has heard `want` for `key`. */
const heardIs = ({ page }, want, key = 'dayHeight') =>
	page.waitForFunction(`(${stripSettings})().${key} === ${JSON.stringify(want)}`);

/** Tell the strip's owner — the root tab in front's — some settings, as a saved layout would. */
const say = ({ page }, settings) =>
	page.evaluate(
		`window.datum.layout.apply((tree) => {
			const copy = structuredClone(tree);
			const walk = (pane) => {
				for (const tab of pane.tabs) {
					if ('children' in tab.content) tab.content.children.forEach(walk);
					else if (tab.content.kind === 'daystrip') tab.content.settings = { ...tab.content.settings, ...${JSON.stringify(settings)} };
				}
			};
			walk({ tabs: [copy.root.tabs[copy.root.active]] });
			return copy;
		})`
	);

/** Wait until every band has been re-drawn at the height it is due. */
const waitForHeights = ({ page }, height) =>
	page.waitForFunction((want) => {
		const svgs = [...document.querySelectorAll('.band svg.svg')];
		return svgs.length > 0 && svgs.every((svg) => svg.getAttribute('height') === String(want));
	}, height);

/** Drag a handle by (dx, dy) pixels, as a pointer does: down, move, up. */
async function dragBy({ page }, selector, dx, dy) {
	const box = await page.locator(selector).boundingBox();
	const x = box.x + box.width / 2;
	const y = box.y + box.height / 2;
	await page.mouse.move(x, y);
	await page.mouse.down();
	await page.mouse.move(x + dx, y + dy, { steps: 8 });
	await page.mouse.up();
}

/**
 * Turn a real wheel over the past, as a reader does: the pointer in the
 * middle of the stack, `by` pixels of deltaY — negative is back in time,
 * since the past is above.
 *
 * Not `el.scrollTop = ...`, which is what every other scroll assertion in
 * this file does and which passed throughout da-bewc, while nothing a
 * hand could do moved the strip at all.
 */
async function wheelOverPast({ page }, by) {
	const box = await page.locator('.stack').boundingBox();
	await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
	await page.mouse.wheel(0, by);
}

/**
 * Drag a finger down the stack, which is back in time for the same
 * reason.
 *
 * Playwright's touchscreen only taps, so the sequence goes over CDP —
 * where a real touch comes from, and the only way to tell a finger that
 * scrolls from a finger a `touch-action` or a pointer capture has eaten.
 */
async function dragOverPast({ page }, by) {
	const box = await page.locator('.stack').boundingBox();
	const x = box.x + box.width / 2;
	const y = box.y + box.height / 2;
	const cdp = await page.context().newCDPSession(page);
	const at = (type, py) =>
		cdp.send('Input.dispatchTouchEvent', {
			type,
			touchPoints: type === 'touchEnd' ? [] : [{ x, y: py }]
		});
	try {
		await at('touchStart', y);
		for (let i = 1; i <= 10; i++) await at('touchMove', y + (by * i) / 10);
		await at('touchEnd', y + by);
	} finally {
		await cdp.detach();
	}
}

/**
 * Put the day `daysAgo` before today at the top of the stack's box, where
 * a flick leaves it: one scroll, so what comes into view comes into view
 * at once rather than every day between here and there passing through.
 */
const scrollToDay = ({ page }, daysAgo) =>
	page.locator('.stack').evaluate((stack, back) => {
		stack.scrollTop = stack.querySelector(`.band[data-days-ago="${back}"]`).closest('.row')
			.offsetTop;
	}, daysAgo);

/**
 * Wait for the stack to have left `top` and for a scroll event past the
 * `seen`-th to have reached the strip, then give it the frame it answers
 * a scroll on. Waited for, not counted in frames: under a pinned clock
 * `requestAnimationFrame` is Playwright's fake, and two of them passed
 * before the wheel had landed about one run in nine (da-6ehq). The strip's
 * own frame is asked for inside that scroll event, so it runs before ours.
 */
async function scrolledFrom({ page }, top, seen) {
	await page.waitForFunction(
		([top, seen]) => document.querySelector('.stack').scrollTop !== top && window.__scrolls > seen,
		[top, seen]
	);
	return page.evaluate(async () => {
		await new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done)));
		return window.__scrolls;
	});
}

/**
 * Which days the `dayview` PICTURES a spy saw were for, as days ago — the
 * geometry's `--show json` is not a picture, and today carries no
 * `--end-date` because it is the argv every other band is an offset from.
 */
const viewsOf = async (seen) =>
	(await seen())
		.filter((argv) => argv[0] === 'dayview' && !argv.includes('json'))
		.map((argv) => (argv.includes('--end-date') ? -Number(argv[argv.indexOf('--end-date') + 1]) : 0))
		.sort((a, b) => a - b);

/** The box today's band occupies on the screen — it is not to move. */
const todayBand = ({ page }) => page.locator('.band[data-days-ago="0"]').boundingBox();

/** The stack's scroll box, as the strip reads it. */
const stackScroll = ({ page }) =>
	page.locator('.stack').evaluate((el) => ({
		top: el.scrollTop,
		client: el.clientHeight,
		total: el.scrollHeight
	}));

/** Whether the jagged line is on screen. */
const jagged = ({ page }) => page.locator('button.jagged').count();

/**
 * Every key in the page's store, so a write anywhere in it shows — save
 * the layout's own draft, which is the tree remembering what the strip
 * told it (da-wpgy), and not the strip writing.
 */
const storeKeys = ({ page }) =>
	page.evaluate(() =>
		Object.keys(localStorage)
			.filter((key) => !key.startsWith('datum.layout.'))
			.sort()
	);

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

			// A box exactly today's band, divider and ruler holds today alone.
			await sizeFor(app, 0);
			assert.equal(await app.page.locator('.stack').count(), 0);
		});
	});

	// The bead's own case (da-iu9i): five days of the reader's data, and
	// five separate `dayview` runs of ONE day each — the snappiness is
	// that a band lands on its own, not that the past arrives at once.
	test('five days is five one-day dayviews, each asked bands-only', async () => {
		await withApp( // own app: its own screen and clock, which no other test shares
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
					assert.equal(flag(argv, '--n-days'), '1', `not one day: ${argv.join(' ')}`);
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
			},
			{ viewport: { width: 1280, height: 1000 }, clock: todayAt(12) }
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
			assert.deepEqual([...named].sort(), [...named], 'the ruler runs backwards');

			// The stitching. The strip's own gridlines sit at the x the CLI
			// reports for them, and land on each band's own 12:00 line.
			const geometry = await app.page.evaluate(async () => {
				const grip = document.querySelector('button.day-grip');
				const width = Math.round(document.querySelector('.band').getBoundingClientRect().width);
				const result = await window.datum.runQuiet([
					'dayview', '--width', String(width),
					'--day-height', grip.getAttribute('aria-valuenow'),
					'--margin', '0', '--label-width', '0', '--time-axis-height', '0',
					'--n-days', '1', '--show', 'json'
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
				// A day SHORT enough that the phone's strip holds a past at
				// all (da-r0ydu): the day follows the widget until the reader
				// shrinks it, so a strip nobody has touched shows exactly one
				// day and has nothing above today to scroll to. Reaching that
				// state is the handle's own test; what this one is about is
				// the finger, so the height is set the way a saved layout
				// would set it.
				await say(app, { dayHeight: 40 });
				await settledAt(app, 40);
				// And ANCHORED before the scroll is read: a fresh day height
				// re-draws every band and `anchor` puts the newest back
				// against the divider a beat later, so a `scrollTop` read
				// before that is 0 — and `<= 0 - 200` is a wait that cannot
				// end.
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
				const today = await todayBand(app);

				// A finger pulled DOWN the stack is back in time.
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
				assert.deepEqual(await todayBand(app), today, "today's band moved");
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
		await withApp( // own app: its own screen and clock, which no other test shares
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
					.map((argv) => [flag(argv, '--n-days'), flag(argv, '--end-date')]);
				assert.deepEqual(views, [['1', '-16']]);
			},
			{ viewport: { width: 1280, height: 1400 }, clock: todayAt(12) }
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
			await setBox(app, m.band + m.divider + m.axis + m.pitch - 2 + m.chrome);
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
		// What the default is is the strip's business, not this file's, so
		// it is read off an ordinary app rather than restated here. Nothing
		// is HELD on a fresh strip (da-r0ydu): the day follows its widget
		// until the handle is touched, so the owner's settings are silent
		// about it and the handle still says a number.
		let fallback;
		await shared.solo(async (app) => {
			fallback = await valueOf(app);
			assert.ok(fallback > 0, `the handle says ${fallback}`);
			assert.equal(await heard(app), undefined);
		});

		await settledRoom(shared, 'prop', async (app) => {
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
		await withApp(async (app) => {
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
		// 10:00 UTC, so the fast-forward below lands after midnight and
		// inside the next day rather than on its boundary.
		const noon = Date.UTC(2026, 8, 9, 10, 0, 0);
		// The CLI draws the pictures, in the database worker: its clock too.
		const clock = workerClock(noon);
		/** The days `from` back to 0 before `ms`, oldest first, as stamps. */
		const marks = (ms, from) => Array.from({ length: from + 1 }, (_, i) => labelFor(ms, from - i));
		await withApp( // own app: it moves the clock
			async (app) => {
				// Deep enough that the stack scrolls, so the jagged line has
				// something to say at the end.
				await sizeFor(app, 5);
				assert.deepEqual(await stamps(app), marks(noon, heldFor(5)));

				// Over the boundary. The strip redraws on the frame's epoch,
				// which is the same beat today's band has always refreshed
				// on — so a command is what makes it look at the clock again.
				const after = noon + 16 * 60 * 60 * 1000;
				await app.page.clock.setFixedTime(new Date(after));
				await clock.set(app.page, after);
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
			},
			{
				prepare: async (context) => {
					await context.clock.setFixedTime(new Date(noon));
					await clock.prepare(context);
				}
			}
		);
	});

	// The strip is full-width, and the fix for horizontal scrollbars over
	// it landed the day before this did (1ceada8) — so every depth the
	// box can hold is swept, at every width the app is read at.
	test('never scrolls the page sideways, at any width or depth', async (t) => {
		await withApp( // own app: its own screen, which no other test shares
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
			},
			{ viewport: { width: 360, height: 800 } }
		);
	});

	// The strip's row is a fixed height now, so it cannot push the thing a
	// phone user logs with off the bottom of the screen (da-hkpd) — as
	// long as that height is one a phone can spare.
	test("the strip's row leaves the widget bar on screen", async () => {
		const screen = { width: 360, height: 640 };
		await withApp( // own app: its own screen, which no other test shares
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
			},
			{ viewport: screen }
		);
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
	try {
		for (const _ of [1, 2]) {
			await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [point] });
			await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
		}
	} finally {
		await cdp.detach();
	}
}

const PHONE = { device: devices['Pixel 5'], viewport: { width: 390, height: 640 } };
const phone = sharedApp(PHONE);

/** The zoom reset in the strip's corner (da-8rqb9). */
const UNZOOM = '.strip button.unzoom';

/** Today's band — the one at the bottom, whose picture is the day so far. */
const TODAY = '.band[data-days-ago="0"]';

describe('the day strip zooms onto an hour range', () => {
	test('a pinch windows every band at once, and the stamps stay where they are', async () => {
		await withApp(async (app) => { // own app: a phone with a clock, which no other test shares
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
		}, { ...PHONE, clock: todayAt(12) });
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
		await withApp(async (app) => { // own app: it reloads
			const { page } = app;
			await seedLines(
				app,
				['mood', 'work', 'place', 'posture'].flatMap((field, lane) =>
					[0, 6, 12, 18].map(
						(hour) => `switch ${field} s${lane}${hour} -t ${String(hour).padStart(2, '0')}:00`
					)
				)
			);
			await app.reload();
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
		}, { ...PHONE, clock: todayAt(23) });
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
			// inside it is (Beta.svelte's pattern).
			assert.ok(seen.width >= 24 && seen.height >= 24, `the ⊖ is ${seen.width}×${seen.height}`);
			assert.ok(seen.glyph < seen.width, 'the glyph is not smaller than its hit area');
			assert.equal(seen.fromBottom, 0, 'the ⊖ is not in the strip’s bottom corner');
			assert.ok(seen.inside, 'the ⊖ hangs off the strip');
			assert.ok(seen.clearOfThePictures, 'the ⊖ is over a picture');

			// And clear of the control that shares that column: the freeze
			// toggle, which drops to this same seam when today is not frozen.
			const boxOf = (sel) => page.locator(sel).evaluate((el) => el.getBoundingClientRect().toJSON());
			const [reset, freeze] = [await boxOf(UNZOOM), await boxOf('.strip button.freeze')];
			assert.ok(
				reset.left >= freeze.right - 0.5 || reset.top >= freeze.bottom - 0.5,
				`the ⊖ (${reset.left}–${reset.right}) sits on the freeze toggle (${freeze.left}–${freeze.right})`
			);
			assert.deepEqual(app.errors, []);
		});
	});

	test('ctrl+wheel zooms, shift+wheel pans, and the ⊖ is the way back', async () => {
		await withApp( // own app: its own screen, which no other test shares
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
			},
			{ viewport: { width: 1280, height: 900 } }
		);
	});

	test('the window is a setting: this session’s, saved with the layout when asked, reset with the widget', async () => {
		await withApp( // own app: it saves `day` and reloads
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
			},
			{ viewport: { width: 1280, height: 900 } }
		);
	});
});
