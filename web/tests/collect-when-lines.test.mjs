// Collect's time section, its lines and its motion (da-qjq60). vividn,
// 2026-11-07: «see the nice line that separates the time section from the
// field section in the collect widget when the time section is open? put
// another one separating the time section from the now section. When the
// time section is closed also put that line between the fields section and
// the now section, and put the clock button directly on that line. The time
// section should animate more smoothly as it opens and closes too».

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { ON_COLLECT, startApp, stopApp, todayAt, withApp } from './support/app.mjs';
import { DESKTOP, PHONE, pickOption } from './support/layout.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const WIDGET = 'section.pane[data-title="collect"]';
const WHEN = `${WIDGET} .bar [data-when]`;
const CLOCK = `${WHEN} [data-when-clock]`;
const SHOTS = process.env.DATUM_SHOTS;

const SEED = ['setup mood --verb switch --states good,bad,meh', 'setup coffee --verb occur', 'occur coffee -t 09:00', 'occur tea -t 12:00', 'switch mood bad -t 13:00'];

/** Collect at rest on a phone, its strip on `side`, the time section closed. */
async function atRest(side, fn, options = {}) {
	await withApp(
		async (app) => {
			for (const line of SEED) assert.equal((await app.run(line.split(' '))).code, 0, line);
			await app.reload();
			await app.page.locator(`${WIDGET} .bar .collect .room .spot.shelf`).first().waitFor();
			if (side === 'right') await pickOption(app.page, 'bar', 'side', 'right');
			await app.page.locator(`${CLOCK}[aria-expanded="false"]`).waitFor();
			await app.idle();
			await fn(app);
		},
		{ ...PHONE, home: ON_COLLECT, clock: todayAt(15), ...options }
	);
}

/**
 * The section's two edges, each its line's width, color and x, the theme's
 * --rule resolved, the strip's edge, the clock's center and the nearest
 * field's edge, all toward the rows: for `side: right` x is mirrored, so
 * the rows are always past the strip.
 */
const geometry = (page, side) =>
	page.locator(WHEN).evaluate((when, side) => {
		const flip = (x) => (side === 'right' ? -x : x);
		const css = getComputedStyle(when);
		const probe = document.createElement('i');
		probe.style.color = 'var(--rule)';
		when.append(probe);
		const rule = getComputedStyle(probe).color;
		probe.remove();
		const r = when.getBoundingClientRect();
		const [near, far] = side === 'right' ? ['right', 'left'] : ['left', 'right'];
		const line = (edge) => ({ width: parseFloat(css[`border-${edge}-width`]), color: css[`border-${edge}-color`], x: flip(edge === 'left' ? r.left + 0.5 : r.right - 0.5) });
		const widget = when.closest('section.pane');
		const strip = widget.querySelector('[data-strip]').getBoundingClientRect();
		const clock = when.querySelector('[data-when-clock]').getBoundingClientRect();
		const rows = [...widget.querySelectorAll('.room [data-field]')].map((b) => b.getBoundingClientRect()).map((b) => flip(side === 'right' ? b.right : b.left));
		return {
			rule,
			near: line(near),
			far: line(far),
			width: r.width,
			strip: flip(side === 'right' ? strip.left : strip.right),
			clock: { x: flip(clock.left + clock.width / 2), y: clock.top + clock.height / 2, mid: r.top + r.height / 2 },
			rows: Math.min(...rows),
			dpr: devicePixelRatio
		};
	}, side);

/**
 * How much of each line is drawn, as a reader sees it: down the section's
 * height but a pixel at each end (a clip's fractional edge), clear of the clock, the device pixels of --rule in each row of a
 * screenshot 6px across the line at `x` (mirrored for `side: right`), each
 * count once. One line a CSS pixel wide reads `[dpr]`, however the strip's
 * picture under it is drawn: two side by side read twice that.
 */
