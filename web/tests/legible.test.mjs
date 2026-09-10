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
import { startApp, stopApp, withApp } from './support/app.mjs';

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

/** The drawing a pane is showing, by the pane's title. */
function paneImage({ page }, title) {
	return page
		.locator('section.pane', { has: page.locator('h2', { hasText: title }) })
		.locator('img.svg');
}

/** The drawing the newest command left in the REPL's scrollback. */
function replImage({ page }) {
	return page.locator('.repl img.svg').last();
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

describe('a drawn view', () => {
	for (const { name, viewport } of VIEWPORTS) {
		test(`is legible in the dayview pane on ${name}`, async () => {
			await withApp(
				async (app) => {
					await logADay(app);

					const shown = await measure(paneImage(app, 'dayview'));
					assertLabelled(shown, assertLegible(shown, `dayview pane at ${viewport.width}px`));
				},
				{ viewport }
			);
		});

		test(`is legible as dayview typed into the repl on ${name}`, async () => {
			await withApp(
				async (app) => {
					await logADay(app);
					await app.submit('dayview');

					const shown = await measure(replImage(app));
					assertLabelled(shown, assertLegible(shown, `dayview in the repl at ${viewport.width}px`));
				},
				{ viewport }
			);
		});

		// nowview used to pass on its own: authored at 520px, it was only
		// scaled to 0.66 on a phone, so its 14px labels still landed at
		// 10.6. Only "still legible", though — it WAS being scaled, and
		// da-45wt stopped scaling it, here as in its pane. It stays
		// measured because it shares the REPL's render path with dayview,
		// so a fix there could take it down silently. `-o -`, because bare
		// `nowview` is the CLI's terminal band, not a picture (da-57dj).
		test(`is legible as nowview typed into the repl on ${name}`, async () => {
			await withApp(
				async (app) => {
					await logADay(app);
					await app.submit('nowview -o -');

					assertLegible(await measure(replImage(app)), `nowview in the repl at ${viewport.width}px`);
				},
				{ viewport }
			);
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
		// 400 since da-nk84 carved the now strip out of the canvas rather
		// than adding a panel's width to it.
		[['nowview', '-o', '-'], 400]
	]) {
		test(`is still the CLI’s ${width}px ${argv[0]} canvas when the argv asks for nothing`, async () => {
			await withApp(async (app) => {
				await logADay(app);
				const drawn = await app.run(argv);
				assert.equal(drawn.code, 0, drawn.stderr);
				assert.match(drawn.stdout, new RegExp(`<svg[^>]*\\swidth="${width}"`));
				assert.match(drawn.stdout, new RegExp(`<svg[^>]*\\sviewBox="0 0 ${width} `));
			});
		});
	}
});
