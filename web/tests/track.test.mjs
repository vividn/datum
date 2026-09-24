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
import { splitStates, trackArgv, trackLabel, VERBS, verbAsked } from '../src/lib/track.ts';
import { stripAnsi } from './support/tail.mjs';

const MODAL = 'dialog.track';
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

describe('the argv it composes', () => {
	test('`track`, not `spec set`: declaring a field is an event in the log', () => {
		assert.deepEqual(trackArgv('coffee', 'occur'), ['track', 'coffee', '--verb', 'occur']);
		assert.deepEqual(trackArgv('sleep', 'start'), ['track', 'sleep', '--verb', 'start']);
	});

	test('a switch carries its states, one flag each; a colour is optional', () => {
		assert.deepEqual(trackArgv('mood', 'switch', ['good', 'bad']), [
			'track', 'mood', '--verb', 'switch', '--state', 'good', '--state', 'bad'
		]);
		assert.deepEqual(trackArgv('coffee', 'occur', [], '#ffa500').slice(-2), ['--color', '#ffa500']);
		// The states box belongs to `switch` alone: a point with one typed
		// into it and then changed away would otherwise smuggle it out.
		assert.deepEqual(trackArgv('coffee', 'occur', ['good']), ['track', 'coffee', '--verb', 'occur']);
	});

	test('the states box splits on commas and drops the blanks a trailing one leaves', () => {
		assert.deepEqual(splitStates(' good , bad ,'), ['good', 'bad']);
		assert.deepEqual(splitStates(''), []);
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
	// reads its own echo from the end. The pictograms (`?repl`), the phone
	// screen and a test that leaves the modal open open their own.
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

	test('Track + opens the modal: four pictograms, `track it` shut until a name and a verb', async () => {
		await withApp(async (app) => { // leaves the modal open, which `solo` does not close
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

			// The states box belongs to the switch, and to nothing else.
			assert.equal(await page.locator(`${MODAL} input.states`).count(), 0);
			await page.locator(`${MODAL} button[data-verb="switch"]`).click();
			await page.locator(`${MODAL} input.states`).waitFor();
			await page.locator(`${MODAL} input.states`).fill('good, bad');
			assert.equal(
				(await page.locator(`${MODAL} code.preview`).innerText()).trim(),
				'track mood --verb switch --state good --state bad'
			);
			assert.deepEqual(app.errors, []);
		});
	});

	test('`track it` declares the field — TRACK▞ in the log, a button in the bar', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await openIt(page);
			await fill(page, 'mood', 'switch');
			await page.locator(`${MODAL} input.states`).fill('good, bad');
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
			await app.submit('ignore run');
			await page.waitForFunction(() => !document.querySelector('.bar [data-field="run"]'));
			await openIt(page);
			await page.locator(`${MODAL} [data-ignored="run"]`).click();
			await page.locator('.bar button[data-field="run"]').waitFor();
			assert.equal((await app.echoes()).at(-1), 'track run');
		});
	});

	// The old inline form is gone, not merely unused: it hung off the bar,
	// scrolled with it, and had to be hung against the screen to be reached
	// at all (da-hkpd). A modal has none of those problems and the form has
	// no second life.
	test('the form that used to hang off the bar is gone from the DOM', async () => {
		await shared.solo(async ({ page }) => { // leaves the modal open: last of the shared
			await page.locator('.bar button.track').click();
			await page.locator(MODAL).waitFor();
			assert.equal(await page.locator('.bar form.add, .bar .panel.add').count(), 0);
			assert.equal(await page.locator(`${MODAL} select`).count(), 0, 'the `<select>` came along');
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
