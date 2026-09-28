// The fields widget (da-c7u8): every field `datum fields` lists, Track +
// first, each tracked field shaped by its kind and filled by what it is
// now, the untracked ones dim after them.
//
// The logic is pure (`$lib/fields.ts`) and node loads it as it is; the
// rest is the built app, driven through the bottom bar's prompt — the way
// a person types — and asserted as computed colours, radii and roles,
// never as "different from before".
//
// The five: `fields` (the pure logic, and the widget as it is drawn),
// `fields-press` (what a press runs), `fields-fit` (at a screen's size,
// and its ink), `fields-box` (the pane's own box) and `fields-states` (a
// state field's states). There was a sixth, `fields-bubbles`, over the
// hold panel's `-t` / `dur` bubbles; da-e78x6 removed the panel and the
// file with it. What more than one of them needs is
// `./support/fields.mjs`; a helper one file uses stays in that file.
//
// Budget (da-lt0k): node's `--test-timeout` is per TEST and a FILE is a
// test, so 120 s is this file's whole budget — `before(startApp)`, the
// Chromium it launches, and every test in it. One file held all of this,
// reached 32 tests, and `test timed out after 120000ms` took main's Web
// job red on four of five pushes; a cancelled file hides every other
// file's failures behind it. Measured node 22, at concurrency 3 on the
// 24-core build box under a load average of 95: fields 47s, press 60s,
// fit 63s, states 53s, box 36s (and bubbles, now gone, 35s) — the one
// file was 215s serial. A group that costs a minute wants a file of its own, not another
// `describe` in one of these.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { defaultColor, inkOver, lookOf, pressArgv, requiredKeys, shapeFor, buttonsFor, bands, fieldRows } from '../src/lib/fields.ts';
import { sharedApp, startApp, stopApp } from './support/app.mjs';
import { BLUE, CLEAR, FIXTURE, GREEN, ORANGE, PURPLE, type, untilFieldStyle } from './support/fields.mjs';
import { listed, rowsOf } from './support/rows.mjs';

const spec = (field, rest = {}) => ({ field, ...rest });

