// A field Track + creates has to be a field the app can log to.
//
// It was not. The form ran `spec set <whatever was typed>`, said "✓ Spec
// saved", and drew a chip. `my coffee` then logged by pressing that chip
// and errored when the same name was typed — a space in a field name being
// exactly what a person naturally types. `cups=2` and `-weird` errored on
// the chip the app itself had just made, in two different ways, and could
// not be typed either: the field was dead on arrival (da-b049).
//
// Four things are checked here, and they are four because fixing any one
// alone leaves the bug:
//
//   1. the modal refuses a name that cannot be logged to, and says why;
//   2. `spec set` typed into the REPL refuses it too — otherwise the fix is
//      a coat of paint on one of the two ways to declare a field;
//   3. every name the form DOES accept logs from both surfaces, and the
//      entry is read back out of storage. Checking only that no `error:`
//      appeared passes on the broken build, because `occur cups=2` printed
//      nothing at all;
//   4. the names that already worked still work — café, 日本語, emoji☕,
//      très-bien. A `[a-z0-9_]` rule would satisfy 1-3 and still be a
//      regression.
//
// The cases come from `tests/fixtures/field-name.json`, which
// `crates/datum-parse/tests/field_name_fixture.rs` also reads: the rule has
// a copy in each language and the fixture is what keeps them one rule.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { startApp, stopApp, withApp } from './support/app.mjs';

const { cases } = JSON.parse(
	readFileSync(
		fileURLToPath(new URL('../../tests/fixtures/field-name.json', import.meta.url)),
		'utf8'
	)
);

/** The three names the bead was filed about, each failing its own way. */
const THE_BEAD = ['my coffee', 'cups=2', '-weird'];

const acceptable = cases.filter((c) => c.ok);
// The empty name is not a state the form can be typed into — it is what the
// modal starts as, and Track + has always refused to submit it.
const refused = cases.filter((c) => !c.ok && c.field !== '');

before(startApp);
after(stopApp);

/**
 * Open the Track modal and type `field` into it, without submitting.
 *
 * A verb is chosen at the same time: `track it` needs both (da-94q1), so a
 * test about the NAME has to answer the other half or every case would
 * read as refused for the wrong reason.
 */
async function typeIntoTheForm(page, field) {
	if ((await page.locator('dialog.track').count()) === 0) {
		await page.locator('.bar button.add-button').click();
		await page.locator('dialog.track').waitFor();
	}
	const form = page.locator('dialog.track');
	await form.locator('input.field').fill(field);
	await form.locator('button[data-verb="occur"]').click();
	const hint = form.locator('p.hint');
	return {
		submittable: await form.locator('button.commit').isEnabled(),
		hint: (await hint.count()) ? (await hint.innerText()).trim() : null
	};
}

/** Press "track it" on the open modal, the way a user declares a field. */
async function trackIt(app, field) {
	await app.page.locator('dialog.track button.commit').click();
	await chipIndex(app, field);
}

/**
 * Where the chip for exactly this field is — not one whose name merely
 * contains it, and not before the bar has rebuilt around it.
 */
async function chipIndex(app, field) {
	await app.page.waitForFunction(
		(name) =>
			[...document.querySelectorAll('.bar button.widget')].some(
				(chip) => chip.textContent.trim() === name
			),
		field
	);
	const labels = await app.page.locator('.bar button.widget').allInnerTexts();
	return labels.findIndex((label) => label.trim() === field);
}

/** Press that chip, and wait for the REPL to echo what it ran. */
async function pressChip(app, field) {
	const index = await chipIndex(app, field);
	const echoes = await app.page.locator('.repl .echo').count();
	await app.page.locator('.bar button.widget').nth(index).click();
	await app.page.waitForFunction(
		(n) => document.querySelectorAll('.repl .echo').length > n,
		echoes
	);
}

