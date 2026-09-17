// datum.day's behaviour, expressed through `run(argv)`.
//
// Every assertion here is a command sequence and its observable result —
// never an internal of the runner behind the seam. That is deliberate: this
// suite was written against a TypeScript stand-in and outlived it, because
// the seam is the thing that did not change when the real CLI arrived
// behind it (da-g8o8). What changed are the cases that described the
// stand-in's gaps; those now describe the CLI answering.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { startApp, stopApp, withApp, todayAt, TIMEZONE } from './support/app.mjs';
import { comparableTail, rowTexts, stripAnsi } from './support/tail.mjs';

/** What the real `datum tail` printed for the same day. See support/tail.mjs. */
const tailFixture = JSON.parse(
	readFileSync(fileURLToPath(new URL('./fixtures/cli-tail.json', import.meta.url)), 'utf8')
);

before(startApp);
after(stopApp);

/**
 * `withApp` on the repl layout: for a test that never looks at the page.
 *
 * Most of this file asks the seam a question and reads the answer back
 * through the seam, and the day frame around it is scenery — scenery that
 * costs a fortnight of `dayview` bands on every open, better than a second
 * a test and two dozen tests over (da-x6ea). A context each is still a
 * database each, which is the isolation these need.
 *
 * The tests that DO read the page — the tail table, the two drawn panes, a
 * widget press, the banner — keep `withApp` and the day page, because the
 * page is what they are about.
 */
const onSeam = (fn, options) => withApp(fn, { query: 'repl', ...options });

/** Exit codes: success, the CLI's generic failure, and its duplicate. */
const OK = 0;
const ERROR = 1;
const DUPLICATE = 11;

describe('logging verbs', () => {
	test('occur / start / stop / switch each land an entry the tail shows', async () => {
		await withApp(async ({ run, entries, reload, paneTable }) => {
			// Distinct times, because a field's slug is its field and its
			// occur time: `start work` and `stop work` at the same instant
			// are one slug and the second is a duplicate.
			const commands = [
				['occur', 'coffee', '-t', '0700'],
				['start', 'work', '-t', '0800'],
				['stop', 'work', '-t', '1200'],
				['switch', 'mood', 'happy', '-t', '0930']
			];
			for (const argv of commands) {
				const result = await run(argv);
				assert.equal(result.code, OK, `${argv.join(' ')} → ${result.stderr}`);
				assert.equal(result.stderr, '');
				assert.notEqual(result.stdout, '', `${argv.join(' ')} printed nothing`);
			}

			const stored = await entries();
			assert.deepEqual(
				stored.map((e) => e.field).sort(),
				['coffee', 'mood', 'work', 'work']
			);

			// `tail` is a command like any other, so the seam answers it —
			// with the CLI's table: a header, then one row per entry.
			const listed = await run(['tail']);
			assert.equal(listed.code, OK);
			const rows = rowTexts(listed.stdout);
			// Header, the four entries, and one TRACK document per field the
			// commands brought into being — coffee, work, mood (da-c8mh).
			assert.equal(rows.length, commands.length + 4, `header plus rows: ${listed.stdout}`);
			assert.ok(
				rows.slice(1).some((row) => row.includes('mood') && row.includes('happy')),
				`tail did not show the switch: ${listed.stdout}`
			);

			// And the tail pane is that same listing, laid out in a real
			// table rather than in a terminal's padding — so what is
			// compared is the cells, not the spaces between them. The
			// panes re-run after a command the *page* ran, so reload to see
			// what a fresh visit shows. The pane shows the rows its height
			// holds (da-qrc6), so the screen is tall enough for all four.
			await reload();
			const table = await paneTable('tail');
			// The pane shows the rows its box holds — the newest page of the
			// listing — and draws no `action` column of its own (da-c8mh),
			// so the REPL's lines are compared without that cell, which
			// follows `_n` (da-431c).
			const withoutAction = (line) => line.replace(/^(_\S* +)?(action|TRACK.) +/, '$1');
			assert.deepEqual(
				rowTexts([table.headers, ...table.rows].map((cells) => cells.join(' ')).join('\n')),
				[rows[0], ...rows.slice(-table.rows.length)].map(withoutAction),
				'the pane and the REPL disagree about the same listing'
			);
		}, { viewport: { width: 1280, height: 900 } });
	});

	test('field-first argv means the same as verb-first', async () => {
		await onSeam(async ({ run, entries }) => {
			assert.equal((await run(['mood', 'switch', 'happy'])).code, OK);
			const [stored] = await entries();
			assert.equal(stored.field, 'mood');
			assert.equal(stored.state, 'happy');
		});
	});

	test('a state logged twice at the same minute is a duplicate, not a second entry', async () => {
		await onSeam(async ({ run, entries }) => {
			assert.equal((await run(['switch', 'mood', 'happy', '-t', '0915'])).code, OK);
			const again = await run(['switch', 'mood', 'sad', '-t', '0915']);
			assert.equal(again.code, DUPLICATE, again.stderr);
			assert.match(again.stderr, /Duplicate entry/);
			assert.equal((await entries()).length, 1);
		});
	});
});

