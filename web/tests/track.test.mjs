// The Track modal: the web's «which verb?» (da-94q1).
//
// A field cannot be born verbless (da-c8mh), so declaring one is a
// question with four answers, and this is where the web asks it. The old
// form was a `<select>` hanging off the widget bar; it is a `<dialog>` now,
// with a pictogram per verb — vividn, 2026-09-15: «let's make it a modal, and
// have nice pictograms for each of the verbs».
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
	editedSpec, GLYPHS, keyRows, specArgv, specKeys, stateRows, trackArgv, trackLabel, VERBS, verbAsked, wide
} from '../src/lib/track.ts';
import { stripAnsi } from './support/tail.mjs';

const MODAL = 'dialog.spec';
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

/** Open the modal from the widget bar's `Track +` and wait for it. */
async function openIt(page) {
	await page.locator('.bar button.track').click();
	await page.locator(MODAL).waitFor();
}

/** Fill the name box and choose a verb, as a person does. */
async function fill(page, field, verb) {
	await page.locator(`${MODAL} input.field`).fill(field);
	if (verb) await page.locator(`${MODAL} button[data-verb="${verb}"]`).click();
}

/** On the States tab, add one row per name. */
async function addStates(page, names) {
	await page.locator(`${MODAL} [data-tab="states"]`).click();
	for (const name of names) {
		await page.locator(`${MODAL} button.add`).click();
		await page.locator(`${MODAL} .state input.name`).last().fill(name);
	}
}