describe('a field name the (+) form accepts', () => {
	test('the form refuses the names nothing can log to, and names the rule', async () => {
		await withApp(async ({ page }) => {
			for (const testCase of refused) {
				const { submittable, hint } = await typeIntoTheForm(page, testCase.field);
				assert.equal(submittable, false, `${testCase.name}: (+) still offered to create it`);
				assert.ok(hint, `${testCase.name}: refused with no explanation`);
				assert.match(hint, /field name is one word/, testCase.name);
				if (testCase.suggest !== null) {
					assert.ok(
						hint.includes(testCase.suggest),
						`${testCase.name}: no way out — the hint never names ${testCase.suggest}`
					);
				}
			}
		});
	});

	test('the form still accepts every name that works, unicode included', async () => {
		await withApp(async ({ page }) => {
			for (const testCase of acceptable) {
				const { submittable, hint } = await typeIntoTheForm(page, testCase.field);
				assert.equal(hint, null, `${testCase.name}: refused a working name`);
				assert.equal(submittable, true, testCase.name);
			}
		});
	});

	test('one press of the offered name gets you a usable one', async () => {
		await withApp(async (app) => {
			await typeIntoTheForm(app.page, 'my coffee');
			await app.page.locator('dialog.track p.hint button.fix').click();
			assert.equal(await app.page.locator('dialog.track input.field').inputValue(), 'my_coffee');

			await trackIt(app, 'my_coffee');
			await pressChip(app, 'my_coffee');
			const stored = await app.entries();
			assert.equal(stored.length, 1, 'the chip stored nothing');
			assert.equal(stored[0].field, 'my_coffee');
		});
	});

	test('typing `spec set` refuses the same names', async () => {
		await withApp(async ({ submit, outputs, run }) => {
			for (const testCase of refused) {
				const result = await run(['spec', 'set', testCase.field, '--verb', 'occur']);
				assert.notEqual(result.code, 0, `${testCase.name}: '✓ Spec saved' for a dead field`);
				// A lone comma never reaches `spec set` as a name: the CLI
				// splits its entry chain on it first, and what is left has
				// no name at all. Refused all the same, in the CLI's words.
				const rule = testCase.field === ',' ? /missing required positional/ : /field name is one word/;
				assert.match(result.stderr, rule, testCase.name);
			}
			// And through the REPL as a person reaches it, quoted so the line
			// is one token: the refusal has to be on the screen, not only in
			// the value the seam returned.
			await submit('spec set "my coffee"');
			assert.match((await outputs()).at(-1), /field name is one word/);
		});
	});

	// One app for every name: a fresh app per name was ten cold opens in one
	// test, which the parallel suite stretched past its two minutes. Each
	// name's rows are its own, and the total says nothing else was stored.
	test('every accepted name logs from the chip AND from the keyboard', async () => {
		await withApp(async (app) => {
			const stored = async (field) => {
				const all = await app.entries(100);
				return { total: all.length, mine: all.filter((e) => e.field === field).length };
			};
			for (const [i, testCase] of acceptable.entries()) {
				await typeIntoTheForm(app.page, testCase.field);
				await trackIt(app, testCase.field);

				// Surface one: the chip the app just drew.
				await pressChip(app, testCase.field);
				assert.deepEqual(await stored(testCase.field), { total: 2 * i + 1, mine: 1 }, `${testCase.name}: the chip`);

				// Surface two: the same name typed. Five minutes back, because
				// the slug is UNIQUE per field per minute.
				await app.submit(`occur ${testCase.field} -t -5`);
				assert.deepEqual(await stored(testCase.field), { total: 2 * i + 2, mine: 2 }, `${testCase.name}: typing`);
			}
		});
	});

	test('the fixture still covers the three names the bead was filed about', () => {
		for (const field of THE_BEAD) {
			assert.ok(
				cases.some((c) => c.field === field && !c.ok),
				`no case refuses ${JSON.stringify(field)}`
			);
		}
	});
});