describe('time and duration land on the entry', () => {
	test('-t HHMM sets the entry to that clock time', async () => {
		await onSeam(async ({ run, entries }) => {
			assert.equal((await run(['occur', 'coffee', '-t', '0915'])).code, OK);
			const [stored] = await entries();
			// The browser context runs in UTC (see support/app.mjs), so the
			// stored instant's UTC clock is the clock the user typed.
			assert.match(stored.occur_time.utc, /T09:15:00/);
		});
	});

	test('-t -N backdates by N minutes', async () => {
		await onSeam(async ({ run, entries }) => {
			const before = Date.now();
			assert.equal((await run(['occur', 'coffee', '-t', '-30'])).code, OK);
			const [stored] = await entries();
			const minutesAgo = (before - Date.parse(stored.occur_time.utc)) / 60000;
			assert.ok(
				minutesAgo > 29 && minutesAgo < 31,
				`expected ~30 minutes ago, got ${minutesAgo}`
			);
		});
	});

	test('a duration is stored as ISO-8601 and makes the entry a block', async () => {
		await onSeam(async ({ run, entries }) => {
			assert.equal((await run(['occur', 'nap', 'dur=30m', '-t', '1000'])).code, OK);
			assert.equal((await run(['occur', 'walk', 'dur=1:30', '-t', '1100'])).code, OK);
			const stored = await entries();
			const byField = Object.fromEntries(stored.map((e) => [e.field, e]));
			assert.equal(byField.nap.dur, 'PT30M');
			assert.equal(byField.walk.dur, 'PT1H30M');
			assert.equal(byField.nap.verb, 'block');
			assert.equal(byField.walk.verb, 'block');
		});
	});

	test('no duration leaves a point entry', async () => {
		await onSeam(async ({ run, entries }) => {
			await run(['occur', 'coffee', '-t', '0915']);
			const [stored] = await entries();
			// `MOMENT` is datum's in-column spelling of "no duration, this is
			// a point" (da-ju71.2): `occur` stamps it so the verb is locked to
			// `point` whatever shape the state has. The browser stores the
			// same sentinel the CLI does.
			assert.equal(stored.dur, 'MOMENT');
			assert.equal(stored.verb, 'point');
		});
	});
});

describe('failure', () => {
	// A word datum has never heard of is the CLI's own error, exit 1.
	test('a word datum has never heard of fails the way the CLI fails', async () => {
		await onSeam(async ({ run }) => {
			const result = await run(['definitelynotacommand']);
			assert.equal(result.code, ERROR);
			assert.match(result.stderr, /unknown subcommand/);
			assert.match(result.stderr, /definitelynotacommand/);
			assert.equal(result.stdout, '');
		});
	});

	test('a command the stand-in never had is the CLI’s, and runs', async () => {
		await onSeam(async ({ run, entries }) => {
			assert.equal((await run(['occur', 'coffee', '-t', '0700'])).code, OK);
			const result = await run(['grep', 'coffee']);
			assert.equal(result.code, OK, result.stderr);
			assert.match(stripAnsi(result.stdout), /coffee/);
			// And it is a command, not a field called "grep".
			assert.deepEqual(
				(await entries()).map((e) => e.field),
				['coffee']
			);
		});
	});

	test('an empty argv is a no-op success', async () => {
		await onSeam(async ({ run, entries }) => {
			const result = await run([]);
			assert.equal(result.code, OK);
			assert.equal(result.stdout, '');
			assert.equal((await entries()).length, 0);
		});
	});
});

