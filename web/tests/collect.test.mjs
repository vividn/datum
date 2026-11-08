// The collect widget (da-w6437): the fields widget retitled and built for
// logging. vividn, 2026-11-03: «the primary interaction widget on the
// collect pane … this widget really adapts to whatever you're doing».
//
// At rest its fields stand in rows by height, those on in a section of their
// own (collect-rest.test.mjs). Between the now strip and the rows, at rest
// and beside a field's view, the time and duration section (da-us1n1):
// `time −1m −5m · dur 1m 5m`, summing into pills, `time` or `dur` opening its
// options. A tap opens a field in collect's rows: back, its name, its state and
// off on the first row; a switch field's states down the rows under it
// (collect-open.test.mjs); its keys
// and `+ data` in a drawer on the far edge (da-igqs2), closed unless a key is
// required or it is dragged open. Every pill it makes is on the input bar too.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, todayAt } from './support/app.mjs';
import { choiceLines, pickOption } from './support/layout.mjs';
import { drag } from './support/press.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const WIDGET = 'section.pane[data-title="collect"]';
const VIEW = `${WIDGET} .bar .collect`;
const BAR_PILLS = '.prompt-bar .pill';
const DRAWER = `${WIDGET} [data-drawer]`;

const SEED = [
	'setup mood --verb switch --states good,bad,meh -k why= -k with=',
	'setup coffee --verb occur',
	'setup work --verb start',
	'occur coffee -t 09:00',
	'occur coffee -t 10:00',
	'occur coffee -t 11:00',
	'occur tea -t 12:00',
	'switch mood bad -t 13:00'
];

const shared = sharedApp({ clock: todayAt(15) });

/**
 * The app seeded, its fields sorted again as on a load, coffee drawn. One
 * click logs a moment or block on its tap (da-gra0z), so a test of the view
 * such a field opens to taps it in 2-click mode (`twoClicks`).
 */
const collect = (fn, { twoClicks = false } = {}) =>
	shared.solo(async (app) => {
		// The session a test before left (its time section open, da-iunoo) forgotten, as a fresh device has none.
		await app.page.evaluate(() => window.datum.layout.forgetSession());
		if (twoClicks) await pickOption(app.page, 'bar', 'a tap', 'opens and chooses, 2-click');
		for (const line of SEED) {
			const logged = await app.run(line.split(' '));
			assert.equal(logged.code, 0, `${line}: ${logged.stderr}`);
		}
		await app.redraw();
		// What a load does, and a return to the foreground: `resort`.
		await app.page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));
		await app.page.locator(`${VIEW} [data-field="coffee"]`).waitFor();
		await fn(app);
	});

/** The center of what `selector` names, on screen. */
const center = (page, selector) =>
	page.locator(selector).first().evaluate((el) => {
		const r = el.getBoundingClientRect();
		return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
	});

/** The pills on the input bar, by kind and what each says. */
const barPills = (page) =>
	page.locator(BAR_PILLS).evaluateAll((ps) => ps.map((p) => `${[...p.classList].find((c) => c !== 'pill')}:${p.innerText.replace(/[×\s]+$/, '').trim()}`));

/** The time and duration section (da-us1n1), its buttons, and its pill of `kind` (`time`, `dur`). */
const WHEN = `${WIDGET} [data-when]`;
/** The time section opened by its clock, where it is closed, as a reader would (da-iunoo), and done opening (da-qjq60). */
const openWhen = async (page, when = WHEN) => {
	const clock = page.locator(`${when} [data-when-clock][aria-expanded="false"]`);
	if ((await clock.count()) > 0) await clock.click();
	await page.locator(when).evaluate(async (w) => {
		for (let a; (a = w.getAnimations({ subtree: true })).length; ) await Promise.allSettled(a.map((x) => x.finished));
	});
};
/** A part of the section tapped, the section opened by its clock first. */
const tap = async (page, what) => (await openWhen(page), page.locator(`${WHEN} ${what}`).click());
const whenPill = (page, kind) => page.locator(`${WHEN} .pill[data-pill="${kind}"] .said`);
const saysPill = (page, kind, text) =>
	page.waitForFunction(([w, k, t]) => document.querySelector(`${w} .pill[data-pill="${k}"] .said`)?.innerText === t, [WHEN, kind, text]);

