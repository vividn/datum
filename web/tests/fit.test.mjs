// Whether a pane fits what it is showing, or grows a scrollbar over it.
//
// A pane scrolls when its content is bigger than its box, and that is
// right when the content is a day's worth of lines and wrong when the
// content is one picture. Two things in the app were the second kind
// (da-45wt):
//
//   nowview authored a 520x276 canvas. `max-width: 100%` fitted the WIDTH
//   to the pane and `height: auto` then put the height wherever the
//   aspect ratio put it — 209px into a 273px box at 1280, which is why it
//   "always read correctly", and 183px into a 94px box on a phone, which
//   is a vertical scrollbar over a picture that could simply have been
//   drawn shorter.
//
//   Every CLI text block carried `min-width: 80ch` = 674px, so the tail
//   pane and the REPL scrolled SIDEWAYS at every viewport — 1280 included
//   — past blank space, because their lines are nothing like 80 columns
//   long.
//
// So the assertions split by what the content IS, not by which pane holds
// it. A drawn view is a picture and may be drawn at any size: it must fit
// both ways, always. A text block is as tall as its line count and as
// wide as its longest line, so its height is content — scrollback grows
// downward, that is what scrollback is — but its WIDTH must not exceed
// the pane over text that isn't there.
//
// The drawings are inline `<svg class="svg">` (da-s5da); their markup is
// read off the element, their authored width off its `width` attribute.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { devices } from 'playwright';
import { startApp, stopApp, withApp } from './support/app.mjs';

before(startApp);
after(stopApp);

/**
 * The three screens. 1280x900 is the control — it is the size the app was
 * built on, so anything failing there was failing in front of everyone.
 * 900x600 is a narrowed desktop, still above the 720px phone breakpoint.
 * The phone is Playwright's own iPhone 14 profile, taken whole.
 */
const SCREENS = [
	{ name: '1280x900', viewport: { width: 1280, height: 900 } },
	{ name: '900x600', viewport: { width: 900, height: 600 } },
	{ name: 'an iPhone 14', device: devices['iPhone 14'] }
];

/** The two panes whose content is a drawing datum can author at any size. */
const DRAWN = ['dayview', 'nowview'];

/** Enough of a day that every pane has something in it. */
const A_DAY = [
	'occur coffee -t 0700',
	'start work -t 0800',
	'switch mood good -t 0930',
	'switch posture standing -t 1100',
	'stop work -t 1200'
];

/**
 * Log the day through the REPL, then wait for the panes to catch up.
 *
 * Through the REPL, not through `run`: the panes re-run their command on
 * the frame's epoch, which only a command typed at the prompt bumps —
 * `run(argv)` stores the row and the panes never hear about it. A test on
 * the seam alone would measure four panes still showing the empty day
 * they mounted with, which is the one arrangement where nothing
 * overflows.
 *
 * And then the wait, because `submit` returns on the REPL's own echo: the
 * pane commands it set off are still in flight, so a measurement taken
 * straight after it can be of the pane as it was an entry ago. That is
 * not hypothetical — it read a `work: true` on a `work` this day had
 * already stopped. Settled used to mean two identical reads 200ms apart,
 * which held while the frame ran three commands per entry and stopped
 * holding once the day strip drew a band per day its box holds
 * (da-zb0d): the runs queue behind one database lock, and a pane can sit
 * unchanged for longer than that while its turn comes. `idle` counts the
 * runs themselves.
 */
async function logADay(app) {
	for (const line of A_DAY) await app.submit(line);
	await app.idle();
}

/**
 * Bring a pane to the front, where it is a tab.
 *
 * The phone's default layout ($lib/layout/defaults.ts) puts nowview
 * behind tail as two tabs of one pane, so a phone measures nowview by
 * showing it first. On a desktop there is no such tab and this is a
 * no-op. A pane's title is its `data-title` (the h2 with one tab, the
 * active tab's label with more) — which is what the readers below use.
 */
async function show(app, title) {
	const tab = app.page.locator('[role="tab"]', { hasText: title });
	if ((await tab.count()) === 0) return;
	await tab.click();
	await app.page.locator(`section.pane[data-title="${title}"] svg.svg`).waitFor();
}

