// The nowview's now section is collect's now strip (da-d6vzq). vividn,
// 2026-11-05: «the now section in the nowview should behave the same way as
// this section, so wipes on changing states, moment dots flowing in from the
// right. It should be as responsive too. The dots appear later than in the
// collect widget currently.»
//
// Both on one screen, a tap on collect is read in both strips at once: the
// database's files held, so what either draws is the tap's own mark.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, todayAt, withApp } from './support/app.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const NOWVIEW = 'section.pane[data-title="nowview"]';
const COLLECT = 'section.pane[data-title="collect"]';
/** Each strip's marks, and its box: the nowview's is the slot it is mounted over, a nested `<svg>`'s client rect being its content's. */
const STRIPS = {
	collect: [`${COLLECT} svg.now-strip`, `${COLLECT} svg.now-strip`],
	nowview: [`${NOWVIEW} .now-live svg.now-strip`, `${NOWVIEW} .at-now rect.now-strip`]
};

const pane = (id, kind, settings = {}) => ({ id, tabs: [{ id: `${id}-tab`, content: { id: `${id}-widget`, kind, settings } }], active: 0, bar: { edge: 'top', align: 'start' } });

/** The nowview at a 15-minute window over collect, half the page each. */
function desk(orientation = 'horizontal') {
	const split = { id: 'main', direction: 'column', ratios: [0.5, 0.5], children: [pane('nowview', 'nowview', { history: '15m', orientation }), pane('bar', 'widgetbar')] };
	return { version: 1, name: 'default', designedFor: 'desktop', root: { id: 'root', tabs: [{ id: 'root-tab', content: split }], active: 0, bar: { edge: 'top', align: 'start' } } };
}

const at = (field, verb, y, more = {}) => `setup ${field} --from-json '${JSON.stringify({ field, verb, y, height: 0.1, ...more })}'`;
const SEED = [at('coffee', 'occur', 0.2), at('work', 'start', 0.6), 'occur coffee', 'start work', 'end work'];

/** Seed, a minute a line, and load with both strips drawn. */
async function seeded(app) {
	for (const line of SEED) {
		assert.equal((await app.runLine(line)).code, 0, line);
		await app.clock.forward(60_000);
	}
	await app.reload();
	await app.page.locator(STRIPS.nowview[0]).waitFor({ state: 'attached' });
	await app.page.locator(`${COLLECT} [data-field="coffee"]`).first().waitFor();
	await app.idle();
}

/**
 * Each strip's marks of `field`, in one read: kind, fill, and where each
 * stands along time inside the strip, from its past edge (`from`, `to`, `mid`),
 * cut to it, with the strip's length (`of`). A mark wholly outside is left out.
 */
const both = (page, field) =>
	page.evaluate(
		({ strips, field }) =>
			Object.fromEntries(
				Object.entries(strips).map(([name, [selector, of]]) => {
					const svg = document.querySelector(selector);
					if (!svg) return [name, null];
					const box = document.querySelector(of).getBoundingClientRect();
					const down = svg.closest('.nowview')?.classList.contains('vertical') ?? false;
					const [lo, len] = down ? [box.top, box.height] : [box.left, box.width];
					const marks = [...svg.querySelectorAll(`[data-mark="${field}"]:is(circle, rect)`)]
						.map((el) => {
							const r = el.getBoundingClientRect();
							const [a, b] = down ? [r.top, r.bottom] : [r.left, r.right];
							return { kind: el.localName, fill: getComputedStyle(el).fill, from: Math.max(a, lo) - lo, to: Math.min(b, lo + len) - lo, mid: (a + b) / 2 - lo, of: len };
						})
						.filter((m) => m.to >= m.from);
					return [name, marks];
				})
			),
		{ strips: STRIPS, field }
	);

const holdFiles = (page) =>
	page.evaluate(() => new Promise((granted) => navigator.locks.request('datum-cli-files', () => (granted(), new Promise((release) => (window.release = release))))));
const release = (page) => page.evaluate(() => window.release());