async function drawn(page, side, at) {
	const box = await page.locator(WHEN).boundingBox();
	const clock = await page.locator(CLOCK).boundingBox();
	const x = side === 'right' ? -at.x : at.x;
	const shot = await page.screenshot({ clip: { x: x - 3, y: box.y + 1, width: 6, height: box.height - 2 } });
	return page.evaluate(
		async ([png, rule, skip]) => {
			const img = new Image();
			img.src = `data:image/png;base64,${png}`;
			await img.decode();
			const canvas = Object.assign(document.createElement('canvas'), { width: img.width, height: img.height });
			const g = canvas.getContext('2d');
			g.drawImage(img, 0, 0);
			const { data } = g.getImageData(0, 0, img.width, img.height);
			const [r, gr, b] = rule.match(/\d+/g).map(Number);
			const counts = new Set();
			for (let y = 0; y < img.height; y++) {
				if (y >= skip[0] && y <= skip[1]) continue;
				let n = 0;
				for (let i = 0; i < img.width; i++) {
					const o = (y * img.width + i) * 4;
					if (data[o] === r && data[o + 1] === gr && data[o + 2] === b) n++;
				}
				counts.add(n);
			}
			return [...counts];
		},
		[shot.toString('base64'), at.rule, [(clock.y - box.y - 3) * at.dpr, (clock.y + clock.height - box.y + 1) * at.dpr]]
	);
}

/** Whether `color` is a color with nothing of it showing. */
const clear = (color) => color === 'transparent' || /rgba\(.*,\s*0\)$/.test(color);

/** Past the glide: nothing in the section is moving. */
const still = (page) => page.waitForFunction((w) => document.querySelector(w).getAnimations({ subtree: true }).length === 0, WHEN);

