// The day strip: today pinned at the bottom, the past stacked above it.
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
// Since da-hnyb the past is ONE picture of many days rather than a band
// per day — so its gridlines run through and its ground is one — up to a
// fortnight to a picture. So a day is a date stamp inside a picture, and
// the pictures are counted, and measured, by what they hold.
//
// The jagged line is the third thing: it is a claim about what is
// currently ON SCREEN — the day above today is not the day before it —
// so it is asserted against the stack's scroll position, driven both ways.
//
// Since da-zb0d the strip has no handle for its own height: its BOX says
// how tall it is, and how tall it is says how much past it SHOWS — which
// since da-bewc is not how much it HOLDS. The stack keeps a fortnight of
// past above the box so that there is somewhere to scroll to; `heldFor`
// is that arithmetic, `onScreen` is what the box shows of it, and the
// scrolling itself is driven by a real wheel and a real finger. The
// one thing a reader sets on the strip itself is how tall a day is drawn
// — the notch in the left margin — and that is a prop in and a `change`
// event out, which is asserted from the outside: what the strip's owner
// heard, and the picture's own SVG `height`, not a class name that claims
// to mean it. The strip remembers nothing, and that is asserted too.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { devices } from 'playwright';
import { startApp, stopApp, withApp, workerClock } from './support/app.mjs';

before(startApp);
after(stopApp);

/** datum's 3-line day label, as $lib/runners/views.ts builds one. */
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
 * One picture's SVG, by how many days back its newest day is: 0 is
 * today's, 1 the past's that ends yesterday.
 */
async function band({ page }, daysAgo) {
	return page
		.locator(`.band[data-days-ago="${daysAgo}"] svg.svg`)
		.evaluate((svg) => svg.outerHTML);
}

/** Every line of a picture's date stamps, top to bottom. */
const dateLines = (svg) =>
	[...svg.matchAll(/<text class="date-label"[^>]*>([^<]*)</g)].map((m) => m[1]);

/** A picture's days, top to bottom, each as the three lines of its date stamp. */
const days = (svg) => {
	const lines = dateLines(svg);
	return Array.from({ length: lines.length / 3 }, (_, i) => lines.slice(3 * i, 3 * i + 3));
};

/** Where every hourly gridline sits along the picture's x-axis. */
const gridlines = (svg) =>
	[...svg.matchAll(/<line class="gridline"[^>]*\sx1="([\d.]+)"/g)].map((m) => Number(m[1]));

/** How tall datum actually drew the picture, off the document it wrote. */
const svgHeight = (svg) => Number(svg.match(/<svg[^>]*\sheight="([\d.]+)"/)[1]);

/**
 * The strip's own arithmetic, read off the element it styles.
 *
 * One day's box, and the pitch each further day adds, are what the past
 * COSTS in strip height, so the tests never restate a constant that lives
 * in $lib/panes.ts. `chrome` is what the page's box spends around the
 * strip — the pane's title bar, borders and padding — which is what
 * separates the height a test sets from the height the strip gets.
 */
const metrics = ({ page }) =>
	page.locator('.strip').evaluate((el) => {
		const style = getComputedStyle(el);
		const px = (name) => parseFloat(style.getPropertyValue(name));
		const box = document.querySelector('.cell.dayview').getBoundingClientRect().height;
		return {
			band: px('--band'),
			unit: px('--unit'),
			today: px('--today'),
			gap: px('--gap'),
			strip: el.clientHeight,
			chrome: box - el.clientHeight
		};
	});

/** How tall `days` whole days of the past are: the first one's box and a pitch per day more. */
const pastHeight = (m, days) => m.band + (days - 1) * m.unit;

/** The same arithmetic with every day drawn `by` pixels taller. */
const grown = (m, by) => ({ ...m, band: m.band + by, unit: m.unit + by, today: m.today + by });

/**
 * Give the strip's box `px` of height, as its pane does.
 *
 * The box is the pane's cell in the layout tree (da-jdop): a flex item
 * whose size is its share of the split, so a height alone is ignored
 * and the basis is what has to be set.
 */
const setBox = ({ page }, px) =>
	page.evaluate((h) => {
		document.querySelector('.cell.dayview').style.flex = `0 0 ${h}px`;
	}, px);