const HISTORY_DOT = (field) => `${NOWVIEW} svg.svg circle.point-bg[data-field="${field}"]`;
/** The history's own dot of `field`, newest: is it shown. */
const historyDot = (page, field) => page.locator(HISTORY_DOT(field)).last().evaluate((dot) => getComputedStyle(dot).visibility);

describe('the now section is collect’s now strip', () => {
	for (const orientation of ['horizontal', 'vertical'])
		test(`a tap's dot is in both strips at once, enters at now's edge, crosses in half a second and leaves past the other (${orientation})`, async () => {
			await withApp(
				async (app) => {
					await seeded(app);
					const { page } = app;
					const seeds = await page.locator(HISTORY_DOT('coffee')).count();
					await holdFiles(page);
					// The tap's instant is the held now less the click's age as its handler reads it (`gestureAt`), real time,
					// so a busy page lays the dot that much short of now's edge (da-kzm2v). The age read before every handler and after.
					await page.evaluate(() => {
						const age = (e) => performance.now() - e.timeStamp;
						addEventListener('click', (e) => (window.ageBefore = age(e)), { capture: true, once: true });
						addEventListener('click', (e) => (window.ageAfter = age(e)), { once: true });
					});
					await page.locator(`${COLLECT} [data-field="coffee"]`).first().click();
					const dots = (seen) => Object.fromEntries(Object.entries(seen).map(([name, marks]) => [name, marks?.filter((m) => m.kind === 'circle') ?? []]));
					// One read of both: whichever drew it first, the other drew it in the same frame.
					const first = dots(await until(() => both(page, 'coffee'), (seen) => Object.values(dots(seen)).some((d) => d.length)));
					await release(page);
					const [before, after] = await page.evaluate(() => [window.ageBefore, window.ageAfter]);
					assert.ok(before !== undefined && after !== undefined, `the click's age was not read on both sides of the handlers (before=${before}, after=${after})`);
					for (const [name, [dot]] of Object.entries(first)) {
						assert.ok(dot, `the ${name} strip has no dot in the frame the other has it`);
						// Half a second a strip: the dot's instant is between the two ages short of now's edge.
						const [lo, hi] = [after, before].map((ms) => dot.of * (1 - ms / 500));
						assert.ok(dot.mid >= lo - 0.02 * dot.of && dot.mid <= hi + 0.02 * dot.of, `the ${name} dot enters at ${dot.mid} of ${dot.of}, not at the tap's instant (${hi.toFixed(2)}, ${before.toFixed(1)} ms short of now's edge)`);
					}
					// The layer holding the write landed: its dot in the history waits for the strip's.
					await until(() => page.locator(HISTORY_DOT('coffee')).count(), (n) => n === seeds + 1);
					assert.equal(await historyDot(page, 'coffee'), 'hidden', "the history's dot is drawn while the strip's crosses");
					await app.clock.forward(250);
					for (const name of Object.keys(STRIPS))
						await until(
							() => both(page, 'coffee').then((seen) => dots(seen)[name][0]),
							(d) => d && Math.abs(d.mid - d.of / 2) <= 3
						);
					// Past the far edge, and gone once wholly past it: a dot as big as its band makes it, longer in a short strip.
					await app.clock.forward(300);
					for (const name of Object.keys(STRIPS))
						await until(
							() => both(page, 'coffee').then((seen) => dots(seen)[name]),
							(d) => d.every((m) => m.mid < -0.05 * m.of)
						);
					await app.clock.forward(1000);
					await until(
						() => both(page, 'coffee').then(dots),
						(seen) => Object.values(seen).every((d) => d.length === 0)
					);
					// Past the strip, the dot is the history's, at the now-line.
					assert.equal(await historyDot(page, 'coffee'), 'visible', "the history's dot is not drawn once the strip's has crossed");
					assert.deepEqual(app.errors, []);
				},
				{ clock: todayAt(15), desk: desk(orientation), viewport: { width: 1000, height: 800 } }
			);
		});

	test('a state started wipes in from now’s edge in both strips over half a second, then is solid', async () => {
		await withApp(
			async (app) => {
				await seeded(app);
				const { page } = app;
				await page.locator(`${COLLECT} [data-field="work"]`).first().click();
				const hex = JSON.parse((await app.runLine('fields --json')).stdout).find((f) => f.field === 'work').color;
				const fill = `rgb(${[1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)).join(', ')})`;
				/** Each strip holds `field`'s colour from its now edge `want` px in, within `slack`. */
				const sweep = (share, slack = 3) =>
					until(
						() => both(page, 'work'),
						(seen) =>
							Object.values(seen).every((marks) =>
								marks?.some((m) => m.kind === 'rect' && m.fill === fill && Math.abs(m.to - m.of) <= 0.5 && Math.abs(m.to - m.from - share * m.of) <= slack)
							)
					);
				await sweep(0);
				// Nothing behind the wipe holds the new state whole: the CLI's chunk is what a press finds, drawn clear.
				// The tap's mark comes first: the chunk, once the layer holding the write lands.
				const chunks = await until(
					() => page.locator(`${NOWVIEW} svg.svg g.field[data-field="work"] .now-chunk`).evaluateAll((all) => all.map((c) => getComputedStyle(c).fillOpacity)),
					(all) => all.length > 0
				);
				assert.ok(chunks.every((o) => o === '0'), `the now section's chunks are painted: ${chunks}`);
				await app.clock.forward(250);
				await sweep(0.5);
				await app.clock.forward(250);
				await sweep(1, 0.5);
				await app.clock.forward(5000);
				await sweep(1, 0.5);
				assert.deepEqual(app.errors, []);
			},
			{ clock: todayAt(15), desk: desk(), viewport: { width: 1000, height: 800 } }
		);
	});
});

