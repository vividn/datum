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
// how tall it is, and how tall it is says how much past it holds. So the
// tests size the box the page gives it and read which days appear. The
// one thing a reader sets on the strip itself is how tall a day is drawn
// — the notch in the left margin — and that is a prop in and a `change`
// event out, which is asserted from the outside: what the strip's owner
// heard, and the picture's own SVG `height`, not a class name that claims
// to mean it. The strip remembers nothing, and that is asserted too.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
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
 * How many days before today the strip owes at arithmetic `m`: the room
 * above today's band, in days, counting a partly revealed one.
 */
function owedAt(m) {
	const room = Math.max(0, m.strip - m.today - m.gap);
	return room > 0 ? Math.min(60, Math.ceil((room - m.band) / m.unit) + 1) : 0;
}
const owed = async (app) => owedAt(await metrics(app));

/**
 * Size the box so the strip shows exactly `days` days before today.
 *
 * Half a pitch short of `days` whole ones, so the top day is only partly
 * revealed: that is what makes the stack scroll, and a scrolling stack is
 * what the jagged line is about. Zero days is a box exactly today's
 * height — which is a side bar's floor, not a top bar's: a top bar's
 * title lies over 12px of the content, and its floor keeps them off
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
	await waitForDays(app, days);
}

/** Size the box so the room above today is exactly `days` whole days. */
async function sizeWhole(app, days) {
	const m = await metrics(app);
	await setBox(app, m.today + m.gap + pastHeight(m, days) + m.chrome);
	await waitForDays(app, days);
}

/**
 * Wait until the strip shows exactly `depth` days before today, and every
 * picture on it is drawn with as many days as its box claims — no day
 * more, and none still the depth it was before.
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
			await waitForDays(app, few);

			// A tall box: more of them.
			await setBox(app, 600);
			const many = await owed(app);
			assert.ok(many > few, `600px holds no more days than 300px: ${many} vs ${few}`);
			await waitForDays(app, many);

			// ONE picture for the past, holding every day of it (da-hnyb),
			// and today's band apart from it.
			const past = app.page.locator('.stack .band');
			assert.equal(await past.count(), 1, 'the past is not one picture');
			assert.equal(Number(await past.getAttribute('data-days')), many);

			// And each day is the day it claims: the days immediately before
			// today, oldest at the top.
			assert.deepEqual(
				days(await band(app, 1)),
				Array.from({ length: many }, (_, i) => labelFor(nowMs, many - i)),
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

			// Shrunk back, the days that no longer fit go away — the box is
			// a height, not a ratchet.
			await setBox(app, 300);
			await waitForDays(app, few);
			assert.equal(days(await band(app, 1)).length, few);

			// A box exactly today's height holds today alone.
			await sizeFor(app, 0);
			assert.equal(await app.page.locator('.stack').count(), 0);
		});
	});

	test('the past fills a room of whole days exactly, and scrolls in one that is not', async () => {
		await withApp(async (app) => {
			// Room for three whole days: the picture is the stack's height,
			// to the pixel, and there is nothing to scroll.
			await sizeWhole(app, 3);
			const m = await metrics(app);
			const whole = await stackScroll(app);
			assert.equal(svgHeight(await band(app, 1)), pastHeight(m, 3), 'datum drew the past another height');
			assert.equal(whole.client, pastHeight(m, 3), `the stack is not three days tall: ${JSON.stringify(whole)}`);
			assert.equal(whole.total, whole.client, 'three whole days scroll');

			// Half a day less and the top one is partly revealed: the same
			// three days, and now the stack scrolls to them.
			await sizeFor(app, 3);
			const part = await stackScroll(app);
			assert.equal(part.total, pastHeight(m, 3));
			assert.ok(part.total > part.client, `the stack does not scroll: ${JSON.stringify(part)}`);
		});
	});

	test('one more day re-draws the past one day deeper, and not today', async () => {
		await withApp(async (app) => {
			await sizeFor(app, 3);
			await app.idle();
			const seen = await app.spyOnRunner();

			await sizeFor(app, 4);
			await app.idle();

			// Every dayview the strip ran for the taller box, by the days it
			// asked for and where they ended: the past at four, once, and
			// today not at all — its picture did not change.
			const flag = (argv, name) => (argv.includes(name) ? argv[argv.indexOf(name) + 1] : null);
			const views = (await seen())
				.filter((argv) => argv[0] === 'dayview')
				.map((argv) => [flag(argv, '--n-days'), flag(argv, '--end-date')]);
			assert.deepEqual(views, [['4', '-1']]);
		});
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
				assert.equal(svgHeight(await band(app, 1)), pastHeight(m, 2), 'the past');
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
				await waitForHeights(app, { 1: pastHeight(tall, owedAt(tall)), 0: tall.today });

				// ↻ says the default, to the owner as well.
				await app.page.locator('button.reset').click();
				assert.equal(await valueOf(app), fallback);
				await heardIs(app, fallback);
				const at = await metrics(app);
				await waitForHeights(app, { 1: pastHeight(at, owedAt(at)), 0: at.today });
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
			await waitForHeights(app, { 1: pastHeight(up, 2), 0: up.today });
			await waitForDays(app, 2);

			// And the thing the stack is for still holds at the new height:
			// the same hour is the same column in the past as today.
			const today = gridlines(await band(app, 0));
			assert.ok(today.length >= 4, `today's band drew ${today.length} gridlines`);
			assert.deepEqual(gridlines(await band(app, 1)), today, 'the past is on a different x-axis after a resize');

			// And DOWN is shorter, by the same 40, from the same start —
			// and more days fit.
			await app.page.locator('button.reset').click();
			await waitForHeights(app, { 1: pastHeight(m, 3), 0: m.today });
			await dragBy(app, 'button.day-grip', grow);
			assert.equal(await valueOf(app), was - grow);
			const down = grown(m, -grow);
			await waitForHeights(app, { 1: pastHeight(down, owedAt(down)), 0: down.today });
			await waitForDays(app, owedAt(down));
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
			await app.page.waitForTimeout(300);

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
			// not throw away the days the box holds.
			assert.equal(await app.page.locator('.stack .band').getAttribute('data-days'), '5');

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
				assert.deepEqual(days(await band(app, 1)), stamps(noon, 5, 1));

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
					stamps(after, 5, 1),
					'the past did not follow the new today'
				);
				// The box did not move, so neither did the depth.
				assert.equal(await app.page.locator('.stack .band').getAttribute('data-days'), '5');
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
				await app.page.locator('.bar button.widget').first().waitFor();

				const bar = await app.page.locator('.bar button.widget').first().boundingBox();
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
