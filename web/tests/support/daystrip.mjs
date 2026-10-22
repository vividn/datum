// What the day strip's tests read it with (`daystrip.test.mjs`,
// `daystrip-height.test.mjs`): its arithmetic off the element it styles, the
// settings its owner heard, and a wheel, a finger and a drag as a reader
// makes them. No app is opened here: each file opens its own.

import { createHash } from 'node:crypto';
import { devices } from 'playwright';
import { swipeNoWait } from './press.mjs';
import { optionChecked, toggleOption } from './layout.mjs';

/** «Save options with layout» (da-vhwq): what hands a widget's own move to the layout. */
export const SAVE_OPTIONS = 'Save options with layout';

export const NOWVIEW = 'section.pane[data-title="nowview"]';
export const NARROW = { viewport: { width: 360, height: 640 } };

/**
 * A room on `on`, once its strip has heard its box. A room's strip is a new
 * widget, and it hears its box a settle after the room is drawn
 * (layout/resizing.ts) — for ~100 ms it holds a day height from before —
 * where a fresh boot has long since settled. Waited FOR, not out: a room's
 * strip holds nothing, so it has heard its box once its day is the one
 * that box derives — `dayFilling` of the floored content box its
 * ResizeObserver reports (DayStrip.svelte), read off the pane's body — less
 * the sliver of yesterday, `dayUnderSliver` (da-2hd21).
 */
export const settledRoom = (on, name, fn) =>
	on.room(name, async (app) => {
		await app.page.waitForFunction(([min, max, sliver]) => {
			const strip = document.querySelector('.strip');
			const body = strip?.closest('.body');
			const grip = strip?.querySelector('.today-line');
			if (!body || !grip) return false;
			const s = getComputedStyle(body);
			const px = (name) => parseFloat(s.getPropertyValue(name));
			const content =
				body.getBoundingClientRect().height -
				px('padding-top') -
				px('padding-bottom') -
				px('border-top-width') -
				px('border-bottom-width');
			const css = getComputedStyle(strip);
			const axis = parseFloat(css.getPropertyValue('--axis'));
			const seam = parseFloat(css.getPropertyValue('--seam-line'));
			const day = (Math.floor(content) - axis - seam) / (1 + sliver);
			const derived = Math.min(max, Math.max(min, Math.round(day)));
			return Number(grip.getAttribute('aria-valuenow')) === derived;
		}, [MIN_DAY, MAX_DAY, SLIVER]);
		await app.idle();
		return fn(app);
	});

/** datum's 3-line day label, as `format_day_label` builds one. */
export const DAYS = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
export const MONTHS = [
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
export function labelFor(nowMs, daysAgo) {
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
export const colorOf = (name) => '#' + createHash('md5').update(name).digest('hex').slice(0, 6);

/**
 * One band's SVG, by how many days back it is: 0 is today's.
 */
export async function band({ page }, daysAgo) {
	return page
		.locator(`.band[data-days-ago="${daysAgo}"] svg.svg`)
		.evaluate((svg) => svg.outerHTML);
}

/**
 * Every row's date stamp, top to bottom — the strip's own, in its own
 * column, rather than text inside the picture (da-iu9i).
 */
export const stamps = ({ page }) =>
	page
		.locator('.strip .row')
		.evaluateAll((rows) =>
			rows.map((row) => [...row.querySelectorAll('.stamp span')].map((s) => s.textContent))
		);

/** Where every hourly gridline sits along a band's own x-axis. */
export const gridlines = (svg) =>
	[...svg.matchAll(/<line class="gridline"[^>]*\sx1="([\d.]+)"/g)].map((m) => Number(m[1]));

/** How tall datum actually drew the picture, off the document it wrote. */
export const svgHeight = (svg) => Number(svg.match(/<svg[^>]*\sheight="([\d.]+)"/)[1]);

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
export const metrics = ({ page }) =>
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
			seam: px('--seam-line'),
			strip: el.clientHeight,
			chrome: box - el.clientHeight
		};
	});

/** How tall `days` whole days of the past are: a pitch each, the divider included. */
export const pastHeight = (m, days) => days * m.pitch;

