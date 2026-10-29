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
import { HELD_AT, sharedApp, startApp, stopApp, TIMEZONE } from './support/app.mjs';
import { rowTexts, stripAnsi } from './support/tail.mjs';
import { DESKTOP, ONE_PANE } from './support/layout.mjs';

before(startApp);
after(stopApp);

/**
 * ONE app for the whole file (da-6b3dj, sharedApp in support/app.mjs; one
 * boot where there were eight, da-6i7cn).
 *
 * Most of this file asks the seam a question and reads the answer back
 * through the seam, and the day frame around it is scenery that costs a
 * fortnight of `dayview` bands to draw (da-x6ea) — so `onSeam` opens `main`
 * as a draft of ONE CLI widget. The tests that DO read the page — the tail
 * table, the two drawn panes, a widget press, the CLI widget — take `main` as it
 * ships (`onPage`), and where they seeded through the quiet door they
 * `redraw` rather than reload: a reload is a boot. `solo` empties the
 * database between tests, which is the isolation these need; the CLI widget's
 * scrollback it keeps, so a test reads its own command's entry (`answer`).
 *
 * What the CLI computes from an argv is not asked here: the parity fixture
 * runs the same argv against the native binary (parity.test.mjs), and the
 * drawings' shapes are datum-cli's own tests (tests/nowview_cmd.rs).
 */
const shared = sharedApp(DESKTOP);
const onSeam = (fn) => shared.solo(fn, ONE_PANE);
const onPage = (fn) => shared.solo(fn);

/** The CLI widget's entry a typed `line` made: its own output, not the scrollback's. */
const answer = (page, line) =>
	page.locator('.cli .entry').filter({ has: page.locator('.echo', { hasText: line }) }).first();

/** Exit codes: success, the CLI's generic failure, and its duplicate. */
const OK = 0;
const ERROR = 1;
const DUPLICATE = 11;

