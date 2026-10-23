// The field form: the web's «which verb?» (da-94q1), and a field's spec.
//
// A field cannot be born verbless (da-c8mh), so declaring one is a
// question with four answers, and this is where the web asks it: the form
// of the field widget (da-fu29p), which opens as a place — a modal on a
// desktop — with a pictogram per verb. vividn, 2026-09-15: «have nice
// pictograms for each of the verbs». What a newcomer meets there — the
// sentence, the examples, the chips — is `fieldwidget.test.mjs`'s.
//
// The claim that rots if nobody watches it is the first one: the four
// glyphs are the CLI's, not the web's. `track_action_label` in
// crates/datum-core/src/track.rs paints `TRACK●`/`█`/`▞`/`¢` where a
// SINGLE document names itself — the write echo and `get` — so a pictogram
// here that drifted from it would teach the wrong shape for the row the
// press is about to write. The test below declares one field per verb
// through the real CLI and reads the glyph back out of `get --show json`.
// Not out of `tail`: a table names no action at all (da-ftg8p), because a
// column only a declaration fills re-lays-out every row around it.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, withApp } from './support/app.mjs';
import {
	cycle, declareArgv, editedSpec, GLYPHS, keyRows, modeOf, specArgv, stateRows, trackLabel, VERBS, verbAsked, wide
} from '../src/lib/track.ts';
import { stripAnsi } from './support/tail.mjs';
import { FIELD_FORM as MODAL, cancelField, declared, openField, pickImage } from './support/field.mjs';

const PHONE = { width: 320, height: 480 };

/**
 * The action word the CLI prints for the document that declared `field` —
 * read off `get --show json`, the surface a single document names itself
 * on. A table has no action column (da-ftg8p).
 */
async function actionWord(app, field) {
	const { stdout } = await app.run(['get', field, '--show', 'json']);
	return stripAnsi(JSON.parse(stdout).docs[0].action).trim();
}

/** Open the field widget on a new field, and wait for its form. */
async function openIt(page) {
	await openField(page);
	await page.locator(MODAL).waitFor();
}

/** Leave the place a new field's Save stays in, on the field it made. */
async function leave(page) {
	await cancelField(page);
	await page.locator(MODAL).waitFor({ state: 'detached' });
}

/** Fill the name box and choose a verb, as a person does. */
async function fill(page, field, verb) {
	await page.locator(`${MODAL} input.field`).fill(field);
	if (verb) await page.locator(`${MODAL} button[data-verb="${verb}"]`).click();
}

/** Add one state row per name, on a switch's Basics (da-i9upr). */
async function addStates(page, names) {
	for (const name of names) {
		await page.locator(`${MODAL} .states + button.add`).click();
		await page.locator(`${MODAL} .state input.name`).last().fill(name);
	}
}

