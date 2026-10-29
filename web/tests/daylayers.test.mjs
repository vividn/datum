// One data layer by instant, shared by the day strip and the nowview (da-m6i2a).
//
// vividn, 2026-10-27: «we could still do cache by day, just batch the data
// layer write when scrolling up, so generate 2 weeks at a time of data and cut
// it into days after generation». Both widgets draw a day from the same
// `dayview --data-layer` picture, held by #lib/dayLayers.ts: so a day either
// asks is asked once for both, and a day the clock changed in is drawn by
// instant in the nowview and as the CLI lived it in the strip.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp, seedLines, todayAt, ON_COLLECT } from './support/app.mjs';
import { DESKTOP } from './support/layout.mjs';

before(startApp);
after(stopApp);

const NOWVIEW = 'section[data-pane="nowview"] .nowview-band';
const band = (end) => `.band[data-days-ago="${end}"] svg.svg`;

/** The dates a data layer run draws (`--pictures`) or checks (`--digests`), by its `--end-date`. */
function layerDates(argv, kind = '--pictures') {
	if (argv[0] !== 'dayview' || !argv.includes('--data-layer') || !argv.includes(kind)) return [];
	const flag = (name) => argv[argv.indexOf(name) + 1];
	const list = flag(kind);
	const end = Date.parse(flag('--end-date'));
	return list.split(',').map((back) => new Date(end - Number(back) * 86_400_000).toISOString().slice(0, 10));
}

test('the strip and the nowview draw a day from one data layer, asked once for both', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			await app.idle();
			const seen = await app.spyOnRunner();
			// Ten minutes past midnight: both widgets draw yesterday's end and today.
			await seedLines(app, ['switch mood calm -t -20', 'occur coffee -t -5']);
			for (const at of [`${band(1)} rect.block[data-state="calm"]`, `${band(0)} [data-field="coffee"]`, `${NOWVIEW} svg.svg rect.block[data-state="calm"]`, `${NOWVIEW} svg.svg [data-field="coffee"]`])
				await page.locator(at).first().waitFor({ state: 'attached' });
			await app.idle();

			// Each day drawn once, and checked at most once, for both widgets.
			const runs = await seen();
			const [dates, checked] = ['--pictures', '--digests'].map((kind) => runs.flatMap((argv) => layerDates(argv, kind)));
			for (const list of [dates, checked])
				assert.deepEqual(list.filter((date, i) => list.indexOf(date) !== i), [], `a day asked twice: ${dates} | ${checked}`);
			const [today, yesterday] = [0, 1].map((back) => new Date(todayAt(0) - back * 86_400_000).toISOString().slice(0, 10));
			assert.ok(dates.includes(today) && dates.includes(yesterday), `not today and yesterday asked: ${dates}`);
			assert.deepEqual(app.errors, []);
		},
		{ ...DESKTOP, home: ON_COLLECT, clock: todayAt(0, 10) }
	);
});

// 2026-10-25 in Berlin: the clock went back from 03:00 to 02:00 at 01:00 UTC.
const FALL_BACK = { ...DESKTOP, home: ON_COLLECT, timezone: 'Europe/Berlin', clock: Date.UTC(2026, 9, 25, 1, 10) };