describe('logging verbs', () => {
	test('occur / start / stop / switch each land an entry the tail shows', async () => {
		await onPage(async ({ run, entries, redraw, paneTable }) => {
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
			// Header and the four entries. da-gvcen: each first log declares
			// its own field in its own `following_state`, so no TRACK
			// document sits beside any of them.
			assert.equal(rows.length, commands.length + 1, `header plus rows: ${listed.stdout}`);
			assert.ok(
				rows.slice(1).some((row) => row.includes('mood') && row.includes('happy')),
				`tail did not show the switch: ${listed.stdout}`
			);

			// And the tail pane is that same listing, laid out in a real
			// table rather than in a terminal's padding — so what is
			// compared is the cells, not the spaces between them. The
			// panes re-run after a command the *page* ran, so redraw to see
			// what a fresh visit shows. The pane shows the rows its height
			// holds (da-qrc6), so the screen (DESKTOP) is tall enough for all four.
			await redraw();
			const table = await paneTable('tail');
			// The pane shows the rows its box holds — the newest page of the
			// listing. No table draws an `action` column at all any more
			// (da-ftg8p), and da-gvcen leaves no TRACK row to name, so the
			// shim only has to survive both being absent.
			const withoutAction = (line) => line.replace(/^(_\S* +)?(action|TRACK.) +/, '$1');
			// The pane shows the CLI's listing, EVERY column of it. da-fknn8
			// splits the state cell into the CLI's own three painted parts
			// so the edges can carry a hover and be selected — and those
			// parts concatenate to the printed cell, so this comparison is
			// as whole as it ever was. vividn: "really mirror the CLI here."
			assert.deepEqual(
				rowTexts([table.headers, ...table.rows].map((cells) => cells.join(' ')).join('\n')),
				[rows[0], ...rows.slice(-table.rows.length)].map(withoutAction),
				'the pane and the CLI widget disagree about the same listing'
			);
		});
	});

	test("every command's entry carries this browser's client id (da-mmkre)", async () => {
		await onSeam(async ({ run, entries, page }) => {
			assert.equal((await run(['occur', 'coffee', '-t', '0700'])).code, OK);
			assert.equal((await run(['occur', 'tea', '-t', '0701'])).code, OK);
			const client = await page.evaluate(() => localStorage.getItem('datum-client-id'));
			assert.match(client, /^web-/);
			assert.deepEqual((await entries()).map((e) => e.client_id), [client, client]);
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
	test('no duration leaves a point entry', async () => {
		await onSeam(async ({ run, entries }) => {
			// Declared first: a FIRST log declares its field in its own
			// `following_state` and is compound for that reason (da-gvcen).
			// What is under test here is MOMENT.
			await run(['setup', 'coffee', '--verb', 'occur']);
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
			// Every command a reader runs is asked for its echo as data
			// (da-xfa4), but in the env, not the argv (da-vx31e): the CLI
			// records argv as the entry's `raw_input`, so the web's plumbing
			// is neither taught nor recorded.
			const [executed] = await seen();
			assert.deepEqual(executed, ['note', 'word', 'correr', 'to run', 'srcLang=pt']);
		});
	});

	test('a widget press echoes the argv it ran, not a reconstruction of it', async () => {
		await onPage(async ({ run, redraw, pressWidget, echoes, spyOnRunner }) => {
			// Give the bar a widget to press: a declared field gets one before
			// its first entry.
			assert.equal((await run(['setup', 'coffee', '--verb', 'occur'])).code, OK);
			await redraw();

			const seen = await spyOnRunner();
			await pressWidget('coffee');
			const [executed] = await seen();
			// No `--show json`: the echo is asked for in the env (da-vx31e).
			assert.deepEqual(executed, ['occur', 'coffee']);
			// The ECHO is the argv the widget asked for (da-xfa4).
			assert.equal((await echoes()).at(-1), 'occur coffee');
			// The first run seen is the press only while no minute tick
			// re-draws the strip inside the window (da-6ehq): the shared
			// app's clock is held (da-ydao), so none does.
		});
	});
});

describe('storage', () => {
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
			// To the next lane: a picture drawn ahead nests the lane's marks a
			// group deeper, with the chunk after them (da-hklaz).
			[...svg.matchAll(/<g class="field" data-field="([^"]*)">((?:(?!<g class="field").)*)/g)]
				.map(([, field, group]) => [
					field,
					/<rect class="now-chunk"[^>]*fill="([^"]*)"/.exec(group)?.[1]
				])
				.filter(([, fill]) => fill !== undefined)
		);

	test('dayview draws the day as an SVG, and the pane shows it', async () => {
		await onPage(async ({ page, run, redraw, paneSvg }) => {
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
			// the hours the two zones straddle a midnight (da-2cgd). And the
			// day is the one the page HOLDS, not node's: a run that crosses
			// midnight before reaching here draws HELD_AT's (da-ne3oe).
			const weekday = new Intl.DateTimeFormat('en-US', {
				timeZone: TIMEZONE,
				weekday: 'short'
			}).format(new Date(HELD_AT));
			assert.match(drawn.stdout, new RegExp(`>${weekday}<`));

			// And the pane is that command's output. Not byte-for-byte:
			// dayview paints a now-line, so the pane's own run of the
			// command a moment later is the same drawing at a later
			// instant — and the strip asks for its bands with
			// `--label-width 0` (da-iu9i), so the day's name is in the
			// column the strip writes beside the picture, not in it.
			await redraw();
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

			const drawn = await run(['nowview', '--context', '30m', '-o', '-']);
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

	// Held mid-afternoon (HELD_AT, the shared app's clock): the clock this
	// draws is the app's own, and unpinned it is the runner's hour — so the
	// assertion below about its SHAPE was one about the machine (da-ldct).
	test('nowview carries the clock and the current state', async () => {
		await onPage(async ({ run, redraw, paneSvg }) => {
			assert.equal((await run(['switch', 'focus', 'deep', '-t', '-5'])).code, OK);

			const drawn = await run(['nowview', '--context', '30m', '-o', '-']);
			assert.equal(drawn.code, OK, drawn.stderr);
			// The viewer's own clock — the one thing about the picture the
			// browser knows and the layout crate cannot.
			assert.match(drawn.stdout, /<text[^>]*>\d{2}:\d{2}<\/text>/);
			// The current state is a chunk past the now-line, not a named
			// panel row — the browser draws the picture the CLI draws
			// because it runs the CLI's renderer (da-nk84).
			assert.equal(nowChunks(drawn.stdout).focus, colorOf('deep'));
			assert.doesNotMatch(drawn.stdout, />focus: deep</, 'the state panel is back');

			await redraw();
			assert.equal(nowChunks(await paneSvg('nowview')).focus, colorOf('deep'));
		});
	});

	test('an empty database still draws both views', async () => {
		await onSeam(async ({ run }) => {
			for (const argv of [['dayview'], ['nowview', '-o', '-'], ['nowview', '--context', '30m', '-o', '-']]) {
				const drawn = await run(argv);
				assert.equal(drawn.code, OK, `${argv[0]} → ${drawn.stderr}`);
				assert.match(drawn.stdout, /^<svg[\s>]/, `${argv[0]} drew nothing`);
			}
		});
	});

	test('a switch, an occur and a switch to nothing draw band, dot and nothing', async () => {
		// The three shapes `render_shape` distinguishes, asserted through
		// the seam. Until da-t7ud this dispatch was a hand-written copy of
		// the table in the browser (#lib/visuals.ts); it is now the CLI's
		// own, so what this test guards is that the wiring reaches it.
		await onSeam(async ({ run }) => {
			assert.equal((await run(['switch', 'mood', 'happy', '-t', '-10'])).code, OK);
			assert.equal((await run(['occur', 'coffee', '-t', '-5'])).code, OK);
			// A switch to `false` terminates a state without opening one.
			assert.equal((await run(['switch', 'sleep', 'false', '-t', '-5'])).code, OK);

			const drawn = await run(['nowview', '--context', '30m', '-o', '-']);
			assert.equal(drawn.code, OK, drawn.stderr);

			// A band: a filled rect in the state's own colour.
			assert.match(
				drawn.stdout,
				new RegExp(`<rect[^>]*fill="${colorOf('happy')}"`),
				`no band for the switch: ${drawn.stdout}`
			);
			// A dot: the DRAWN circle in the field's colour — the disc under
			// it carries the same fill but is never painted (da-qietq).
			assert.match(
				drawn.stdout,
				new RegExp(`<circle class="point-fg"[^>]*fill="${colorOf('coffee')}"`),
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

			const drawn = await run(['nowview', '--context', '30m', '-o', '-']);
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
			for (const argv of [['tail', '-n', '5'], ['dayview', '--days', '3']]) {
				const result = await run(argv);
				assert.equal(result.code, OK, `${argv.join(' ')}: ${result.stderr}`);
			}
			assert.match((await run(['dayview', '--days', '3'])).stdout, /^<svg[\s>]/);
			// A flag no command has is refused by name, as the CLI refuses it.
			const refused = await run(['tail', '--oneline']);
			assert.equal(refused.code, ERROR);
			assert.match(refused.stderr, /--oneline/);
		});
	});
});

describe('setup', () => {
	// Every shape of `datum setup --help` (da-ez40f). The stand-in served
	// `set` alone, and there was no way to remove a field in the browser;
	// the CLI serves them all.
	const SHAPES = [
		['coffee'],
		['coffee', '--json'],
		[],
		['mood', '--add-state', 'happy'],
		['coffee', '--delete']
	];

	test('every spec line answers', async () => {
		await onSeam(async ({ run }) => {
			assert.equal(
				(await run(['setup', 'coffee', '--verb', 'occur', '--color', '#884400'])).code,
				OK
			);
			for (const args of SHAPES) {
				const label = ['setup', ...args].join(' ');
				const result = await run(['setup', ...args]);
				assert.equal(result.code, OK, `${label}: ${result.stderr}`);
				assert.notEqual(result.stdout, '', `${label} answered with nothing at all`);
			}
		});
	});

	test('a spec change still works — it is what the `+` button runs', async () => {
		await onSeam(async ({ run }) => {
			const saved = await run(['setup', 'mood', '--add-state', 'happy']);
			assert.equal(saved.code, OK, saved.stderr);
			assert.match(saved.stdout, /mood/);
			// And a second change after it.
			assert.equal((await run(['setup', 'mood', '--color', '#00ff00'])).code, OK);
		});
	});

	test('typed at the prompt, a spec delete asks, and answers on the screen', async () => {
		// The report this came from was of a blank CLI widget — so one case walks
		// the whole way a reader does, and reads the answer back off the
		// scrollback (da-e0m6's trap).
		await onPage(async ({ submit, page }) => {
			await submit('setup coffee --verb occur');
			// Behind the confirm a delete wears (dbio.ts).
			const typed = submit('setup coffee --delete');
			await page.locator('dialog.confirm').getByRole('button', { name: 'delete', exact: true }).click();
			await typed;
			// The answer is the CLI's DELETE document: what the command did is
			// its label's title, the word the entry's shape (da-hxjrk).
			const entry = answer(page, 'setup coffee --delete');
			await entry.locator('.doc .action[title="DELETE"]').first().waitFor();
			const printed = await entry.innerText();
			assert.notEqual(printed.trim(), '', 'the prompt answered with nothing at all');
			assert.doesNotMatch(
				printed,
				/\bspec\b:?\s+(is\s+)?not available/,
				`the answer calls spec itself unavailable: ${printed}`
			);
			// With no field there is nothing to ask about: the CLI refuses it.
			await submit('setup --delete');
			await answer(page, 'setup --delete').locator('pre.error', { hasText: '--delete' }).waitFor();
			assert.equal(await page.locator('dialog.confirm[open]').count(), 0, 'it asked about a field called --delete');
			// The flag may come first, and it asks all the same. (After the line
			// above: `answer` matches by substring, and this one holds that one.)
			await submit('setup coffee --verb occur');
			const first = submit('setup --delete coffee');
			await page.locator('dialog.confirm').getByRole('button', { name: 'delete', exact: true }).click();
			await first;
			await answer(page, 'setup --delete coffee').locator('.doc .action[title="DELETE"]').first().waitFor();
		});
	});
});

describe('help and version', () => {
	// The parser signals `--help` and `--version` as ParseError variants —
	// their doc comments say the caller renders the text and exits 0. The
	// web runner used to lump them in with real errors, so the first thing
	// a newcomer types into the CLI widget answered `error: help requested` and
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

	test('the CLI widget shows the help a user typed, not an error', async () => {
		await onPage(async ({ page, submit, runLine }) => {
			// Through the prompt itself: what the bead reported is what a
			// person sees in the scrollback, and the scrollback paints
			// stderr as an error.
			await submit('help');
			const entry = answer(page, 'help');
			await entry.locator('pre').first().waitFor();
			const shown = await entry.locator('pre').first().innerText();
			assert.match(shown, /USAGE:/, `the CLI widget showed: ${shown}`);
			assert.doesNotMatch(shown, /help requested/);
			assert.equal(await entry.locator('pre.error').count(), 0);

			// And the seam agrees with what the pane rendered.
			assert.equal((await runLine('help')).stdout.split('\n')[0], shown.split('\n')[0]);
		});
	});
});

describe('the tail is the CLI\'s tail', () => {
	// Colour is the readability half of the report: a field badge and a
	// state chip are how a row is scanned, and the module renders them
	// only because the request says to (`RenderRequest::color`) — it has
	// no environment and no terminal to ask.
	test('paints the field badge, and the pane shows that colour', async () => {
		await onPage(async ({ run, redraw, paneCellColors, tailRows }) => {
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
			await redraw();
			const { rows, columns } = await tailRows();
			// One row per log: each declared its own field (da-gvcen).
			assert.deepEqual(
				rows.map((row) => row.field),
				['coffee', 'mood']
			);
			const cells = (key) => columns.find((column) => column.key === key).cells;
			const painted = async (key) => (await paneCellColors('tail', key)).map(distinct);
			// The pane's newest page, against the listing's last N cells.
			const shown = await painted('field');
			assert.deepEqual(shown, cells('field').slice(-shown.length).map(backgrounds));
			// The state cell too — a different colour from the field's. The
			// CLI's state cell is `[LEAD][state][TRAIL]` and each slot
			// paints its own block, so the whole cell is compared, not one
			// chip. da-fknn8 splits it into those three parts so the edges
			// can carry a hover, and the page draws the CLI's own paint for
			// each — so the backgrounds are unchanged (vividn: "really mirror
			// the CLI here").
			assert.deepEqual((await painted('state')).at(-1), backgrounds(cells('state').at(-1)));
			assert.notDeepEqual(backgrounds(cells('state').at(-1)), backgrounds(cells('field').at(-1)));
			// …and the slot's colour is on the row too, for the hover.
			assert.match(rows.at(-1).previous_state_color, /^#[0-9a-f]{6}$/);
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
