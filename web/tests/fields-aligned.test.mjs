// The fields widget «aligned to the picture» (da-omw4n), in the built
// app: each button stands at the height its field is drawn (`y`, and a
// tall band's `height`), a crowded band scrolls sideways, a pinch zooms
// and shows the range marker, a tap on the marker zooms out, and the
// option set back gives today's rows. At a desktop's size and a phone's,
// on a small database and on one of sixty fields wherever their names
// hash them, which crowds every band.
//
// The geometry itself is pure: fields-aligned-pure.test.mjs.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP, PHONE, pickOption } from './support/layout.mjs';

before(startApp);
after(stopApp);

const PANE = 'section.pane[data-title="fields"]';
const BAR = `${PANE} .bar`;

const at = (field, verb, y, height) =>
	`spec set ${field} --from-json '${JSON.stringify({ field, verb, y, ...(height ? { height } : {}) })}'`;

/** A point whose band is the middle tenth, a tall block at the top, and a band of sixteen at 0.8. */
const SMALL = [
	at('mid', 'occur', 0.45, 0.1),
	at('sleep', 'start', 0, 0.4),
	...Array.from({ length: 16 }, (_, i) => at(`crowd${i}`, 'occur', 0.8))
];

/** Sixty fields where their names put them. */
const MANY = Array.from({ length: 60 }, (_, i) => `spec set f${i} --verb occur`);

async function aligned(app, seed) {
	for (const line of seed) assert.equal((await app.runLine(line)).code, 0, line);
	await app.reload();
	await app.page.locator(`${BAR} [data-field]`).first().waitFor();
	await pickOption(app.page, 'bar', 'the buttons stand', 'aligned to the picture');
	await app.page.locator(`${PANE} .fields[data-align="picture"]`).waitFor();
	await settle(app.page);
}

/** Wait until no slot is still gliding. */
const settle = (page) =>
	page.waitForFunction(
		(bar) => document.querySelector(bar)?.getAnimations({ subtree: true }).length === 0,
		BAR
	);

/** Every field button's box, against the bar's top left corner as scrolled. */
const boxes = (page) =>
	page.$eval(BAR, (bar) => {
		const b = bar.getBoundingClientRect();
		return {
			bar: { height: bar.clientHeight, width: bar.clientWidth, scroll: bar.scrollWidth },
			fields: Object.fromEntries(
				[...bar.querySelectorAll('[data-field]')].map((el) => {
					const r = el.getBoundingClientRect();
					return [el.dataset.field, { top: r.top - b.top, bottom: r.bottom - b.top, left: r.left - b.left + bar.scrollLeft, right: r.right - b.left + bar.scrollLeft }];
				})
			)
		};
	});

/** No two buttons cover the same pixel, and every one is inside the bar's height. */
function apart({ bar, fields }) {
	const all = Object.entries(fields);
	for (const [name, r] of all) {
		assert.ok(r.top >= -0.5 && r.bottom <= bar.height + 0.5, `${name} ${r.top}–${r.bottom} outside 0–${bar.height}`);
	}
	for (const [i, [a, p]] of all.entries())
		for (const [b, q] of all.slice(i + 1))
			assert.ok(p.right <= q.left || q.right <= p.left || p.bottom <= q.top || q.bottom <= p.top, `${a} covers ${b}`);
}

/** A trackpad's pinch, as the browser hands it over: ctrl+wheel. */
const pinchWheel = (page, deltaY) =>
	page.$eval(
		BAR,
		(bar, deltaY) => {
			const r = bar.getBoundingClientRect();
			bar.dispatchEvent(
				new WheelEvent('wheel', { deltaY, ctrlKey: true, clientX: r.left + 40, clientY: r.top + r.height / 2, bubbles: true, cancelable: true })
			);
		},
		deltaY
	);

/** Two fingers drawn apart, as pointer events. */
const pinchTouch = (page) =>
	page.$eval(BAR, (bar) => {
		const r = bar.getBoundingClientRect();
		const x = r.left + 30;
		const mid = r.top + r.height / 2;
		const fire = (type, pointerId, y, on = bar) =>
			on.dispatchEvent(new PointerEvent(type, { pointerId, pointerType: 'touch', isPrimary: pointerId === 11, clientX: x, clientY: y, bubbles: true }));
		fire('pointerdown', 11, mid - 5);
		fire('pointerdown', 12, mid + 5);
		for (let k = 1; k <= 5; k++) {
			fire('pointermove', 11, mid - 5 - 4 * k);
			fire('pointermove', 12, mid + 5 + 4 * k);
		}
		fire('pointerup', 11, mid - 25);
		fire('pointerup', 12, mid + 25);
	});

