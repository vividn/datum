// Whether the dayview band's labels are big enough to read.
//
// They were not, and every earlier pass missed it because the labels are
// all *there*: 27 `<text>` nodes, the hour row and the three-line date
// stamp, exactly as the CLI writes them. What was wrong was the size they
// arrived on screen at. `datum dayview` authors a 2000px canvas — the
// right size for a .svg on disk — and the pane scaled it to fit: 0.17 on a
// phone, which is a 9px hour label rendered at 1.55px, and 0.70 on a
// 1440px laptop, which is 6.28px. Both illegible; the second is the one
// that makes this not a phone bug (da-m0cj).
//
// So the assertion here is not "labels exist" and not "the markup says 9".
// It is the arithmetic that caught it: the smallest font-size in the SVG
// multiplied by the scale the browser is showing that SVG at. Nothing
// short of that number fails on the build this test was written against —
// a screenshot did not, twice.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';

before(startApp);
after(stopApp);

/**
 * The floor, in on-screen pixels.
 *
 * nowview clears it already — it is authored near its display size, which
 * is the whole difference — at 10.6px on a phone and 13.8 on a laptop. 9
 * is the dayview's own authored axis size, so meeting the floor is the
 * band being shown at the size datum drew it.
 */
const FLOOR = 9;

/** The viewports this has to hold at: a phone, and a laptop. */
const VIEWPORTS = [
	{ name: 'a phone', viewport: { width: 390, height: 844 } },
	{ name: 'a 1440px laptop', viewport: { width: 1440, height: 900 } }
];

/** The dayview band as it is actually on screen: its markup and its scale. */
async function band({ page }) {
	const img = page
		.locator('section.pane', { has: page.locator('h2', { hasText: 'dayview' }) })
		.locator('img.svg');
	await img.waitFor();
	const svg = decodeURIComponent(
		(await img.getAttribute('src')).replace(/^data:image\/svg\+xml,/, '')
	);
	// `naturalWidth` is the canvas the SVG declares; the bounding box is
	// the room it got. Their ratio is what every glyph is multiplied by.
	const { natural, rendered } = await img.evaluate((el) => ({
		natural: el.naturalWidth,
		rendered: el.getBoundingClientRect().width
	}));
	const fonts = [...svg.matchAll(/font-size="([\d.]+)"/g)].map((m) => Number(m[1]));
	return {
		svg,
		natural,
		rendered,
		scale: rendered / natural,
		smallest: Math.min(...fonts),
		ticks: [...svg.matchAll(/class="tick-label"/g)].length,
		dateLines: [...svg.matchAll(/class="date-label"/g)].length
	};
}

describe('the dayview band', () => {
	for (const { name, viewport } of VIEWPORTS) {
		test(`is legible on ${name}`, async () => {
			await withApp(
				async (app) => {
					for (const argv of [
						['switch', 'mood', 'good', '-t', '0930'],
						['start', 'work', '-t', '0800'],
						['stop', 'work', '-t', '1200'],
						['occur', 'coffee', '-t', '0700']
					]) {
						const result = await app.run(argv);
						assert.equal(result.code, 0, `${argv.join(' ')} → ${result.stderr}`);
					}

					const shown = await band(app);
					const effective = shown.smallest * shown.scale;
					const where = `${viewport.width}px: ${shown.natural} → ${shown.rendered.toFixed(1)} (×${shown.scale.toFixed(3)}), smallest ${shown.smallest} → ${effective.toFixed(2)}px`;

					// The labels have to be on the band before their size is
					// worth measuring — a band that dropped its axis entirely
					// would satisfy any floor.
					assert.equal(shown.dateLines, 3, `date stamp missing — ${where}`);
					assert.ok(shown.ticks >= 4, `only ${shown.ticks} hour labels — ${where}`);

					assert.ok(effective >= FLOOR, `smallest text under ${FLOOR}px — ${where}`);
				},
				{ viewport }
			);
		});
	}
});