describe('the argv it composes', () => {
	/** What `declareArgv` sends for a new field: the spec it carries, and the flags around it. */
	const declaring = (field, edit, ignore) => {
		const argv = declareArgv(field, { color: '', description: '', states: [], ...edit }, ignore);
		return { around: [...argv.slice(0, 3), ...argv.slice(4)], spec: JSON.parse(argv[3]) };
	};

	// da-2pbvd: a new field's Save is ONE command, which one Undo takes back
	// whole: the spec and the TRACK (or IGNORED) row in one transaction.
	test('declaring a field is one `setup F --from-json` with `--track`, or `--ignore`', () => {
		assert.deepEqual(declaring('coffee', { verb: 'occur' }), {
			around: ['setup', 'coffee', '--from-json', '--track'],
			spec: { field: 'coffee', verb: 'occur', color: null, description: null, allowed_states: null, state_colors: {} }
		});
		assert.deepEqual(declaring('sleep', { verb: 'start' }, true).around, ['setup', 'sleep', '--from-json', '--ignore']);
	});

	const row = (name, color = '', description = '', glyph = '') => ({ name, color, description, glyph });

	test('a switch carries its states, a painted one its colour; a blank state is dropped, a colour optional', () => {
		const { spec } = declaring('mood', { verb: 'switch', color: '#ffa500', states: [row('good', '#00ff00'), row(' bad '), row('  ')] });
		assert.deepEqual([spec.color, spec.allowed_states, spec.state_colors], ['#ffa500', ['good', 'bad'], { good: '#00ff00' }]);
	});

	// da-5j4rn: a new field's states are a switch's alone.
	test('a new field\'s `spec F --track` carries its states on a switch alone', () => {
		const edit = (verb) => ({ verb, color: '', description: '', states: [row('good', '#00ff00')] });
		const sent = (verb) => JSON.parse(declareArgv('coffee', edit(verb))[3]);
		assert.deepEqual(sent('switch').allowed_states, ['good']);
		assert.deepEqual(sent('switch').state_colors, { good: '#00ff00' });
		// The states belong to `switch` alone: a field given one and then
		// changed to another verb would otherwise smuggle it out.
		for (const verb of ['occur', 'start', 'add']) {
			assert.equal(sent(verb).allowed_states, null, verb);
			assert.deepEqual(sent(verb).state_colors, {}, verb);
		}
	});

	// da-h119q, da-p3hxn: a field's keys are declared in the same command,
	// as slots in the form's order (da-uipyt) — they bind by position, as
	// subfields would not — `-K` ones in the slug, an assign in its map.
	test('keys ride on the declaration as slots: `K`, `K=`, a default after `=`, in the slug or not, an assign in its map, a blank name dropped', () => {
		const key = (name, kind, value = '', in_slug = false) => ({ name, kind, value, value_type: 'any', in_slug });
		const keys = [
			key('id', 'required', '', true),
			key(' cups ', 'required'),
			key('size', 'optional', 'x'),
			key('temp', 'default', 'hot'),
			key('brand', 'assign', 'acme'),
			key(' ', 'required')
		];
		const { spec } = declaring('coffee', { verb: 'occur', keys });
		assert.deepEqual([spec.id_keys, spec.data_keys, spec.assign], [['id'], ['cups', 'size=', 'temp=hot'], { brand: 'acme' }]);
	});

	// da-ci85o: an edited spec is saved WHOLE, in one `setup F` — the
	// shape that replaces the list, so a state removed takes its colour and
	// description with it, and a blank clears where a flag could not.
	test('an edited spec is the whole spec, in one `setup --from-json`', () => {
		const spec = {
			field: 'mood', verb: 'switch', color: '#123456', description: null,
			allowed_states: ['good', 'bad'], state_colors: { good: '#00ff00', bad: '#ff0000' },
			state_descriptions: { bad: 'awful' }, subfields: { note: {} }, tags: ['x']
		};
		assert.deepEqual(stateRows(spec), [row('good', '#00ff00'), row('bad', '#ff0000', 'awful')]);
		const edited = editedSpec(spec, {
			verb: 'start', color: '', description: ' how I feel ',
			states: [row('good', '#00ff00', 'fine'), row(' meh ', ''), row('')]
		});
		assert.deepEqual(edited, {
			...spec, verb: 'start', color: null, description: 'how I feel',
			allowed_states: ['good', 'meh'], state_colors: { good: '#00ff00' },
			state_descriptions: { good: 'fine' }
		});
		assert.deepEqual(edited.subfields, spec.subfields, 'what the modal does not edit is carried');
		// `setup F --json` leaves an empty description map out; so does the save.
		const bare = { ...spec, state_descriptions: undefined };
		delete bare.state_descriptions;
		assert.deepEqual(stateRows(bare), [row('good', '#00ff00'), row('bad', '#ff0000')]);
		assert.equal('state_descriptions' in editedSpec(bare, { verb: 'switch', color: '', description: '', states: stateRows(bare) }), false);
		// A spec with no list keeps none when none is added.
		assert.equal(editedSpec({ ...spec, allowed_states: null }, { verb: 'occur', color: '', description: '', states: [] }).allowed_states, null);
		const argv = specArgv(edited);
		assert.deepEqual(argv.slice(0, 3), ['setup', 'mood', '--from-json']);
		assert.deepEqual(JSON.parse(argv[3]), edited);
	});

	// da-ogyxa: a state's glyph goes with the state when the state leaves the
	// list, and ONLY then — `state_glyphs` may name a state the list never
	// did, and a caller whose rows carry no glyph must not wipe them.
	test('a removed state takes its glyph with it; a kept or renamed one keeps it', () => {
		const spec = {
			field: 'mood', verb: 'switch', color: null, description: null,
			allowed_states: ['up', 'down', 'flat'], state_colors: {},
			state_glyphs: { up: '▲', down: '▼', flat: '■', logged: '○' }
		};
		assert.deepEqual(stateRows(spec).map((r) => r.glyph), ['▲', '▼', '■']);
		const base = { verb: 'switch', color: '', description: '' };
		// `down` removed, `flat` renamed to `level` (the row carries its glyph), `up` cleared.
		const [up, , flat] = stateRows(spec);
		const edited = editedSpec(spec, { ...base, states: [{ ...up, glyph: '' }, { ...flat, name: 'level' }] });
		assert.deepEqual(edited.state_glyphs, { level: '■', logged: '○' });
		// Rows with no glyph key — the rows that re-add colours — keep every glyph of a kept state.
		const bare = editedSpec(spec, { ...base, states: [{ name: 'up', color: '', description: '' }] });
		assert.deepEqual(bare.state_glyphs, { up: '▲', logged: '○' });
		// None left: the key is left out, as `setup F --json` leaves it out.
		assert.equal('state_glyphs' in editedSpec({ ...spec, state_glyphs: { down: '▼' } }, { ...base, states: [{ ...up, glyph: '' }] }), false);
	});

	test('one glyph, or a glyph by a key with the field\'s as the default', () => {
		const spec = {
			field: 'tea', verb: 'occur', color: null, description: null,
			allowed_states: null, state_colors: {}, glyph: '●', glyph_by: { key: 'size', map: { big: '☕' } },
			subfields: { size: {} }, data_keys: ['cup=mug'],
			keys: [{ name: 'cup', kind: 'default', value: 'mug', in_slug: false }, { name: 'size', kind: 'optional', in_slug: false }]
		};
		const base = { verb: 'occur', color: '', description: '', states: [] };
		// Not edited: carried as it was.
		assert.deepEqual(editedSpec(spec, base).glyph_by, spec.glyph_by);
		assert.equal(editedSpec(spec, base).glyph, '●');
		// «one glyph»: no key; an empty glyph draws the circle and is left out.
		const one = editedSpec(spec, { ...base, glyph: '', glyph_by: null });
		assert.equal('glyph' in one, false);
		assert.equal('glyph_by' in one, false);
		// A map row with no value or no glyph says nothing, and is dropped.
		const by = editedSpec(spec, { ...base, glyph: '○', glyph_by: { key: ' cup ', map: { ' mug ': '🍵', '': '▲', tiny: '' } } });
		assert.deepEqual(by.glyph_by, { key: 'cup', map: { mug: '🍵' } });
		assert.equal(by.glyph, '○');
		assert.equal('glyph_by' in editedSpec(spec, { ...base, glyph_by: { key: ' ', map: {} } }), false);
	});

	// The band gives each slice of time ONE column, so a glyph two wide draws
	// as `●` there (`push_cell_point`, by unicode-width 0.2.2). These are that
	// crate's answers, measured, for the whole grid and a few probes past it.
	test('the two-column flag is unicode-width\'s, over the whole grid', () => {
		const measured = [...'☔⚡🌙🌈⛅⭐☕🍵🍺🍷🥛🍎🍞🍕🍫🍩🍪🥗'];
		const grid = GLYPHS.flatMap((g) => g.glyphs);
		assert.deepEqual(grid.filter(wide), measured);
		assert.deepEqual([...'日한Ａｱ¢é▞'].filter(wide), [...'日한Ａ']);
		assert.equal(wide(''), false);
	});

	// da-cap5m: the keys are the whole answer, a default stored as it was
	// typed; the aliases are sent only once datum no longer makes them.
	// da-p3hxn: the rows are the CLI's parsed `keys`, and each is saved in
	// ONE place — a slot, or an assign in the map. da-zqnt3: every slot in
	// the rows' order, which is the order its words bind in, and a typed key
	// keeps a subfield for its type.
	test('keys are saved whole, each in one place, and only a list datum does not make is sent', () => {
		const sub = (over) => ({ required: false, value_type: 'any', in_slug: false, default: null, ...over });
		const spec = {
			field: 'word', verb: 'add', color: null, description: null, allowed_states: null, state_colors: {},
			subfields: { pos: sub({ required: true, value_type: 'string', in_slug: true }), reps: sub({ value_type: 'number', default: 3 }) },
			id_keys: [], data_keys: ['lang=pt'], assign: { src: 'book' },
			keys: [
				{ name: 'lang', kind: 'default', value: 'pt', in_slug: false },
				{ name: 'pos', kind: 'required', in_slug: true },
				{ name: 'reps', kind: 'default', value: '3', in_slug: false },
				{ name: 'src', kind: 'assign', value: 'book', in_slug: false }
			],
			derived_aliases: [{ state: 'word', expands_to: 'add word', kind: 'bare_field' }], no_autoalias: false
		};
		const row = (name, kind, value = '', over = {}) => ({ name, kind, value, value_type: 'any', in_slug: false, ...over });
		const keys = keyRows(spec);
		assert.deepEqual(keys, [
			row('lang', 'default', 'pt'),
			row('pos', 'required', '', { value_type: 'string', in_slug: true }),
			row('reps', 'default', '3', { value_type: 'number' }),
			row('src', 'assign', 'book')
		]);
		const base = { verb: 'add', color: '', description: '', states: [] };
		keys[0].kind = 'optional';
		Object.assign(keys[1], { kind: 'default', value: '42' });
		keys[2].kind = 'assign';
		keys[3].kind = 'required';
		const made = editedSpec(spec, { ...base, keys: [...keys, row('n', 'default', '42'), row(' ', 'default')], autoalias: true, aliases: [] });
		assert.deepEqual(made.data_keys, ['lang=', 'src', 'n=42'], 'every key a slot, in the rows\' order');
		assert.deepEqual(made.id_keys, ['pos=42']);
		assert.deepEqual(made.assign, { reps: '3' }, 'an assign leaves the slots, and the one that was is gone');
		assert.deepEqual(made.subfields, { pos: sub({ value_type: 'string', in_slug: true, default: '42' }) }, 'only a typed key keeps a subfield');
		assert.equal('keys' in made, false, "the CLI's reading of the spec is not sent");
		assert.equal('id_assign' in made, false, 'an empty map is left out, as `setup F --json` prints it');
		assert.equal(made.no_autoalias, false);
		assert.deepEqual(made.derived_aliases, spec.derived_aliases, 'a made list is carried: the CLI makes it again');

		const mine = editedSpec(spec, { ...base, autoalias: false, aliases: [{ state: ' zz ', expands_to: ' add word ' }, { state: '', expands_to: 'x' }] });
		assert.equal(mine.no_autoalias, true);
		assert.deepEqual(mine.derived_aliases, [{ state: 'zz', expands_to: 'add word' }]);
		assert.deepEqual([mine.subfields, mine.data_keys, mine.assign], [spec.subfields, spec.data_keys, spec.assign], 'keys the edit leaves out are carried');
	});

	// da-45wgz: the icon is replaced when given, cleared when given empty —
	// the key left out, as `setup F --json` leaves it — and carried when not given.
	test('an edited icon is set, cleared, or carried', () => {
		const spec = { field: 'coffee', verb: 'occur', color: null, description: null, allowed_states: null, state_colors: {}, icon: '<svg/>' };
		const edit = { verb: 'occur', color: '', description: '', states: [] };
		assert.equal(editedSpec(spec, edit).icon, '<svg/>');
		assert.equal('icon' in editedSpec(spec, { ...edit, icon: '' }), false);
		const set = editedSpec({ ...spec, icon: undefined }, { ...edit, icon: '<svg viewBox="0 0 1 1"/>\n' });
		assert.equal(set.icon, '<svg viewBox="0 0 1 1"/>');
	});

	// da-fu29p: where a line's leftover words go rides on the declaration too.
	// da-zqnt3: it is a mode of one row — `-R`, `-SR` or `-C` — saved as
	// the spec's remainder, and carried when the keys are not edited.
	test('the row that takes the rest of the line is `-R`, `-SR` or `-C`, saved as the remainder on a declaration and an edit', () => {
		const key = (name, rest) => ({ name, kind: 'optional', value: '', value_type: 'any', in_slug: false, ...(rest && { rest }) });
		const rest = (keys) => {
			const { spec } = declaring('f', { verb: 'occur', keys });
			return [spec.data_keys, spec.remainder, spec.string_remainder, spec.comment_remainder];
		};
		assert.deepEqual(rest([key(' emotions ', 'R')]), [[], 'emotions', undefined, undefined]);
		assert.deepEqual(rest([key('words', 'SR'), key('n')]), [['n='], 'words', true, undefined]);
		assert.deepEqual(rest([key('', 'C')]), [[], undefined, undefined, true]);
		assert.deepEqual(rest([key(' ', 'R')]), [[], undefined, undefined, undefined], 'a row with no name takes nothing');
		const spec = { field: 'emotion', verb: 'occur', color: null, description: null, allowed_states: null, state_colors: {}, remainder: 'emotions' };
		const edit = { verb: 'occur', color: '', description: '', states: [] };
		assert.deepEqual(keyRows(spec), [key('emotions', 'R')]);
		assert.deepEqual(keyRows({ ...spec, string_remainder: true }), [key('emotions', 'SR')]);
		assert.deepEqual(keyRows({ ...spec, remainder: undefined, comment_remainder: true }), [key('comment', 'C')]);
		assert.equal(editedSpec(spec, edit).remainder, 'emotions', 'carried when the keys are not edited');
		const routed = (keys) => {
			const { remainder, string_remainder, comment_remainder } = editedSpec({ ...spec, string_remainder: true }, { ...edit, keys });
			return [remainder, string_remainder, comment_remainder];
		};
		assert.deepEqual(routed([key(' words ', 'R')]), ['words', undefined, undefined]);
		assert.deepEqual(routed([key('words', 'SR')]), ['words', true, undefined]);
		assert.deepEqual(routed([key('', 'C')]), [undefined, undefined, true]);
		assert.deepEqual(routed([key('n')]), [undefined, undefined, undefined], 'no row takes it: cleared');
	});

	// vividn, 2026-10-14: «at the beginning have (-k) button that if you click
	// let's you select -R -SR -C as well».
	test('the mode button cycles -k -K -R -SR -C, and only one row takes the rest of the line', () => {
		const key = (name) => ({ name, kind: 'required', value: '', value_type: 'any', in_slug: false });
		const [a, b] = [key('a'), key('b')];
		const rows = [a, b];
		const seen = [];
		for (let i = 0; i < 6; i++) seen.push([modeOf(a), a.in_slug]), cycle(rows, a);
		assert.deepEqual(seen, [['k', false], ['K', true], ['R', false], ['SR', false], ['C', false], ['k', false]]);
		cycle(rows, a), cycle(rows, a); // a: -R
		cycle(rows, b), cycle(rows, b); // b: -R, which a gives up
		assert.deepEqual(rows.map(modeOf), ['k', 'R']);
		cycle(rows, a); // a: -K, and b keeps the rest
		assert.deepEqual(rows.map(modeOf), ['K', 'R']);
	});

	test('the refusal is read for the field it names, and nothing else is', () => {
		const said = (stderr, code = 1) => ({ argv: [], code, stdout: '', stderr });
		assert.equal(
			verbAsked(['track', 'mood'], said("error: 'mood' is new — say what a press of it means: --verb occur|start|switch|add")),
			'mood'
		);
		assert.equal(verbAsked(['track', 'mood'], said('', 0)), null, 'a success is not a question');
		assert.equal(verbAsked(['occur', 'mood'], said("'mood' is new — say what a press of it means:")), null);
		assert.equal(verbAsked(['track', 'mood'], said('error: no such field')), null);
	});
});