test('a day the clock went back in: the nowview by instant, the strip as it was lived', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			// Ten minutes either side of the change, as a Berlin clock read them.
			for (const line of ['switch mood calm -t 2026-10-25T02:45:00+02:00', 'occur coffee -t 2026-10-25T02:55:00+02:00', 'occur tea -t 2026-10-25T02:05:00+01:00'])
				assert.equal((await app.runLine(line)).code, 0, line);
			await app.redraw();
			await page.locator(`${NOWVIEW} svg.svg [data-field="tea"][data-at]`).first().waitFor({ state: 'attached' });
			await page.locator(`${band(0)} [data-field="tea"]`).first().waitFor({ state: 'attached' });
			await app.idle();

			// The nowview: ten minutes apart is ten minutes apart across the
			// change, read off the overlay's own axis, and the state is one
			// block across it, a lane's full height.
			const now = await page.locator(NOWVIEW).evaluate((band) => {
				const svg = band.querySelector('svg.svg');
				const x = (el) => {
					const r = el.getBoundingClientRect();
					return (r.left + r.right) / 2;
				};
				const axis = [...svg.querySelectorAll('[data-lo]')].find((g) => g.closest('.plot') && !g.closest('.crop'));
				const ctm = axis.getScreenCTM();
				const at = (name) => new DOMPoint(Number(axis.getAttribute(name)), 0).matrixTransform(ctm).x;
				const msPerPx = (Date.parse(axis.getAttribute('data-to')) - Date.parse(axis.getAttribute('data-from'))) / (at('data-hi') - at('data-lo'));
				const dot = (field) => x(svg.querySelector(`[data-field="${field}"].point-fg, [data-field="${field}"] .point-fg`));
				const calm = [...svg.querySelectorAll('.block[data-state="calm"]')];
				return { apart: (dot('tea') - dot('coffee')) * msPerPx, calm: calm.map((b) => b.localName), heights: calm.map((b) => Math.round(b.getBoundingClientRect().height)) };
			});
			assert.ok(Math.abs(now.apart - 600_000) < 30_000, `coffee and tea ${now.apart / 60_000} minutes apart`);
			assert.deepEqual([...new Set(now.calm)], ['rect'], `the state is drawn as ${now.calm}`);
			assert.equal(new Set(now.heights).size, 1, `the state narrows: ${now.heights}`);

			// The strip: today is the CLI's own picture, a stretch a clock,
			// the repeated hour in lanes; yesterday, one clock, the layer.
			const strip = await page.evaluate(
				([today, yesterday]) => ({
					stretches: document.querySelectorAll(`${today} .day-data`).length,
					labels: [...document.querySelectorAll(`${today} .zone-label`)].map((t) => t.textContent),
					lanes: document.querySelectorAll(`${today} .lane`).length,
					yesterday: [...document.querySelectorAll(`${yesterday} .day-data`)].map((g) => g.getAttribute('data-offset'))
				}),
				[band(0), band(1)]
			);
			assert.equal(strip.stretches, 2, JSON.stringify(strip));
			assert.ok(strip.labels.includes('UTC+2') && strip.labels.includes('UTC+1'), JSON.stringify(strip));
			assert.ok(strip.lanes > 0, JSON.stringify(strip));
			assert.deepEqual(strip.yesterday, ['7200']);
			assert.deepEqual(app.errors, []);
		},
		FALL_BACK
	);
});

// A band laid out from its held layer goes to the page as the node it was
// laid out in (da-mi2s6): serialized, `inlineSvg` parsed a dense day of it
// again at every draw, ~1.2 s of main thread at CPU 4x, on every write and
// minute tick. The layer is sanitized once, when it is parsed, and the
// band shows it: no `<title>`, its text on the mark. And with no selection
// drawn there is no pill to place, so nothing reads the strip's box, which
// laid out the bands just put in (~600 ms a re-lay at CPU 4x).
test('a band laid out from its held layer is drawn as a node, never parsed again', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			await app.idle();
			await seedLines(app, ['switch mood calm -t -20', 'occur coffee -t -5']);
			await page.locator(`${band(0)} [data-field="coffee"]`).first().waitFor({ state: 'attached' });
			await app.idle();
			await page.evaluate(() => {
				const parse = DOMParser.prototype.parseFromString;
				window.parsed = [];
				DOMParser.prototype.parseFromString = function (text, type) {
					window.parsed.push(text);
					return parse.call(this, text, type);
				};
				const box = Element.prototype.getBoundingClientRect;
				window.stripRead = 0;
				Element.prototype.getBoundingClientRect = function () {
					if (this.classList.contains('strip')) window.stripRead++;
					return box.call(this);
				};
			});
			await seedLines(app, ['occur tea -t -2']);
			await page.locator(`${band(0)} [data-field="tea"]`).first().waitFor({ state: 'attached' });
			await app.idle();

			const seen = await page.locator(band(0)).evaluate((svg) => ({
				viewBox: svg.getAttribute('viewBox'),
				titles: svg.querySelectorAll('title').length,
				named: [...svg.querySelectorAll('[data-state][data-title]')].map((m) => m.getAttribute('data-title')),
				parsed: window.parsed,
				stripRead: window.stripRead
			}));
			const again = seen.parsed.filter((text) => text.includes(`viewBox="${seen.viewBox}"`));
			assert.equal(seen.parsed.length > 0, true, 'the new layer was parsed, so the spy saw the redraw');
			assert.deepEqual(again.map((text) => text.slice(0, 120)), [], `today's band parsed again at ${seen.viewBox}`);
			assert.equal(seen.stripRead, 0, "the strip's box was read with no selection drawn");
			assert.equal(seen.titles, 0, 'a <title> reached the page');
			assert.ok(seen.named.some((title) => title.includes('tea')), `no mark named tea: ${seen.named}`);
			assert.deepEqual(app.errors, []);
		},
		{ ...DESKTOP, home: ON_COLLECT, clock: todayAt(12) }
	);
});