describe('the logic', () => {
	test('a spec\'s verb wins, whatever the log says; a spec-less field falls back to the log', () => {
		// occur is a declaration too: a field logged with `true`/`false` under
		// an occur spec still stays a point (da-u8n7), not a block.
		assert.deepEqual(shapeFor(spec('sleep', { verb: 'occur' }), [true, false]), { kind: 'point' });
		assert.deepEqual(shapeFor(spec('coffee', { verb: 'occur' })), { kind: 'point' });
		assert.deepEqual(shapeFor(spec('work', { verb: 'start' })), { kind: 'state' });
		assert.deepEqual(shapeFor(spec('note', { verb: 'add' })), { kind: 'record' });
		// No spec: the log is the only evidence. A boolean log is a block...
		assert.deepEqual(shapeFor(undefined, [true, false]), { kind: 'state' });
		// ...and named states are a menu.
		assert.deepEqual(shapeFor(undefined, ['b', 'a']), { kind: 'states', states: ['a', 'b'] });
	});

	test('every field, a spec or not; `ignored` is the CLI\'s; `now` says what is on', () => {
		const widgets = buttonsFor(
			[
				{ field: 'a', spec: null, states: [true], color: '#111111', state_colors: {}, ignored: false, y: 0 },
				{ field: 'b', spec: spec('b', { verb: 'start' }), states: [], color: '#222222', state_colors: {}, ignored: true, y: 0 },
				{ field: 'c', spec: spec('c', { verb: 'switch' }), states: ['x'], color: '#333333', state_colors: { x: '#444444' }, ignored: false, y: 0 },
				{ field: 'd', spec: spec('d', { verb: 'occur', glyph: '☕' }), states: [], color: '#555555', state_colors: {}, ignored: false, y: 1 }
			],
			// `now --json`'s active rows, which are only ever the truthy ones:
			// a field it does not name — `a` — is off.
			[{ field: 'b', state: true }, { field: 'c', state: 'x' }]
		);
		assert.deepEqual(widgets.map((w) => [w.field, w.shape.kind, w.ignored, w.current, w.glyph]), [
			['a', 'state', false, undefined, undefined],
			['b', 'state', true, true, undefined],
			['c', 'states', false, 'x', undefined],
			['d', 'point', false, undefined, '☕']
		]);
	});

	test('each as high up the pictures as it is drawn, and a field on now moves nowhere (da-xtkcp)', () => {
		// `y` is `fields --json`'s, 0 at the top; equals keep the CLI's
		// alphabetical order. The 09-22 rule put the fields on now first,
		// and that is what moved `mood` in front of `coffee` under a long
		// press (da-8i1kn): an active button already shows its state.
		const row = (field, y) => ({ field, spec: null, states: [], color: '#111111', state_colors: {}, ignored: false, y });
		const widgets = buttonsFor(
			[row('a', 0.9), row('b', 0.1), row('c', 0.5), row('d', 0.7), row('e', 0.1), row('f', 0.3)],
			[{ field: 'd', state: true }, { field: 'f', state: 'x' }]
		);
		assert.deepEqual(widgets.map((w) => w.field), ['b', 'e', 'f', 'c', 'd', 'a']);
	});

	// da-lxqqz: only a key with no default asks anything of the person.
	// da-p3hxn: the kinds are the CLI's (`spec show`'s `keys`); nothing splits a spelling.
	test('requiredKeys: the keys of kind required, in binding order', () => {
		const key = (name, kind, value) => ({ name, kind, value, in_slug: false });
		assert.deepEqual(
			requiredKeys(
				spec('word', {
					keys: [key('lang', 'required'), key('pos', 'default', 'noun'), key('gloss', 'required'), key('note', 'optional'), key('src', 'assign', 'book')]
				})
			),
			['lang', 'gloss']
		);
		assert.deepEqual(requiredKeys(spec('coffee', { data_keys: ['cups'] })), [], 'a spelling alone is never read');
		assert.deepEqual(requiredKeys(null), []);
	});

	test('bands: as many runs as rows, of equal count, none empty', () => {
		const seven = [...'abcdefg'];
		assert.deepEqual(bands(seven, 3), [['a', 'b'], ['c', 'd'], ['e', 'f', 'g']]);
		assert.deepEqual(bands(seven, 1), [seven]);
		assert.deepEqual(bands(['a', 'b'], 3), [['a'], ['b']]);
		assert.deepEqual(bands([], 3), []);
	});

	describe('fieldRows: rows by y, most used first along each (da-xtkcp)', () => {
		const at = (field, y) => ({ field, y });
		/** Six fields, two of them clustered low — the case fixed ranges of `y` get wrong. */
		const six = [at('a', 0.05), at('b', 0.1), at('c', 0.15), at('d', 0.2), at('e', 0.9), at('f', 0.95)];
		const stood = (frecency) => new Map(six.map(({ field, y }) => [field, { y, frecency: frecency[field] ?? 0 }]));
		const names = (rows) => rows.map((row) => row.map((b) => b.field));

		test('equal-count bands of y, top to bottom; each band by frecency, left to right', () => {
			const frecency = { a: 1, b: 3, c: 0.5, d: 2, e: 0, f: 4 };
			assert.deepEqual(names(fieldRows(six, 3, stood(frecency))), [
				['b', 'a'],
				['d', 'c'],
				['f', 'e']
			]);
			// Equals keep the order of y.
			assert.deepEqual(names(fieldRows(six, 2, stood({}))), [
				['a', 'b', 'c'],
				['d', 'e', 'f']
			]);
		});

		test('it reads the standings, not the fields as they are now', () => {
			// `a` has since moved to the bottom and been logged a hundred
			// times: nothing moves until the next sort.
			const now = [at('a', 0.99), ...six.slice(1)];
			assert.deepEqual(names(fieldRows(now, 3, stood({ b: 1 }))), [
				['b', 'a'],
				['c', 'd'],
				['e', 'f']
			]);
		});

		test('a field new since the sort moves nothing: it ends the row its y falls in', () => {
			const withNew = [...six, at('g', 0.16), at('h', 0.01)];
			assert.deepEqual(names(fieldRows(withNew, 3, stood({}))), [
				['a', 'b', 'h'],
				['c', 'd', 'g'],
				['e', 'f']
			]);
		});

		test('before any sort, the rows are the bands of y', () => {
			assert.deepEqual(names(fieldRows(six, 3, new Map())), [
				['a', 'b'],
				['c', 'd'],
				['e', 'f']
			]);
		});
	});

	test('`active` is whether `now` named the field — not whether the state has a colour', () => {
		// Which is a different question from `current`: datum counts an
		// empty string a state, and so active (`is_truthy`,
		// datum-core/src/db/now.rs), but it paints no button. The press
		// ranking asks `active` (da-q5jq); `lookOf` asks `current`.
		const listing = ['a', 'b'].map((field) => ({
			field,
			spec: null,
			states: [],
			color: '#111111',
			state_colors: {},
			ignored: false,
			y: 0
		}));
		const widgets = buttonsFor(listing, [{ field: 'a', state: '' }]);
		assert.deepEqual(
			widgets.map((w) => [w.field, w.active, w.current]),
			[
				['a', true, undefined],
				['b', false, undefined]
			]
		);
	});

	test("the ink is the CLI's YIQ rule", () => {
		// crates/datum-commands/src/render/colors.rs `contrast_fg`, and datum-parity's tests of it.
		assert.equal(inkOver('#ffff00'), '#000000');
		assert.equal(inkOver('#000080'), '#ffffff');
		assert.equal(inkOver('#808080'), '#ffffff', 'brightness 128 is not above 128');
		assert.equal(inkOver('#818181'), '#000000');
	});

	test("the default colour is the CLI's md5 of the name, at every padding boundary", () => {
		// crates/datum-commands/src/render/colors.rs `md5_rgb`: the first three bytes of md5(name).
		assert.equal(defaultColor(''), '#d41d8c');
		assert.equal(defaultColor('abc'), '#900150');
		// Every length through three 64-byte blocks, so the 55/56 padding edge and the second block both run.
		for (const name of [...Array.from({ length: 140 }, (_, n) => 'x'.repeat(n)), '日本語 state']) {
			const want = '#' + createHash('md5').update(name).digest('hex').slice(0, 6);
			assert.equal(defaultColor(name), want, `${name.length} chars`);
		}
	});

	test('a point is an outline with its mark; a block fills while on; a switch takes its state colour inside the field outline; a filled label is always the fill’s ink', () => {
		const w = (shape, rest = {}) => ({ field: 'f', shape, tracked: true, color: '#7c7cff', stateColors: {}, ...rest });
		// da-h2io6: empty, the name in the app's ink, and beside it the mark
		// an occur of it draws — the CLI's dot, or the spec's glyph.
		assert.deepEqual(lookOf(w({ kind: 'point' }, { color: '#ffff00' })), { fill: null, line: '#ffff00', text: 'var(--ink)', label: 'f', glyph: '●' });
		assert.equal(lookOf(w({ kind: 'point' }, { glyph: '☕' })).glyph, '☕');
		assert.equal(lookOf(w({ kind: 'point' }, { color: '#ffff00', current: 'x' })).fill, null, 'never coloured by a state');
		assert.equal(lookOf(w({ kind: 'state' })).glyph, undefined, 'a point\'s alone');
		assert.deepEqual(lookOf(w({ kind: 'state' }, { color: '#0000ff' })), { fill: null, line: '#0000ff', text: '#0000ff', label: 'f' });
		assert.deepEqual(lookOf(w({ kind: 'state' }, { color: '#0000ff', current: true })).fill, '#0000ff');
		const mood = w({ kind: 'states', states: ['good', 'meh'] }, { color: '#800080', stateColors: { good: '#00ff00' }, current: 'good' });
		assert.deepEqual(lookOf(mood), { fill: '#00ff00', line: '#800080', text: '#000000', label: 'f' });
		assert.deepEqual(lookOf(mood, 'state'), { fill: '#00ff00', line: '#800080', text: '#000000', label: 'good' }, 'the variant picks the word, not the ink');
		assert.equal(lookOf({ ...mood, current: 'meh' }).fill, '#800080', 'a state without a colour takes the field colour');
		assert.deepEqual(lookOf(w({ kind: 'record' })), { fill: null, line: '#7c7cff', text: '#7c7cff', label: 'f' }, 'a record is outlined and inked in its (resolved) field colour');

		// da-qnvh: the text was the field's colour and the fill falls back to
		// the field's colour, so the two collapsed — the name of the field
		// printed in exactly its own background. Every colour it could be.
		// `color` is always a resolved `#rrggbb` in production (da-yiyj) — no
		// longer an "uncoloured" case for `lookOf` itself to handle.
		for (const color of ['#7c7cff', '#e8820c', '#00ff00', '#000080']) {
			const painted = lookOf(w({ kind: 'states', states: ['on'] }, { color, current: 'on' }));
			assert.notEqual(painted.text, painted.fill, `a switch in ${color} is painted on itself`);
		}
	});

	test('a block that is on ends; a record is `add`', () => {
		const w = (shape, current) => ({ field: 'f', shape, tracked: true, stateColors: {}, current });
		assert.deepEqual(pressArgv(w({ kind: 'state' })), ['start', 'f']);
		assert.deepEqual(pressArgv(w({ kind: 'state' }, true)), ['end', 'f']);
		assert.deepEqual(pressArgv(w({ kind: 'record' })), ['add', 'f']);
		// The COMMAND and nothing else (da-e78x6): the second argument is the
		// state, not the `-t` / `dur=` an `Adjust` used to carry. A press that
		// appended a flag of its own would say so here.
		assert.deepEqual(pressArgv(w({ kind: 'states', states: ['ok'] }), 'ok'), ['switch', 'f', 'ok']);
	});

	test('an occur spec presses `occur`, even once it has been logged with a boolean state (da-u8n7)', () => {
		const shape = shapeFor(spec('jumps', { verb: 'occur' }), [true]);
		assert.deepEqual(shape, { kind: 'point' });
		assert.deepEqual(pressArgv({ field: 'jumps', shape, tracked: true, stateColors: {} }), ['occur', 'jumps']);
	});

	// da-c8mh: every spec carries a verb, so only a field with NO spec is
	// read off its log — the one the next log is about to write a spec for.
	test('only a spec-less field is read off its log', () => {
		assert.deepEqual(shapeFor(undefined, [true, false]), { kind: 'state' });
		assert.deepEqual(shapeFor(undefined, ['sad', 'ok']), { kind: 'states', states: ['ok', 'sad'] });
		assert.deepEqual(shapeFor(undefined), { kind: 'point' });
	});

	// What Track + declares a field WITH is the Track modal's (da-94q1):
	// `trackArgv` in $lib/track.ts, held to the CLI in tests/track.test.mjs.
});

