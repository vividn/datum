// Whether a drawn view's labels are big enough to read — in the pane it
// has, and in the REPL it is typed into.
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
// Fixing the pane did not fix the REPL — different components, and only
// one of them was ever measured. `dayview` typed at the prompt still
// arrived at 0.46 on a laptop and 0.17 on a phone: 4.12px and 1.55px
// (da-wexl). So each surface is measured on its own here, and nowview
// rides along because a change to how a drawn view is asked for reaches
// it too.
//
// The assertion is not "labels exist" and not "the markup says 9". It is
// the arithmetic that caught it: the smallest font-size in the SVG
// multiplied by the scale the browser is showing that SVG at. Nothing
// short of that number fails on the builds these were written against — a
// screenshot did not, twice.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp } from './support/app.mjs';
import { resolveLibAsSource } from './support/libhook.mjs';

before(startApp);
after(stopApp);

/**
 * The floor, in on-screen pixels.
 *
 * 9 is the dayview's own authored axis size, so meeting the floor is the
 * band being shown at the size datum drew it.
 */
const FLOOR = 9;

/** The viewports this has to hold at: a phone, and a laptop. */
const VIEWPORTS = [
	{ name: 'a phone', viewport: { width: 390, height: 844 } },
	{ name: 'a 1440px laptop', viewport: { width: 1440, height: 900 } }
];

/** Enough of a day to draw: a state, a block and a point. */
const A_DAY = [
	['switch', 'mood', 'good', '-t', '0930'],
	['start', 'work', '-t', '0800'],
	['stop', 'work', '-t', '1200'],
	['occur', 'coffee', '-t', '0700']
];

async function logADay(app) {
	for (const argv of A_DAY) {
		const result = await app.run(argv);
		assert.equal(result.code, 0, `${argv.join(' ')} → ${result.stderr}`);
	}
}