// vividn, 2026-11-06 (da-kqloo): «when turning a bar(on/off,change) off, that is drawn over another bar, the wipe in
// nowview and now section of collect widget make the bar drawn black, which is then visible on the bar beneath it. it
// should wipe transparent instead».
describe('an end wipes its own bar transparent', () => {
	/** `over` drawn across the middle of `under`, both on. */
	const STACKED = [
		at('under', 'start', 0.5, { height: 0.3, z_index: 1, color: '#2050e0' }),
		at('over', 'start', 0.55, { height: 0.1, z_index: 2, color: '#e03020' }),
		'start under',
		'start over'
	];

	/**
	 * What each strip paints three quarters of the way to now's edge, as `[r, g, b]` read from a screenshot: in
	 * `over`'s band, and in `under`'s alone.
	 */
	async function painted(page) {
		const points = await page.evaluate(
			(strips) =>
				Object.entries(strips).map(([name, [selector, of]]) => {
					const svg = document.querySelector(selector);
					const box = document.querySelector(of).getBoundingClientRect();
					const band = (field) => svg.querySelector(`g[data-mark="${field}"] rect.strip-block`).getBoundingClientRect();
					const [o, u] = [band('over'), band('under')];
					const x = box.left + 0.75 * box.width;
					return { name, over: [x, (o.top + o.bottom) / 2], under: [x, u.top + 0.85 * u.height] };
				}),
			STRIPS
		);
		const png = await page.screenshot();
		return page.evaluate(
			async ({ b64, points }) => {
				const img = await createImageBitmap(await (await fetch(`data:image/png;base64,${b64}`)).blob());
				const ctx = new OffscreenCanvas(img.width, img.height).getContext('2d');
				ctx.drawImage(img, 0, 0);
				const scale = img.width / innerWidth;
				const at = ([x, y]) => [...ctx.getImageData(Math.round(x * scale), Math.round(y * scale), 1, 1).data.slice(0, 3)];
				return Object.fromEntries(points.map(({ name, over, under }) => [name, { over: at(over), under: at(under) }]));
			},
			{ b64: png.toString('base64'), points }
		);
	}
	const apart = (a, b) => Math.max(...a.map((v, i) => Math.abs(v - b[i])));
	/** Each strip paints `under`'s colour in `over`'s band, or, `shows` false, does not. */
	const through = async (page, when, shows = true) => {
		for (const [name, { over, under }] of Object.entries(await painted(page)))
			assert.equal(apart(over, under) <= 12, shows, `${when}, the ${name} strip paints ${over} in over's band, under's being ${under}`);
	};

	test('in both strips, during the wipe and after it, the bar beneath shows where the ended bar was', async () => {
		await withApp(
			async (app) => {
				for (const line of STACKED) {
					assert.equal((await app.runLine(line)).code, 0, line);
					await app.clock.forward(60_000);
				}
				await app.reload();
				const { page } = app;
				await page.locator(STRIPS.nowview[0]).waitFor({ state: 'attached' });
				await page.locator(`${COLLECT} [data-field="over"]`).first().waitFor();
				await app.idle();
				await through(page, 'before the end', false);
				// The files held, the end waits: what is drawn is the tap's own.
				await holdFiles(page);
				await page.locator(`${COLLECT} [data-field="over"]`).first().click();
				await app.clock.forward(250);
				await through(page, 'halfway through the wipe');
				await app.clock.forward(500);
				await through(page, 'the wipe past');
				await release(page);
				await until(async () => (await app.runLine('get over::state')).stdout.trim(), (s) => s === 'false');
				assert.deepEqual(app.errors, []);
			},
			{ clock: todayAt(15), desk: desk(), viewport: { width: 1000, height: 800 } }
		);
	});
});