/**
 * How many days before today the box SHOWS at arithmetic `m`: the room
 * above today's band, in days, counting a partly revealed one.
 */
function owedAt(m) {
	const room = Math.max(0, m.strip - m.today - m.gap);
	return room > 0 ? Math.ceil((room - m.band) / m.unit) + 1 : 0;
}
const owed = async (app) => owedAt(await metrics(app));

/** The scrollback the stack holds below what the box shows — DayStrip's CHUNK. */
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
 * stamp is cut is a day they cannot read. Read off the pictures rather
 * than off the depth, because since da-bewc the stack holds days the box
 * does not show, and what the box shows is the claim being made.
 */
const onScreen = ({ page }) =>
	page.locator('.stack').evaluate((stack) => {
		const box = stack.getBoundingClientRect();
		const seen = [];
		for (const band of stack.querySelectorAll('.band.past')) {
			const end = Number(band.dataset.daysAgo);
			const n = Number(band.dataset.days);
			const labels = [...band.querySelectorAll('svg.svg text.date-label')];
			for (let i = 0; i < n; i++) {
				const first = labels[3 * i]?.getBoundingClientRect();
				const last = labels[3 * i + 2]?.getBoundingClientRect();
				if (first && last && first.top >= box.top - 1 && last.bottom <= box.bottom + 1)
					seen.push(end + n - 1 - i);
			}
		}
		return seen.sort((a, b) => a - b);
	});

/**
 * Size the box so the strip shows exactly `days` days before today.
 *
 * Half a pitch short of `days` whole ones, so the top day is only partly
 * revealed: that is what makes the stack scroll, and a scrolling stack is
 * what the jagged line is about. Zero days is a box exactly today's
 * height — which is a side bar's floor, not a top bar's: a top bar's
 * title lies over 8px of the content, and its floor keeps them off
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
	const strip = days === 0 ? m.today : m.today + m.gap + pastHeight(m, days) - m.unit / 2;
	await setBox(app, Math.ceil(strip + m.chrome));
	await waitForDays(app, heldFor(days));
}

/** Size the box so the room above today is exactly `days` whole days. */
async function sizeWhole(app, days) {
	const m = await metrics(app);
	await setBox(app, m.today + m.gap + pastHeight(m, days) + m.chrome);
	await waitForDays(app, heldFor(days));
}

/**
 * Wait until the stack HOLDS exactly `depth` days before today, and every
 * picture on it is drawn with as many days as its box claims — no day
 * more, and none still the depth it was before. What the box SHOWS of
 * them is `onScreen` (da-bewc).
 */
const waitForDays = ({ page }, depth) =>
	page.waitForFunction((want) => {
		const bands = [...document.querySelectorAll('.band')];
		const drawn = (b) =>
			b.querySelectorAll('svg.svg text.date-label').length === 3 * Number(b.dataset.days ?? 1);
		const past = bands.filter((b) => b.matches('.past'));
		return (
			bands.every(drawn) &&
			bands.at(-1).dataset.daysAgo === '0' &&
			past.reduce((n, b) => n + Number(b.dataset.days), 0) === want
		);
	}, depth);

/** What the day-height handle currently says it is set to. */
const valueOf = async ({ page }) =>
	Number(await page.locator('button.day-grip').getAttribute('aria-valuenow'));

/**
 * In the page: the strip widget's settings in the live tree — what the
 * strip's owner, the layout (da-jdop), last heard from it.
 */
const stripSettings = () => {
	const walk = (pane) => {
		for (const tab of pane.tabs) {
			if ('children' in tab.content) {
				const hit = tab.content.children.map(walk).find(Boolean);
				if (hit) return hit;
			} else if (tab.content.kind === 'daystrip') return tab.content.settings;
		}
	};
	return walk(window.datum.layout.current().root);
};

const heard = ({ page }) => page.evaluate(`(${stripSettings})().dayHeight`);

/** Wait until the tree has heard `want`. */
const heardIs = ({ page }, want) => page.waitForFunction(`(${stripSettings})().dayHeight === ${want}`);