/** The same arithmetic with every day drawn `by` pixels taller. */
export const grown = (m, by) => ({
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
export async function setBox(app, px) {
	// NOTHING HEARD IS THE STRIP STILL FOLLOWING (da-r0ydu): the day height
	// is absent from the owner's settings exactly while it is derived, so
	// `undefined` here is the question «is this strip still following its
	// box?» asked of the one place that can answer it.
	if ((await heard(app)) === undefined) await say(app, { dayHeight: await valueOf(app) });
	await boxTo(app, px);
}

/**
 * The box alone, with nothing held: what «follows its widget» is driven by.
 *
 * Returns once the strip has HEARD the box, a settle after the cell has it
 * (layout/resizing.ts). What the strip draws from its height moves only
 * then — the divider among it, 93px on a box grown from shorter than a day —
 * so a press aimed at the line before it lands on the band under where the
 * line was, and a drag reads as nothing on a loaded runner (da-s6395).
 */
export async function boxTo({ page }, px) {
	await page.evaluate((h) => {
		document.querySelector('.cell.dayview').style.flex = `0 0 ${h}px`;
	}, px);
	await page.waitForFunction(() => {
		const strip = document.querySelector('.strip');
		return Number(strip.dataset.height) === Math.floor(strip.getBoundingClientRect().height);
	});
}

/**
 * The day height the box would choose for itself — `dayUnderSliver`
 * (da-2hd21), read off the element the strip styles rather than restated
 * (`metrics`).
 *
 * ⚑ CLAMPED, because `dayFilling` is: without this the helper and the
 * strip agree only while the derived height is inside
 * [`MIN_DAY_HEIGHT`, `MAX_DAY_HEIGHT`], and past the ceiling `settledAt`
 * would wait on an `aria-valuenow` the strip will never print — a hang
 * rather than a failure.
 */
export const following = async (app) => {
	const m = await metrics(app);
	const day = (m.strip - m.axis - m.seam) / (1 + SLIVER);
	return Math.min(MAX_DAY, Math.max(MIN_DAY, Math.round(day)));
};

/** «today frozen at the bottom»: the strip's freeze, a widget option (da-2hd21). */
export const FREEZE = 'today frozen at the bottom';

/** The dayview pane's id, in `main` or in a room. */
const stripPane = ({ page }) => page.locator('section.pane[data-title="dayview"]').first().getAttribute('data-pane');

/** Whether today is frozen, read off the option's switch. */
export const frozen = async (app) => optionChecked(app.page, await stripPane(app), FREEZE);

/** Turn the freeze over, as a reader does: the switch in the dayview's options. */
export const toggleFreeze = async (app) => toggleOption(app.page, await stripPane(app), FREEZE);

/** The strip's own floor and ceiling (`panes.ts`), which `dayFilling` clamps to. */
export const MIN_DAY = 40;
export const MAX_DAY = 240;
/** The part of a day a pinned today leaves of yesterday (`SLIVER`, panes.ts). */
export const SLIVER = 1 / 4;

/** Wait until the handle says `want` — the strip has heard its new box. */
export const settledAt = ({ page }, want) =>
	page.waitForFunction(
		(want) => Number(document.querySelector('.today-line')?.getAttribute('aria-valuenow')) === want,
		want
	);

/** The box the cell has now, which a follow test moves from. */
export const boxOf = ({ page }) =>
	page.locator('.cell.dayview').evaluate((el) => Math.round(el.getBoundingClientRect().height));

/**
 * How many days before today the box SHOWS at arithmetic `m`: the room
 * above the divider, in days, counting a partly revealed one.
 */
export function owedAt(m) {
	const room = Math.max(0, m.strip - m.axis - m.seam - m.band);
	return room > 0 ? Math.ceil((room - m.band) / m.pitch) + 1 : 0;
}
export const owed = async (app) => owedAt(await metrics(app));

/** The scrollback the stack holds below what the box shows — DayStrip's FORTNIGHT. */
export const FORTNIGHT = 14;

/**
 * How many days the stack HOLDS when the box shows `shown` of them.
 *
 * The two numbers are not one (da-bewc): a stack holding exactly what its
 * box shows overflows it by at most the one partly revealed day, which is
 * a few dozen pixels of scroll and then nothing. So there is always a
 * fortnight of past above the box to scroll back through — a box that
 * shows none of it too (da-2hd21) — and a box that shows more than a
 * fortnight holds what it shows.
 */
export const heldFor = (shown) => Math.max(FORTNIGHT, shown);
export const heldAt = (m) => heldFor(owedAt(m));

/**
 * Which days of the past are ON SCREEN, by how many days back each is.
 *
 * A day is on screen when its whole date stamp is inside the stack's box
 * — the stamp is how a reader tells one day from another, so a day whose
 * stamp is cut is a day they cannot read.
 */
export const onScreen = ({ page }) =>
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
export async function sizeFor(app, days) {
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
	const floor = m.band + m.seam + m.axis;
	const strip = days === 0 ? floor : floor + pastHeight(m, days) - m.pitch / 2;
	await setBox(app, Math.ceil(strip + m.chrome));
	await waitForDays(app, heldFor(days));
}

/** Size the box so the room above the divider is exactly `days` whole days. */
export async function sizeWhole(app, days) {
	const m = await metrics(app);
	await setBox(app, m.band + m.seam + m.axis + pastHeight(m, days) + m.chrome);
	await waitForDays(app, heldFor(days));
}

/**
 * Wait until the stack HOLDS exactly `depth` days before today, and every
 * band on screen has a picture. What the box SHOWS of them is `onScreen`
 * (da-bewc).
 */
export const waitForDays = ({ page }, depth) =>
	page.waitForFunction((want) => {
		// The past alone: unfrozen, today is in the stack too.
		const past = [...document.querySelectorAll('.stack .band:not([data-days-ago="0"])')];
		const all = [...document.querySelectorAll('.band')];
		return (
			past.length === want &&
			all.length === want + 1 &&
			all.every((b) => b.querySelector('svg.svg'))
		);
	}, depth);

/** What the day line's slider currently says the day height is. */
export const valueOf = async ({ page }) =>
	Number(await page.locator('.today-line').getAttribute('aria-valuenow'));
/** And the width of the date column, the spine's slider. */
export const widthOf = async ({ page }) =>
	Number(await page.locator('.strip .spine').getAttribute('aria-valuenow'));

/**
 * In the page: the settings the strip widget is DRAWN from — what the
 * strip's owner (the layout, da-jdop) last heard from it, with whatever
 * THIS SESSION holds over it. Since da-vhwq a move of the strip's own
 * lands in the session and reaches the tree only on «Save options with
 * layout», so the tree alone is no longer what the strip was told. The
 * strip of the root tab in FRONT, so a room reads its own (da-6b3dj).
 */
export const stripSettings = () => {
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

export const heard = ({ page }, key = 'dayHeight') =>
	page.evaluate(`(${stripSettings})().${key}`);

/** Wait until the tree has heard `want` for `key`. */
export const heardIs = ({ page }, want, key = 'dayHeight') =>
	page.waitForFunction(`(${stripSettings})().${key} === ${JSON.stringify(want)}`);

/** Tell the strip's owner — the root tab in front's — some settings, as a saved layout would. */
export const say = ({ page }, settings) =>
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
export const waitForHeights = ({ page }, height) =>
	page.waitForFunction((want) => {
		const svgs = [...document.querySelectorAll('.band svg.svg')];
		return svgs.length > 0 && svgs.every((svg) => svg.getAttribute('height') === String(want));
	}, height);

/** Drag a handle by (dx, dy) pixels, as a pointer does: down, move, up. */
export async function dragBy({ page }, selector, dx, dy) {
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
export async function wheelOverPast({ page }, by) {
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
 *
 * The press arms no long-press wait (`swipeNoWait`, da-o504v): a first move
 * the runner sent after the 400ms found a hold, whose moves the strip keeps,
 * and the stack did not scroll. A press on a band arms that one timer.
 */
export async function dragOverPast({ page }, by) {
	const box = await page.locator('.stack').boundingBox();
	await swipeNoWait(page, box.x + box.width / 2, box.y + box.height / 2, 0, by, 10);
}

/**
 * Put the day `daysAgo` before today at the top of the stack's box, where
 * a flick leaves it: one scroll, so what comes into view comes into view
 * at once rather than every day between here and there passing through.
 */
export const scrollToDay = ({ page }, daysAgo) =>
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
export async function scrolledFrom({ page }, top, seen) {
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
export const viewsOf = async (seen) =>
	(await seen())
		.filter((argv) => argv[0] === 'dayview' && !argv.includes('json'))
		.map((argv) => (argv.includes('--end-date') ? -Number(argv[argv.indexOf('--end-date') + 1]) : 0))
		.sort((a, b) => a - b);

/** The box today's band occupies on the screen — it is not to move. */
export const todayBand = ({ page }) => page.locator('.band[data-days-ago="0"]').boundingBox();

/** The stack's scroll box, as the strip reads it. */
export const stackScroll = ({ page }) =>
	page.locator('.stack').evaluate((el) => ({
		top: el.scrollTop,
		client: el.clientHeight,
		total: el.scrollHeight
	}));

/** Whether the jagged line is on screen. */
export const jagged = ({ page }) => page.locator('button.jagged').count();

/**
 * Every key in the page's store, so a write anywhere in it shows — save
 * the layout's own draft, which is the tree remembering what the strip
 * told it (da-wpgy), and not the strip writing.
 */
export const storeKeys = ({ page }) =>
	page.evaluate(() =>
		Object.keys(localStorage)
			.filter((key) => !key.startsWith('datum.layout.'))
			.sort()
	);

/** A phone as it comes, which the finger and the pinch are read on. */
export const PHONE = { device: devices['Pixel 5'], viewport: { width: 390, height: 640 } };

/**
 * Two fingers over `box`'s middle, `from` px apart, drawn to `to` px apart:
 * side by side, or one over the other with `axis` 'y'.
 *
 * Through CDP, as every other touch in this suite: Playwright's
 * touchscreen only taps, and a pinch made of mouse events would be a test
 * of the wrong handler. `midway` is called with the fingers still down —
 * the gesture's preview is only on screen until they lift.
 */
export async function pinch(page, box, from, to, midway, axis = 'x') {
	const y = box.y + box.height / 2;
	const x = box.x + box.width / 2;
	const cdp = await page.context().newCDPSession(page);
	const both = (apart) =>
		axis === 'x'
			? [
					{ id: 1, x: x - apart / 2, y },
					{ id: 2, x: x + apart / 2, y }
				]
			: [
					{ id: 1, x, y: y - apart / 2 },
					{ id: 2, x, y: y + apart / 2 }
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