describe('a dot at the newest instant of the history', () => {
	/** Log `line` on the bar, and wait for `field`'s dot in the history. */
	async function log(app, line, field) {
		const input = app.page.locator('.prompt-bar input');
		await input.fill(line);
		await input.press('Enter');
		await app.page.locator(`${NOWVIEW} svg.svg circle.point-bg[data-field="${field}"]`).last().waitFor({ state: 'attached' });
	}

	test('is whole, over the edge, once the strip has passed it on: a press past the now-line finds it', async () => {
		await withApp(
			async (app) => {
				await log(app, 'moment coffee', 'coffee');
				await app.clock.forward(600);
				await until(() => historyDot(app.page, 'coffee'), (v) => v === 'visible');
				const seen = await app.page.locator(`${NOWVIEW} svg.svg circle.point-bg[data-field="coffee"]`).evaluate((mark) => {
					const r = mark.getBoundingClientRect();
					const svg = mark.ownerSVGElement;
					const now = new DOMPoint(Number(svg.querySelector('.current-time-line').dataset.now), 0).matrixTransform(svg.getScreenCTM()).x;
					const hit = document.elementFromPoint(r.right - 1, r.top + r.height / 2);
					return { center: r.left + r.width / 2, right: r.right, now, hit: hit === mark || `${hit?.tagName}.${hit?.getAttribute('class')}`, clipped: !!mark.closest('[clip-path]') };
				});
				assert.ok(Math.abs(seen.center - seen.now) < 1, `the dot is at ${seen.center}, now at ${seen.now}`);
				assert.ok(seen.right > seen.now + 2, `the dot's right edge ${seen.right} is not past now at ${seen.now}, so nothing was checked`);
				assert.equal(seen.clipped, false, 'the dot is under a clip');
				assert.equal(seen.hit, true, 'the dot is not what is drawn at its right edge');
				assert.deepEqual(app.errors, []);
			},
			{ clock: todayAt(15), desk: desk() }
		);
	});

	test('one still to come waits behind the clip at the now-line', async () => {
		await withApp(
			async (app) => {
				await log(app, 'moment coffee -t +5', 'coffee');
				const clip = await app.page.locator(`${NOWVIEW} svg.svg circle.point-bg[data-field="coffee"]`).evaluate((mark) => mark.closest('[clip-path^="url(#flow-past"]') !== null);
				assert.equal(clip, true, 'a dot five minutes ahead stands over the now section');
				assert.deepEqual(app.errors, []);
			},
			{ clock: todayAt(15), desk: desk() }
		);
	});
});