describe('the teach-echo', () => {
	test('a typed line is echoed back exactly as typed, and shell-split for the runner', async () => {
		await onSeam(async ({ submit, echoes, spyOnRunner }) => {
			const line = 'note word correr "to run" srcLang=pt';
			const seen = await spyOnRunner();
			await submit(line);

			// The scrollback keeps the line the user wrote — quotes and all,
			// not a re-quoted rebuild of the argv.
			assert.equal((await echoes()).at(-1), line);

			// And what ran is that line shell-split the way the native REPL
			// splits it: the quoted words are one argument.
			// The panes re-run their own commands after every submission, so
			// the submitted one is simply the first the runner saw.
			// `--show json` after it is the web's own (da-xfa4): every command
			// a reader runs is asked for its echo as data. It is NOT in the
			// scrollback above, which is the point of the two assertions
			// standing together — the web's plumbing runs, and is never taught.
			const [executed] = await seen();
			assert.deepEqual(executed, [
				'note',
				'word',
				'correr',
				'to run',
				'srcLang=pt',
				'--show',
				'json'
			]);
		});
	});

	test('a widget press echoes the argv it ran, not a reconstruction of it', async () => {
		await withApp(async ({ run, reload, pressWidget, echoes, spyOnRunner }) => {
			// Give the bar a widget to press: a declared field gets one before
			// its first entry.
			assert.equal((await run(['spec', 'set', 'coffee', '--verb', 'occur'])).code, OK);
			await reload();

			const seen = await spyOnRunner();
			await pressWidget('coffee');
			const [executed] = await seen();
			assert.deepEqual(executed, ['occur', 'coffee', '--show', 'json']);
			// The ECHO is the argv the widget asked for, without the web's own
			// `--show json` (da-xfa4): the plumbing is not a command to teach.
			assert.equal((await echoes()).at(-1), 'occur coffee');
			// Pinned: the first run seen is the press only while no minute
			// tick re-draws the strip inside the window (da-6ehq).
		}, { clock: todayAt(12) });
	});
});

describe('storage', () => {
	test('entries survive a reload', async () => {
		await withApp(async ({ run, reload, entries, paneText }) => {
			assert.equal((await run(['switch', 'mood', 'happy', '-t', '0915'])).code, OK);
			const before = await entries();
			assert.equal(before.length, 1);

			await reload();

			const after = await entries();
			assert.equal(after.length, 1, 'the entry did not survive the reload');
			// The hid is the durable identity (da-zlug) — same row, not a
			// re-derived lookalike.
			assert.equal(after[0].hid, before[0].hid);
			assert.equal(after[0].slug, before[0].slug);
			assert.match(await paneText('tail'), /happy/);
		});
	});

	test('the app does not claim durability it lacks', async () => {
		await withApp(async ({ bannerText }) => {
			// The banner reads the storage that actually opened. In-memory
			// wording here means the OPFS database failed to open — the
			// da-5iqw regression, and the one a missing COOP/COEP header
			// would reintroduce.
			assert.doesNotMatch(await bannerText(), /held in memory only/);
		});
	});

	test('the page throws nothing while logging', async () => {
		await onSeam(async ({ run, errors }) => {
			await run(['occur', 'coffee']);
			assert.deepEqual(errors, []);
		});
	});
});