/** What the widget's entry for `field` looks like, as computed. */
const look = (page, field) =>
	page.locator(`.bar [data-field="${field}"]`).evaluate((el) => {
		const cs = getComputedStyle(el);
		return {
			tag: el.tagName,
			role: el.getAttribute('role'),
			bg: cs.backgroundColor,
			border: cs.borderTopColor,
			color: cs.color,
			radius: parseFloat(cs.borderTopLeftRadius),
			underline: cs.textDecorationLine,
			underlineColor: cs.textDecorationColor,
			text: el.textContent.trim(),
			glyph: ((g) => g && { text: g.textContent, color: getComputedStyle(g).color })(el.querySelector('.glyph'))
		};
	});

describe('in the app', () => {
	before(startApp);
	after(stopApp);

	// ONE APP for the file (da-6b3dj, sharedApp in support/app.mjs): `solo`
	// empties the database each test wrote, and the REPL is read from its end.
	const shared = sharedApp();

	test('Track + first and largest, then every field as high up the pictures as the CLI draws it, each drawn by its kind', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			for (const line of FIXTURE) await type(page, line);
			await page.locator('.bar [data-field="untracked"]').waitFor();

			// Read row by row: Track + starts the top row, and with nothing
			// sorted yet (the seed is typed after the load) the rows are bands
			// of y, so top to bottom and left to right is the order of y.
			const order = (await rowsOf(page.locator('.bar'), 'button.track, [data-field]', (el) => el.field ?? 'Track +')).flat();
			// Nothing is on, so the bar is the pictures' order top to bottom —
			// the `y` `fields --json` resolves (da-vu7pi) — with no
			// tracked/untracked partition and so no dim group after it (da-d08c).
			const byY = JSON.parse((await app.run(['fields', '--json'])).stdout)
				.sort((a, b) => a.y - b.y)
				.map((f) => f.field);
			assert.notDeepEqual(byY, [...byY].sort(), 'this seed is alphabetical by y too: the order proves nothing');
			assert.deepEqual(order, ['Track +', ...byY]);
			const track = await page.locator('.bar button.track').evaluate((el) => ({
				height: el.getBoundingClientRect().height,
				bg: getComputedStyle(el).backgroundColor,
				text: el.textContent.trim()
			}));
			assert.equal(track.text, 'Track +');
			assert.equal(track.bg, 'rgb(124, 124, 255)');
			const heights = await page.locator('.bar button.widget').evaluateAll((all) => all.map((el) => el.getBoundingClientRect().height));
			assert.ok(heights.every((h) => track.height > h), `Track + is ${track.height}px, the fields ${heights}`);

			// da-h2io6: an occur is an empty pill, its mark beside the name in
			// the field's colour — the CLI's dot until the spec names a glyph.
			const coffee = await look(page, 'coffee');
			assert.deepEqual([coffee.bg, coffee.border, coffee.glyph], [CLEAR, ORANGE, { text: '●', color: ORANGE }]);
			assert.equal(coffee.text, '●coffee');
			assert.ok(coffee.radius >= 12, `coffee's radius is ${coffee.radius}`);
			assert.equal((await look(page, 'untracked')).glyph?.text, '●', 'a spec-less point draws the dot too');
			assert.equal((await look(page, 'work')).glyph, null, 'a point\'s alone');

			const work = await look(page, 'work');
			assert.deepEqual([work.bg, work.border, work.color], [CLEAR, BLUE, BLUE]);
			assert.ok(work.radius <= 4, `work's radius is ${work.radius}`);

			assert.equal((await look(page, 'mood')).border, PURPLE);

			const note = await look(page, 'note');
			assert.notEqual(note.role, null);
			assert.notEqual(note.role, 'button');
			assert.equal(note.underline, 'underline');
			// No spec colour: resolved server-side to md5("note") (da-yiyj), not
			// the app's accent — `crates/datum-commands/src/render/colors.rs`.
			assert.equal(note.underlineColor, 'rgb(170, 214, 83)');

			// No spec, logged once: a point, a button like any other.
			assert.equal((await look(page, 'untracked')).tag, 'BUTTON');

			await type(page, 'spec set coffee --glyph ☕');
			await page.waitForFunction(() => document.querySelector('.bar [data-field="coffee"] .glyph')?.textContent === '☕');
			assert.equal((await look(page, 'coffee')).text, '☕coffee', "the spec's glyph, in place of the dot");
		});
	});

	// da-d08c, the three cases as measured: whether a field has a spec says
	// nothing about whether it gets a button — its track/ignore status does.
	test('track tea and occur walk are buttons; an ignored run with a spec is not, and Track + un-ignores it', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			for (const line of ['track tea --verb occur', 'occur walk', 'spec set run --verb occur']) await type(page, line);
			// Drawn BEFORE it is ignored. `['tea', 'walk']` is also what the bar
			// says one beat EARLIER, before `spec set run` lands — the store
			// catches up a refresh behind the echo `type` waits for — so
			// asserting it without first seeing `run` appear passes on the
			// wrong state, and the Track modal then opens on an ignored list
			// that has not heard of `run` either.
			await page.locator('.bar button[data-field="run"]').waitFor();
			await type(page, 'ignore run');
			await page.waitForFunction(() => !document.querySelector('.bar [data-field="run"]'));
			// Which fields are buttons: the order is the test above's.
			const drawn = async () => (await listed(page.locator('.bar'), '[data-field]')).map((el) => `${el.tag}:${el.field}`).sort();
			assert.deepEqual(await drawn(), ['BUTTON:tea', 'BUTTON:walk']);

			// A second on: the clock is held, and a re-track at the instant
			// of the ignore would share its slug.
			await app.clock.forward(1000);
			await page.locator('.bar button.track').click();
			await page.locator('dialog.spec [data-ignored="run"]').click();
			await page.locator('.bar button[data-field="run"]').waitFor();
			assert.equal((await app.echoes()).at(-1), 'track run');
			assert.deepEqual(await drawn(), ['BUTTON:run', 'BUTTON:tea', 'BUTTON:walk']);
		});
	});

	test('a field that arrives by `db restore --merge -` is a button after that command, with no reload', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await type(page, 'occur walk');
			await page.locator('.bar button[data-field="walk"]').waitFor();
			// The backup's own header and walk's own row, re-keyed as a field
			// the page has never seen — what an import from elsewhere brings.
			const [header, ...rows] = (await app.run(['db', 'backup', '-'])).stdout.trim().split('\n');
			// The ENTRY's row, not the TRACK document that declares the field
			// (da-c8mh) — a declaration re-keyed to `swim` would make the
			// button without ever logging anything under it.
			const walk = JSON.parse(rows.find((l) => l.includes('"walk"') && !l.includes('"track"') && !l.includes('"field":"spec"')));
			const swim = { ...walk, hid: 'swim0000', slug: walk.slug.replace('walk', 'swim'), field: 'swim', content_hash: '0'.repeat(64), raw_input: 'occur swim' };
			// Typed, as a person would: the page's own command, so its own beat
			// refreshes the bar. The chooser is awaited BEFORE the echo, never
			// alongside it: the echo cannot land until the file is chosen, so a
			// `type` that waits for the echo deadlocks the chooser it opened and
			// the test could never pass (da-q15y).
			// `--merge`: an import ADDS, and a bare restore would replace —
			// deleting walk, which the page asks about first (da-jrbwc).
			const before = await page.locator('.repl .echo').count();
			const input = page.locator('.prompt-bar input');
			await input.fill('db restore --merge -');
			const [chooser] = await Promise.all([page.waitForEvent('filechooser'), input.press('Enter')]);
			await chooser.setFiles({ name: 'swim.jsonl', mimeType: 'application/jsonl', buffer: Buffer.from(`${header}\n${JSON.stringify(swim)}\n`) });
			await page.waitForFunction((n) => document.querySelectorAll('.repl .echo').length > n, before);
			await page.locator('.bar button[data-field="swim"]').waitFor();
			assert.equal((await app.entries()).some((e) => e.field === 'swim'), true);
		});
	});

	test('a block fills while it is on, a switch takes its state colour inside the field outline', async () => {
		await shared.solo(async ({ page, clock }) => {
			for (const line of FIXTURE) await type(page, line);
			await type(page, 'start work');
			await untilFieldStyle(page, 'work', 'backgroundColor', BLUE);
			// A second on: the clock is held, and a block that ends where it
			// began is no block.
			await clock.forward(1000);
			await type(page, 'end work');
			await untilFieldStyle(page, 'work', 'backgroundColor', CLEAR);
			assert.equal((await look(page, 'work')).border, BLUE);

			await type(page, 'switch mood good');
			await untilFieldStyle(page, 'mood', 'backgroundColor', GREEN);
			assert.equal((await look(page, 'mood')).border, PURPLE);
		});
	});

	test('Track + is there on an empty database, the hint one line beside it', async () => {
		await shared.solo(async ({ page }) => {
			const track = await page.locator('.bar button.track').boundingBox();
			const hint = await page.locator('.bar .hint.empty').boundingBox();
			assert.ok(Math.abs(hint.y + hint.height / 2 - (track.y + track.height / 2)) < 4, 'the hint is not beside Track +');
			assert.ok(hint.height < 24, `the hint is ${hint.height}px tall`);
		});
	});
});