/** Tell the strip's owner a day height, as a saved layout would. */
const say = ({ page }, dayHeight) =>
	page.evaluate(
		`window.datum.layout.apply((tree) => {
			const copy = structuredClone(tree);
			const walk = (pane) => {
				for (const tab of pane.tabs) {
					if ('children' in tab.content) tab.content.children.forEach(walk);
					else if (tab.content.kind === 'daystrip') tab.content.settings = { ...tab.content.settings, dayHeight: ${dayHeight} };
				}
			};
			walk(copy.root);
			return copy;
		})`
	);

/**
 * Wait until each named picture has been re-drawn at the height it is due.
 *
 * `want` is keyed by `data-days-ago` — 0 today's, 1 the past's — because
 * the heights are NOT the same number: the past holds several days and
 * today carries the hour axis. A single expected height for both is a
 * wait that never ends.
 */
const waitForHeights = ({ page }, want) =>
	page.waitForFunction(
		(w) =>
			Object.entries(w).every(([daysAgo, height]) => {
				const img = document.querySelector(`.band[data-days-ago="${daysAgo}"] svg.svg`);
				return img !== null && img.outerHTML.includes(`height="${height}"`);
			}),
		want
	);

/** Drag a handle by `dy` pixels, as a pointer does: down, move, up. */
async function dragBy({ page }, selector, dy) {
	const box = await page.locator(selector).boundingBox();
	const x = box.x + box.width / 2;
	const y = box.y + box.height / 2;
	await page.mouse.move(x, y);
	await page.mouse.down();
	await page.mouse.move(x, y + dy, { steps: 8 });
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

/** The box today's band occupies on the screen — it is not to move. */
const todayBand = ({ page }) => page.locator('.band.today').boundingBox();

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
	test('the box says how many days: today, and the past that fits above it, in one picture', async () => {
		await withApp(async (app) => {
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

			// ONE picture for the past, holding every day of it (da-hnyb),
			// and today's band apart from it.
			const past = app.page.locator('.stack .band');
			assert.equal(await past.count(), 1, 'the past is not one picture');
			assert.equal(Number(await past.getAttribute('data-days')), heldFor(many));

			// And each day is the day it claims: the days immediately before
			// today, oldest at the top.
			assert.deepEqual(
				days(await band(app, 1)),
				Array.from({ length: heldFor(many) }, (_, i) => labelFor(nowMs, heldFor(many) - i)),
				'the past is stamped with the wrong days'
			);
			assert.deepEqual(days(await band(app, 0)), [labelFor(nowMs, 0)]);

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
			assert.ok(!(await band(app, 1)).includes(happy), 'the past drew an entry logged today');

			// Shrunk back, the days that no longer fit go off screen — the
			// box is a height, not a ratchet.
			await setBox(app, 300);
			await waitForDays(app, heldFor(few));
			assert.deepEqual(await onScreen(app), short);

			// A box exactly today's height holds today alone.
			await sizeFor(app, 0);
			assert.equal(await app.page.locator('.stack').count(), 0);
		});
	});

	test('the stack holds a fortnight above the box, whether or not the box holds whole days', async () => {
		await withApp(async (app) => {
			// Room for three whole days. The box is three days tall — and
			// the picture in it is a fortnight, which is what there is to
			// scroll to. It used to be three days, to the pixel, which left
			// the wheel and the finger nothing to move (da-bewc).
			await sizeWhole(app, 3);
			const m = await metrics(app);
			const whole = await stackScroll(app);
			assert.equal(svgHeight(await band(app, 1)), pastHeight(m, FORTNIGHT), 'datum drew the past another height');
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
		await withApp(async (app) => {
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
		await withApp(
			async (app) => {
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
			},
			{ device: devices['Pixel 5'], viewport: { width: 390, height: 640 } }
		);
	});

	test('one more day re-draws the picture that gained it, and not today', async () => {
		// The shortest day, so a box past the fortnight fits on a screen:
		// inside the fortnight the stack holds the same days however the
		// box moves, and the argv does not change, so nothing is re-drawn.
		await withApp(
			async (app) => {
				await say(app, 40);
				await app.page.waitForFunction(
					() => Number(document.querySelector('button.day-grip')?.getAttribute('aria-valuenow')) === 40
				);
				await sizeFor(app, 15);
				await app.idle();
				const seen = await app.spyOnRunner();

				await sizeFor(app, 16);
				await app.idle();

				// Every dayview the strip ran for the taller box, by the days
				// it asked for and where they ended: the OLDEST picture,
				// which is the one that gained the day, once — and neither
				// the fortnight below it nor today, whose pictures are the
				// same argv as before.
				const flag = (argv, name) => (argv.includes(name) ? argv[argv.indexOf(name) + 1] : null);
				const views = (await seen())
					.filter((argv) => argv[0] === 'dayview')
					.map((argv) => [flag(argv, '--n-days'), flag(argv, '--end-date')]);
				assert.deepEqual(views, [['2', '-15']]);
			},
			{ viewport: { width: 1280, height: 1400 } }
		);
	});

	test('a past deeper than a fortnight is a picture a fortnight, a day apart', async () => {
		await withApp(
			async (app) => {
				const nowMs = await app.page.evaluate(() => Date.now());
				// The shortest day, so sixteen of them fit on the screen.
				await say(app, 40);
				await app.page.waitForFunction(
					() => Number(document.querySelector('button.day-grip')?.getAttribute('aria-valuenow')) === 40
				);
				await sizeWhole(app, 16);

				// Yesterday's fortnight against today, the two days before it
				// above — and every day the one it claims, across the seam.
				const pictures = await app.page
					.locator('.stack .band')
					.evaluateAll((els) => els.map((b) => [Number(b.dataset.daysAgo), Number(b.dataset.days)]));
				assert.deepEqual(pictures, [
					[15, 2],
					[1, 14]
				]);
				assert.deepEqual(
					[...days(await band(app, 15)), ...days(await band(app, 1))],
					Array.from({ length: 16 }, (_, i) => labelFor(nowMs, 16 - i))
				);

				// The seam costs what the space between two days costs, so
				// sixteen whole days still fill the stack to the pixel.
				const m = await metrics(app);
				const box = await stackScroll(app);
				assert.equal(box.total, pastHeight(m, 16));
				assert.equal(box.total, box.client, 'sixteen whole days scroll');
			},
			{ viewport: { width: 1280, height: 1400 } }
		);
	});

	test('the day height comes in as a prop and goes out as a change', async () => {
		// What the default is is the strip's business, not this file's, so
		// it is read off an ordinary app rather than restated here.
		let fallback;
		await withApp(async (app) => {
			fallback = await valueOf(app);
			assert.equal(await heard(app), fallback);
		});

		await withApp(async (app) => {
				// The owner said 120 — through the tree, as a saved layout
				// does: the handle says so, and both pictures are drawn in the
				// box that height gives them — the past's two days at a day box
				// and a pitch, today's at a day box plus the hour axis.
				await say(app, 120);
				await app.page.waitForFunction(
					() => Number(document.querySelector('button.day-grip')?.getAttribute('aria-valuenow')) === 120
				);
				assert.equal(await valueOf(app), 120);
				await sizeFor(app, 2);
				const m = await metrics(app);
				assert.ok(m.band > 120, `the day box is not built on the 120 day height: ${m.band}`);
				// Waited for: the stack holds the same fortnight at either
				// height, so the day COUNT `sizeFor` waits on does not move
				// and the re-draw at 120 is still in flight behind it.
				await waitForHeights(app, { 1: pastHeight(m, heldAt(m)), 0: m.today });
				assert.equal(svgHeight(await band(app, 1)), pastHeight(m, heldAt(m)), 'the past');
				assert.equal(svgHeight(await band(app, 0)), m.today, "today's band");

				// The notch moves it — up is taller — and the owner hears
				// the new value: the strip does not keep it to itself.
				const grow = 30;
				await dragBy(app, 'button.day-grip', -grow);
				assert.equal(await valueOf(app), 120 + grow);
				await heardIs(app, 120 + grow);
				// And both are re-drawn, not stretched: the picture datum
				// wrote is the new size, every day in it taller by `grow`.
				// Waited for, because the render happens on the drop.
				const tall = grown(m, grow);
				await waitForHeights(app, { 1: pastHeight(tall, heldAt(tall)), 0: tall.today });

				// ↻ says the default, to the owner as well.
				await app.page.locator('button.reset').click();
				assert.equal(await valueOf(app), fallback);
				await heardIs(app, fallback);
				const at = await metrics(app);
				await waitForHeights(app, { 1: pastHeight(at, heldAt(at)), 0: at.today });
		});
	});

	test('the margin handle re-draws every day at the new height', async () => {
		await withApp(async (app) => {
			// Three, so that two are still there once the days are taller:
			// the box does not grow with them, so fewer fit.
			await sizeFor(app, 3);
			const m = await metrics(app);
			const was = await valueOf(app);
			const grow = 40;

			// UP is taller (da-hsba): the pointer moves 40px up, and every
			// day is drawn 40px taller — today's, and each of the past's.
			await dragBy(app, 'button.day-grip', -grow);
			assert.equal(await valueOf(app), was + grow);
			const up = grown(m, grow);
			assert.equal(owedAt(up), 2, 'the taller days do not leave two in the box');
			await waitForHeights(app, { 1: pastHeight(up, heldAt(up)), 0: up.today });
			await waitForDays(app, heldAt(up));

			// And the thing the stack is for still holds at the new height:
			// the same hour is the same column in the past as today.
			const today = gridlines(await band(app, 0));
			assert.ok(today.length >= 4, `today's band drew ${today.length} gridlines`);
			assert.deepEqual(gridlines(await band(app, 1)), today, 'the past is on a different x-axis after a resize');

			// And DOWN is shorter, by the same 40, from the same start —
			// and more days fit.
			await app.page.locator('button.reset').click();
			await waitForHeights(app, { 1: pastHeight(m, heldAt(m)), 0: m.today });
			await dragBy(app, 'button.day-grip', grow);
			assert.equal(await valueOf(app), was - grow);
			const down = grown(m, -grow);
			await waitForHeights(app, { 1: pastHeight(down, heldAt(down)), 0: down.today });
			await waitForDays(app, heldAt(down));
		});
	});

	test('the keyboard works the handle and the reset', async () => {
		await withApp(async (app) => {
			const fallback = await valueOf(app);

			await app.page.locator('button.day-grip').focus();
			assert.ok(
				await app.page.evaluate(() => document.activeElement.classList.contains('day-grip')),
				'the handle does not take focus'
			);
			// Up is taller, as it is for the pointer — the two paths are one
			// convention.
			await app.page.keyboard.press('ArrowUp');
			const taller = await valueOf(app);
			assert.ok(taller > fallback, `up did not make the day taller: ${taller}`);
			await app.page.keyboard.press('ArrowDown');
			await app.page.keyboard.press('ArrowDown');
			assert.equal(await valueOf(app), 2 * fallback - taller);
			assert.equal(await heard(app), 2 * fallback - taller);

			await app.page.locator('button.reset').press('Enter');
			assert.equal(await valueOf(app), fallback);
			assert.equal(await heard(app), fallback);
		});
	});

	test('the strip writes nothing down, and draws with the store blocked', async () => {
		// Whatever is in the store before the handle moves is what is in it
		// after: the strip has no key of its own, and adds none.
		let fallback;
		await withApp(async (app) => {
			fallback = await valueOf(app);
			await sizeFor(app, 2);
			const before = await storeKeys(app);

			await dragBy(app, 'button.day-grip', -20);
			await app.page.locator('button.day-grip').focus();
			await app.page.keyboard.press('ArrowUp');
			const m = await metrics(app);
			await waitForHeights(app, { 0: m.today });
			await app.page.locator('button.reset').click();
			await heardIs(app, fallback);
			// Heard, and drawn: the strip has done what it does with it.
			await app.idle();

			assert.deepEqual(await storeKeys(app), before, 'the strip wrote to localStorage');
		});

		// And a store that throws on the ACCESS — which is what a browser
		// set to block site data actually does — changes nothing: the strip
		// draws, and the handle works for the visit.
		await withApp(
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

	test('the jagged line says the day above today is not yesterday', async () => {
		await withApp(async (app) => {
			// Deeper than the box shows whole, so there is something to
			// scroll. Nothing to tear yet: yesterday is at the bottom,
			// against today.
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
			assert.equal(
				Number(await app.page.locator('.stack .band').getAttribute('data-days')),
				heldFor(5)
			);

			// And the same by pointer, which is the other half of "reachable
			// by keyboard and by touch".
			await app.page.locator('.stack').evaluate((el) => (el.scrollTop = 0));
			await app.page.locator('button.jagged').click();
			await app.page.locator('button.jagged').waitFor({ state: 'detached' });
		});
	});

	test('a midnight moves the whole strip on by a day', async () => {
		// 10:00 UTC, so the fast-forward below lands after midnight and
		// inside the next day rather than on its boundary.
		const noon = Date.UTC(2026, 8, 9, 10, 0, 0);
		// The CLI draws the pictures, in the database worker: its clock too.
		const clock = workerClock(noon);
		/** The days `from` back to `to` before `ms`, oldest first, as stamps. */
		const stamps = (ms, from, to) =>
			Array.from({ length: from - to + 1 }, (_, i) => labelFor(ms, from - i));
		await withApp(
			async (app) => {
				// Deep enough that the stack scrolls, so the jagged line has
				// something to say at the end.
				await sizeFor(app, 5);
				assert.deepEqual(days(await band(app, 0)), stamps(noon, 0, 0));
				assert.deepEqual(days(await band(app, 1)), stamps(noon, heldFor(5), 1));

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
				assert.deepEqual(days(await band(app, 0)), stamps(after, 0, 0));
				assert.deepEqual(
					days(await band(app, 1)),
					stamps(after, heldFor(5), 1),
					'the past did not follow the new today'
				);
				// The box did not move, so neither did the depth.
				assert.equal(
					Number(await app.page.locator('.stack .band').getAttribute('data-days')),
					heldFor(5)
				);
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

	test('the past is drawn against the same x-axis as today, one gap above it', async () => {
		await withApp(async (app) => {
			await sizeFor(app, 3);

			// The hour gridlines are the axis as the data area sees it, and
			// the past's run through every one of its days — the tick LABELS
			// are today's alone. The same x in both, or the stack is a
			// comparison of two different rulers.
			const today = gridlines(await band(app, 0));
			assert.ok(today.length >= 4, `today's band drew ${today.length} gridlines`);
			assert.deepEqual(gridlines(await band(app, 1)), today, 'the past is on a different x-axis');

			// And on the screen, not only in the documents: the first hour
			// line lands on the same pixel in both, and yesterday's picture
			// ends the strip's gap above today's.
			const seen = await app.page.evaluate(() => {
				const box = (sel) => document.querySelector(sel).getBoundingClientRect();
				return {
					lines: [1, 0].map((n) => box(`.band[data-days-ago="${n}"] line.gridline`).x),
					seam:
						box('.band[data-days-ago="0"] svg.svg').top -
						box('.band[data-days-ago="1"] svg.svg').bottom
				};
			});
			const { gap } = await metrics(app);
			assert.ok(Math.abs(seen.lines[0] - seen.lines[1]) <= 1, `the first hour lines: ${seen.lines}`);
			assert.ok(Math.abs(seen.seam - gap) <= 1, `yesterday ends ${seen.seam}px above today, not ${gap}`);

			// The axis strip itself is drawn once, under today.
			const ticks = (svg) => [...svg.matchAll(/class="tick-label"/g)].length;
			assert.ok(ticks(await band(app, 0)) >= 4, "today's band lost its hour axis");
			assert.equal(ticks(await band(app, 1)), 0, 'the past repeated the hour axis');
		});
	});

	// The strip is full-width, and the fix for horizontal scrollbars over
	// it landed the day before this did (1ceada8) — so every depth the
	// box can hold is swept, at every width the app is read at.
	for (const width of [360, 390, 768, 1280]) {
		test(`never scrolls the page sideways at ${width}px, at any depth`, async () => {
			await withApp(
				async (app) => {
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
				},
				{ viewport: { width, height: 800 } }
			);
		});
	}

	// The strip's row is a fixed height now, so it cannot push the thing a
	// phone user logs with off the bottom of the screen (da-hkpd) — as
	// long as that height is one a phone can spare.
	test("the strip's row leaves the widget bar on screen", async () => {
		const screen = { width: 360, height: 640 };
		await withApp(
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