describe('the visual views', () => {
	/** The colour datum gives a name: the first three bytes of its md5. */
	const colorOf = (name) =>
		'#' + createHash('md5').update(name).digest('hex').slice(0, 6);

	/**
	 * Every `<rect class="{cls}" …>` inside one field's band, in document
	 * order. The renderer writes the attributes in a fixed order, which is
	 * what makes this scan enough.
	 */
	const fieldRects = (svg, field, cls) => {
		const open = `<g class="field" data-field="${field}">`;
		const start = svg.indexOf(open);
		if (start < 0) return [];
		const group = svg.slice(start + open.length, svg.indexOf('</g>', start));
		const rect = new RegExp(
			`<rect class="${cls}"[^>]*\\bx="([\\d.-]+)"[^>]*\\bwidth="([\\d.-]+)"[^>]*\\bfill="([^"]*)"`,
			'g'
		);
		return [...group.matchAll(rect)].map((m) => ({
			x: Number(m[1]),
			w: Number(m[2]),
			fill: m[3]
		}));
	};

	/** Each field's current-state chunk colour, past the now-line. */
	const nowChunks = (svg) =>
		Object.fromEntries(
			[...svg.matchAll(/<g class="field" data-field="([^"]*)">(.*?)<\/g>/g)]
				.map(([, field, group]) => [
					field,
					/<rect class="now-chunk"[^>]*fill="([^"]*)"/.exec(group)?.[1]
				])
				.filter(([, fill]) => fill !== undefined)
		);

	test('dayview draws the day as an SVG, and the pane shows it', async () => {
		await withApp(async ({ page, run, reload, paneSvg }) => {
			const drawn = await run(['dayview']);
			assert.equal(drawn.code, OK, drawn.stderr);
			assert.equal(drawn.stderr, '');
			// The CLI writes an SVG document to stdout for `datum dayview`;
			// so does the browser, because it is running the same
			// `compute_dayview` + `render_dayview_svg` the CLI runs.
			assert.match(drawn.stdout, /^<svg[\s>]/);

			// Today's own label — the same three lines `format_day_label`
			// builds. Which day that is has to be read in the zone the page
			// runs in, not node's: the browser context is pinned to
			// TIMEZONE and the host is wherever the test happens to run, so
			// an expectation off node's clock disagrees with the drawing for
			// the hours the two zones straddle a midnight (da-2cgd).
			const weekday = new Intl.DateTimeFormat('en-US', {
				timeZone: TIMEZONE,
				weekday: 'short'
			}).format(new Date());
			assert.match(drawn.stdout, new RegExp(`>${weekday}<`));

			// And the pane is that command's output. Not byte-for-byte:
			// dayview paints a now-line, so the pane's own run of the
			// command a moment later is the same drawing at a later
			// instant — and the strip asks for its bands with
			// `--label-width 0` (da-iu9i), so the day's name is in the
			// column the strip writes beside the picture, not in it.
			await reload();
			const shown = await paneSvg('dayview');
			assert.match(shown, /^<svg[\s>]/);
			assert.ok(!shown.includes('date-label'), 'the band still carries datum\'s date labels');
			assert.match(await page.locator('.strip .row.pinned .stamp').innerText(), new RegExp(`^${weekday}\\b`));
		});
	});

	test('a switch paints a band in the state colour the CLI would pick', async () => {
		await onSeam(async ({ run }) => {
			// Five minutes back, so the region runs to now inside both the
			// day window and nowview's 30-minute one whatever the hour.
			assert.equal((await run(['switch', 'mood', 'happy', '-t', '-5'])).code, OK);

			const drawn = await run(['nowview', '-o', '-']);
			assert.equal(drawn.code, OK, drawn.stderr);
			assert.ok(
				drawn.stdout.includes(colorOf('happy')),
				`no band in happy's colour (${colorOf('happy')}): ${drawn.stdout}`
			);
			// And the lane runs on past the now-line in that same colour:
			// the chunk that says what the field is in *now* (da-nk84).
			assert.equal(
				nowChunks(drawn.stdout).mood,
				colorOf('happy'),
				`no now-chunk in happy's colour: ${drawn.stdout}`
			);
		});
	});

	// Pinned mid-afternoon: the clock this draws is the app's own, and
	// unpinned it is the runner's hour — so the assertion below about its
	// SHAPE was an assertion about the machine (da-ldct).
	test('nowview carries the clock and the current state', async () => {
		await withApp(async ({ run, reload, paneSvg }) => {
			assert.equal((await run(['switch', 'focus', 'deep', '-t', '-5'])).code, OK);

			const drawn = await run(['nowview', '-o', '-']);
			assert.equal(drawn.code, OK, drawn.stderr);
			// The viewer's own clock — the one thing about the picture the
			// browser knows and the layout crate cannot.
			assert.match(drawn.stdout, /<text[^>]*>\d{2}:\d{2}<\/text>/);
			// The current state is a chunk past the now-line, not a named
			// panel row — the browser draws the picture the CLI draws
			// because it runs the CLI's renderer (da-nk84).
			assert.equal(nowChunks(drawn.stdout).focus, colorOf('deep'));
			assert.doesNotMatch(drawn.stdout, />focus: deep</, 'the state panel is back');

			await reload();
			assert.equal(nowChunks(await paneSvg('nowview')).focus, colorOf('deep'));
		}, { clock: todayAt(15) });
	});

	// The shape of the picture, asserted in the browser the way
	// `nowview_lanes_fill_edge_to_edge_and_carry_a_now_chunk` asserts it
	// for the CLI: nothing is labelled but the axis, each lane tiles the
	// history window with no gap, and a field in a state gets exactly one
	// chunk past the now-line while an occur-only field gets none.
	test('nowview fills each lane to the now-line and labels only the axis', async () => {
		await onSeam(async ({ run }) => {
			// The first switch is before the window opens, so the lane is
			// filled from its left edge by the lead-in row alone.
			assert.equal((await run(['switch', 'mood', 'calm', '-t', '-45'])).code, OK);
			assert.equal((await run(['switch', 'mood', 'happy', '-t', '-20'])).code, OK);
			assert.equal((await run(['switch', 'mood', 'sad', '-t', '-5'])).code, OK);
			assert.equal((await run(['occur', 'coffee', '-t', '-8'])).code, OK);

			const svg = (await run(['nowview', '-o', '-'])).stdout;

			for (const cls of [...svg.matchAll(/<text class="([^"]*)"/g)].map((m) => m[1])) {
				assert.ok(
					cls === 'tick-label' || cls === 'current-time-text',
					`nowview labelled a <text class="${cls}">: ${svg}`
				);
			}

			const nowX = Number(/<line class="current-time-line" x1="([\d.]+)"/.exec(svg)[1]);
			const lane = fieldRects(svg, 'mood', 'block');
			assert.ok(lane.length >= 2, `mood switched inside the window: ${svg}`);
			let x = 0;
			for (const rect of lane) {
				assert.ok(Math.abs(rect.x - x) < 1e-6, `gap before ${rect.x} (expected ${x}): ${svg}`);
				x = rect.x + rect.w;
			}
			assert.ok(Math.abs(x - nowX) < 1e-6, `lane ends at ${x}, not the now-line ${nowX}`);

			// One chunk, in the state it switched INTO — the lane left of
			// the line still carries what it was.
			const chunks = fieldRects(svg, 'mood', 'now-chunk');
			assert.equal(chunks.length, 1);
			assert.equal(chunks[0].fill, colorOf('sad'));
			assert.equal(lane.at(-2).fill, colorOf('happy'));
			// The occur-only field is in no state now, so it gets nothing.
			assert.deepEqual(fieldRects(svg, 'coffee', 'now-chunk'), []);
		});
	});

	test('an empty database still draws both views', async () => {
		await onSeam(async ({ run }) => {
			for (const argv of [['dayview'], ['nowview', '-o', '-']]) {
				const drawn = await run(argv);
				assert.equal(drawn.code, OK, `${argv[0]} → ${drawn.stderr}`);
				assert.match(drawn.stdout, /^<svg[\s>]/, `${argv[0]} drew nothing`);
			}
		});
	});

	test('a switch, an occur and a switch to nothing draw band, dot and nothing', async () => {
		// The three shapes `render_shape` distinguishes, asserted through
		// the seam. Until da-t7ud this dispatch was a hand-written copy of
		// the table in the browser ($lib/visuals.ts); it is now the CLI's
		// own, so what this test guards is that the wiring reaches it.
		await onSeam(async ({ run }) => {
			assert.equal((await run(['switch', 'mood', 'happy', '-t', '-10'])).code, OK);
			assert.equal((await run(['occur', 'coffee', '-t', '-5'])).code, OK);
			// A switch to `false` terminates a state without opening one.
			assert.equal((await run(['switch', 'sleep', 'false', '-t', '-5'])).code, OK);

			const drawn = await run(['nowview', '-o', '-']);
			assert.equal(drawn.code, OK, drawn.stderr);

			// A band: a filled rect in the state's own colour.
			assert.match(
				drawn.stdout,
				new RegExp(`<rect[^>]*fill="${colorOf('happy')}"`),
				`no band for the switch: ${drawn.stdout}`
			);
			// A dot: a circle in the field's colour, over black — nothing
			// was prevailing under it.
			assert.match(
				drawn.stdout,
				new RegExp(`<circle[^>]*fill="${colorOf('coffee')}"`),
				`no dot for the occur: ${drawn.stdout}`
			);
			// And nothing at all for the terminating switch — `sleep` has
			// no other mark, so the field is dropped rather than drawn.
			assert.doesNotMatch(drawn.stdout, /data-field="sleep"/);
		});
	});

	test('a block runs back from its occur time rather than forward from it', async () => {
		await onSeam(async ({ run }) => {
			// `start`/`stop` brackets a block by its two instants; a
			// duration declares one — and occur_time is the block's
			// TRAILING edge, so the span PRECEDES it (da-zwct, reversing
			// da-a8rs). Both land 20m–5m ago. Read forward, `read` would
			// run from 5m ago to 10m from now instead, so the direction is
			// what the geometry below holds the picture to: asserting only
			// that a band exists is how the wrong wording survived here.
			assert.equal((await run(['start', 'work', '-t', '-20'])).code, OK);
			assert.equal((await run(['stop', 'work', '-t', '-5'])).code, OK);
			assert.equal((await run(['start', 'read', '-t', '-5', '15m'])).code, OK);

			const drawn = await run(['nowview', '-o', '-']);
			assert.equal(drawn.code, OK, drawn.stderr);
			const svg = drawn.stdout;

			// The window the picture covers, and the now-line at its end:
			// between them they turn an x back into minutes before now.
			const seen = /<g class="day-data"[^>]*data-from="([^"]*)" data-to="([^"]*)"/.exec(svg);
			const nowX = Number(/<line class="current-time-line" x1="([\d.]+)"/.exec(svg)[1]);
			const minutes = (Date.parse(seen[2]) - Date.parse(seen[1])) / 60000;
			const minutesAgo = (x) => (1 - x / nowX) * minutes;

			for (const field of ['work', 'read']) {
				const bands = fieldRects(svg, field, 'block');
				assert.equal(bands.length, 1, `one band for ${field}: ${svg}`);
				const [band] = bands;
				assert.equal(band.fill, colorOf(field), `${field}'s band is not its colour: ${svg}`);
				// A minute of tolerance: the commands and the drawing are
				// separate runs, so the window has moved on a little.
				assert.ok(
					Math.abs(minutesAgo(band.x) - 20) < 1,
					`${field} starts ${minutesAgo(band.x)}m ago, not 20m: ${svg}`
				);
				assert.ok(
					Math.abs(minutesAgo(band.x + band.w) - 5) < 1,
					`${field} ends ${minutesAgo(band.x + band.w)}m ago, not 5m: ${svg}`
				);
			}
		});
	});

	test('the arguments are the CLI’s, and so are the refusals', async () => {
		await onSeam(async ({ run }) => {
			assert.equal((await run(['occur', 'coffee', '-t', '0700'])).code, OK);
			for (const argv of [['tail', '-n', '5'], ['dayview', '--n-days', '3']]) {
				const result = await run(argv);
				assert.equal(result.code, OK, `${argv.join(' ')}: ${result.stderr}`);
			}
			assert.match((await run(['dayview', '--n-days', '3'])).stdout, /^<svg[\s>]/);
			// A flag no command has is refused by name, as the CLI refuses it.
			const refused = await run(['tail', '--oneline']);
			assert.equal(refused.code, ERROR);
			assert.match(refused.stderr, /--oneline/);
		});
	});
});