describe('in the app', () => {
	before(startApp);
	after(stopApp);

	// ONE APP for the tests on the default screen (da-6b3dj, sharedApp in
	// support/app.mjs): `solo` empties the database between them and each
	// reads its own echo from the end, and `home` closes a modal a test left
	// open. The pictograms (`?repl`) and the phone screen open their own.
	const shared = sharedApp();

	// The claim this file exists for. One field per verb, declared through
	// the real CLI, and the action cell `tail` paints for each declaration
	// read back: the web's table has to BE those four glyphs.
	test('the four pictograms are the glyphs the CLI prints for the row each one writes', async () => {
		await withApp(async (app) => {
			for (const { verb } of VERBS) await app.run(declareArgv(`f_${verb}`, { verb, color: '', description: '', states: [] }));
			for (const { verb, glyph } of VERBS) {
				assert.equal(await actionWord(app, `f_${verb}`), `TRACK${glyph}`, `the ${verb} pictogram is not the CLI's`);
				assert.equal(trackLabel(verb), `TRACK${glyph}`);
			}
			// And the table names none of them: no `action` column at all.
			const { columns } = await app.tailRows();
			assert.deepEqual(columns.filter((c) => c.key === 'action'), []);
			// The day frame is scenery here — nothing but the seam is read
			// (docs/web-conventions.md #21).
		}, { query: 'repl' });
	});

	test('a new field opens the form on the four pictograms, Save shut until a name and a verb', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await openIt(page);
			const commit = page.locator(`${MODAL} button.commit`);
			assert.equal(await commit.isEnabled(), false, 'nothing named, nothing chosen');
			await fill(page, 'mood', null);
			assert.equal(await commit.isEnabled(), false, 'a name alone is not a field');
			const glyphs = await page.locator(`${MODAL} .verb .glyph`).allInnerTexts();
			assert.deepEqual(glyphs.map((g) => g.trim()), VERBS.map((v) => v.glyph));
			// Named as the welcome names it, with its line beside it.
			assert.deepEqual(await page.locator(`${MODAL} .verb .name`).allInnerTexts(), VERBS.map((v) => v.what));
			assert.deepEqual(await page.locator(`${MODAL} .verb .line`).allInnerTexts(), VERBS.map((v) => v.line));
			await page.locator(`${MODAL} button[data-verb="occur"]`).click();
			await fill(page, '', null);
			assert.equal(await commit.isEnabled(), false, 'a verb alone is not a field');
			await fill(page, 'mood', null);
			assert.equal(await commit.isEnabled(), true);

			// The states belong to the switch, and to nothing else.
			assert.equal(await page.locator(`${MODAL} .states`).count(), 0);
			await page.locator(`${MODAL} button[data-verb="switch"]`).click();
			await addStates(page, ['good', 'bad']);
			await commit.click();
			await page.locator('.bar button[data-field="mood"]').waitFor();
			const { line, spec } = declared((await app.echoes()).at(-1));
			assert.equal(line, 'setup mood --from-json --track');
			assert.deepEqual([spec.verb, spec.allowed_states], ['switch', ['good', 'bad']]);
			assert.deepEqual(app.errors, []);
		});
	});

	test('Done declares the field — TRACK▞ in the log, a button in the bar', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await openIt(page);
			await fill(page, 'mood', 'switch');
			await addStates(page, ['good', 'bad']);
			await page.locator(`${MODAL} button.commit`).click();

			await page.locator('.bar button[data-field="mood"]').waitFor();
			assert.match((await app.echoes()).at(-1), /^setup mood --from-json .* --track$/);
			const spec = JSON.parse((await app.run(['setup', 'mood', '--json'])).stdout);
			assert.equal(spec.verb, 'switch');
			assert.deepEqual(spec.allowed_states, ['good', 'bad']);

			assert.equal(await actionWord(app, 'mood'), 'TRACK▞');
			// And the widget stays, on the field it made (da-fu29p).
			assert.equal((await page.locator(`${MODAL} h2`).innerText()).trim(), 'mood');
		});
	});

	test('Escape runs nothing', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await openIt(page);
			await fill(page, 'mood', 'occur');
			await page.keyboard.press('Escape');
			await page.waitForFunction((sel) => !document.querySelector(sel), MODAL);
			await app.idle();
			assert.deepEqual(await app.allEntries(), []);
			assert.equal(await page.locator('.bar button[data-field="mood"]').count(), 0);
		});
	});

	// The door a filtered list's `+` comes in by (the commands widget): the
	// verb is already chosen, so the form opens on it.
	test('a preset verb opens already chosen, and only that one', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			const { saved: ran } = await openField(page, '', 'occur');
			await page.locator(MODAL).waitFor();
			assert.deepEqual(
				await page.locator(`${MODAL} .verb[aria-checked="true"]`).evaluateAll((all) => all.map((b) => b.dataset.verb)),
				['occur']
			);
			await fill(page, 'coffee', null);
			await page.locator(`${MODAL} button.commit`).click();
			const result = await ran;
			assert.equal(result.code, 0, result.stderr);
			await page.locator('.bar button[data-field="coffee"]').waitFor();
			assert.equal(JSON.parse((await app.run(['setup', 'coffee', '--json'])).stdout).verb, 'occur');
		});
	});

	// The CLI's own refusal is the third door: `track <field>` on a field
	// with neither a spec nor a log comes back asking, and the form is the
	// asking. What it saves replaces the refused command.
	test('a bare `track` on a new field opens the form, with the field already in it', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			const typed = page.evaluate(() => window.datum.runLine('track sleep'));
			await page.locator(MODAL).waitFor();
			assert.equal(await page.locator(`${MODAL} input.field`).inputValue(), 'sleep');
			await page.locator(`${MODAL} button[data-verb="start"]`).click();
			await page.locator(`${MODAL} button.commit`).click();
			assert.equal((await typed).code, 0);
			await page.locator('.bar button[data-field="sleep"]').waitFor();
			assert.equal(JSON.parse((await app.run(['setup', 'sleep', '--json'])).stdout).verb, 'start');
		});
	});

	test('an ignored field is offered back, and a press re-tracks it', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await app.submit('track run --verb occur');
			// Drawn BEFORE it is taken away: a bar that has not got round to
			// drawing `run` yet satisfies "no run button" too, and the form
			// would then open on an ignored list the widget had not seen.
			await page.locator('.bar button[data-field="run"]').waitFor();
			// A second between the three: the clock is held (da-ydao), and
			// each declaration of `run` needs an instant of its own.
			await app.clock.forward(1000);
			await app.submit('ignore run');
			await page.waitForFunction(() => !document.querySelector('.bar [data-field="run"]'));
			await app.clock.forward(1000);
			await openIt(page);
			await page.locator(`${MODAL} [data-ignored="run"]`).click();
			await page.locator('.bar button[data-field="run"]').waitFor();
			assert.equal((await app.echoes()).at(-1), 'track run');
		});
	});

	// da-oxaij: the button's title names a command that works when typed.
	test('an ignored spaced field’s title quotes it', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await app.submit('setup "my x" --verb occur');
			await page.locator('.bar button[data-field="my x"]').waitFor();
			await app.clock.forward(1000);
			await app.submit('ignore "my x"');
			await page.waitForFunction(() => !document.querySelector('.bar [data-field="my x"]'));
			await app.clock.forward(1000);
			await openIt(page);
			assert.equal(await page.locator(`${MODAL} [data-ignored="my x"]`).getAttribute('title'), 'track "my x"');
		});
	});

	// da-ci85o: declaring is the spec's form on Basics, and it must stay as
	// quick as the modal it absorbed: a shape, the name box (a shape stopped
	// opening the keyboard, da-zz9rn), Save. The fields widget's (+) that
	// opened it, the fourth tap, is gone (da-myet1).
	test('declaring a field in the open form is three taps: a shape, the name, Save', async () => {
		await withApp(async (app) => { // a listener of its own, which `solo` would keep
			const { page } = app;
			await page.evaluate(() => {
				window.__taps = 0;
				document.addEventListener('pointerdown', (e) => e.isTrusted && window.__taps++, true);
			});
			await openIt(page);
			await page.locator(`${MODAL} button[data-verb="occur"]`).click();
			await page.locator(`${MODAL} input.field`).click();
			await page.keyboard.type('coffee');
			await page.locator(`${MODAL} button.commit`).click();
			await page.locator('.bar button[data-field="coffee"]').waitFor();
			assert.equal(await page.evaluate(() => window.__taps), 3);
		});
	});

	// The same form on a field that has a spec: everything it edits comes
	// back in ONE `setup F`, and `setup F --json` then reads what was sent.
	test('editing a spec writes one `setup F`, and the CLI reads back what it was sent', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'sleep', '--verb', 'start']);
			await app.run(['setup', 'sleep', '--description', 'night']);
			const before = (await app.echoes()).length;
			const { saved } = await openField(page, 'sleep');
			await page.locator(MODAL).waitFor();
			assert.equal((await page.locator(`${MODAL} h2`).innerText()).trim(), 'sleep');
			assert.equal(await page.locator(`${MODAL} .verb[aria-checked="true"]`).getAttribute('data-verb'), 'start');
			assert.equal(await page.locator(`${MODAL} input.description`).inputValue(), 'night');

			await page.locator(`${MODAL} button[data-verb="switch"]`).click();
			// The swatch is the control: a tap opens the set, a pick sets it and folds the set away.
			const swatch = page.locator(`${MODAL} .row .swatch[aria-expanded]`);
			await swatch.click();
			const pick = page.locator(`${MODAL} .set .swatch`).nth(3);
			const color = await pick.getAttribute('data-color');
			await pick.click();
			assert.equal(await page.locator(`${MODAL} .set`).count(), 0);
			assert.equal(await swatch.getAttribute('data-color'), color);

			await addStates(page, ['awake', 'asleep']);
			await page.locator(`${MODAL} .state`).nth(1).locator('.swatch[aria-expanded]').click();
			const statePick = page.locator(`${MODAL} .state .set .swatch`).nth(8);
			const stateColor = await statePick.getAttribute('data-color');
			await statePick.click();
			await page.locator(`${MODAL} .state input.meaning`).first().fill('up');
			await page.locator(`${MODAL} button.commit`).click();

			const result = await saved;
			assert.equal(result.code, 0, result.stderr);
			await app.idle();
			const echoes = await app.echoes();
			assert.equal(echoes.length, before + 1, `one command, not a stream: ${echoes.slice(before).join(' | ')}`);
			assert.match(echoes.at(-1), /^setup sleep --from-json /);

			const sent = JSON.parse(result.argv[result.argv.indexOf('--from-json') + 1]);
			const shown = JSON.parse((await app.run(['setup', 'sleep', '--json'])).stdout);
			// The CLI regenerates the aliases a spec derives from its states,
			// so those are the one key it does not take as sent; and `keys` is
			// its reading of the spec, which is never sent (da-p3hxn).
			const { derived_aliases: _sent, ...kept } = sent;
			const { derived_aliases: _shown, keys: _keys, ...read } = shown;
			assert.deepEqual(read, kept);
			assert.equal(shown.verb, 'switch');
			assert.equal(shown.color, color);
			assert.equal(shown.description, 'night');
			assert.deepEqual(shown.allowed_states, ['awake', 'asleep']);
			assert.deepEqual(shown.state_colors, { asleep: stateColor });
			assert.deepEqual(shown.state_descriptions, { awake: 'up' });

			// Opened and saved untouched, a spec with no state descriptions —
			// the key `setup F --json` leaves out — reads back byte for byte.
			await app.run(['setup', 'sleep', '--state-description', 'awake=']);
			const untouched = (await app.run(['setup', 'sleep', '--json'])).stdout;
			assert.doesNotMatch(untouched, /state_descriptions/);
			const { saved: again } = await openField(page, 'sleep');
			await page.locator(`${MODAL} button.commit`).click();
			assert.equal((await again).code, 0);
			assert.equal((await app.run(['setup', 'sleep', '--json'])).stdout, untouched);
		});
	});

	// da-klaf8: a key this datum does not know — a newer datum's — rides
	// through the form's save, so an older datum.day editing a spec never
	// erases what a newer copy wrote to it.
	test('a key the form does not know is saved back as it was read', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'mood', '--verb', 'switch']);
			const spec = JSON.parse((await app.run(['setup', 'mood', '--json'])).stdout);
			const future = { a: [1, 2.5] };
			await app.run(['setup', 'mood', '--from-json', JSON.stringify({ ...spec, zz_future_key: future })]);
			const { saved } = await openField(page, 'mood');
			await page.locator(`${MODAL} input.description`).fill('how I feel');
			await page.locator(`${MODAL} button.commit`).click();
			const result = await saved;
			assert.equal(result.code, 0, result.stderr);
			const shown = JSON.parse((await app.run(['setup', 'mood', '--json'])).stdout);
			assert.equal(shown.description, 'how I feel');
			assert.deepEqual(shown.zz_future_key, future);
		});
	});

	// da-45wgz, da-rpfws: an .svg is a symbol — «image (svg)» in the symbol's
	// picker — saved as the spec's icon, which the dayview draws; the CLI's
	// band is text, and draws the dot: vividn, «if someone chooses this, it can
	// just be a dot glyph or something in the cli». So the image takes the
	// place of a glyph the field had. What the file must be is the CLI's to
	// say, and it says it here too.
	test('an .svg picked as the symbol is the field\'s icon, drawn by the dayview and as the dot in `now`; a bad one is the CLI\'s refusal', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'coffee', '--verb', 'occur', '--glyph', '▲']);
			// The clock is held (da-ydao): a second on, or the dot lands on the TRACK and is not written.
			await app.clock.forward(1000);
			await app.run(['occur', 'coffee']);
			const beans = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><ellipse cx="12" cy="12" rx="6" ry="9" fill="currentColor"/></svg>';
			const pick = async (name, svg) => {
				const { saved } = await openField(page, 'coffee');
				await page.locator(MODAL).waitFor();
				await pickImage(page, name, svg);
				assert.equal(await page.locator(`${MODAL} .picker`).count(), 0, 'the pick closed the picker');
				await page.locator(`${MODAL} button.commit`).click();
				return saved;
			};

			const ok = await pick('beans.svg', beans);
			assert.equal(ok.code, 0, ok.stderr);
			const shown = JSON.parse((await app.run(['setup', 'coffee', '--json'])).stdout);
			assert.equal(shown.icon, beans);
			assert.equal('glyph' in shown, false, 'the image took the glyph\'s place: the standard dot');
			const svg = (await app.run(['dayview'])).stdout;
			assert.equal(svg.match(/<svg id="icon-coffee"/g)?.length, 1);
			assert.match(svg, /href="#icon-coffee"/);
			assert.match(await band(app, 'coffee'), /●/);
			assert.doesNotMatch(await band(app, 'coffee'), /▲/);

			const big = await pick('big.svg', `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1 1"><path d="${'M0 0'.repeat(1200)}"/></svg>`);
			assert.notEqual(big.code, 0);
			assert.match(big.stderr, /at most 4 kB/);
			assert.equal(JSON.parse((await app.run(['setup', 'coffee', '--json'])).stdout).icon, beans, 'the refusal wrote nothing');
		});
	});

	// A spec is a place (da-hbrfg), and these are its two ways back.
	test('Cancel and the browser\'s back leave a spec and write nothing', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'sleep', '--verb', 'start']);
			const spec = (await app.run(['setup', 'sleep', '--json'])).stdout;
			const before = (await app.echoes()).length;
			for (const dismiss of [() => cancelField(page), () => page.goBack()]) {
				const { saved: shut } = await openField(page, 'sleep');
				await page.locator(`${MODAL} input.description`).fill('h');
				await dismiss();
				assert.equal(await shut, null);
				await page.waitForFunction((sel) => !document.querySelector(sel), MODAL);
			}
			await app.idle();
			assert.equal((await app.run(['setup', 'sleep', '--json'])).stdout, spec);
			assert.equal((await app.echoes()).length, before);
		});
	});

	// vividn: «show the color swatch, and then if the user taps on it, they can
	// easily select a new one» — no hex box, declaring or editing.
	test('no hex box anywhere: every input is a name or words, and a colour is a swatch', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			const inputs = () =>
				page.locator(`${MODAL} input`).evaluateAll((els) => els.map((e) => [e.type, e.value, e.placeholder, e.className]));
			const noHex = async (where) => {
				for (const [type, value, placeholder, cls] of await inputs()) {
					// The icon's picker (da-45wgz) takes a file, and no typing at all;
					// whether off is a state (da-cwieh, da-i9upr) is a box to tick.
					assert.ok(
						type === 'text' || (type === 'file' && cls.includes('icon')) || (type === 'checkbox' && cls.includes('off-state')),
						`${where}: input.${cls} is a ${type}`
					);
					assert.doesNotMatch(`${value} ${placeholder}`, /#|hex|rgb/i, `${where}: input.${cls} takes a colour`);
				}
			};
			await openIt(page);
			await fill(page, 'mood', 'switch');
			await noHex('track, basics');
			await addStates(page, ['good']);
			await noHex('track, states');
			await page.keyboard.press('Escape');
			await page.waitForFunction((sel) => !document.querySelector(sel), MODAL);

			await app.run(['track', 'mood', '--verb', 'switch', '--add-state', 'good', '--state-color', 'good=#00ff00', '--color', '#123456']);
			const { saved: shut } = await openField(page, 'mood');
			await page.locator(MODAL).waitFor();
			await noHex('spec, basics');
			assert.equal(await page.locator(`${MODAL} .row .swatch[aria-expanded]`).getAttribute('data-color'), '#123456');
			assert.equal(await page.locator(`${MODAL} .state .swatch[aria-expanded]`).getAttribute('data-color'), '#00ff00');
			await cancelField(page);
			assert.equal(await shut, null);
		});
	});

	// da-rpfws, vividn, 2026-10-19: «let's move color and symbol (not glyph) to one line, next to each other.
	// make the symbol take on the color of color», and «open up a color picker popup, rather than moving
	// everything down»; the symbol picker «also should pop up rather than push down».
	test('colour and symbol share a line, the symbol in the colour, and each opens a popup that moves nothing', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'tea', '--verb', 'occur', '--glyph', '▲']);
			const [listed] = JSON.parse((await app.run(['fields', '--json'])).stdout).filter((f) => f.field === 'tea');
			const rgb = (hex) => `rgb(${[1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)).join(', ')})`;
			const { saved: shut } = await openField(page, 'tea');
			await page.locator(MODAL).waitFor();
			const swatch = page.locator(`${MODAL} .row .swatch[aria-expanded]`);
			const symbol = page.locator(`${MODAL} .tile[aria-expanded][aria-label^="the field's symbol:"]`);
			const ink = (el) => el.evaluate((e) => getComputedStyle(e).color);
			const [a, b] = [await swatch.boundingBox(), await symbol.boundingBox()];
			assert.ok(Math.abs(a.y + a.height / 2 - (b.y + b.height / 2)) < 2, `not one line: ${JSON.stringify([a, b])}`);
			assert.ok(b.x > a.x + a.width, 'the symbol stands after the colour');
			assert.equal(await ink(symbol), rgb(listed.color), 'at auto, the colour datum paints it');

			// What stands under the line: the placement, which draws the shape's sample (da-srrqd).
			const data = page.locator(`${MODAL} .panel .placement`);
			const at = (await data.boundingBox()).y;
			for (const [opener, set] of [[swatch, '.set'], [symbol, '.picker']]) {
				await opener.click();
				await page.locator(`${MODAL} dialog[open] ${set}`).waitFor();
				assert.equal((await data.boundingBox()).y, at, `${set} pushed the form down`);
				await page.locator(`${MODAL} dialog[open] button.close`).click();
				await page.locator(`${MODAL} ${set}`).waitFor({ state: 'detached' });
			}

			await swatch.click();
			const pick = page.locator(`${MODAL} .set .swatch`).nth(5);
			const color = await pick.getAttribute('data-color');
			await pick.click();
			assert.equal(await ink(symbol), rgb(color), 'the symbol takes the colour picked');
			await cancelField(page);
			assert.equal(await shut, null);
		});
	});

	// vividn, 2026-10-19: «when field is blank, have the color just be nothing (transparent) rather than the pink by default».
	test('a field with no name yet has no colour: the swatch is transparent until it is named', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await openIt(page);
			const swatch = page.locator(`${MODAL} .row .swatch[aria-expanded]`);
			const fill = () => swatch.evaluate((el) => getComputedStyle(el).backgroundColor);
			assert.equal(await fill(), 'rgba(0, 0, 0, 0)');
			assert.match(await swatch.getAttribute('aria-label'), /auto, transparent$/);
			await page.locator(`${MODAL} input.field`).fill('tea');
			assert.notEqual(await fill(), 'rgba(0, 0, 0, 0)');
			await leave(page);
		});
	});

	// da-jf6l2, vividn: «actually show the default color, maybe show that it is
	// the default somehow else (dashed border perhaps)». The default is the
	// CLI's — `fields --json` resolves it — not a colour the page picks.
	test('a swatch left at auto is the colour datum paints it, dashed; a picked one is solid', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'hue', '--verb', 'switch', '--add-state', 'calm']);
			const [listed] = JSON.parse((await app.run(['fields', '--json'])).stdout).filter((f) => f.field === 'hue');
			const rgb = (hex) => `rgb(${[1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)).join(', ')})`;
			const looks = (swatch) =>
				swatch.evaluate((el) => {
					const s = getComputedStyle(el);
					return { fill: s.backgroundColor, border: s.borderTopStyle, color: el.dataset.color };
				});

			const { saved: shut } = await openField(page, 'hue');
			await page.locator(MODAL).waitFor();
			const swatch = page.locator(`${MODAL} .row .swatch[aria-expanded]`);
			assert.deepEqual(await looks(swatch), { fill: rgb(listed.color), border: 'dashed', color: '' });

			await swatch.click();
			assert.deepEqual(await looks(page.locator(`${MODAL} .set .swatch[aria-label^="auto"]`)), {
				fill: rgb(listed.color),
				border: 'dashed',
				color: ''
			});
			const pick = page.locator(`${MODAL} .set .swatch`).nth(3);
			const color = await pick.getAttribute('data-color');
			await pick.click();
			assert.deepEqual(await looks(swatch), { fill: rgb(color), border: 'solid', color });

			// A state falls back to its OWN name's colour, not the field's.
			const state = page.locator(`${MODAL} .state .swatch[aria-expanded]`);
			assert.notEqual(listed.state_colors.calm, listed.color);
			assert.deepEqual(await looks(state), { fill: rgb(listed.state_colors.calm), border: 'dashed', color: '' });
			await cancelField(page);
			assert.equal(await shut, null);
		});
	});

	test('`setup` on a field with no spec declares it, with the field already in the box', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			const { saved: ran } = await openField(page, 'walk');
			await page.locator(MODAL).waitFor();
			assert.equal(await page.locator(`${MODAL} input.field`).inputValue(), 'walk');
			await page.locator(`${MODAL} button[data-verb="occur"]`).click();
			await page.locator(`${MODAL} button.commit`).click();
			assert.equal((await ran).code, 0);
			assert.equal(JSON.parse((await app.run(['setup', 'walk', '--json'])).stdout).verb, 'occur');
		});
	});

	// da-h119q: vividn, «when tracking something new something users
	// immediately want to do is to add in keys they care about». So Basics
	// asks for them, in place of the field-level unit nothing read — a line
	// that goes to the Data tab (da-i9upr) — and a field being declared
	// carries them in its one `setup F --track` (da-2pbvd).
	test('Basics asks for keys and not a unit, and a new field\'s keys are saved in its one `setup F --track`', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await openIt(page);
			await fill(page, 'coffee', 'occur');
			assert.equal(await page.locator(`${MODAL} input.unit`).count(), 0, 'no unit on a new field');
			assert.equal(await page.locator(`${MODAL} ul.data`).count(), 0, 'the keys are on Basics');
			assert.equal((await page.locator(`${MODAL} .data-line .ask`).innerText()).trim(), 'want to record extra data with each entry?');
			await page.locator(`${MODAL} button[data-to-tab="data"]`).click();
			assert.equal(await page.locator(`${MODAL} [data-panel]`).getAttribute('data-panel'), 'data');
			await page.locator(`${MODAL} ul.data + button.add`).click();
			const row = page.locator(`${MODAL} ul.data li.key`).last();
			await row.locator('input.name').fill('cups');
			assert.equal(await row.locator('button.eq').getAttribute('aria-pressed'), 'false', 'a new key is required, as `-k cups` is');
			assert.equal(await row.locator('input.value').count(), 0, 'no default box before the =');
			await row.locator('button.eq').click();
			assert.equal(await row.locator('button.eq').getAttribute('aria-pressed'), 'true');
			await row.locator('input.value').fill('2');
			const before = (await app.echoes()).length;
			await page.locator(`${MODAL} button.commit`).click();
			await page.locator('.bar button[data-field="coffee"]').waitFor();
			await app.idle();
			const echoes = (await app.echoes()).slice(before);
			assert.equal(echoes.length, 1, `one command: ${echoes.join(' | ')}`);
			assert.match(echoes[0], /^setup coffee --from-json .* --track$/);
			assert.deepEqual(declared(echoes[0]).spec.data_keys, ['cups=2']);
			const shown = JSON.parse((await app.run(['setup', 'coffee', '--json'])).stdout);
			assert.deepEqual(shown.keys, [{ name: 'cups', kind: 'default', value: '2', in_slug: false }]);

			// Saved, Basics names its keys, and there is still no unit; and so it opens again.
			await page.locator(`${MODAL} h2`).waitFor();
			assert.equal((await page.locator(`${MODAL} .data-line .ask`).innerText()).trim(), 'each entry records cups');
			await leave(page);
			const { saved: shut } = await openField(page, 'coffee');
			await page.locator(MODAL).waitFor();
			assert.equal(await page.locator(`${MODAL} input.unit`).count(), 0, 'no unit on a spec');
			await page.locator(`${MODAL} button[data-to-tab="data"]`).click();
			assert.deepEqual(await page.locator(`${MODAL} ul.data input.name`).evaluateAll((els) => els.map((e) => e.value)), ['cups']);
			await cancelField(page);
			assert.equal(await shut, null);
		});
	});

	// vividn, 2026-10-14: «Display the full widget even in + new field mode» (da-uipyt), none of it required.
	test('every tab from the start: a field being declared has all three, and so does a saved one', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			const tabs = async () => (await page.locator(`${MODAL} [role="tab"]`).allInnerTexts()).map((t) => t.trim().toLowerCase());
			await openIt(page);
			assert.deepEqual(await tabs(), ['basics', 'data', 'aliases']);
			await fill(page, 'coffee', 'occur');
			await page.locator(`${MODAL} button.commit`).click();
			await page.locator(`${MODAL} h2`).waitFor();
			assert.deepEqual(await tabs(), ['basics', 'data', 'aliases']);
			// Three words across a 320px phone, none of them cut.
			await page.setViewportSize({ width: 320, height: 640 });
			const cut = await page.locator(`${MODAL} [role="tab"]`).evaluateAll((tabs) =>
				tabs.filter((t) => {
					const box = t.getBoundingClientRect();
					const bar = t.parentElement.getBoundingClientRect();
					return t.scrollWidth > t.clientWidth || box.left < bar.left || box.right > bar.right + 0.5;
				}).map((t) => t.textContent)
			);
			assert.deepEqual(cut, []);
			await page.keyboard.press('Escape');
			await page.locator(MODAL).waitFor({ state: 'detached' });
		});
	});

	/**
	 * Open `field`'s spec, on `tab` if one is named. What saving it ran comes
	 * back boxed — an async function returning the bare promise would wait
	 * for the save — beside the echoes before it, counted while the REPL is
	 * drawn: behind the place in front, it is not.
	 */
	async function openFieldTab(app, field, tab) {
		const before = (await app.echoes()).length;
		const { saved } = await openField(app.page, field);
		await app.page.locator(MODAL).waitFor();
		if (tab) await app.page.locator(`${MODAL} [data-tab="${tab}"]`).click();
		return { saved, before };
	}

	/** Press Done, and hold the save to ONE command that the CLI took. */
	async function save(app, { saved, before }) {
		await app.page.locator(`${MODAL} button.commit`).click();
		const result = await saved;
		assert.equal(result.code, 0, result.stderr);
		await app.page.locator(MODAL).waitFor({ state: 'detached' });
		await app.idle();
		const echoes = (await app.echoes()).slice(before);
		assert.equal(echoes.length, 1, `one command, not a stream: ${echoes.join(' | ')}`);
		assert.match(echoes[0], /^setup /);
	}

	/** The rows' names, in order. */
	const names = (page) => page.locator(`${MODAL} ul.data li.key input.name`).evaluateAll((els) => els.map((e) => e.value));

	/** Drag the `from`th row's handle onto the `to`th row. */
	async function drag(page, from, to) {
		const rows = page.locator(`${MODAL} ul.data li.key`);
		const handle = (await rows.nth(from).locator('.handle').boundingBox());
		const target = await rows.nth(to).boundingBox();
		await page.mouse.move(handle.x + handle.width / 2, handle.y + handle.height / 2);
		await page.mouse.down();
		await page.mouse.move(handle.x + handle.width / 2, target.y + target.height / 2, { steps: 8 });
		await page.mouse.up();
	}

	// vividn, 2026-10-14: «handles to drag and reorder the keys». The order is
	// the order a line's words bind in, so it is saved as slots (da-zqnt3),
	// a typed key keeping its type beside its slot.
	test('Data: a key dragged to the top binds the first word, and keeps its type', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			const shown = async () => JSON.parse((await app.run(['setup', 'word', '--json'])).stdout);
			await app.run(['track', 'word', '--verb', 'occur']);

			let opened = await openFieldTab(app, 'word', 'data');
			for (const name of ['pos', 'gloss', 'reps']) {
				await page.locator(`${MODAL} ul.data + button.add`).click();
				await page.locator(`${MODAL} li.key input.name`).last().fill(name);
			}
			await drag(page, 2, 0);
			assert.deepEqual(await names(page), ['reps', 'pos', 'gloss']);
			// And the arrow keys on a handle, as a drag.
			await page.locator(`${MODAL} li.key .handle`).nth(2).press('ArrowUp');
			assert.deepEqual(await names(page), ['reps', 'gloss', 'pos']);
			await page.locator(`${MODAL} li.key`).first().locator('select.type').selectOption('number');
			await save(app, opened);
			const spec = await shown();
			assert.deepEqual(spec.keys.map((k) => k.name), ['reps', 'gloss', 'pos']);
			assert.deepEqual(spec.data_keys, ['reps', 'gloss', 'pos']);
			assert.equal(spec.subfields.reps.value_type, 'number');
			await app.clock.forward(1000);
			assert.equal((await app.run(['occur', 'word', '3', 'run', 'verb'])).code, 0);
			assert.deepEqual((await app.entries(1))[0].data, { reps: 3, gloss: 'run', pos: 'verb' });

			// Opened again, in the order saved.
			opened = await openFieldTab(app, 'word', 'data');
			assert.deepEqual(await names(page), ['reps', 'gloss', 'pos']);
			await cancelField(page);
			assert.deepEqual(app.errors, []);
		});
	});

	// vividn: «at the beginning have (-k) button that if you click let's you
	// select -R -SR -C as well with a little explanation of what each does».
	test('the mode button: -K says it is the id, and one row takes the rest of the line as a list, a string or a comment', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			const rows = page.locator(`${MODAL} ul.data li.key`);
			await openIt(page);
			await fill(page, 'book', 'add');
			for (const name of ['id', 'title', 'notes']) {
				await page.locator(`${MODAL} ul.data + button.add`).click();
				await rows.last().locator('input.name').fill(name);
			}
			const modes = () => rows.locator('.mode').allInnerTexts();
			assert.deepEqual(await modes(), ['-K', '-K', '-K']);
			assert.match(await rows.first().locator('.line').innerText(), /id/);
			await rows.nth(1).locator('.mode').click(); // -R
			assert.deepEqual(await modes(), ['-K', '-R', '-K']);
			assert.match(await rows.nth(1).locator('.line').innerText(), /rest of the line, as a list/);
			assert.equal(await rows.nth(1).locator('button.eq').count(), 0, 'the rest of the line has no default');
			await rows.nth(1).locator('.mode').click(); // -SR
			assert.deepEqual(await modes(), ['-K', '-SR', '-K']);
			for (let i = 0; i < 3; i++) await rows.nth(2).locator('.mode').click(); // -K → -C, which -SR gives up
			assert.deepEqual(await modes(), ['-K', '-k', '-C']);
			assert.equal(await rows.nth(1).locator('.line').count(), 0, 'plain -k has no line');
			assert.equal(await rows.nth(2).locator('input.name').inputValue(), 'comment');
			await page.locator(`${MODAL} button.commit`).click();
			await page.locator(`${MODAL} h2`).waitFor();
			// Where the saved spec puts the keys and the rest of the line.
			const sent = declared((await app.echoes()).at(-1)).spec;
			assert.deepEqual(
				[sent.id_keys, sent.data_keys, sent.remainder, sent.string_remainder, sent.comment_remainder],
				[['id'], ['title'], undefined, undefined, true]
			);
			const spec = JSON.parse((await app.run(['setup', 'book', '--json'])).stdout);
			assert.equal(spec.comment_remainder, true);
			assert.equal((await app.run(['add', 'book', 'dune', 'Dune', 'a', 'great', 'read'])).code, 0);
			assert.deepEqual((await app.entries(1))[0].data, { id: 'dune', title: 'Dune', comment: ['a great read'] });
			assert.deepEqual(app.errors, []);
		});
	});

	// da-p3hxn: vividn, «required · optional · default · assign, mapping onto
	// `-k key`, `-k key=`, `-k key=default`, `key=value`». da-zqnt3: `=`
	// makes a key optional and shows its default; a fixed one is on Data.
	// Each is what `setup F --json` reads back, and a key lives in ONE place (da-ulc0v).
	test('Data: each kind round-trips through `setup F --json`, and a key changing kind moves, never copied', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			const shown = async () => JSON.parse((await app.run(['setup', 'tea', '--json'])).stdout);
			const kinds = (spec) => spec.keys.map(({ name, kind, value }) => [name, kind, value]);
			const rows = page.locator(`${MODAL} ul.data li.key`);
			await app.run(['track', 'tea', '--verb', 'occur']);

			let opened = await openFieldTab(app, 'tea', 'data');
			for (const [name, eq, value, fixed] of [['a'], ['b', true], ['c', true, 'hot'], ['d', true, 'acme', true]]) {
				await page.locator(`${MODAL} ul.data + button.add`).click();
				const row = rows.last();
				await row.locator('input.name').fill(name);
				if (eq) await row.locator('button.eq').click();
				if (value) await row.locator('input.value').fill(value);
				if (fixed) await row.locator('input.fixed').check();
			}
			await save(app, opened);
			let spec = await shown();
			assert.deepEqual(kinds(spec), [['a', 'required', undefined], ['b', 'optional', undefined], ['c', 'default', 'hot'], ['d', 'assign', 'acme']]);
			assert.deepEqual(spec.assign, { d: 'acme' });

			// Opened again, the rows are what the CLI read back; a and d swap kinds.
			opened = await openFieldTab(app, 'tea', 'data');
			assert.deepEqual(await rows.locator('button.eq').evaluateAll((els) => els.map((e) => e.getAttribute('aria-pressed'))), ['false', 'true', 'true']);
			assert.equal(await rows.nth(3).locator('button.eq').count(), 0, 'a fixed key has no =, which would quietly unfix it');
			assert.equal(await rows.nth(3).locator('input.value').inputValue(), 'acme');
			await rows.nth(0).locator('button.eq').click();
			await rows.nth(0).locator('input.value').fill('x');
			await rows.nth(0).locator('input.fixed').check();
			await rows.nth(3).locator('input.fixed').uncheck();
			await rows.nth(3).locator('button.eq').click();
			await save(app, opened);
			spec = await shown();
			assert.deepEqual(kinds(spec), [['b', 'optional', undefined], ['c', 'default', 'hot'], ['d', 'required', undefined], ['a', 'assign', 'x']]);
			assert.deepEqual(spec.assign, { a: 'x' }, 'd left the map');
			assert.deepEqual(spec.data_keys, ['b=', 'c=hot', 'd'], 'a left the slots');

			// And a log binds it: the assign fixed, a bare word to the first slot, the default filled.
			await app.clock.forward(1000);
			assert.equal((await app.run(['occur', 'tea', 'p', 'd=r'])).code, 0);
			assert.deepEqual((await app.entries(1))[0].data, { a: 'x', b: 'p', c: 'hot', d: 'r' });
			assert.deepEqual(app.errors, []);
		});
	});

	// The acceptance is the behaviour, not the stored list: an alias removed
	// must stop RESOLVING, and one added must run what it says.
	test('an alias removed stops resolving, one added runs, and made again when switched back on', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			const shown = async () => JSON.parse((await app.run(['setup', 'sleep', '--json'])).stdout);
			await app.run(['track', 'sleep', '--verb', 'start']);

			let opened = await openFieldTab(app, 'sleep', 'aliases');
			assert.deepEqual(await page.locator(`${MODAL} .alias code.name`).allInnerTexts(), ['sleep', 'sleepe']);
			assert.equal(await page.locator(`${MODAL} .alias input`).count(), 0, 'a made alias is not edited: the save makes it again');
			await page.locator(`${MODAL} input.autoalias`).uncheck();
			await page.locator(`${MODAL} button[aria-label="remove sleep"]`).click();
			await page.locator(`${MODAL} button.add`).click();
			await page.locator(`${MODAL} .alias input.name`).last().fill('zz');
			await page.locator(`${MODAL} .alias input.expands`).last().fill('start sleep');
			await save(app, opened);
			let spec = await shown();
			assert.equal(spec.no_autoalias, true);
			assert.deepEqual(spec.derived_aliases.map((a) => [a.state, a.expands_to]), [['sleepe', 'end sleep'], ['zz', 'start sleep']]);

			// Each start a minute on: at one instant it would collide with the one
			// before it, or with `track`'s own document (da-edr7t).
			await app.clock.forward(60_000);
			const zz = await app.run(['zz']);
			assert.equal(zz.code, 0, zz.stderr);
			assert.match(stripAnsi(zz.stdout), /sleep\s+on/, 'zz ran `start sleep`');
			const bare = await app.run(['sleep']);
			assert.notEqual(bare.code, 0, 'the removed alias still resolves');

			opened = await openFieldTab(app, 'sleep', 'aliases');
			await page.locator(`${MODAL} input.autoalias`).check();
			await save(app, opened);
			spec = await shown();
			assert.equal(spec.no_autoalias, false);
			assert.deepEqual(spec.derived_aliases.map((a) => a.state), ['sleep', 'sleepe']);
			assert.notEqual((await app.run(['zz'])).code, 0, 'a hand alias outlived the switch back');
			await app.clock.forward(60_000);
			assert.equal((await app.run(['sleep'])).code, 0);
		});
	});

	// The old inline form is gone, not merely unused: it hung off the bar,
	// scrolled with it, and had to be hung against the screen to be reached
	// at all (da-hkpd). A modal has none of those problems and the form has
	// no second life.
	test('the form that used to hang off the bar is gone from the DOM', async () => {
		await shared.solo(async ({ page }) => {
			await openIt(page);
			assert.equal(await page.locator('.bar form.add, .bar .panel.add').count(), 0);
			assert.equal(await page.locator(`${MODAL} select`).count(), 0, 'the `<select>` came along');
		});
	});

	// da-ogyxa: the glyph picker. Each test reads what it wrote back through
	// the CLI, and what the CLI then DRAWS: the dayview's SVG and `now`'s band.
	// `track` writes its document at the held instant (#132), and an entry for
	// the same field at the same instant is a duplicate the CLI refuses, so a
	// test moves the clock a second on before it logs the dot, and checks the
	// log went in: a refused one drew nothing and read as a drawing bug (da-edr7t).

	/** The glyph tile labelled `label`, in the open modal. */
	const tile = (page, label) => page.locator(`${MODAL} .tile[aria-expanded][aria-label^="${label}:"]`);
	/** The row `now` draws for `field` in its band. */
	const band = async (app, field) =>
		stripAnsi((await app.run(['now'])).stdout).split('\n').find((l) => l.startsWith(`${field} `)) ?? '';
	/** The glyphs the dayview draws for its dots. */
	const drawn = async (app) =>
		[...(await app.run(['dayview'])).stdout.matchAll(/<text class="point-fg"[^>]*>([^<]*)<\/text>/g)].map((m) => m[1]);

	test('a field glyph is picked from the grid, saved in one `setup F`, and drawn by `now` and the dayview', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'tea', '--verb', 'occur']);
			const before = (await app.echoes()).length;
			const { saved } = await openField(page, 'tea');
			await page.locator(MODAL).waitFor();
			const field = tile(page, "the field's symbol");
			assert.equal(await field.getAttribute('data-glyph'), '', 'no glyph yet: the circle');
			await field.click();
			// vividn, 2026-10-19: «remove the two columns wide line, that makes no sense». A tile's title still says it.
			const picker = page.locator(`${MODAL} .picker`);
			assert.doesNotMatch(await picker.innerText(), /two columns/);
			assert.match(await picker.locator('[data-glyph="☕"]').getAttribute('title'), /the band draws it ●/);
			await picker.locator('[data-glyph="▲"]').click();
			assert.equal(await picker.count(), 0, 'a pick folds the set away');
			assert.equal(await field.getAttribute('data-glyph'), '▲');
			await page.locator(`${MODAL} button.commit`).click();
			assert.equal((await saved).code, 0);
			await app.idle();
			const echoes = await app.echoes();
			assert.equal(echoes.length, before + 1, `one command: ${echoes.slice(before).join(' | ')}`);
			assert.equal(JSON.parse((await app.run(['setup', 'tea', '--json'])).stdout).glyph, '▲');

			await app.clock.forward(1000);
			const logged = await app.run(['occur', 'tea']);
			assert.equal(logged.code, 0, logged.stderr);
			assert.match(await band(app, 'tea'), /▲/);
			assert.deepEqual(await drawn(app), ['▲']);
		});
	});

	test('a state glyph is typed beside its swatch and drawn on a `switch -m` dot', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'mood', '--verb', 'switch', '--states', 'up,down']);
			const { saved } = await openField(page, 'mood');
			await page.locator(MODAL).waitFor();
			await tile(page, 'the symbol of up').click();
			// A state's symbol is a character: the image is the field's alone.
			assert.equal(await page.locator(`${MODAL} .picker input[type="file"]`).count(), 0);
			// Any one character: the first of whatever is typed.
			await page.locator(`${MODAL} .picker input.any`).fill('✺✺');
			assert.equal(await tile(page, 'the symbol of up').getAttribute('data-glyph'), '✺');
			await page.locator(`${MODAL} button.commit`).click();
			assert.equal((await saved).code, 0);
			assert.deepEqual(JSON.parse((await app.run(['setup', 'mood', '--json'])).stdout).state_glyphs, { up: '✺' });

			await app.clock.forward(1000);
			const logged = await app.run(['switch', 'mood', 'up', '-m']);
			assert.equal(logged.code, 0, logged.stderr);
			assert.match(await band(app, 'mood'), /✺/);
			assert.deepEqual(await drawn(app), ['✺']);
		});
	});

	test('removing a state leaves no glyph for it; the states kept keep theirs', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'mood', '--verb', 'switch', '--states', 'up,down,flat']);
			await app.run(['setup', 'mood', '--state-glyph', 'up=▲', '--state-glyph', 'down=▼', '--state-glyph', 'flat=■']);
			const { saved } = await openField(page, 'mood');
			await page.locator(MODAL).waitFor();
			await page.locator(`${MODAL} button.remove[aria-label="remove down"]`).click();
			// A rename is the same row under a new name, and its glyph comes along.
			await page.locator(`${MODAL} .state input.name`).nth(1).fill('level');
			await page.locator(`${MODAL} button.commit`).click();
			assert.equal((await saved).code, 0);
			const shown = JSON.parse((await app.run(['setup', 'mood', '--json'])).stdout);
			assert.deepEqual(shown.allowed_states, ['up', 'level']);
			assert.deepEqual(shown.state_glyphs, { up: '▲', level: '■' });
		});
	});

	// vividn, 2026-10-19: «completely get rid of the "by a key" here … Even when there is by a key symbols,
	// there can still be … a top-level symbol». The form neither shows a `glyph_by` nor loses it.
	test('a glyph by a key is not offered, survives a save, and the field\'s own symbol is still set', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'tea', '--verb', 'occur']);
			await app.run(['setup', 'tea', '--glyph-by', 'size', '--glyph-map', 'large=☕']);
			const { saved } = await openField(page, 'tea');
			await page.locator(MODAL).waitFor();
			assert.doesNotMatch(await page.locator(MODAL).innerText(), /by a key/);
			await tile(page, "the field's symbol").click();
			assert.doesNotMatch(await page.locator(`${MODAL} .picker`).innerText(), /by a key/);
			await page.locator(`${MODAL} .picker [data-glyph="○"]`).click();
			await page.locator(`${MODAL} button.commit`).click();
			assert.equal((await saved).code, 0);
			const shown = (await app.run(['setup', 'tea', '--json'])).stdout;
			assert.deepEqual(JSON.parse(shown).glyph_by, { key: 'size', map: { large: '☕' } });
			assert.equal(JSON.parse(shown).glyph, '○');

			// Opened and saved untouched, the glyphs read back byte for byte.
			const { saved: again } = await openField(page, 'tea');
			await page.locator(`${MODAL} button.commit`).click();
			assert.equal((await again).code, 0);
			assert.equal((await app.run(['setup', 'tea', '--json'])).stdout, shown);
		});
	});

	// A pictogram a thumb cannot hit is not a pictogram (the bead's point 5).
	// The shapes and the name are what it opens on (da-uipyt); on a screen this
	// small the rest of the whole form is under them, a scroll away.
	test(`every pictogram is a fingertip on a ${PHONE.width}x${PHONE.height} screen`, async () => {
		await withApp(async ({ page }) => {
			await openIt(page);
			await fill(page, 'tea', null);
			const box = await page.locator(MODAL).boundingBox();
			const screen = await page.evaluate(() => ({
				width: document.documentElement.clientWidth,
				height: document.documentElement.clientHeight
			}));
			assert.ok(box.x >= 0 && box.x + box.width <= screen.width + 1, `the form is ${box.x}–${box.x + box.width} on a ${screen.width}px screen`);
			const verbs = page.locator(`${MODAL} .verb`);
			assert.equal(await verbs.count(), VERBS.length);
			for (let i = 0; i < VERBS.length; i++) {
				const item = await verbs.nth(i).boundingBox();
				assert.ok(item.height >= 24, `${VERBS[i].verb} is ${item.height}px tall`);
				assert.ok(item.width >= 24, `${VERBS[i].verb} is ${item.width}px wide`);
				assert.ok(item.x >= 0 && item.x + item.width <= screen.width + 1, `${VERBS[i].verb} is off the screen`);
			}
			for (const control of ['.verb', 'input.field', 'button.commit']) {
				if (control === 'button.commit') await page.locator(`${MODAL} ${control}`).scrollIntoViewIfNeeded();
				const item = await page.locator(`${MODAL} ${control}`).last().boundingBox();
				assert.ok(item.y >= 0 && item.y + item.height <= screen.height + 1, `${control} is off the screen`);
			}
		}, { viewport: PHONE });
	});
});