describe('the argv it composes', () => {
	test('`track`, not `spec set`: declaring a field is an event in the log', () => {
		assert.deepEqual(trackArgv('coffee', 'occur'), ['track', 'coffee', '--verb', 'occur']);
		assert.deepEqual(trackArgv('sleep', 'start'), ['track', 'sleep', '--verb', 'start']);
	});

	const row = (name, color = '', description = '', glyph = '') => ({ name, color, description, glyph });

	test('a switch carries its states, one flag each, a painted one twice; a colour is optional', () => {
		assert.deepEqual(trackArgv('mood', 'switch', [row('good', '#00ff00'), row(' bad '), row('  ')]), [
			'track', 'mood', '--verb', 'switch', '--state', 'good', '--state', 'bad', '--state', 'good=#00ff00'
		]);
		assert.deepEqual(trackArgv('coffee', 'occur', [], '#ffa500').slice(-2), ['--color', '#ffa500']);
		// The states belong to `switch` alone: a point with one added and
		// then changed away would otherwise smuggle it out.
		assert.deepEqual(trackArgv('coffee', 'occur', [row('good')]), ['track', 'coffee', '--verb', 'occur']);
	});

	// da-ci85o: an edited spec is saved WHOLE, in one `spec set` — the
	// shape that replaces the list, so a state removed takes its colour and
	// description with it, and a blank clears where a flag could not.
	test('an edited spec is the whole spec, in one `spec set --from-json`', () => {
		const spec = {
			field: 'mood', verb: 'switch', color: '#123456', unit: 'pts', description: null,
			allowed_states: ['good', 'bad'], state_colors: { good: '#00ff00', bad: '#ff0000' },
			state_descriptions: { bad: 'awful' }, subfields: { note: {} }, tags: ['x']
		};
		assert.deepEqual(stateRows(spec), [row('good', '#00ff00'), row('bad', '#ff0000', 'awful')]);
		const edited = editedSpec(spec, {
			verb: 'start', color: '', unit: ' ', description: ' how I feel ',
			states: [row('good', '#00ff00', 'fine'), row(' meh ', ''), row('')]
		});
		assert.deepEqual(edited, {
			...spec, verb: 'start', color: null, unit: null, description: 'how I feel',
			allowed_states: ['good', 'meh'], state_colors: { good: '#00ff00' },
			state_descriptions: { good: 'fine' }
		});
		assert.deepEqual(edited.subfields, spec.subfields, 'what the modal does not edit is carried');
		// `spec show` leaves an empty description map out; so does the save.
		const bare = { ...spec, state_descriptions: undefined };
		delete bare.state_descriptions;
		assert.deepEqual(stateRows(bare), [row('good', '#00ff00'), row('bad', '#ff0000')]);
		assert.equal('state_descriptions' in editedSpec(bare, { verb: 'switch', color: '', unit: '', description: '', states: stateRows(bare) }), false);
		// A spec with no list keeps none when none is added.
		assert.equal(editedSpec({ ...spec, allowed_states: null }, { verb: 'occur', color: '', unit: '', description: '', states: [] }).allowed_states, null);
		const argv = specArgv(edited);
		assert.deepEqual(argv.slice(0, 4), ['spec', 'set', 'mood', '--from-json']);
		assert.deepEqual(JSON.parse(argv[4]), edited);
	});

	// da-ogyxa: a state's glyph goes with the state when the state leaves the
	// list, and ONLY then — `state_glyphs` may name a state the list never
	// did, and a caller whose rows carry no glyph must not wipe them.
	test('a removed state takes its glyph with it; a kept or renamed one keeps it', () => {
		const spec = {
			field: 'mood', verb: 'switch', color: null, unit: null, description: null,
			allowed_states: ['up', 'down', 'flat'], state_colors: {},
			state_glyphs: { up: '▲', down: '▼', flat: '■', logged: '○' }
		};
		assert.deepEqual(stateRows(spec).map((r) => r.glyph), ['▲', '▼', '■']);
		const base = { verb: 'switch', color: '', unit: '', description: '' };
		// `down` removed, `flat` renamed to `level` (the row carries its glyph), `up` cleared.
		const [up, , flat] = stateRows(spec);
		const edited = editedSpec(spec, { ...base, states: [{ ...up, glyph: '' }, { ...flat, name: 'level' }] });
		assert.deepEqual(edited.state_glyphs, { level: '■', logged: '○' });
		// Rows with no glyph key — the rows that re-add colours — keep every glyph of a kept state.
		const bare = editedSpec(spec, { ...base, states: [{ name: 'up', color: '', description: '' }] });
		assert.deepEqual(bare.state_glyphs, { up: '▲', logged: '○' });
		// None left: the key is left out, as `spec show` leaves it out.
		assert.equal('state_glyphs' in editedSpec({ ...spec, state_glyphs: { down: '▼' } }, { ...base, states: [{ ...up, glyph: '' }] }), false);
	});

	test('one glyph, or a glyph by a key with the field\'s as the default', () => {
		const spec = {
			field: 'tea', verb: 'occur', color: null, unit: null, description: null,
			allowed_states: null, state_colors: {}, glyph: '●', glyph_by: { key: 'size', map: { big: '☕' } },
			subfields: { size: {} }, data_keys: ['cup=mug', 'size']
		};
		const base = { verb: 'occur', color: '', unit: '', description: '', states: [] };
		assert.deepEqual(specKeys(spec), ['size', 'cup']);
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

	// da-cap5m: the keys are the whole map, a default stored as it was
	// typed; the aliases are sent only once datum no longer makes them.
	test('keys are saved whole, and only a list datum does not make is sent', () => {
		const key = (over) => ({ required: false, value_type: 'any', in_slug: false, default: null, unit: null, ...over });
		const spec = {
			field: 'word', verb: 'add', color: null, unit: null, description: null, allowed_states: null, state_colors: {},
			subfields: { pos: key({ required: true, value_type: 'string', in_slug: true }), reps: key({ value_type: 'number', default: 3, unit: 'x' }) },
			derived_aliases: [{ state: 'word', expands_to: 'add word', kind: 'bare_field' }], no_autoalias: false
		};
		const keys = keyRows(spec);
		assert.deepEqual(keys, [
			{ ...key({ required: true, value_type: 'string', in_slug: true }), name: 'pos', default: '' },
			{ ...key({ value_type: 'number', unit: 'x' }), name: 'reps', default: '3' }
		]);
		const base = { verb: 'add', color: '', unit: '', description: '', states: [] };
		keys[0].required = false;
		keys[0].default = '42';
		keys[1].default = ' ';
		const made = editedSpec(spec, { ...base, keys: [...keys, { ...key(), name: 'n', default: '42' }, { ...key(), name: ' ', default: '' }], autoalias: true, aliases: [] });
		assert.deepEqual(made.subfields, {
			pos: key({ value_type: 'string', in_slug: true, default: '42' }),
			reps: key({ value_type: 'number', unit: 'x' }),
			n: key({ default: 42 })
		});
		assert.equal(made.no_autoalias, false);
		assert.deepEqual(made.derived_aliases, spec.derived_aliases, 'a made list is carried: the CLI makes it again');

		const mine = editedSpec(spec, { ...base, autoalias: false, aliases: [{ state: ' zz ', expands_to: ' add word ' }, { state: '', expands_to: 'x' }] });
		assert.equal(mine.no_autoalias, true);
		assert.deepEqual(mine.derived_aliases, [{ state: 'zz', expands_to: 'add word' }]);
		assert.deepEqual(mine.subfields, spec.subfields, 'keys the edit leaves out are carried');
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
			for (const { verb } of VERBS) await app.run(trackArgv(`f_${verb}`, verb));
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

	test('Track + opens the modal: four pictograms, Done shut until a name and a verb', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await openIt(page);
			const glyphs = await page.locator(`${MODAL} .verb .glyph`).allInnerTexts();
			assert.deepEqual(glyphs.map((g) => g.trim()), VERBS.map((v) => v.glyph));
			assert.deepEqual(
				await page.locator(`${MODAL} .verb .name`).allInnerTexts(),
				VERBS.map((v) => v.verb)
			);

			const commit = page.locator(`${MODAL} button.commit`);
			assert.equal(await commit.isEnabled(), false, 'nothing named, nothing chosen');
			await fill(page, 'mood', null);
			assert.equal(await commit.isEnabled(), false, 'a name alone is not a field');
			await fill(page, '', 'occur');
			assert.equal(await commit.isEnabled(), false, 'a verb alone is not a field');
			await fill(page, 'mood', null);
			assert.equal(await commit.isEnabled(), true);

			// The States tab belongs to the switch, and to nothing else.
			assert.equal(await page.locator(`${MODAL} [data-tab="states"]`).isEnabled(), false);
			await page.locator(`${MODAL} button[data-verb="switch"]`).click();
			await addStates(page, ['good', 'bad']);
			assert.equal(
				(await page.locator(`${MODAL} code.preview`).innerText()).trim(),
				'track mood --verb switch --state good --state bad'
			);
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
			assert.equal((await app.echoes()).at(-1), 'track mood --verb switch --state good --state bad');
			const spec = JSON.parse((await app.run(['spec', 'show', 'mood'])).stdout);
			assert.equal(spec.verb, 'switch');
			assert.deepEqual(spec.allowed_states, ['good', 'bad']);

			assert.equal(await actionWord(app, 'mood'), 'TRACK▞');
			assert.equal(await page.locator(MODAL).count(), 0, 'the modal stayed open');
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
	// verb is already chosen, so the modal opens on it.
	test('a preset verb opens already chosen, and only that one', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			const ran = page.evaluate(() => window.datum.track({ verb: 'occur' }));
			await page.locator(MODAL).waitFor();
			assert.deepEqual(
				await page.locator(`${MODAL} .verb[aria-checked="true"] .name`).allInnerTexts(),
				['occur']
			);
			await fill(page, 'coffee', null);
			await page.locator(`${MODAL} button.commit`).click();
			const result = await ran;
			assert.equal(result.code, 0, result.stderr);
			await page.locator('.bar button[data-field="coffee"]').waitFor();
			assert.equal(JSON.parse((await app.run(['spec', 'show', 'coffee'])).stdout).verb, 'occur');
		});
	});

	// The CLI's own refusal is the third door: `track <field>` on a field
	// with neither a spec nor a log comes back asking, and the modal is the
	// asking. What it answers with replaces the refused command.
	test('a bare `track` on a new field opens the modal, with the field already in it', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			const typed = page.evaluate(() => window.datum.runLine('track sleep'));
			await page.locator(MODAL).waitFor();
			assert.equal(await page.locator(`${MODAL} input.field`).inputValue(), 'sleep');
			await page.locator(`${MODAL} button[data-verb="start"]`).click();
			await page.locator(`${MODAL} button.commit`).click();
			assert.equal((await typed).code, 0);
			await page.locator('.bar button[data-field="sleep"]').waitFor();
			assert.equal(JSON.parse((await app.run(['spec', 'show', 'sleep'])).stdout).verb, 'start');
		});
	});

	test('an ignored field is offered back, and a press re-tracks it', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await app.submit('track run --verb occur');
			// Drawn BEFORE it is taken away: a bar that has not got round to
			// drawing `run` yet satisfies "no run button" too, and the modal
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

	// da-ci85o: Track is the spec modal opened on Basics, and it must stay
	// as quick as the modal it absorbed — `Track +`, a pictogram, Done:
	// three taps, the name typed into the box that already has the focus.
	test('declaring a field is three taps, as it was before the tabs', async () => {
		await withApp(async (app) => { // a listener of its own, which `solo` would keep
			const { page } = app;
			await page.evaluate(() => {
				window.__taps = 0;
				document.addEventListener('pointerdown', (e) => e.isTrusted && window.__taps++, true);
			});
			await openIt(page);
			await page.keyboard.type('coffee');
			await page.locator(`${MODAL} button[data-verb="occur"]`).click();
			await page.locator(`${MODAL} button.commit`).click();
			await page.locator('.bar button[data-field="coffee"]').waitFor();
			assert.equal(await page.evaluate(() => window.__taps), 3);
		});
	});

	// The same modal on a field that has a spec: everything it edits comes
	// back in ONE `spec set`, and `spec show` then reads what was sent.
	test('editing a spec writes one `spec set`, and the CLI reads back what it was sent', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'sleep', '--verb', 'start']);
			await app.run(['spec', 'set', 'sleep', '--description', 'night']);
			const before = (await app.echoes()).length;
			const saved = page.evaluate(() => window.datum.spec('sleep'));
			await page.locator(MODAL).waitFor();
			assert.equal((await page.locator(`${MODAL} h2`).innerText()).trim(), 'sleep');
			assert.deepEqual(await page.locator(`${MODAL} .verb[aria-checked="true"] .name`).allInnerTexts(), ['start']);
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
			await page.locator(`${MODAL} input.unit`).fill('h');

			await addStates(page, ['awake', 'asleep']);
			await page.locator(`${MODAL} .state`).nth(1).locator('.swatch[aria-expanded]').click();
			const statePick = page.locator(`${MODAL} .state .set .swatch`).nth(8);
			const stateColor = await statePick.getAttribute('data-color');
			await statePick.click();
			await page.locator(`${MODAL} .state input.description`).first().fill('up');
			await page.locator(`${MODAL} button.commit`).click();

			const result = await saved;
			assert.equal(result.code, 0, result.stderr);
			await app.idle();
			const echoes = await app.echoes();
			assert.equal(echoes.length, before + 1, `one command, not a stream: ${echoes.slice(before).join(' | ')}`);
			assert.match(echoes.at(-1), /^spec set sleep --from-json /);

			const sent = JSON.parse(result.argv[result.argv.indexOf('--from-json') + 1]);
			const shown = JSON.parse((await app.run(['spec', 'show', 'sleep'])).stdout);
			// The CLI regenerates the aliases a spec derives from its states,
			// so those are the one key it does not take as sent.
			const { derived_aliases: _sent, ...kept } = sent;
			const { derived_aliases: _shown, ...read } = shown;
			assert.deepEqual(read, kept);
			assert.equal(shown.verb, 'switch');
			assert.equal(shown.color, color);
			assert.equal(shown.unit, 'h');
			assert.equal(shown.description, 'night');
			assert.deepEqual(shown.allowed_states, ['awake', 'asleep']);
			assert.deepEqual(shown.state_colors, { asleep: stateColor });
			assert.deepEqual(shown.state_descriptions, { awake: 'up' });

			// Opened and saved untouched, a spec with no state descriptions —
			// the key `spec show` leaves out — reads back byte for byte.
			await app.run(['spec', 'set', 'sleep', '--state-description', 'awake=']);
			const untouched = (await app.run(['spec', 'show', 'sleep'])).stdout;
			assert.doesNotMatch(untouched, /state_descriptions/);
			const again = page.evaluate(() => window.datum.spec('sleep'));
			await page.locator(`${MODAL} button.commit`).click();
			assert.equal((await again).code, 0);
			assert.equal((await app.run(['spec', 'show', 'sleep'])).stdout, untouched);
		});
	});

	test('Escape and the backdrop close a spec and write nothing', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'sleep', '--verb', 'start']);
			const spec = (await app.run(['spec', 'show', 'sleep'])).stdout;
			const before = (await app.echoes()).length;
			for (const dismiss of [() => page.keyboard.press('Escape'), () => page.mouse.click(2, 2)]) {
				const shut = page.evaluate(() => window.datum.spec('sleep'));
				await page.locator(`${MODAL} input.unit`).fill('h');
				await dismiss();
				assert.equal(await shut, null);
				await page.waitForFunction((sel) => !document.querySelector(sel), MODAL);
			}
			await app.idle();
			assert.equal((await app.run(['spec', 'show', 'sleep'])).stdout, spec);
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
					assert.equal(type, 'text', `${where}: input.${cls} is a ${type}`);
					assert.doesNotMatch(`${value} ${placeholder}`, /#|hex|rgb/i, `${where}: input.${cls} takes a colour`);
				}
			};
			await openIt(page);
			await page.locator(`${MODAL} button[data-verb="switch"]`).click();
			await noHex('track, basics');
			await addStates(page, ['good']);
			await noHex('track, states');
			await page.keyboard.press('Escape');
			await page.waitForFunction((sel) => !document.querySelector(sel), MODAL);

			await app.run(['track', 'mood', '--verb', 'switch', '--state', 'good', '--state', 'good=#00ff00', '--color', '#123456']);
			const shut = page.evaluate(() => window.datum.spec('mood'));
			await page.locator(MODAL).waitFor();
			await noHex('spec, basics');
			assert.equal(await page.locator(`${MODAL} .row .swatch[aria-expanded]`).getAttribute('data-color'), '#123456');
			await page.locator(`${MODAL} [data-tab="states"]`).click();
			await noHex('spec, states');
			assert.equal(await page.locator(`${MODAL} .state .swatch[aria-expanded]`).getAttribute('data-color'), '#00ff00');
			await page.keyboard.press('Escape');
			assert.equal(await shut, null);
		});
	});

	test('`spec` on a field with no spec declares it, with the field already in the box', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			const ran = page.evaluate(() => window.datum.spec('walk'));
			await page.locator(MODAL).waitFor();
			assert.equal(await page.locator(`${MODAL} input.field`).inputValue(), 'walk');
			await page.locator(`${MODAL} button[data-verb="occur"]`).click();
			await page.locator(`${MODAL} button.commit`).click();
			assert.equal((await ran).code, 0);
			assert.equal(JSON.parse((await app.run(['spec', 'show', 'walk'])).stdout).verb, 'occur');
		});
	});

	// `track` carries neither keys nor aliases, so a field being declared has them shut.
	test('Keys and Aliases are a spec\'s tabs: declaring a field has them shut', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await openIt(page);
			assert.deepEqual(
				(await page.locator(`${MODAL} [role="tab"]`).allInnerTexts()).map((t) => t.trim().toLowerCase()),
				['basics', 'states', 'keys', 'aliases', 'placement']
			);
			// Five words across a 320px phone, none of them cut.
			await page.setViewportSize({ width: 320, height: 640 });
			const cut = await page.locator(`${MODAL} [role="tab"]`).evaluateAll((tabs) =>
				tabs.filter((t) => {
					const box = t.getBoundingClientRect();
					const bar = t.parentElement.getBoundingClientRect();
					return t.scrollWidth > t.clientWidth || box.left < bar.left || box.right > bar.right + 0.5;
				}).map((t) => t.textContent)
			);
			assert.deepEqual(cut, []);
			// Nor is there a spec to place yet (da-btyvj).
			for (const tab of ['keys', 'aliases', 'placement']) assert.ok(await page.locator(`${MODAL} [data-tab="${tab}"]`).isDisabled(), tab);
			await page.keyboard.press('Escape');
			await page.locator(MODAL).waitFor({ state: 'detached' });
		});
	});

	/**
	 * Open `field`'s spec on `tab`. What saving it ran comes back boxed: an
	 * async function returning the bare promise would wait for the save.
	 */
	async function openSpecTab(page, field, tab) {
		const saved = page.evaluate((f) => window.datum.spec(f), field);
		await page.locator(MODAL).waitFor();
		await page.locator(`${MODAL} [data-tab="${tab}"]`).click();
		return { saved };
	}

	/** Press Done, and hold the save to ONE command that the CLI took. */
	async function save(app, saved) {
		const before = (await app.echoes()).length;
		await app.page.locator(`${MODAL} button.commit`).click();
		const result = await saved;
		assert.equal(result.code, 0, result.stderr);
		await app.idle();
		const echoes = (await app.echoes()).slice(before);
		assert.equal(echoes.length, 1, `one command, not a stream: ${echoes.join(' | ')}`);
		assert.match(echoes[0], /^spec set /);
	}

	test('a key toggled required moves to the required group, and `spec show` reads it back', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			const names = (group) => page.locator(`${MODAL} .keys[data-group="${group}"] input.name`).evaluateAll((els) => els.map((e) => e.value));
			const subfields = async () => JSON.parse((await app.run(['spec', 'show', 'word'])).stdout).subfields;
			await app.run(['track', 'word', '--verb', 'add']);

			let { saved } = await openSpecTab(page, 'word', 'keys');
			for (const name of ['pos', 'gloss']) {
				await page.locator(`${MODAL} button.add`).click();
				await page.locator(`${MODAL} .key input.name`).last().fill(name);
			}
			assert.deepEqual(await names('optional'), ['pos', 'gloss']);
			const pos = page.locator(`${MODAL} .key`).first();
			await pos.locator('input.in-slug').check();
			await pos.locator('input.default').fill('noun');
			await pos.locator('input.required').check();
			assert.deepEqual(await names('required'), ['pos'], 'required reads off the group, not only the toggle');
			assert.deepEqual(await names('optional'), ['gloss']);
			await save(app, saved);
			assert.deepEqual(await subfields(), {
				pos: { required: true, value_type: 'any', in_slug: true, default: 'noun', unit: null },
				gloss: { required: false, value_type: 'any', in_slug: false, default: null, unit: null }
			});

			({ saved } = await openSpecTab(page, 'word', 'keys'));
			assert.deepEqual(await names('required'), ['pos']);
			// A click, not `uncheck`: the row leaves the group the locator names,
			// so there is nothing left there to confirm unchecked.
			await page.locator(`${MODAL} .keys[data-group="required"] input.required`).click();
			assert.equal(await page.locator(`${MODAL} .keys[data-group="required"]`).count(), 0);
			await save(app, saved);
			assert.equal((await subfields()).pos.required, false);
		});
	});

	// The acceptance is the behaviour, not the stored list: an alias removed
	// must stop RESOLVING, and one added must run what it says.
	test('an alias removed stops resolving, one added runs, and made again when switched back on', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			const shown = async () => JSON.parse((await app.run(['spec', 'show', 'sleep'])).stdout);
			await app.run(['track', 'sleep', '--verb', 'start']);

			let { saved } = await openSpecTab(page, 'sleep', 'aliases');
			assert.deepEqual(await page.locator(`${MODAL} .alias code.name`).allInnerTexts(), ['sleep', 'sleepe']);
			assert.equal(await page.locator(`${MODAL} .alias input`).count(), 0, 'a made alias is not edited: the save makes it again');
			await page.locator(`${MODAL} input.autoalias`).uncheck();
			await page.locator(`${MODAL} button[aria-label="remove sleep"]`).click();
			await page.locator(`${MODAL} button.add`).click();
			await page.locator(`${MODAL} .alias input.name`).last().fill('zz');
			await page.locator(`${MODAL} .alias input.expands`).last().fill('start sleep');
			await save(app, saved);
			let spec = await shown();
			assert.equal(spec.no_autoalias, true);
			assert.deepEqual(spec.derived_aliases.map((a) => [a.state, a.expands_to]), [['sleepe', 'end sleep'], ['zz', 'start sleep']]);

			// Each start a minute on: at one instant it would collide with the one
			// before it, or with `track`'s own document (da-edr7t).
			await app.clock.forward(60_000);
			const zz = await app.run(['zz']);
			assert.equal(zz.code, 0, zz.stderr);
			assert.match(stripAnsi(zz.stdout), /sleep\s+start/, 'zz ran `start sleep`');
			const bare = await app.run(['sleep']);
			assert.notEqual(bare.code, 0, 'the removed alias still resolves');

			({ saved } = await openSpecTab(page, 'sleep', 'aliases'));
			await page.locator(`${MODAL} input.autoalias`).check();
			await save(app, saved);
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
			await page.locator('.bar button.track').click();
			await page.locator(MODAL).waitFor();
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

	test('a field glyph is picked from the grid, saved in one `spec set`, and drawn by `now` and the dayview', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'tea', '--verb', 'occur']);
			const before = (await app.echoes()).length;
			const saved = page.evaluate(() => window.datum.spec('tea'));
			await page.locator(MODAL).waitFor();
			const field = tile(page, "the field's glyph");
			assert.equal(await field.getAttribute('data-glyph'), '', 'no glyph yet: the circle');
			await field.click();
			// Two columns wide is said where it is picked: ☕ is marked, ▲ is not.
			const picker = page.locator(`${MODAL} .picker`);
			assert.equal(await picker.locator('[data-glyph="☕"]').getAttribute('data-wide'), 'true');
			assert.match(await picker.locator('[data-glyph="☕"]').getAttribute('title'), /the band draws it ●/);
			assert.equal(await picker.locator('[data-glyph="▲"]').getAttribute('data-wide'), null);
			assert.match(await picker.locator('.legend').innerText(), /two columns wide: .*draws it as ●/);
			await picker.locator('[data-glyph="▲"]').click();
			assert.equal(await picker.count(), 0, 'a pick folds the set away');
			assert.equal(await field.getAttribute('data-glyph'), '▲');
			await page.locator(`${MODAL} button.commit`).click();
			assert.equal((await saved).code, 0);
			await app.idle();
			const echoes = await app.echoes();
			assert.equal(echoes.length, before + 1, `one command: ${echoes.slice(before).join(' | ')}`);
			assert.equal(JSON.parse((await app.run(['spec', 'show', 'tea'])).stdout).glyph, '▲');

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
			await app.run(['track', 'mood', '--verb', 'switch', '--state', 'up', '--state', 'down']);
			const saved = page.evaluate(() => window.datum.spec('mood'));
			await page.locator(MODAL).waitFor();
			await page.locator(`${MODAL} [data-tab="states"]`).click();
			await tile(page, 'the glyph of up').click();
			// Any one character: the first of whatever is typed.
			await page.locator(`${MODAL} .picker input.any`).fill('✺✺');
			assert.equal(await tile(page, 'the glyph of up').getAttribute('data-glyph'), '✺');
			await page.locator(`${MODAL} button.commit`).click();
			assert.equal((await saved).code, 0);
			assert.deepEqual(JSON.parse((await app.run(['spec', 'show', 'mood'])).stdout).state_glyphs, { up: '✺' });

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
			await app.run(['track', 'mood', '--verb', 'switch', '--state', 'up', '--state', 'down', '--state', 'flat']);
			await app.run(['spec', 'set', 'mood', '--state-glyph', 'up=▲', '--state-glyph', 'down=▼', '--state-glyph', 'flat=■']);
			const saved = page.evaluate(() => window.datum.spec('mood'));
			await page.locator(MODAL).waitFor();
			await page.locator(`${MODAL} [data-tab="states"]`).click();
			await page.locator(`${MODAL} button.remove[aria-label="remove down"]`).click();
			// A rename is the same row under a new name, and its glyph comes along.
			await page.locator(`${MODAL} .state input.name`).nth(1).fill('level');
			await page.locator(`${MODAL} button.commit`).click();
			assert.equal((await saved).code, 0);
			const shown = JSON.parse((await app.run(['spec', 'show', 'mood'])).stdout);
			assert.deepEqual(shown.allowed_states, ['up', 'level']);
			assert.deepEqual(shown.state_glyphs, { up: '▲', level: '■' });
		});
	});

	test('a glyph by a key: a value → glyph table, the field\'s glyph for any other value', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'tea', '--verb', 'occur']);
			const saved = page.evaluate(() => window.datum.spec('tea'));
			await page.locator(MODAL).waitFor();
			await page.locator(`${MODAL} .glyph-by [role="radio"]`, { hasText: 'by a key' }).click();
			await page.locator(`${MODAL} input.glyph-key`).fill('size');
			await page.locator(`${MODAL} .glyphs + button.add`).click();
			await page.locator(`${MODAL} .glyphs input.value`).fill('large');
			await tile(page, 'the glyph for large').click();
			await page.locator(`${MODAL} .picker [data-glyph="☕"]`).click();
			assert.equal(await tile(page, 'the glyph for large').getAttribute('data-wide'), 'true', 'the tile keeps the mark');
			assert.equal((await page.locator(`${MODAL} .glyphs .default .value`).innerText()).trim(), '(other)');
			assert.equal(await tile(page, "the field's glyph").getAttribute('data-glyph'), '', 'the default is the circle until picked');
			await tile(page, "the field's glyph").click();
			await page.locator(`${MODAL} .picker [data-glyph="○"]`).click();
			await page.locator(`${MODAL} button.commit`).click();
			assert.equal((await saved).code, 0);
			const shown = (await app.run(['spec', 'show', 'tea'])).stdout;
			assert.deepEqual(JSON.parse(shown).glyph_by, { key: 'size', map: { large: '☕' } });
			assert.equal(JSON.parse(shown).glyph, '○');

			await app.run(['occur', 'tea', 'size=large', '-t', '-20']);
			await app.clock.forward(1000);
			const logged = await app.run(['occur', 'tea', 'size=small']);
			assert.equal(logged.code, 0, logged.stderr);
			assert.deepEqual(await drawn(app), ['☕', '○']);
			// Two columns wide, so the band draws the mapped ☕ as ●.
			const row = await band(app, 'tea');
			assert.match(row, /●.*○/);
			assert.doesNotMatch(row, /☕/);

			// Opened and saved untouched, the glyphs read back byte for byte.
			const again = page.evaluate(() => window.datum.spec('tea'));
			await page.locator(`${MODAL} button.commit`).click();
			assert.equal((await again).code, 0);
			assert.equal((await app.run(['spec', 'show', 'tea'])).stdout, shown);
		});
	});

	// A pictogram a thumb cannot hit is not a pictogram (the bead's point 5).
	test(`every pictogram is a fingertip on a ${PHONE.width}x${PHONE.height} screen`, async () => {
		await withApp(async ({ page }) => {
			await openIt(page);
			const box = await page.locator(MODAL).boundingBox();
			const screen = await page.evaluate(() => ({
				width: document.documentElement.clientWidth,
				height: document.documentElement.clientHeight
			}));
			assert.ok(box.x >= 0 && box.x + box.width <= screen.width + 1, `the modal is ${box.x}–${box.x + box.width} on a ${screen.width}px screen`);
			assert.ok(box.y >= 0 && box.y + box.height <= screen.height + 1, 'the modal is taller than the screen');
			const verbs = page.locator(`${MODAL} .verb`);
			assert.equal(await verbs.count(), VERBS.length);
			for (let i = 0; i < VERBS.length; i++) {
				const item = await verbs.nth(i).boundingBox();
				assert.ok(item.height >= 24, `${VERBS[i].verb} is ${item.height}px tall`);
				assert.ok(item.width >= 24, `${VERBS[i].verb} is ${item.width}px wide`);
				assert.ok(item.x >= 0 && item.x + item.width <= screen.width + 1, `${VERBS[i].verb} is off the screen`);
			}
			for (const control of ['input.field', 'button.commit']) {
				const item = await page.locator(`${MODAL} ${control}`).boundingBox();
				assert.ok(item.y >= 0 && item.y + item.height <= screen.height + 1, `${control} is off the screen`);
			}
		}, { viewport: PHONE });
	});
});