/** Every pane's box against what is inside it, by title. */
function boxes({ page }) {
	return page.evaluate(() =>
		Object.fromEntries(
			[...document.querySelectorAll('section.pane')].map((pane) => {
				const body = pane.querySelector('.body');
				return [
					pane.dataset.title,
					{
						scrollWidth: body.scrollWidth,
						clientWidth: body.clientWidth,
						scrollHeight: body.scrollHeight,
						clientHeight: body.clientHeight,
						// How far the CLI text the pane holds reaches into the scroll
						// box: each `pre` is as wide as its longest line and no wider
						// (CliOutput.svelte), measured from the box's scroll origin, plus
						// the box's own end padding, which the scrollable overflow
						// includes. This is how far the pane may legitimately scroll.
						textExtent: (() => {
							const origin = body.getBoundingClientRect().left - body.scrollLeft;
							const pad = parseFloat(getComputedStyle(body).paddingRight) || 0;
							const reach = [...body.querySelectorAll('pre')].map(
								(pre) => pre.getBoundingClientRect().left - origin + pre.scrollWidth
							);
							return Math.max(0, ...reach) + pad;
						})()
					}
				];
			})
		)
	);
}

/** What a pane's drawing says, off the `src`, by SVG class. */
async function drawn(app, title) {
	const svg = await app.paneSvg(title);
	const texts = (cls) =>
		[...svg.matchAll(new RegExp(`<text class="${cls}"[^>]*>([^<]*)<`, 'g'))].map((m) => m[1]);
	return {
		// Which field is in which state now, since da-nk84: not a labelled
		// panel row but the chunk each lane carries past the now-line, read
		// as `field → fill`.
		states: [...svg.matchAll(/<g class="field" data-field="([^"]*)">(.*?)<\/g>/g)]
			.map(([, field, group]) => [
				field,
				/<rect class="now-chunk[^"]*"[^>]*fill="([^"]*)"/.exec(group)?.[1]
			])
			.filter(([, fill]) => fill !== undefined)
			.map(([field, fill]) => `${field}=${fill}`),
		ticks: texts('tick-label'),
		clock: texts('current-time-text'),
		fonts: [...svg.matchAll(/font-size="([\d.]+)"/g)].map((m) => Number(m[1]))
	};
}

describe('a pane', () => {
	for (const { name, viewport, device } of SCREENS) {
		test(`shows its drawn views whole on ${name}`, async () => {
			await withApp(
				async (app) => {
					await logADay(app);
					for (const title of DRAWN) {
						await show(app, title);
						const seen = await boxes(app);
						const box = seen[title];
						const said = `${title} on ${name}: ${box.scrollWidth}x${box.scrollHeight} of content in ${box.clientWidth}x${box.clientHeight}`;
						assert.ok(box.scrollHeight <= box.clientHeight, `scrolls down — ${said}`);
						assert.ok(box.scrollWidth <= box.clientWidth, `scrolls sideways — ${said}`);
					}
				},
				{ viewport, device }
			);
		});

		test(`never scrolls sideways over text it hasn't got on ${name}`, async () => {
			await withApp(
				async (app) => {
					await logADay(app);
					for (const [title, box] of Object.entries(await boxes(app))) {
						// Text the pane HAS may run wider than a phone — the CLI's
						// `CREATE <hid> <slug>` line does — and then the pane scrolls
						// to it, by design (CliOutput.svelte: no reflow). What it must
						// never do is scroll over blank space past its longest line.
						assert.ok(
							box.scrollWidth <= Math.max(box.clientWidth, box.textExtent),
							`${title} on ${name} scrolls sideways over text it hasn't got: ${box.scrollWidth} of content in ${box.clientWidth}, text reaches ${box.textExtent}`
						);
					}
				},
				{ viewport, device }
			);
		});

		// Fitting a picture to a pane by SCALING it is the other way to
		// stop the scrollbar, and it is the one da-m0cj already rejected:
		// the drawing shrinks and every glyph in it shrinks too. So the
		// canvas the command authored has to be the canvas on screen, to
		// the pixel, at every width — which is only true if the pane
		// ASKED for that size rather than squeezing what came back.
		test(`draws its views at 1:1, never scaled, on ${name}`, async () => {
			await withApp(
				async (app) => {
					await logADay(app);
					await show(app, 'nowview');
					const sizes = await app.page.evaluate(() =>
						Object.fromEntries(
							[...document.querySelectorAll('section.pane')]
								.filter((p) => p.querySelector('svg.svg'))
								.map((p) => {
									const img = p.querySelector('svg.svg');
									return [
										p.dataset.title,
										{ natural: img.width.baseVal.value, shown: img.getBoundingClientRect().width }
									];
								})
						)
					);
					for (const title of DRAWN) {
						const { natural, shown } = sizes[title];
						assert.ok(
							Math.abs(shown - natural) <= 1,
							`${title} on ${name} authored ${natural}px and is shown at ${shown.toFixed(1)}px — ×${(shown / natural).toFixed(3)}`
						);
					}
				},
				{ viewport, device }
			);
		});
	}

	// Fitting the picture to the pane by blanking it would satisfy every
	// assertion above. The current state is what nowview is FOR — which
	// field is in which state right now — so it has to carry the same
	// chunks past the now-line on a phone that it carries on a desktop,
	// plus its clock and an axis to read the bands against.
	//
	// The axis is counted, not compared: how many ticks fit is a function
	// of the plot's width, and `nowview_axis_ticks` gets to answer that
	// for the width it was actually given. Asking for the same tick count
	// at 266px as at 393px would be asking for the picture NOT to have
	// been drawn at the size it is shown at, which is the fix.
	test('says the same thing however narrow it gets', async () => {
		let wide;
		for (const { name, viewport, device } of SCREENS) {
			await withApp(
				async (app) => {
					await logADay(app);
					await show(app, 'nowview');
					const shown = await drawn(app, 'nowview');
					assert.ok(shown.ticks.length >= 1, `nowview on ${name} drew no axis`);
					// Counted, not compared: the three screens are three runs
					// of the app, and the clock is the wall clock — comparing
					// its TEXT fails whenever a run straddles a minute.
					assert.equal(shown.clock.length, 1, `nowview on ${name} drew no clock`);
					if (wide === undefined) {
						wide = shown;
						assert.ok(wide.states.length > 0, 'nowview drew no current state even at 1280');
					} else {
						assert.deepEqual(
							shown.states,
							wide.states,
							`nowview on ${name} lost a current state: ${JSON.stringify(shown.states)}`
						);
					}
					// What is left to read once the panel is gone is the
					// axis and the clock, and both have to stay legible in a
					// cramped band: a pane 78px tall gets the picture drawn
					// small rather than half of it drawn big behind a
					// scrollbar. Asserted so a change that draws text
					// smaller than datum ever does has to say so out loud.
					assert.ok(
						Math.min(...shown.fonts) >= 6,
						`nowview on ${name} drew ${Math.min(...shown.fonts)}px text`
					);
				},
				{ viewport, device }
			);
		}
	});

	// The REPL's prompt line grew two things at once (da-9ozz): a live
	// clock to the left of the caret, and a scrollback that survives a
	// reload. Either could take the input's width on a phone — the clock
	// by sitting beside it, the scrollback by being wide enough to widen
	// the pane under it. A command line you cannot see what you typed in
	// is not a command line, so the clock drops its seconds down here and
	// the input keeps the room.
	// And the input widget (da-0dhc) is the same prompt line, its history
	// taking the place of the scrollback — so it is held to the same width.
	const PROMPTS = [
		{ what: 'the REPL input', path: '/', line: '.repl .line', frame: '.repl section.pane' },
		{ what: "the input widget's input", path: '/input', line: '.input-widget .line', frame: '.input-widget' }
	];

	for (const { what, path, line: lineSelector, frame } of PROMPTS) {
		test(`keeps ${what} wide on a 360px phone, ten commands of scrollback and all`, async () => {
			await withApp(
				async (app) => {
					const { page } = app;
					if (path !== '/') await app.goto(path);
					const clock = await page.locator(`${lineSelector} .stamp`).innerText();
					assert.match(clock, /^\d{2}:\d{2}$/, `the prompt clock kept its seconds at 360px: ${clock}`);

					const { input, line, pane } = await page.evaluate(
						([lineSelector, frame]) => {
							const form = document.querySelector(lineSelector);
							return {
								input: form.querySelector('input').getBoundingClientRect().width,
								line: form.getBoundingClientRect().width,
								pane: document.querySelector(frame).getBoundingClientRect().width
							};
						},
						[lineSelector, frame]
					);
					assert.ok(
						input >= line * 0.6,
						`the prompt gave the input ${input.toFixed(0)}px of a ${line.toFixed(0)}px line`
					);
					assert.ok(line <= pane + 1, `the prompt line runs past its pane: ${line} in ${pane}`);
				},
				{
					viewport: { width: 360, height: 640 },
					prepare: (context) =>
						context.addInitScript((key) => {
							const ranAt = Date.now();
							localStorage.setItem(
								key,
								JSON.stringify({
									scrollback: Array.from({ length: 10 }, (_, i) => ({
										line: `occur coffee cups=${i}`,
										ranAt: ranAt - (10 - i) * 60_000,
										result: {
											argv: ['occur', 'coffee', `cups=${i}`],
											code: 0,
											stdout: `CREATE 0192f3c4d5e6 coffee:2026-09-09T0${i}:00:00Z`,
											stderr: ''
										}
									})),
									history: ['occur coffee cups=9']
								})
							);
						}, 'datum.repl.v1')
				}
			);
		});
	}

	// The drawn views are re-REQUESTED at the pane's size rather than
	// scaled into it, so the size has to be re-asked on every resize and
	// not only on mount — otherwise a window drag leaves the old canvas
	// stretched, which is half of "sometimes" in the report. The canvas
	// the browser holds is the command's own answer, so this reads the
	// `width` it declares: a stretched picture keeps its declared size and
	// only a re-run changes it.
	test('redraws both views when the window changes, with no reload', async () => {
		await withApp(
			async (app) => {
				await logADay(app);
				const canvases = () =>
					app.page.evaluate(() =>
						Object.fromEntries(
							[...document.querySelectorAll('section.pane')]
								.filter((p) => p.querySelector('svg.svg'))
								.map((p) => [
									p.dataset.title,
									p.querySelector('svg.svg').width.baseVal.value
								])
						)
					);
				const before = await canvases();
				await app.page.setViewportSize({ width: 800, height: 600 });
				await app.page.waitForFunction(
					(was) =>
						[...document.querySelectorAll('section.pane')]
							.filter((p) => p.querySelector('svg.svg'))
							.every(
								(p) =>
									p.querySelector('svg.svg').width.baseVal.value !==
									was[p.dataset.title]
							),
					before
				);
				const after = await canvases();
				for (const title of DRAWN) {
					assert.ok(after[title] < before[title], `${title} kept its ${before[title]}px canvas`);
				}
			},
			{ viewport: { width: 1280, height: 900 } }
		);
	});
});

// The tail pane is the one pane whose content is a grid, and a grid is
// the thing a terminal fits by PADDING: `datum tail` aligns its columns
// to a column count, and the browser showing that padded text scrolled
// sideways over the spaces (da-6ec2). The pane lays the CLI's own cells
// out in a real `<table>` instead, so the claim to check is the strict
// one — it fits, at every width, with a full listing in it.
describe('the tail pane', () => {
	/** The four widths the report named, narrowest first. */
	const WIDTHS = [360, 390, 768, 1280];

	/**
	 * Ten rows — `datum tail`'s own default count — including a state
	 * long enough that nothing but wrapping can fit it. Ten is what makes
	 * this a test of a full pane rather than of an empty one.
	 */
	const TEN_ROWS = [
		'occur coffee -t 0700',
		'start work -t 0800',
		'switch mood happy -t 0900',
		'switch posture standing -t 0930',
		'occur nap dur=30m -t 1000',
		'switch mood contemplative-about-the-weather -t 1030',
		'switch project datum-day-tail-table -t 1100',
		'occur water -t 1130',
		'switch posture sitting -t 1200',
		'stop work -t 1230'
	];

	for (const width of WIDTHS) {
		test(`fits ten rows at ${width}px`, async () => {
			await withApp(
				async (app) => {
					for (const line of TEN_ROWS) await app.submit(line);
					// `submit` returns on the REPL's own echo; the pane's
					// re-read is still in flight behind it, so a measurement
					// taken straight after can be of the pane an entry ago.
					await app.page.waitForFunction(
						(n) => document.querySelectorAll('section.pane tbody tr').length === n,
						TEN_ROWS.length
					);
					const table = await app.paneTable('tail');
					assert.equal(table.rows.length, 10, 'the pane is not showing the full listing');

					const box = await app.page.evaluate(() => {
						const body = [...document.querySelectorAll('section.pane')]
							.find((p) => p.dataset.title === 'tail')
							.querySelector('.body');
						return { scrollWidth: body.scrollWidth, clientWidth: body.clientWidth };
					});
					assert.ok(
						box.scrollWidth <= box.clientWidth,
						`the tail pane scrolls sideways at ${width}px: ${box.scrollWidth} of table in ${box.clientWidth}`
					);

					// Fitting by dropping the columns would satisfy that. The
					// time column is the one that may never wrap or go — a
					// clock split across two lines is unreadable, and a row
					// without one says nothing about when.
					assert.ok(table.keys.includes('time'), `no time column at ${width}px`);
					const heights = await app.page.evaluate(() =>
						[...document.querySelectorAll('section.pane td.time')].map(
							(td) => td.getClientRects().length
						)
					);
					assert.deepEqual(
						[...new Set(heights)],
						[1],
						`a time cell wrapped at ${width}px: ${JSON.stringify(heights)}`
					);
				},
				{ viewport: { width, height: 900 } }
			);
		});
	}
});

// The shell (da-0a5o): the root bar above and the bottom bar below are on
// screen at every width, the prompt keeps most of its bar, and nothing —
// bar, page or body — scrolls sideways. The four widths the tail pane is
// held to, so the bars are measured where the panes are.
describe('the shell', () => {
	for (const width of [360, 390, 768, 1280]) {
		test(`the root bar and the bottom bar fit at ${width}px`, async () => {
			await withApp(
				async (app) => {
					await logADay(app);
					const seen = await app.page.evaluate(() => {
						const box = (s) => document.querySelector(s).getBoundingClientRect();
						const root = box('.root-bar');
						const bar = box('.prompt-bar');
						const input = box('.prompt-bar input');
						return {
							screen: { w: document.documentElement.clientWidth, h: document.documentElement.clientHeight },
							root: { top: root.top, bottom: root.bottom, right: root.right },
							bar: { top: bar.top, bottom: bar.bottom, right: bar.right, width: bar.width },
							input: input.width,
							sideways: document.documentElement.scrollWidth - document.documentElement.clientWidth,
							mainSideways: document.querySelector('main').scrollWidth - document.querySelector('main').clientWidth
						};
					});
					assert.ok(seen.root.top >= 0 && seen.root.bottom <= seen.screen.h, `the root bar is off-screen: ${JSON.stringify(seen.root)}`);
					assert.ok(seen.bar.top >= 0 && seen.bar.bottom <= seen.screen.h + 1, `the bottom bar is off-screen: ${JSON.stringify(seen.bar)}`);
					assert.ok(seen.root.right <= seen.screen.w + 1 && seen.bar.right <= seen.screen.w + 1, 'a bar runs past the screen');
					assert.equal(seen.sideways, 0, `the page scrolls sideways by ${seen.sideways}px`);
					assert.equal(seen.mainSideways, 0, `main scrolls sideways by ${seen.mainSideways}px`);
					assert.ok(seen.input >= seen.bar.width * 0.35, `the prompt has ${seen.input.toFixed(0)}px of a ${seen.bar.width.toFixed(0)}px bar`);
				},
				{ viewport: { width, height: 900 } }
			);
		});
	}
});