describe('collect', () => {
	test('at rest the widget is titled collect, and the field that is on stands in the active section, its state on the strip', async () => {
		await collect(async ({ page }) => {
			// mood was switched to bad: it is on, the others are not (da-lw7ue).
			await page.locator(`${VIEW} .spot.shelf[data-section="active"] [data-field="mood"]`).waitFor();
			await page.locator(`${VIEW} [data-now-field="mood"]`).waitFor();
			const sections = await page.$$eval(`${VIEW} .spot.shelf`, (spots) => spots.map((s) => s.dataset.section));
			assert.equal(sections.filter((s) => s === 'active').length, 1, JSON.stringify(sections));
		});
	});

	// vividn, 2026-11-05: «Let's not show the timezone field in the collect
	// widget, it is kind of a special field and wouldn't be manually changed»
	// (da-cyq92). `fields --json` says it is datum's own; the other widgets draw it.
	test('the timezone field gets no button and is no choice of the fields menu, and the pictures still draw its band', async () => {
		await collect(async (app) => {
			const { page } = app;
			assert.equal((await app.run(['timezone', 'UTC', '-t', '08:00'])).code, 0);
			await app.redraw();
			await page.locator('section.pane:not([data-title="collect"]) svg [data-field="timezone"]').first().waitFor({ state: 'attached' });
			await page.locator(`${VIEW} [data-field="mood"]`).waitFor();
			assert.equal(await page.locator(`${VIEW} [data-field="timezone"], ${VIEW} [data-now-field="timezone"]`).count(), 0);
			const choices = await choiceLines(page, 'bar', 'fields');
			assert.ok(choices.some((c) => c.endsWith('coffee')), JSON.stringify(choices));
			assert.ok(!choices.some((c) => c.endsWith('timezone')), JSON.stringify(choices));
		});
	});

	test('a switch field tapped: back on the first row over its states, its keys and + data in a closed drawer on the right a drag opens', async () => {
		await collect(async ({ page }) => {
			await page.locator(`${VIEW} [data-field="mood"]`).click();
			await page.locator(`${VIEW} [data-state="good"]`).waitFor();
			assert.equal(await page.locator(`${VIEW} [data-field]`).count(), 0, 'the fields are still drawn under the view');
			assert.deepEqual(await page.locator(`${VIEW} [data-state]`).evaluateAll((bs) => bs.map((b) => b.dataset.state)), ['good', 'bad', 'meh']);
			const [back, good] = await Promise.all([center(page, `${VIEW} [data-back]`), center(page, `${VIEW} [data-state="good"]`)]);
			// Back before the states, on the first row, and no keys of the view's own.
			assert.ok(back.y < good.y || back.x < good.x, JSON.stringify({ back, good }));
			assert.equal(await page.locator(`${VIEW} [data-key]`).count(), 0);

			// mood's keys are optional: the drawer is closed, its handle on the right edge, its keys out of view.
			assert.equal(await page.locator(DRAWER).getAttribute('data-drawer'), 'closed');
			const [box, handle] = await Promise.all([page.locator(VIEW).boundingBox(), page.locator(`${DRAWER} [data-drawer-handle]`).boundingBox()]);
			assert.ok(Math.abs(handle.x + handle.width - (box.x + box.width)) <= 2, JSON.stringify({ box, handle }));
			assert.ok((await page.locator(`${DRAWER} [data-key="why"]`).boundingBox()).x >= box.x + box.width - 1, 'a closed drawer\'s keys are past the edge');
			// A drag out of the edge opens it.
			const from = { x: handle.x + handle.width / 2, y: handle.y + handle.height / 2 };
			await drag(page, from, { x: from.x - 200, y: from.y });
			await page.locator(`${DRAWER}[data-drawer="open"]`).waitFor();
			// Its swing done: the drawer no longer moved off the edge.
			await page.waitForFunction((s) => new DOMMatrix(getComputedStyle(document.querySelector(s)).transform).m41 === 0, DRAWER);
			assert.deepEqual(await page.locator(`${DRAWER} .panel button`).allInnerTexts(), ['why=', 'with=', '+ data']);
			const why = await page.locator(`${DRAWER} [data-key="why"]`).boundingBox();
			assert.ok(why.x >= box.x && why.x + why.width <= box.x + box.width + 1, JSON.stringify({ box, why }));
			assert.deepEqual(await barPills(page), ['field:mood'], 'the field is a pill on the bar');

			// mood is in `bad`: its chip says so with the CLI's yellow `!`.
			assert.deepEqual(await page.locator(`${VIEW} [data-state]:has([data-repeat])`).evaluateAll((bs) => bs.map((b) => b.dataset.state)), ['bad']);

			// A key's chip writes `key=` on the line.
			await page.locator(`${DRAWER} [data-key="why"]`).click();
			await page.waitForFunction(() => document.querySelector('.prompt-bar input')?.value === 'why=');
		});
	});

	test('a field with a required key opens with its drawer open, and a drag back closes it', async () => {
		await collect(async (app) => {
			const { page } = app;
			assert.equal((await app.run(['setup', 'body', '--verb', 'switch', '--states', 'sit,stand', '-k', 'how', '-k', 'note='])).code, 0);
			await app.redraw();
			await page.locator(`${VIEW} [data-field="body"]`).click();
			await page.locator(`${DRAWER}[data-drawer="open"]`).waitFor();
			assert.deepEqual(await page.locator(`${DRAWER} .panel button`).allInnerTexts(), ['how=', 'note=', '+ data']);
			assert.deepEqual(await page.locator(`${DRAWER} .key.required`).allInnerTexts(), ['how=']);
			const handle = await page.locator(`${DRAWER} [data-drawer-handle]`).boundingBox();
			const from = { x: handle.x + handle.width / 2, y: handle.y + handle.height / 2 };
			await drag(page, from, { x: from.x + 200, y: from.y });
			await page.locator(`${DRAWER}[data-drawer="closed"]`).waitFor();
			// Back, and mood, whose keys are optional, opens with it closed.
			await page.locator(`${VIEW} [data-back]`).click();
			await page.locator(`${VIEW} [data-field="mood"]`).click();
			await page.locator(`${VIEW} [data-open-field="mood"]`).waitFor();
			await page.waitForTimeout(100);
			assert.equal(await page.locator(DRAWER).getAttribute('data-drawer'), 'closed');
		});
	});

	// vividn, 16:07: «1-click»: a state logs at once with the time pills set,
	// and collect is back at rest, the pills spent.
	test('a state tapped logs at once with the time set, and collect is back at rest with no pills', async () => {
		await collect(async (app) => {
			const { page } = app;
			await page.locator(`${VIEW} [data-field="mood"]`).click();
			await tap(page, '[data-earlier="5"]');
			await whenPill(page, 'time').waitFor();
			const seen = await app.spyOnRunner();
			await page.locator(`${VIEW} [data-state="meh"]`).click();
			await page.waitForFunction(() => window.__datumSeen.some((a) => a[0] === 'change'));
			assert.deepEqual((await seen()).find((a) => a[0] === 'change' && !a.includes('--dry-run')), ['change', 'mood', 'meh', '-q']);
			await page.locator(`${VIEW} [data-field="coffee"]`).waitFor();
			await page.waitForFunction(() => document.querySelectorAll('.prompt-bar .pill').length === 0);
			await app.idle();
			const [row] = JSON.parse((await app.run(['tail', 'mood', '-n', '1', '--json'])).stdout).rows;
			assert.equal(row.state, 'meh');
		});
	});

	// vividn, 2026-11-06 (da-iunoo): «Let's actually hide it by default. But a
	// little clock button in the middle between the fields section and the now
	// secttion, clicking it toggles whether this time panel is open.» And
	// (da-s2hht): «instead, put it right on the boundary line between the now
	// section and the fields section.»
	test('at rest the section is a clock alone on the boundary between the strip and the rows; a tap opens it, kept for the session, and a second closes it', async () => {
		await collect(async ({ page }) => {
			const clock = page.locator(`${WHEN} [data-when-clock]`);
			const parts = page.locator(`${WHEN} [data-part], ${WHEN} button.step`);
			assert.equal(await clock.getAttribute('aria-expanded'), 'false');
			assert.equal(await parts.count(), 0, 'the section is out at rest');
			const [strip, when] = await Promise.all([`${WIDGET} [data-strip]`, WHEN].map((s) => page.locator(s).first().boundingBox()));
			// The field nearest the strip.
			const rows = { x: Math.min(...(await page.locator(`${VIEW} [data-field]`).evaluateAll((bs) => bs.map((b) => b.getBoundingClientRect().x)))) };
			const boundary = strip.x + strip.width;
			const box = await clock.boundingBox();
			const geometry = JSON.stringify({ strip, when, rows, box });
			// On the boundary, half over the strip, at a finger's size, in the middle of the height.
			assert.ok(Math.abs(box.x + box.width / 2 - boundary) <= 2, `the clock is not on the boundary: ${geometry}`);
			assert.ok(box.width >= 24 && box.height >= 24, `the clock is under a finger's 24px: ${geometry}`);
			assert.ok(Math.abs(box.y + box.height / 2 - (when.y + when.height / 2)) <= 2, `the clock is not in the middle: ${geometry}`);
			// No column of its own: the rows start right past the clock's far half, clear of its press.
			assert.ok(rows.x >= box.x + box.width - 1 && rows.x - boundary <= box.width / 2 + 2 * 6 + 1, `the rows do not start at the strip: ${geometry}`);
			const hit = await clock.evaluate((b) => {
				const r = b.getBoundingClientRect();
				return b.contains(document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2));
			});
			assert.ok(hit, 'a press at the clock lands on something else');
			const held = () => page.evaluate(() => Object.values(window.datum.layout.session()).map((s) => s.timeOpen).filter((o) => o !== undefined));

			await clock.click();
			assert.equal(await clock.getAttribute('aria-expanded'), 'true');
			assert.deepEqual(await parts.evaluateAll((bs) => bs.map((b) => b.dataset.part ?? b.innerText)), ['time', '−1m', '−5m', 'dur', '1m', '5m']);
			assert.deepEqual(await held(), [true]);

			await clock.click();
			await page.locator(`${WHEN} [data-part]`).first().waitFor({ state: 'detached' });
			assert.equal(await parts.count(), 0);
			assert.deepEqual(await held(), [false]);
		});
	});

	// «when you click on time or dur the styling on the panel that opens looks really jarringly off, lots of white».
	test("time's and dur's options are in the page's colors: no input or button there has a light ground", async () => {
		await collect(async ({ page }) => {
			for (const kind of ['time', 'dur']) {
				await tap(page, `[data-part="${kind}"]`);
				const more = page.locator(`${WHEN} [data-when-more="${kind}"]`);
				await more.waitFor();
				const grounds = await more.locator('input, button').evaluateAll((els) =>
					els.map((el) => {
						const [r, g, b, a = 1] = getComputedStyle(el).backgroundColor.match(/[\d.]+/g).map(Number);
						return { what: el.dataset ? Object.keys(el.dataset).join() : el.tagName, light: (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255, a };
					})
				);
				assert.ok(grounds.length >= (kind === 'time' ? 4 : 2), JSON.stringify(grounds));
				for (const g of grounds) assert.ok(g.a === 0 || g.light < 0.3, `${kind}: ${g.what} has a light ground: ${JSON.stringify(g)}`);
				await tap(page, `[data-part="${kind}"]`);
				await more.waitFor({ state: 'detached' });
			}
		});
	});

	// «add a little triangle arrow to the time and dur buttons so that you know you
	// can drag them. Allow dragging them left and right, even when there is nothing yet selected».
	test('time and dur wear their triangles, and a drag on either with no pill makes one: a time from now, a length', async () => {
		await collect(async ({ page }) => {
			await openWhen(page);
			for (const kind of ['time', 'dur']) assert.equal(await page.locator(`${WHEN} [data-part="${kind}"] [data-drags]`).count(), 1, `${kind} wears no triangles`);
			assert.deepEqual(await barPills(page), []);

			const from = await center(page, `${WHEN} [data-part="time"]`);
			await drag(page, from, { x: from.x - 40, y: from.y });
			await whenPill(page, 'time').waitFor();
			const at = await page.evaluate(() => +window.datum.cursor.at());
			assert.ok(at < todayAt(15), `the drag left chose ${new Date(at).toISOString()}, not a time before now`);
			assert.equal(await page.locator(`${WHEN} [data-when-more]`).count(), 0, 'the drag opened the options');
			assert.equal(await whenPill(page, 'time').locator('[data-drags]').count(), 1, 'the pill wears no triangles');
			// The word the drag began on is gone under it, the pill in its place (canary on #1792): the drag
			// ended at its lift, so the mouse passing over the pill after, no button down, moves nothing.
			const pill = await whenPill(page, 'time').boundingBox();
			for (let x = pill.x - 30; x <= pill.x + pill.width + 30; x += 6) await page.mouse.move(x, pill.y + pill.height / 2);
			assert.equal(await page.evaluate(() => +window.datum.cursor.at()), at, 'a move with no button down dragged the pill');

			const dur = await center(page, `${WHEN} [data-part="dur"]`);
			await drag(page, dur, { x: dur.x - 40, y: dur.y });
			await whenPill(page, 'dur').waitFor();
			assert.match(await whenPill(page, 'dur').innerText(), /^\d+m$/);
			assert.ok((await barPills(page)).some((p) => p.startsWith('dur:')), JSON.stringify(await barPills(page)));
			assert.equal(await page.locator(`${WHEN} [data-when-more]`).count(), 0, 'the drag opened the options');
		});
	});

	// «space out the buttons a little more in this time section. right now they overlap some».
	test('no two of the section\'s buttons meet, their ×s among them, at any height: a column, halves and a row', async () => {
		await collect(async ({ page }) => {
			await tap(page, '[data-earlier="5"]');
			await tap(page, '[data-longer="5"]');
			await whenPill(page, 'dur').waitFor();
			const seen = new Set();
			for (const h of [36, 50, 64, 80, 100, 111, 112, 120, 150, 180, 220, 255, 256, 320, 420]) {
				const m = await page.locator(WHEN).evaluate(async (when, h) => {
					when.style.bottom = 'auto';
					when.style.height = `${h}px`;
					await new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(done, 80))));
					for (let a; (a = when.getAnimations({ subtree: true })).length; ) await Promise.allSettled(a.map((x) => x.finished));
					const column = when.querySelector('.column');
					// The clock stands over the column's gap, outside it (da-qjq60).
					const boxes = [...column.querySelectorAll('button'), when.querySelector('[data-when-clock]')].map((b) => {
						const r = b.getBoundingClientRect();
						// A pill's × stands over its own corner, clear of its text (da-rtsun): one pill is one button here.
						return { b: b.dataset.part ?? b.getAttribute('aria-label') ?? b.innerText, pill: b.closest('.pill')?.dataset.pill, l: r.left, r: r.right, t: r.top, d: r.bottom };
					});
					const c = column.getBoundingClientRect();
					const past = [...column.querySelectorAll('*')].filter((e) => { const r = e.getBoundingClientRect(); return r.right > c.right - 2.4 + 0.5 || r.left < c.left + 2.4 - 0.5; }).map((e) => `${e.tagName}.${e.className.baseVal ?? e.className}:${Math.round(e.getBoundingClientRect().left - c.left)}-${Math.round(e.getBoundingClientRect().right - c.left)}`);
					return { need: `${column.scrollWidth}×${column.scrollHeight} in ${column.clientWidth}×${column.clientHeight} ${past.join(' ')}`, mode: when.classList.contains('row') ? 'row' : when.classList.contains('halves') ? 'halves' : 'column', boxes, scrolls: column.scrollHeight > column.clientHeight + 1 || column.scrollWidth > column.clientWidth + 1, column: { l: c.left, r: c.right, t: c.top, d: c.bottom } };
				}, h);
				seen.add(m.mode);
				const say = `${h}px, ${m.mode}: ${JSON.stringify(m.boxes)}`;
				assert.equal(m.boxes.length, 9, say);
				for (const [i, a] of m.boxes.entries())
					for (const b of m.boxes.slice(i + 1).filter((b) => !a.pill || b.pill !== a.pill)) assert.ok(a.r <= b.l || b.r <= a.l || a.d <= b.t || b.d <= a.t, `${a.b} meets ${b.b} at ${say}`);
				assert.ok(!m.scrolls, `${h}px, ${m.mode}: the section scrolls, ${m.need}`);
			}
			assert.deepEqual([...seen].sort(), ['column', 'halves', 'row']);
		});
	});

	// vividn, 2026-11-05: «clicking -1m, -5m, 1m or 5m doesn't expand the
	// section either, it changes the dur or time button to be the time or dur
	// pill, and keeps summing up. Also updates the nowview with the proper selection».
	test('−1m then −5m is a −6m time pill, the selection six minutes back, and the section stays closed', async () => {
		await collect(async ({ page }) => {
			await openWhen(page);
			assert.equal(await page.locator(`${WHEN} [data-part="time"]`).innerText(), 'time');
			await tap(page, '[data-earlier="1"]');
			await saysPill(page, 'time', '−1m');
			await tap(page, '[data-earlier="5"]');
			await saysPill(page, 'time', '−6m');
			assert.deepEqual(await barPills(page), ['time:-t -6']);
			assert.equal(await page.evaluate(() => window.datum.cursor.at()?.toISOString()), new Date(todayAt(14, 54)).toISOString());
			await page.locator('section.pane[data-title="nowview"] line.cursor').waitFor({ state: 'attached' });
			assert.equal(await page.locator(`${WHEN} [data-when-more]`).count(), 0, 'the section opened');
		});
	});

	test('1m then 5m is a 6m dur pill, and −m in its options replaces it', async () => {
		await collect(async ({ page }) => {
			await tap(page, '[data-longer="1"]');
			await tap(page, '[data-longer="5"]');
			await saysPill(page, 'dur', '6m');
			assert.equal(await whenPill(page, 'dur').locator('svg.length-arrow').count(), 1, 'the length wears no arrow');
			assert.deepEqual(await barPills(page), ['dur:6m']);
			assert.equal(await page.locator(`${WHEN} [data-when-more]`).count(), 0, 'the section opened');

			await whenPill(page, 'dur').click();
			await tap(page, '[data-when-more="dur"] [data-moment]');
			await saysPill(page, 'dur', '−m');
			assert.deepEqual(await barPills(page), ['moment:-m']);
			await tap(page, '[data-longer="5"]');
			await saysPill(page, 'dur', '5m');
			assert.deepEqual(await barPills(page), ['dur:5m']);
		});
	});

	// da-rtsun: the 24px × sat over the end of «−6m», reading «−6».
	test("each pill's × stands clear of its label's text, and is 24px", async () => {
		await collect(async ({ page }) => {
			await tap(page, '[data-earlier="1"]');
			await tap(page, '[data-earlier="5"]');
			await saysPill(page, 'time', '−6m');
			await tap(page, '[data-longer="1"]');
			await tap(page, '[data-longer="5"]');
			await saysPill(page, 'dur', '6m');
			for (const kind of ['time', 'dur']) {
				const { text, clear } = await page.locator(`${WHEN} .pill[data-pill="${kind}"]`).evaluate((pill) => {
					const range = document.createRange();
					range.selectNodeContents(pill.querySelector('.said'));
					const box = (r) => ({ left: r.left, right: r.right, top: r.top, bottom: r.bottom, width: r.width, height: r.height });
					return { text: box(range.getBoundingClientRect()), clear: box(pill.querySelector('.clear').getBoundingClientRect()) };
				});
				const meet = text.left < clear.right && clear.left < text.right && text.top < clear.bottom && clear.top < text.bottom;
				assert.ok(!meet, `${kind}: the × ${JSON.stringify(clear)} is over its text ${JSON.stringify(text)}`);
				assert.equal(clear.width, 24, `${kind}: the × is ${clear.width}px wide`);
				assert.equal(clear.height, 24, `${kind}: the × is ${clear.height}px tall`);
			}
		});
	});

	test('a tap on time opens its options, a typed time and all day among them, and a drag closes it', async () => {
		await collect(async ({ page }) => {
			await tap(page, '[data-part="time"]');
			const more = page.locator(`${WHEN} [data-when-more="time"]`);
			await more.waitFor();
			await more.locator('[data-type-time]').fill('9:15am');
			await more.locator('[data-type-time]').press('Enter');
			await saysPill(page, 'time', '09:15');
			assert.equal(await page.evaluate(() => window.datum.cursor.at()?.toISOString()), new Date(todayAt(9, 15)).toISOString());
			await more.locator('[data-allday]').click();
			await saysPill(page, 'time', '09:15 -D');
			assert.deepEqual((await barPills(page)).sort(), ['allday:-D', 'time:09:15']);

			// A drag toward the strip closes it, the pills kept.
			const box = await more.boundingBox();
			const [x, y] = [box.x + box.width / 2, box.y + box.height - 8];
			await page.mouse.move(x, y);
			await page.mouse.down();
			for (let dx = 10; dx <= 60; dx += 10) await page.mouse.move(x - dx, y);
			await page.mouse.up();
			await more.waitFor({ state: 'detached' });
			assert.deepEqual((await barPills(page)).sort(), ['allday:-D', 'time:09:15']);
		});
	});

	// vremya on #1769: Escape takes the options in before the field's view under them.
	test('with a field open and the time options out, Escape closes the options, and a second the view', async () => {
		await collect(async ({ page }) => {
			// Opened by a hold, which a right click is: in 1-click mode a tap would log it.
			await page.locator(`${VIEW} [data-field="coffee"]`).click({ button: 'right' });
			await page.locator(`${VIEW} [data-open-field="coffee"]`).waitFor();
			await tap(page, '[data-part="time"]');
			const more = page.locator(`${WHEN} [data-when-more="time"]`);
			await more.waitFor();
			await page.keyboard.press('Escape');
			await more.waitFor({ state: 'detached' });
			assert.equal(await page.locator(`${VIEW} [data-open-field="coffee"]`).count(), 1, 'Escape closed the view under the options');
			await page.keyboard.press('Escape');
			await page.locator(`${VIEW} [data-open-field]`).waitFor({ state: 'detached' });
			await page.locator(`${VIEW} [data-field="coffee"]`).waitFor();
		});
	});

	// vremya on #1769: a press dragged out of the options and lifted outside
	// them is over; the mouse passing across them after, no button down, is no drag.
	test('a press dragged out of the options and lifted outside them leaves no drag behind', async () => {
		await collect(async ({ page }) => {
			await tap(page, '[data-part="time"]');
			const more = page.locator(`${WHEN} [data-when-more="time"]`);
			await more.waitFor();
			const box = await more.boundingBox();
			const y = box.y + box.height - 8;
			await page.mouse.move(box.x + box.width / 2, y);
			await page.mouse.down();
			for (let x = box.x + box.width / 2; x <= box.x + box.width + 60; x += 10) await page.mouse.move(x, y);
			await page.mouse.up();
			for (let x = box.x + box.width - 4; x >= box.x + 4; x -= 10) await page.mouse.move(x, y);
			await page.waitForTimeout(100);
			assert.equal(await more.count(), 1, 'a move with no button down closed the options');
		});
	});

	// vremya on #1769: a typed length is read as the CLI reads `dur=`.
	test("a typed length is the CLI's reading: 1.5h is 1h30m, and 1h30 is refused", async () => {
		await collect(async ({ page }) => {
			await tap(page, '[data-part="dur"]');
			const typed = page.locator(`${WHEN} [data-when-more="dur"] [data-type-dur]`);
			await typed.fill('1.5h');
			await typed.press('Enter');
			await saysPill(page, 'dur', '1h30m');
			await typed.fill('1h30');
			await typed.press('Enter');
			await page.waitForFunction((s) => document.querySelector(s)?.getAttribute('aria-invalid') === 'true', `${WHEN} [data-type-dur]`);
			assert.equal(await whenPill(page, 'dur').innerText(), '1h30m', 'the refused length changed it');
		});
	});

	test('a moment tapped with the time pill logs six minutes back', async () => {
		await collect(async (app) => {
			const { page } = app;
			await tap(page, '[data-earlier="1"]');
			await tap(page, '[data-earlier="5"]');
			await saysPill(page, 'time', '−6m');
			const seen = await app.spyOnRunner();
			await page.locator(`${VIEW} [data-field="coffee"]`).click();
			await page.waitForFunction(() => window.__datumSeen.some((a) => a[0] === 'moment' && !a.includes('--dry-run')));
			assert.deepEqual((await seen()).find((a) => a[0] === 'moment' && !a.includes('--dry-run')), ['moment', 'coffee', '-t', '-6']);
			await page.waitForFunction(() => document.querySelectorAll('.prompt-bar .pill').length === 0);
			await app.idle();
			const ats = (await app.entries()).filter((e) => e.field === 'coffee').map((e) => Date.parse(e.at.utc));
			assert.ok(ats.includes(todayAt(14, 54)), JSON.stringify(ats));
		});
	});

	test('× clears its own pill and no other', async () => {
		await collect(async ({ page }) => {
			await tap(page, '[data-earlier="5"]');
			await tap(page, '[data-longer="5"]');
			await whenPill(page, 'dur').waitFor();
			await tap(page, '.pill[data-pill="dur"] .clear');
			await page.locator(`${WHEN} [data-part="dur"]:not(.said)`).waitFor();
			assert.equal(await whenPill(page, 'time').innerText(), '−5m');
			assert.deepEqual(await barPills(page), ['time:-q']);

			await tap(page, '.pill[data-pill="time"] .clear');
			await page.locator(`${WHEN} [data-part="time"]:not(.said)`).waitFor();
			assert.deepEqual(await barPills(page), []);
		});
	});

	test('back takes the field and the pills made since the tap, and keeps a time pill already there', async () => {
		await collect(async ({ page }) => {
			// A −5m made in one field's view, its field's pill then taken off the
			// bar: the view closes and the −5m stays, made before the next tap.
			await page.locator(`${VIEW} [data-field="coffee"]`).click();
			await tap(page, '[data-earlier="5"]');
			await whenPill(page, 'time').waitFor();
			await page.locator(`${BAR_PILLS}.field button.remove`).click();
			await page.locator(`${VIEW} [data-field="mood"]`).waitFor();
			assert.deepEqual(await barPills(page), ['time:-q']);

			await page.locator(`${VIEW} [data-field="mood"]`).click();
			await tap(page, '[data-earlier="5"]');
			await tap(page, '[data-part="dur"]');
			await tap(page, '[data-when-more="dur"] [data-moment]');
			await saysPill(page, 'time', '−10m');
			assert.deepEqual((await barPills(page)).sort(), ['field:mood', 'moment:-m', 'time:-qq']);
			// Its options closed again, as a second tap on dur does.
			await tap(page, '.pill[data-pill="dur"] .said');
			await page.locator(`${WHEN} [data-when-more]`).waitFor({ state: 'detached' });

			await page.locator(`${VIEW} [data-back]`).click();
			await page.locator(`${VIEW} [data-field="coffee"]`).waitFor();
			assert.deepEqual(await barPills(page), ['time:-q']);
		}, { twoClicks: true });
	});

	test("a field's view has no time parts at its top, the section standing beside it", async () => {
		await collect(async ({ page }) => {
			await page.locator(`${VIEW} [data-field="coffee"]`).click();
			await page.locator(`${VIEW} [data-open-field="coffee"]`).waitFor();
			assert.equal(await page.locator(`${VIEW} [data-state]`).count(), 0);
			// A moment has no off: back alone on the first row.
			assert.deepEqual(await page.locator(`${VIEW} .spot.open[data-role="head"] button`).allInnerTexts(), ['back']);
			assert.deepEqual(await page.locator(`${DRAWER} .panel button`).allInnerTexts(), ['+ data']);
			assert.match(await page.locator(`${VIEW} .spot.open[data-role="head"]`).innerText(), /back\s*coffee/);
			// A moment has no length to give: the dur half is off beside it.
			await openWhen(page);
			for (const at of ['[data-part="dur"]', '[data-longer="1"]', '[data-longer="5"]']) assert.ok(await page.locator(`${WHEN} ${at}`).isDisabled(), at);
			// The section, between the strip and the view.
			// The view stands past the section once its width is measured, a frame after it opens: waited for, not raced.
			const boxes = () => Promise.all([`${WIDGET} [data-strip]`, WHEN, VIEW].map((s) => page.locator(s).first().boundingBox()));
			await until(boxes, ([strip, when, view]) => strip.x + strip.width <= when.x + 1 && when.x + when.width <= view.x + 1);
		}, { twoClicks: true });
	});

	// da-xdz73. vividn, 2026-11-03: «if you select an hid pill, it could show
	// information about this individual datum … no need to show by metadata
	// here, just the data / put a -A button which toggles showing all the data».
	describe('an entry selected', () => {
		const ENTRY = `${VIEW}[data-entry]`;
		/** mood's entry, `why=tired`, selected by a tap on its tail row. */
		const selected = (fn, data = ['why=tired']) =>
			collect(async (app) => {
				const logged = await app.run(['switch', 'mood', 'good', ...data, '-t', '14:00']);
				assert.equal(logged.code, 0, logged.stderr);
				await app.redraw();
				const [{ hid }] = JSON.parse((await app.run(['tail', 'mood', '-n', '1', '--json'])).stdout).rows;
				await app.page.locator(`section[data-title="tail"] tbody tr[data-hid^="${hid.slice(0, 5)}"]`).click();
				await app.page.locator(`${ENTRY} [data-row="why"]`).waitFor();
				await fn(app, hid);
			});
		const rows = (page) => page.locator(`${ENTRY} [data-row]`).evaluateAll((rs) => rs.map((r) => r.dataset.row));

		test('a tail row selected shows its entry: field and time, then its keys and values, and no metadata', async () => {
			await selected(async ({ page }) => {
				assert.match(await page.locator(`${ENTRY} .title`).innerText(), /mood\s*good\s*14:00/);
				assert.equal(await page.locator(`${ENTRY} [data-row="why"]`).innerText(), 'why\ntired');
				assert.deepEqual(await rows(page), ['why']);
			});
		});

		test('−A shows the metadata too, and a second tap hides it', async () => {
			await selected(async ({ page }, hid) => {
				await page.locator(`${ENTRY} [data-all]`).click();
				await page.locator(`${ENTRY} [data-row="hid"]`).waitFor();
				assert.equal(await page.locator(`${ENTRY} [data-row="hid"] dd`).innerText(), hid);
				for (const key of ['why', 'slug', 'create_time', 'raw_input']) assert.ok((await rows(page)).includes(key), key);
				await page.locator(`${ENTRY} [data-all]`).click();
				await page.locator(`${ENTRY} [data-row="hid"]`).waitFor({ state: 'detached' });
				assert.deepEqual(await rows(page), ['why']);
			});
		});

		test('−A shows a data key named as a column beside the column', async () => {
			await selected(
				async (app, hid) => {
					await app.page.locator(`${ENTRY} [data-all]`).click();
					await app.page.locator(`${ENTRY} [data-row="hid"]`).waitFor();
					const slugs = await app.page.locator(`${ENTRY} [data-row="slug"] dd`).allInnerTexts();
					assert.equal(slugs.length, 2, `slugs: ${slugs}`);
					assert.equal(slugs[0], 'mine');
					assert.equal(await app.page.locator(`${ENTRY} [data-row="hid"] dd`).innerText(), hid);
					assert.deepEqual(app.errors, []);
				},
				['why=tired', 'slug=mine']
			);
		});

		test('back clears the entry pill and returns to the fields', async () => {
			await selected(async ({ page }) => {
				assert.equal(await page.locator(`${BAR_PILLS}.hid`).count(), 1);
				await page.locator(`${ENTRY} [data-back]`).click();
				await page.locator(`${VIEW} [data-field="coffee"]`).waitFor();
				assert.equal(await page.locator(ENTRY).count(), 0);
				assert.equal(await page.locator(`${BAR_PILLS}.hid`).count(), 0);
			});
		});

		// da-ffwmw. vividn, 2026-11-07: «the occur time in the entry view of the
		// collect widget should also be a link which adds that as a time pill. the
		// field should also be a link which changes to the field pill».
		test('its field is a link: the entry pill becomes the field pill, and collect opens the field', async () => {
			await selected(async ({ page }) => {
				await page.locator(`${ENTRY} [data-entry-field]`).click();
				await page.locator(`${VIEW} [data-open-field="mood"]`).waitFor();
				assert.equal(await page.locator(ENTRY).count(), 0);
				assert.deepEqual(await barPills(page), ['field:mood']);
			});
		});

		test('its time is a link: a period\'s time pill is its end, to the second, and the next entry is logged at it', async () => {
			await selected(async (app) => {
				const { page } = app;
				// A period ending at 14:20:30, selected in mood's place.
				assert.equal((await app.run(['start', 'work', '20m', '-t', '14:20:30'])).code, 0);
				await app.redraw();
				const [work] = JSON.parse((await app.run(['tail', 'work', '-n', '1', '--json'])).stdout).rows;
				await page.locator(`section[data-title="tail"] tbody tr[data-hid^="${work.hid.slice(0, 5)}"]`).click();
				await page.locator(`${ENTRY}[data-entry="${work.hid}"]`).waitFor();
				await page.locator(`${ENTRY} [data-entry-time]`).click();
				await page.waitForFunction(() => document.querySelector('.prompt-bar .pill.time')?.innerText.includes('14:20:30'));
				// Back takes the entry's pill and leaves the time's; a coffee tap logs at once with it.
				await page.locator(`${ENTRY} [data-back]`).click();
				await page.locator(`${VIEW} [data-field="coffee"]`).click();
				await page.waitForFunction(() => document.querySelectorAll('.prompt-bar .pill').length === 0);
				await app.idle();
				const [coffee] = JSON.parse((await app.run(['tail', 'coffee', '-n', '1', '--json'])).stdout).rows;
				assert.equal(typeof work.time, 'string', 'a tail row carries its instant as time');
				assert.equal(coffee.time, work.time);
			});
		});

		test('a field with no button of its own, as timezone, is named as text and no link', async () => {
			await selected(async (app) => {
				assert.equal((await app.run(['switch', 'timezone', 'Etc/UTC', '-t', '14:30'])).code, 0);
				await app.redraw();
				const [tz] = JSON.parse((await app.run(['tail', 'timezone', '-n', '1', '--json'])).stdout).rows;
				await app.page.locator(`section[data-title="tail"] tbody tr[data-hid^="${tz.hid.slice(0, 5)}"]`).click();
				// The hid is on the card at once, its entry read after it.
				await app.page.locator(`${ENTRY} .title .name`, { hasText: 'timezone' }).waitFor();
				assert.equal(await app.page.locator(`${ENTRY} [data-entry-field]`).count(), 0);
			});
		});

		// da-ug56t. vividn, 2026-11-07: «if there are multiple entries selected
		// show all of them (in chronological order). still show the now section
		// even when the entry view is open».
		test('several selected, each has its card, oldest first whatever the order they were picked in', async () => {
			await selected(async (app, mood) => {
				const { page } = app;
				// tail's rows oldest first: coffee's 09:00, 10:00, 11:00.
				const coffee = JSON.parse((await app.run(['tail', 'coffee', '-n', '3', '--json'])).stdout).rows;
				// mood's at 14:00 first, then coffee's 11:00, then its 09:00.
				for (const { hid } of [coffee[2], coffee[0]]) {
					await page.locator(`section[data-title="tail"] tbody tr[data-hid^="${hid.slice(0, 5)}"]`).click({ modifiers: ['Control'] });
				}
				await until(
					() => page.locator(`${ENTRY} [data-card]`).count(),
					(n) => n === 3
				);
				assert.deepEqual(await page.locator(`${ENTRY} [data-entry-time]`).allInnerTexts(), ['09:00', '11:00', '14:00']);
				assert.equal(await page.locator(`${ENTRY} [data-card]`).last().getAttribute('data-card'), mood);
				// Each card's −A is its own.
				await page.locator(`${ENTRY} [data-card="${mood}"] [data-all]`).click();
				await page.locator(`${ENTRY} [data-card="${mood}"] [data-row="hid"]`).waitFor();
				assert.equal(await page.locator(`${ENTRY} [data-row="hid"]`).count(), 1);
				// And the cards' reads come to rest: a read of each entry alone moved `_LAST`, a write, which read them again, forever.
				await app.idle();
			});
		});

		test('one or several selected, the now strip stands beside the cards', async () => {
			await selected(async (app) => {
				const { page } = app;
				const beside = async () => {
					const [strip, view] = await Promise.all([`${WIDGET} [data-strip]`, ENTRY].map((s) => page.locator(s).first().boundingBox()));
					assert.ok(strip && strip.width > 0 && strip.height > 0, `strip: ${JSON.stringify(strip)}`);
					assert.ok(strip.x + strip.width <= view.x + 1, `strip ${JSON.stringify(strip)} beside the cards ${JSON.stringify(view)}`);
					assert.ok(await page.locator(`${WIDGET} [data-strip] svg.now-strip`).isVisible());
				};
				await beside();
				const [coffee] = JSON.parse((await app.run(['tail', 'coffee', '-n', '1', '--json'])).stdout).rows;
				await page.locator(`section[data-title="tail"] tbody tr[data-hid^="${coffee.hid.slice(0, 5)}"]`).click({ modifiers: ['Control'] });
				await until(
					() => page.locator(`${ENTRY} [data-card]`).count(),
					(n) => n === 2
				);
				await beside();
			});
		});

		test('its field and time take the keyboard, with a visible ring', async () => {
			await selected(async ({ page }) => {
				await page.locator(`${ENTRY} [data-back]`).focus();
				for (const link of ['data-entry-field', 'data-entry-time']) {
					await page.keyboard.press('Tab');
					const ring = await page.locator(`${ENTRY} [${link}]`).evaluate((el) => el === document.activeElement && el.matches(':focus-visible') && getComputedStyle(el).outlineStyle);
					assert.equal(ring, 'solid', link);
				}
			});
		});
	});
});