describe('spec', () => {
	// Every subcommand from `datum spec --help`. The stand-in served `set`
	// alone, and there was no way to remove a field in the browser; the CLI
	// serves them all.
	const SUBCOMMANDS = [
		['show', 'coffee'],
		['show', 'coffee', '--text'],
		['list'],
		['remove', 'coffee'],
		['add-state', 'mood', 'happy']
	];

	test('every spec subcommand answers', async () => {
		await onSeam(async ({ run }) => {
			assert.equal(
				(await run(['spec', 'set', 'coffee', '--verb', 'occur', '--color', '#884400'])).code,
				OK
			);
			for (const args of SUBCOMMANDS) {
				const label = ['spec', ...args].join(' ');
				const result = await run(['spec', ...args]);
				assert.equal(result.code, OK, `${label}: ${result.stderr}`);
				assert.notEqual(result.stdout, '', `${label} answered with nothing at all`);
			}
		});
	});

	test('`spec set` still works — it is what the `+` button runs', async () => {
		await onSeam(async ({ run }) => {
			const saved = await run(['spec', 'set', 'mood', '--state', 'happy']);
			assert.equal(saved.code, OK, saved.stderr);
			assert.match(saved.stdout, /mood/);
			// And the neighbouring subcommands did not take it down with them.
			assert.equal((await run(['spec', 'set', 'mood', '--color', '#0f0'])).code, OK);
		});
	});

	test('typed at the prompt, a spec subcommand answers on the screen', async () => {
		// The report this came from was of a blank REPL — so one case walks
		// the whole way a reader does, and reads the answer back off the
		// scrollback (da-e0m6's trap).
		await withApp(async ({ submit, page }) => {
			await submit('spec remove coffee');
			// The rendered output blocks, not the whole pane: the echoed
			// `❯ spec remove coffee` above them carries every word of the
			// command, so a scrollback-wide match proves nothing.
			const printed = (await page.locator('.repl pre').allInnerTexts()).join('\n');
			assert.notEqual(printed.trim(), '', 'the prompt answered with nothing at all');
			assert.match(printed, /remove/, `the answer does not name remove: ${printed}`);
			assert.doesNotMatch(
				printed,
				/\bspec\b:?\s+(is\s+)?not available/,
				`the answer calls spec itself unavailable: ${printed}`
			);
		});
	});
});

