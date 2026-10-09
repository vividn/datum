// A field the (+) creates has to be a field the app can log to.
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
import { sharedApp, startApp, stopApp } from './support/app.mjs';
import { along, widgetPane } from './support/layout.mjs';
import { FIELD_FORM } from './support/field.mjs';
const { cases } = JSON.parse(
	readFileSync(
		fileURLToPath(new URL('../../tests/fixtures/field-name.json', import.meta.url)),
		'utf8'
	)
);

/** The names the bead was filed about, each failing its own way. */
const THE_BEAD = ['cups=2', '-weird'];

/** What the rule's sentence says, `field_name::RULE`. */
const THE_RULE = /field name has no equals, commas or colons/;

const acceptable = cases.filter((c) => c.ok);
// The empty name is not a state the form can be typed into — it is what the
// modal starts as, and the (+) has always refused to submit it. Nor is a
// space at either end: the form trims what is typed, so ` coffee` declares
// `coffee` there, and only `spec set` sees the name untrimmed.
const refused = cases.filter((c) => !c.ok && c.field !== '');
const formRefused = refused.filter((c) => c.field.trim() === c.field);

before(startApp);
after(stopApp);

// ONE APP for the file (da-6b3dj, sharedApp in support/app.mjs): `solo`
// empties the database each test wrote, so the counts below are this
// test's, and closes the field widget a test left open.
const shared = sharedApp();
// `main` as a draft of the REPL and the widget bar alone: nothing here
// reads the rest of the day frame, and drawing it costs what a boot did
// (da-m1m9a).
const BAR = JSON.stringify(along('column', widgetPane('repl', 'repl'), widgetPane('bar', 'widgetbar')));
const onBar = (fn) => shared.solo(fn, BAR);

/**
 * Open the field widget on a new field and type `field` into it, without
 * submitting.
 *
 * A verb is chosen at the same time: Done needs both (da-94q1), so a
 * test about the NAME has to answer the other half or every case would
 * read as refused for the wrong reason.
 */
async function typeIntoTheForm(page, field) {
	const form = page.locator(FIELD_FORM);
	if ((await form.count()) === 0) await page.locator('.bar button.add-button').click();
	await form.locator('input.field').fill(field);
	await form.locator('button[data-verb="occur"]').click();
	const hint = form.locator('p.hint');
	return {
		submittable: await form.locator('button.commit').isEnabled(),
		hint: (await hint.count()) ? (await hint.innerText()).trim() : null
	};
}

/** Press Save on the open form, the way a user declares a field. */
async function trackIt(app, field) {
	await app.page.locator(`${FIELD_FORM} button.commit`).click();
	await chipIndex(app, field);
	// The widget stays on the field it made, over the page: left, to press the chip.
	await app.page.keyboard.press('Escape');
	await app.page.locator(FIELD_FORM).waitFor({ state: 'detached' });
	// A second on: the clock is held, and a log at the instant the field
	// was declared would share the declaration's slug.
	await app.clock.forward(1000);
}

/**
 * Where the chip for exactly this field is — not one whose name merely
 * contains it, and not before the bar has rebuilt around it.
 */
async function chipIndex(app, field) {
	await app.page.waitForFunction(
		(name) =>
			[...document.querySelectorAll('.bar button.widget')].some(
				(chip) => chip.dataset.field === name
			),
		field
	);
	// By `data-field`, not the label: an occur chip's label leads with its
	// glyph (da-h2io6).
	const names = await app.page.locator('.bar button.widget').evaluateAll((all) => all.map((chip) => chip.dataset.field));
	return names.indexOf(field);
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
		await onBar(async ({ page }) => {
			for (const testCase of formRefused) {
				const { submittable, hint } = await typeIntoTheForm(page, testCase.field);
				assert.equal(submittable, false, `${testCase.name}: (+) still offered to create it`);
				assert.ok(hint, `${testCase.name}: refused with no explanation`);
				assert.match(hint, THE_RULE, testCase.name);
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
		await onBar(async ({ page }) => {
			for (const testCase of acceptable) {
				const { submittable, hint } = await typeIntoTheForm(page, testCase.field);
				assert.equal(hint, null, `${testCase.name}: refused a working name`);
				assert.equal(submittable, true, testCase.name);
			}
		});
	});

	test('one press of the offered name gets you a usable one', async () => {
		await onBar(async (app) => {
			await typeIntoTheForm(app.page, 'cups=2');
			await app.page.locator(`${FIELD_FORM} p.hint button.fix`).click();
			assert.equal(await app.page.locator(`${FIELD_FORM} input.field`).inputValue(), 'cups_2');

			await trackIt(app, 'cups_2');
			await pressChip(app, 'cups_2');
			const stored = await app.entries();
			assert.equal(stored.length, 1, 'the chip stored nothing');
			assert.equal(stored[0].field, 'cups_2');
		});
	});

	test('typing `spec set` refuses the same names', async () => {
		await onBar(async ({ submit, outputs, run }) => {
			for (const testCase of refused) {
				const result = await run(['spec', 'set', testCase.field, '--verb', 'occur']);
				assert.notEqual(result.code, 0, `${testCase.name}: '✓ Spec saved' for a dead field`);
				// A lone comma never reaches `spec set` as a name: the CLI
				// splits its entry chain on it first, and what is left has
				// no name at all. Refused all the same, in the CLI's words.
				const rule = testCase.field === ',' ? /missing required positional/ : THE_RULE;
				assert.match(result.stderr, rule, testCase.name);
			}
			// And through the REPL as a person reaches it, quoted so the line
			// is one token: the refusal has to be on the screen, not only in
			// the value the seam returned.
			await submit('spec set " coffee"');
			assert.match((await outputs()).at(-1), THE_RULE);
		});
	});

	// One app for every name: a fresh app per name was ten cold opens in one
	// test, which the parallel suite stretched past its two minutes. Each
	// name's rows are its own, and the total says nothing else was stored.
	test('every accepted name logs from the chip AND from the keyboard', async () => {
		await onBar(async (app) => {
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

				// Surface two: the same name typed, quoted as the web writes it
				// (da-7h73h). Five minutes back, because the slug is UNIQUE
				// per field per minute.
				await app.submit(`occur "${testCase.field}" -t -5`);
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
		assert.ok(cases.some((c) => c.field === 'my coffee' && c.ok), 'no case accepts a spaced name (da-7h73h)');
	});
});