describe('collect: the time section', () => {
	for (const side of ['left', 'right']) {
		test(`side: ${side}: closed, one line between the strip and the rows, the clock's center on it; open, a line on each edge of the section in --rule`, async () => {
			await atRest(side, async ({ page }) => {
				await still(page);
				const closed = await geometry(page, side);
				const at = JSON.stringify(closed);
				assert.equal(closed.near.width, 1, `no line on the strip's edge: ${at}`);
				assert.equal(closed.near.color, closed.rule, `the line is not --rule: ${at}`);
				assert.ok(Math.abs(closed.near.x - closed.strip) <= 1, `the line is not on the strip's edge: ${at}`);
				assert.ok(clear(closed.far.color), `a second line closed: ${at}`);
				assert.deepEqual(await drawn(page, side, { ...closed.near, rule: closed.rule, dpr: closed.dpr }), [closed.dpr], `not one line drawn: ${at}`);
				assert.ok(Math.abs(closed.clock.x - closed.near.x) <= 1, `the clock is not on the line: ${at}`);
				assert.ok(Math.abs(closed.clock.y - closed.clock.mid) <= 2, `the clock is not halfway down: ${at}`);
				assert.ok(closed.rows >= closed.clock.x + 12 - 1, `the rows stand under the clock: ${at}`);
				if (SHOTS) await page.locator(WIDGET).screenshot({ path: `${SHOTS}/closed-${side}.png` });

				await page.locator(CLOCK).click();
				await still(page);
				const open = await geometry(page, side);
				const was = JSON.stringify(open);
				for (const edge of ['near', 'far']) {
					assert.equal(open[edge].width, 1, `no line on the ${edge} edge: ${was}`);
					assert.equal(open[edge].color, open.rule, `the ${edge} line is not --rule: ${was}`);
				}
				assert.ok(Math.abs(open.near.x - open.strip) <= 1, `the near line is not on the strip's edge: ${was}`);
				for (const edge of ['near', 'far'])
					assert.deepEqual(await drawn(page, side, { ...open[edge], rule: open.rule, dpr: open.dpr }), [open.dpr], `not one ${edge} line drawn: ${was}`);
				assert.ok(open.rows >= open.far.x, `the rows stand under the section: ${was}`);
				assert.ok(open.width > 40, `the section did not open: ${was}`);
				if (SHOTS) await page.locator(WIDGET).screenshot({ path: `${SHOTS}/open-${side}.png` });
			});
		});
	}

	// da-c93wz, vividn, 2026-11-09: «the vertical line where the clock button shows up when the time section is closed.
	// space it out from the rendered now svg a little more, so that the now section has consistent margins around it».
	for (const [device, side, options] of [
		['phone', 'left', {}],
		['phone', 'right', {}],
		['desktop', 'left', { device: undefined, ...DESKTOP }]
	])
		test(`${device}, side: ${side}: closed, the line stands as far from the strip's picture as the pane's edge does`, async () => {
			await atRest(
				side,
				async ({ page }) => {
					await still(page);
					const m = await page.locator(WHEN).evaluate((when, side) => {
						const flip = (x) => (side === 'right' ? -x : x);
						const pane = when.closest('section.pane');
						const p = pane.getBoundingClientRect();
						const border = parseFloat(getComputedStyle(pane).borderLeftWidth);
						const svg = pane.querySelector('[data-strip] svg.now-strip');
						const s = svg.getBoundingClientRect();
						// The picture's drawn extent across: its marks, cut to the strip's box (overflow hidden).
						const marks = [...svg.querySelectorAll('g rect, g path')].map((e) => e.getBoundingClientRect()).filter((r) => r.width > 0 && r.height > 0);
						const ink = [Math.max(s.left, Math.min(...marks.map((r) => r.left))), Math.min(s.right, Math.max(...marks.map((r) => r.right)))];
						const line = when.getBoundingClientRect();
						const [outer, inner] = side === 'right' ? [p.right - border, line.left] : [p.left + border, line.right - 1];
						const [far, near] = side === 'right' ? [ink[1], ink[0]] : [ink[0], ink[1]];
						return { margin: flip(far - outer), gap: flip(inner - near), marks: marks.length };
					}, side);
					const at = JSON.stringify(m);
					assert.ok(m.marks > 0, `nothing drawn on the strip: ${at}`);
					assert.ok(m.gap >= 6, `the picture runs up to the line: ${at}`);
					assert.ok(Math.abs(m.gap - m.margin) <= 1.5, `the gap to the line is not the picture's margin from the pane: ${at}`);
					if (SHOTS) await page.locator(WIDGET).screenshot({ path: `${SHOTS}/margins-${device}-${side}.png` });
				},
				options
			);
		});

	test('it opens and closes over time: halfway, its width and the rows are between closed and open, each frame no further than the last', async () => {
		await atRest('left', async ({ page }) => {
			await still(page);
			const widths = (open) =>
				page.evaluate(
					async ([w, open]) => {
						const when = document.querySelector(w);
						const rows = () => Math.min(...[...when.closest('section.pane').querySelectorAll('.room [data-field]')].map((b) => b.getBoundingClientRect().left));
						const frame = () => new Promise(requestAnimationFrame);
						const at = (t0) => ({ t: performance.now() - t0, width: when.getBoundingClientRect().width, rows: rows() });
						const from = at(0);
						when.querySelector('[data-when-clock]').click();
						const t0 = performance.now();
						const seen = [];
						while (performance.now() - t0 < 600) (await frame(), seen.push(at(t0)));
						return { from, seen, open };
					},
					[WHEN, open]
				);
			for (const open of [true, false]) {
				const { from, seen } = await widths(open);
				const to = seen.at(-1);
				const mid = seen.find((s) => s.t >= 150);
				const said = JSON.stringify({ from, mid, to });
				const [lo, hi] = open ? [from, to] : [to, from];
				assert.ok(hi.width - lo.width > 40, `it did not ${open ? 'open' : 'close'}: ${said}`);
				assert.ok(mid.width > lo.width + 4 && mid.width < hi.width - 4, `halfway it is not between: ${said}`);
				assert.ok(mid.rows > lo.rows + 4 && mid.rows < hi.rows - 4, `halfway the rows are not between: ${said}`);
				const back = seen.slice(1).filter((s, i) => (open ? s.width < seen[i].width - 0.01 : s.width > seen[i].width + 0.01));
				assert.deepEqual(back, [], `it went back on itself ${open ? 'opening' : 'closing'}`);
				await still(page);
			}
		});
	});

	test('under reduced motion it opens and closes at once', async () => {
		await atRest(
			'left',
			async ({ page }) => {
				await still(page);
				for (const open of [true, false]) {
					const [before, next] = await page.evaluate(async (w) => {
						const when = document.querySelector(w);
						const before = when.getBoundingClientRect().width;
						when.querySelector('[data-when-clock]').click();
						await new Promise(requestAnimationFrame);
						await new Promise(requestAnimationFrame);
						return [before, when.getBoundingClientRect().width];
					}, WHEN);
					await still(page);
					const settled = await page.locator(WHEN).evaluate((w) => w.getBoundingClientRect().width);
					assert.notEqual(before, settled, `it did not ${open ? 'open' : 'close'}`);
					assert.equal(next, settled, `${open ? 'opening' : 'closing'} it moved over time: ${next} then ${settled}`);
				}
				await until(() => page.locator(`${WHEN} [data-part]`).count(), (n) => n === 0);
			},
			{ reducedMotion: 'reduce' }
		);
	});
});