describe('help and version', () => {
	// The parser signals `--help` and `--version` as ParseError variants —
	// their doc comments say the caller renders the text and exits 0. The
	// web runner used to lump them in with real errors, so the first thing
	// a newcomer types into the REPL answered `error: help requested` and
	// no help at all (da-e0m6).
	//
	// Every spelling, because they are two variants reached down two paths
	// each, and each assertion is on the TEXT: "no `error:`" would pass
	// against a runner that printed nothing.
	test('every spelling of help prints the help, exit 0', async () => {
		await onSeam(async ({ run }) => {
			for (const argv of [['help'], ['-h'], ['--help']]) {
				const result = await run(argv);
				assert.equal(result.code, OK, `${argv.join(' ')} → ${result.stderr}`);
				assert.equal(result.stderr, '');
				assert.match(result.stdout, /USAGE:/, `${argv.join(' ')} printed no help`);
				assert.match(result.stdout, /personal data tracker/);
			}
		});
	});

	test('every spelling of version prints the banner, exit 0', async () => {
		await onSeam(async ({ run }) => {
			for (const argv of [['--version'], ['-V']]) {
				const result = await run(argv);
				assert.equal(result.code, OK, `${argv.join(' ')} → ${result.stderr}`);
				assert.equal(result.stderr, '');
				assert.match(result.stdout, /^datum \d/, `${argv.join(' ')} printed no banner`);
			}
		});
	});

	test('help for one command is that command’s page', async () => {
		await onSeam(async ({ run }) => {
			for (const argv of [
				['help', 'occur'],
				['occur', '--help']
			]) {
				const result = await run(argv);
				assert.equal(result.code, OK, `${argv.join(' ')} → ${result.stderr}`);
				assert.match(result.stdout, /datum occur/, `${argv.join(' ')}: ${result.stdout}`);
			}
		});
	});

	test('the REPL shows the help a user typed, not an error', async () => {
		await withApp(async ({ page, runLine }) => {
			// Through the prompt itself: what the bead reported is what a
			// person sees in the scrollback, and the scrollback paints
			// stderr as an error.
			await page.fill('.prompt-bar input', 'help');
			await page.press('.prompt-bar input', 'Enter');
			await page.locator('.repl pre').first().waitFor();
			const shown = await page.locator('.repl pre').first().innerText();
			assert.match(shown, /USAGE:/, `the REPL showed: ${shown}`);
			assert.doesNotMatch(shown, /help requested/);
			assert.equal(await page.locator('.repl pre.error').count(), 0);

			// And the seam agrees with what the pane rendered.
			assert.equal((await runLine('help')).stdout.split('\n')[0], shown.split('\n')[0]);
		});
	});
});