// The nowview's crop goes to the page as the node it was built in (da-cqiil):
// serialized, `inlineSvg` parsed it again at every draw, ~450 ms of main
// thread on a dense day at CPU 4x, on every write. The overlay is sanitized
// as the layers are: no `<title>` reaches the page, its text on the mark.
test("the nowview's crop is drawn as a node, never serialized and parsed again", async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			await app.idle();
			await seedLines(app, ['switch mood calm -t -20', 'occur coffee -t -5']);
			await page.locator(`${NOWVIEW} svg.svg [data-field="coffee"]`).first().waitFor({ state: 'attached' });
			await app.idle();
			await page.evaluate(() => {
				const [parse, serialize] = [DOMParser.prototype.parseFromString, XMLSerializer.prototype.serializeToString];
				window.texts = [];
				DOMParser.prototype.parseFromString = function (text, type) {
					window.texts.push(text);
					return parse.call(this, text, type);
				};
				XMLSerializer.prototype.serializeToString = function (node) {
					const text = serialize.call(this, node);
					window.texts.push(text);
					return text;
				};
			});
			await seedLines(app, ['occur tea -t -2']);
			await page.locator(`${NOWVIEW} svg.svg [data-field="tea"]`).first().waitFor({ state: 'attached' });
			await app.idle();

			const seen = await page.locator(`${NOWVIEW} svg.svg`).evaluate((svg) => ({
				crops: svg.querySelectorAll('g.crop').length,
				titles: svg.querySelectorAll('title').length,
				named: [...svg.querySelectorAll('[data-state][data-title]')].map((m) => m.getAttribute('data-title')),
				texts: window.texts
			}));
			assert.ok(seen.crops > 0, 'no crop drawn, so nothing was checked');
			assert.ok(seen.texts.length > 0, 'nothing parsed, so the spy saw no redraw');
			const again = seen.texts.filter((text) => text.includes('class="crop"'));
			assert.deepEqual(again.map((text) => text.slice(0, 120)), [], 'the crop went through text');
			assert.equal(seen.titles, 0, 'a <title> reached the page');
			assert.ok(seen.named.some((title) => title.includes('tea')), `no mark named tea: ${seen.named}`);
			assert.deepEqual(app.errors, []);
		},
		{ ...DESKTOP, home: ON_COLLECT, clock: todayAt(12) }
	);
});

// Before da-cqiil the overlay reached the page through `inlineSvg`; now
// `compose`'s own `sanitizeSvg` is all that stands between them. So the
// overlay datum printed is handed to the page carrying what the sanitizer
// removes (the runner wrapped, as `spyOnRunner` wraps it), and none of it
// may reach the page: a `<script>`, an `on…` handler, a link that leaves the
// picture, a `<title>` (its text goes onto its mark).
test('the overlay the nowview crops under is sanitized before it reaches the page', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			await app.idle();
			await page.evaluate(() => {
				const evil =
					'<script>window.ran = 1</script>' +
					'<a class="evil-link" href="https://example.com/x"><text>x</text></a>' +
					'<rect class="evil-mark" data-state="evil" onclick="window.ran = 1"><title>evil title</title></rect>';
				const previous = window.datum.setRunner((argv, ...rest) =>
					previous(argv, ...rest).then((result) =>
						argv[0] === 'nowview' && argv.includes('--overlay') && result.stdout.includes('data-lo')
							? { ...result, stdout: result.stdout.replace(/<\/svg>\s*$/, `${evil}</svg>`) }
							: result
					)
				);
			});
			// The overlay opens no database, so a write does not ask for it
			// again: the clock moving the picture on past its look-ahead does.
			await seedLines(app, ['occur coffee -t -5']);
			await app.clock.forward(3 * 3_600_000);
			await page.locator(`${NOWVIEW} svg.svg .evil-mark`).waitFor({ state: 'attached' });
			await app.idle();

			const seen = await page.locator(`${NOWVIEW} svg.svg`).evaluate((svg) => ({
				crops: svg.querySelectorAll('g.crop').length,
				scripts: svg.querySelectorAll('script').length,
				handlers: [...svg.querySelectorAll('*')].flatMap((el) => [...el.attributes].filter((a) => /^on/i.test(a.name)).map((a) => a.name)),
				href: svg.querySelector('.evil-link')?.getAttribute('href') ?? null,
				titles: svg.querySelectorAll('title').length,
				named: svg.querySelector('.evil-mark')?.getAttribute('data-title') ?? null
			}));
			assert.ok(seen.crops > 0, 'the picture was not cropped, so compose never saw the overlay');
			assert.equal(seen.scripts, 0, 'a <script> reached the page');
			assert.deepEqual(seen.handlers, [], 'an on… handler reached the page');
			assert.equal(seen.href, null, 'a link that leaves the picture reached the page');
			assert.equal(seen.titles, 0, 'a <title> reached the page');
			assert.equal(seen.named, 'evil title', "the <title>'s text is not on its mark");
			assert.deepEqual(app.errors, []);
		},
		{ ...DESKTOP, home: ON_COLLECT, clock: todayAt(12) }
	);
});