/** One drawing as it is actually on screen: its markup, and its scale. */
async function measure(img) {
	await img.waitFor();
	const svg = await img.evaluate((el) => el.outerHTML);
	// The `width` attribute is the canvas the SVG declares; the bounding
	// box is the room it got. Their ratio is what every glyph is
	// multiplied by.
	const { natural, rendered } = await img.evaluate((el) => ({
		natural: el.width.baseVal.value,
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

/**
 * The drawing a pane is showing, by the pane's title — the last one in
 * it, which in the day strip is today's band under the stacked days its
 * box holds (da-zb0d). The stacked bands are drawn at the same width and
 * day height, so today's legibility is theirs.
 */
function paneImage({ page }, title) {
	return page
		.locator('section.pane', { has: page.locator('h2', { hasText: title }) })
		.locator('svg.svg')
		.last();
}

/** The drawing the newest command left in the REPL's scrollback. */
function replImage({ page }) {
	return page.locator('.repl svg.svg').last();
}

/** Assert a drawing clears the floor, reporting the arithmetic either way. */
function assertLegible(shown, where) {
	const effective = shown.smallest * shown.scale;
	const sizes = `${where}: ${shown.natural} → ${shown.rendered.toFixed(1)} (×${shown.scale.toFixed(3)}), smallest ${shown.smallest} → ${effective.toFixed(2)}px`;
	assert.ok(effective >= FLOOR, `smallest text under ${FLOOR}px — ${sizes}`);
	return sizes;
}

/**
 * The band's labels are on it at all.
 *
 * Checked alongside every size assertion, because a band that dropped its
 * axis entirely would satisfy any floor.
 */
function assertLabelled(shown, sizes) {
	assert.equal(shown.dateLines, 3, `date stamp missing — ${sizes}`);
	assert.ok(shown.ticks >= 4, `only ${shown.ticks} hour labels — ${sizes}`);
}

// ONE APP per screen (da-6b3dj, sharedApp in support/app.mjs): `solo`
// empties the database each test logged a day into, and a REPL drawing is
// read from the end of the scrollback, which is where the newest is.
const shared = sharedApp();

describe('a drawn view', () => {
	for (const { name, viewport } of VIEWPORTS) {
		const screen = sharedApp({ viewport });
		// The strip's pane is the one place the labels are NOT datum's since
		// da-iu9i: it asks for bands only and writes the date stamps and
		// the hour ruler itself. So the claim splits in two — the picture
		// arrives at the size it was drawn (a scaled band is what made the
		// text illegible in the first place), and the strip's own text
		// clears the same floor.
		test(`is legible in the dayview pane on ${name}`, async () => {
			await screen.solo(async (app) => {
				await logADay(app);

				const shown = await measure(paneImage(app, 'dayview'));
				const sizes = `dayview pane at ${viewport.width}px: ${shown.natural} → ${shown.rendered.toFixed(1)} (×${shown.scale.toFixed(3)})`;
				assert.ok(Math.abs(shown.scale - 1) <= 0.005, `the band is scaled — ${sizes}`);
				assert.equal(shown.dateLines, 0, `the band still carries date labels — ${sizes}`);
				assert.equal(shown.ticks, 0, `the band still carries the hour axis — ${sizes}`);

				const drawn = await app.page.evaluate(() => {
					const strip = document.querySelector('.strip');
					const stamp = strip.querySelector('.row.pinned .stamp');
					const labels = [...strip.querySelectorAll('.axis text.tick-label')];
					const px = (el) => parseFloat(getComputedStyle(el).fontSize);
					return {
						lines: stamp.querySelectorAll('span').length,
						stamp: px(stamp),
						ticks: labels.length,
						tick: labels.length ? Math.min(...labels.map(px)) : 0
					};
				});
				assert.equal(drawn.lines, 3, `date stamp missing — ${sizes}`);
				assert.ok(drawn.ticks >= 4, `only ${drawn.ticks} hour labels — ${sizes}`);
				assert.ok(drawn.stamp >= FLOOR, `the date stamp is ${drawn.stamp}px — ${sizes}`);
				assert.ok(drawn.tick >= FLOOR, `the hour labels are ${drawn.tick}px — ${sizes}`);
			});
		});

		test(`is legible as dayview typed into the repl on ${name}`, async () => {
			await screen.solo(async (app) => {
				await logADay(app);
				await app.submit('dayview');

				const shown = await measure(replImage(app));
				assertLabelled(shown, assertLegible(shown, `dayview in the repl at ${viewport.width}px`));
			});
		});

		// nowview used to pass on its own: authored at 520px, it was only
		// scaled to 0.66 on a phone, so its 14px labels still landed at
		// 10.6. Only "still legible", though — it WAS being scaled, and
		// da-45wt stopped scaling it, here as in its pane. It stays
		// measured because it shares the REPL's render path with dayview,
		// so a fix there could take it down silently. `-o -`, because bare
		// `nowview` is the CLI's terminal band, not a picture (da-57dj).
		test(`is legible as nowview typed into the repl on ${name}`, async () => {
			await screen.solo(async (app) => {
				await logADay(app);
				await app.submit('nowview -o -');

				assertLegible(await measure(replImage(app)), `nowview in the repl at ${viewport.width}px`);
			});
		});
	}

	// The screen's needs reach the command as argv or not at all. `datum
	// dayview` writes a 2000px canvas to a file, and a bare argv through the
	// seam is that same command — so a surface that wants its own size says
	// so, rather than the seam deciding for every caller (trap 2 on da-wexl).
	// True of nowview since da-45wt made it sized too: a pane and a prompt
	// now both name a size, and the bare command still means the default.
	for (const [argv, width] of [
		[['dayview'], 2000],
		// 100 since da-7q8r put nowview's default back to the SNAPSHOT: no
		// context window, so no time band to size a canvas for. A window
		// asked for is 400 — da-nk84 carved the now strip out of that
		// canvas rather than adding a panel's width to it.
		[['nowview', '-o', '-'], 100],
		[['nowview', '--context', '30m', '-o', '-'], 400]
	]) {
		test(`is still the CLI’s ${width}px ${argv[0]} canvas when the argv asks for nothing`, async () => {
			await shared.solo(async (app) => {
				await logADay(app);
				const drawn = await app.run(argv);
				assert.equal(drawn.code, 0, drawn.stderr);
				assert.match(drawn.stdout, new RegExp(`<svg[^>]*\\swidth="${width}"`));
				assert.match(drawn.stdout, new RegExp(`<svg[^>]*\\sviewBox="0 0 ${width} `));
			});
		});
	}

	// The snapshot's floor, which da-7q8r re-derived: with the labels gone
	// there is nothing left to READ along a block, so what the width must
	// buy is a target a finger can land on. `SNAPSHOT_WIDTH` in
	// `$lib/layout/registry.ts` is `2 * REACH_PX + 2 * SVG_MARGIN_PX`;
	// this is that arithmetic measured on the band the CLI actually draws,
	// rather than the constant read back to itself.
	test('at the snapshot’s floor a block is still a whole fingertip across', async () => {
		// registry.ts reaches Svelte components node cannot load; the hook
		// stubs them and reads `$lib/x.js` as the source's `x.ts`.
		resolveLibAsSource(import.meta.url);
		const { KINDS } = await import('../src/lib/layout/registry.ts');
		const { REACH_PX } = await import('../src/lib/lanes.ts');
		const floor = KINDS.nowview.legible({ history: '0' }).width;

		await shared.solo(async (app) => {
			assert.equal((await app.run(['switch', 'mood', 'happy'])).code, 0);
			const drawn = await app.run(['nowview', '-o', '-', '--width', String(floor)]);
			assert.equal(drawn.code, 0, drawn.stderr);
			const band = /<rect class="now-chunk"[^>]*\swidth="([\d.]+)"/.exec(drawn.stdout);
			assert.ok(band, `no block at the floor width ${floor}: ${drawn.stdout}`);
			assert.ok(
				Number(band[1]) >= 2 * REACH_PX,
				`a block ${band[1]}px across at the ${floor}px floor, under a ${2 * REACH_PX}px fingertip`
			);
		});
	});
});