describe('the tail is the CLI\'s tail', () => {
	// The assertion the rest of this file is a proxy for. `datum tail` is a
	// table — aligned columns, a header carrying the listing's UTC offset,
	// `_n` ordinals, colour — and da-6ec2's whole claim is that the browser
	// RENDERS that table rather than deriving one that looks like it. The
	// only way to check the claim is against the real binary's output, so
	// the expectation is captured (`npm run test:fixtures`), never written
	// here: a hand-written table would be a second opinion about the layout,
	// which is the thing being ruled out.
	test('prints, character for character, what the binary printed', async () => {
		assert.ok(tailFixture.capturedWith, 'run `npm run test:fixtures` to capture');
		await onSeam(async ({ run }) => {
			for (const argv of tailFixture.log) {
				const result = await run(argv);
				assert.equal(result.code, OK, `${argv.join(' ')} → ${result.stderr}`);
			}
			const listed = await run(['tail']);
			assert.equal(listed.code, OK, listed.stderr);
			assert.equal(comparableTail(listed.stdout), tailFixture.table);
		});
	});

	// Colour is the readability half of the report: a field badge and a
	// state chip are how a row is scanned, and the module renders them
	// only because the request says to (`RenderRequest::color`) — it has
	// no environment and no terminal to ask.
	test('paints the field badge, and the pane shows that colour', async () => {
		await withApp(async ({ run, reload, paneCellColors, tailRows }) => {
			assert.equal((await run(['occur', 'coffee', '-t', '0700'])).code, OK);
			assert.equal((await run(['switch', 'mood', 'happy', '-t', '0800'])).code, OK);
			const listed = await run(['tail']);
			// md5("coffee") = 24eb05… → rgb(36, 235, 5), the colour
			// `field_rgb` derives with no spec to override it. Asserted as
			// the exact escape rather than as "some colour", so a change of
			// derivation has to say so here.
			assert.match(listed.stdout, /\x1b\[48;2;36;235;5m/, 'the field badge is unpainted');
			assert.ok(stripAnsi(listed.stdout).includes('coffee'));

			// And on screen: what the pane paints a cell is what datum painted
			// it — read off `tail --json`'s own columns, not written down
			// here, so the two cannot drift apart in either direction.
			await reload();
			const { rows, columns } = await tailRows();
			// Each field's TRACK document sits just ahead of its entry.
			assert.deepEqual(
				rows.map((row) => row.field),
				['coffee', 'coffee', 'mood', 'mood']
			);
			const cells = (key) => columns.find((column) => column.key === key).cells;
			const painted = async (key) => (await paneCellColors('tail', key)).map(distinct);
			// The pane's newest page, against the listing's last N cells.
			const shown = await painted('field');
			assert.deepEqual(shown, cells('field').slice(-shown.length).map(backgrounds));
			// The state chip too — a different colour from the field's. The
			// CLI's state cell is `[LEAD][state][TRAIL]` and each slot paints
			// its own block, so the whole cell is compared, not one chip.
			assert.deepEqual((await painted('state')).at(-1), backgrounds(cells('state').at(-1)));
			assert.notDeepEqual(backgrounds(cells('state').at(-1)), backgrounds(cells('field').at(-1)));
		});
	});
});

/** The distinct backgrounds an ANSI-painted cell sets, as the browser reports them. */
function backgrounds(cell) {
	return distinct([...cell.matchAll(/\x1b\[48;2;(\d+);(\d+);(\d+)m/g)].map(([, r, g, b]) => `rgb(${r}, ${g}, ${b})`));
}

function distinct(list) {
	return [...new Set(list)];
}