for (const [name, screen] of [
	['desktop', DESKTOP],
	['phone', PHONE]
]) {
	test(`${name}: a field stands at its height, a crowded band scrolls, a pinch zooms and the marker zooms out`, async () => {
		await withApp(async (app) => {
			const { page } = app;
			await aligned(app, SMALL);
			const seen = await boxes(page);
			apart(seen);

			// The band at mid-height stands at mid-height; the tall band's
			// button is as tall as its band, from the top.
			const centre = (seen.fields.mid.top + seen.fields.mid.bottom) / 2;
			assert.ok(Math.abs(centre - seen.bar.height / 2) <= 1, `mid's centre at ${centre} of ${seen.bar.height}`);
			assert.ok(seen.fields.sleep.top <= 1);
			const band = seen.fields.sleep.bottom - seen.fields.sleep.top;
			assert.ok(band >= Math.max(24, 0.4 * seen.bar.height) - 1, `sleep is ${band} tall`);

			// Sixteen at one height share it, side by side, past the bar's width.
			const crowd = Array.from({ length: 16 }, (_, i) => seen.fields[`crowd${i}`]);
			assert.ok(seen.bar.scroll > seen.bar.width, `the bar does not scroll: ${seen.bar.scroll} in ${seen.bar.width}`);
			assert.ok(crowd.every((r) => Math.abs(r.top - crowd[0].top) <= 1), 'the crowded band is not one height');
			const scrolled = await page.$eval(BAR, (bar) => ((bar.scrollLeft = 10_000), bar.scrollLeft));
			assert.ok(scrolled > 0, 'the bar did not scroll sideways');
			await page.$eval(BAR, (bar) => (bar.scrollLeft = 0));

			// A pinch shows the marker, and spreads the buttons without growing them.
			const marker = page.locator(`${PANE} button.range`);
			assert.equal(await marker.count(), 0, 'a marker before any zoom');
			if (name === 'phone') await pinchTouch(page);
			else await pinchWheel(page, -300);
			await marker.waitFor();
			const [from, to] = await marker.evaluate((el) => [Number(el.dataset.from), Number(el.dataset.to)]);
			assert.ok(to - from < 1 && from >= 0 && to <= 1, `range ${from}–${to}`);
			await settle(page);
			const zoomed = await boxes(page);
			assert.ok(
				Math.abs(zoomed.fields.mid.bottom - zoomed.fields.mid.top - (seen.fields.mid.bottom - seen.fields.mid.top)) <= 1,
				'a zoom resized a button'
			);
			// The pinch was at mid-height, so it is the band far from it that moves.
			assert.ok(zoomed.fields.crowd0.top - seen.fields.crowd0.top > 5, 'the band at 0.8 did not move away');

			// A tap on it zooms back out, to where they stood.
			await marker.click();
			await marker.waitFor({ state: 'detached' });
			await settle(page);
			assert.ok(Math.abs((await boxes(page)).fields.mid.top - seen.fields.mid.top) <= 1);

			// And the option set back is today's rows.
			await pickOption(page, 'bar', 'the buttons stand', 'in rows');
			await page.locator(`${PANE} .fields[data-align="rows"]`).waitFor();
			assert.equal(await page.$eval(`${BAR} [data-field="mid"]`, (el) => getComputedStyle(el.parentElement).position), 'static');
			assert.deepEqual(app.errors, []);
		}, screen);
	});

	test(`${name}: sixty fields where their names put them stand apart, inside the bar`, async () => {
		await withApp(async (app) => {
			await aligned(app, MANY);
			const seen = await boxes(app.page);
			assert.equal(Object.keys(seen.fields).length, 60);
			apart(seen);
			assert.deepEqual(app.errors, []);
		}, screen);
	});
}

// vremya on #494, on her own record: Track + stood first and wide, and every
// field started right of its column, so on the half-width fields pane of a
// fresh phone's `mobile` layout the most used field showed as «ac». The
// fields start at the pane's left edge, and the (+) comes after them.
test('a fresh phone: the most used field is whole on open, and the fields start at the left edge', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			await aligned(app, [...SMALL, ...Array.from({ length: 6 }, (_, i) => `occur crowd7 -t -${i + 1}`)]);
			const { bar, fields } = await boxes(page);
			assert.ok(bar.width < 250, `the fields pane is not the half-width one: ${bar.width}px`);
			const most = fields.crowd7;
			assert.ok(most.left >= -0.5 && most.right <= bar.width + 0.5, `crowd7 at ${most.left}–${most.right} of ${bar.width}`);
			assert.ok(Math.min(...Object.values(fields).map((r) => r.left)) <= 0.5, 'no field starts at the left edge');
			// A `+`, and no `›` drawn after it: app.css's
			// `[data-act='open']:not(.add-button)…` spares it the opener's mark
			// (#488), and the check is on what is DRAWN, not on that class.
			const [plus, after] = await page.$eval(`${BAR} [data-track]`, (el) => [
				el.textContent.trim(),
				getComputedStyle(el, '::after').content
			]);
			assert.equal(plus, '+');
			assert.equal(after, 'none', `the + wears ${after}`);
			assert.deepEqual(app.errors, []);
		},
		{ viewport: { width: 390, height: 844 }, device: { isMobile: true, hasTouch: true }, home: null }
	);
});

