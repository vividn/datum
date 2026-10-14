// The field widget (da-fu29p): the Track dialog and the spec widget as one.
// vividn, 2026-10-08, after a newcomer found the two confusing: «let's fully
// combine track/spec widgets into just 1, and make the appearance better
// when no fields exist … make it so that you can switch the field you're
// editing inside of it more easy and intuitive … It should also be clearer
// what "field" actually is and means.»
//
// So what is held here is what a newcomer meets: the sentence, the examples
// that fill the form and not the database, a save that leaves a field the
// bar can log, and the chips that go from one field to the next. The form's
// own tabs are `track.test.mjs`'s, the place's coming and going
// `specplace.test.mjs`'s.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp } from './support/app.mjs';
import { EXAMPLES, VERBS, trackArgv } from '../src/lib/track.ts';
import { shellJoin } from '../src/lib/shellSplit.ts';
import { DESKTOP } from './support/layout.mjs';
import { FIELD_FORM, openField } from './support/field.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

/** The widget, wherever it is drawn: the place modal on a desktop, or a pane. */
const WIDGET = ':is(dialog.sheet[data-place="field"], section.pane[data-title="field"])';
const chips = (page) => page.locator(`${WIDGET} .chips button`);
const chipTexts = async (page) => (await chips(page).allInnerTexts()).map((t) => t.trim());
const marked = async (page) => (await page.locator(`${WIDGET} .chips [aria-current="true"]`).allInnerTexts()).map((t) => t.trim());
const glyphOf = (verb) => VERBS.find((v) => v.verb === verb).glyph;

/** A line the bar logs each example with, once it is saved, and the data that line stores. */
const LOGGED = {
	alcohol: ['alcohol', null],
	sleep: ['sleep', null],
	activity: ['activity meeting', null],
	person: ['add person id=ada first_name=Ada city=London', { first_name: 'Ada', city: 'London' }],
	mood: ['mood -- -3', { level: -3 }],
	emotion: ['emotion calm tired', { emotions: ['calm', 'tired'] }]
};

describe('the field widget', () => {
	const shared = sharedApp(DESKTOP);

	test('with no fields: what a field is, a name box and the six examples', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await page.locator('.bar button[data-track]').click();
			await page.locator(FIELD_FORM).waitFor();
			assert.equal(
				(await page.locator(`${WIDGET} p.what`).innerText()).trim(),
				'A field is one thing you track: mood, coffee, sleep. Each time you log it, that\'s an entry.'
			);
			assert.deepEqual(await chipTexts(page), ['+ new field']);
			assert.deepEqual(await marked(page), ['+ new field']);
			assert.equal(await page.locator(`${FIELD_FORM} input.field`).inputValue(), '');
			assert.deepEqual(
				await page.locator(`${FIELD_FORM} [data-example]`).evaluateAll((all) => all.map((b) => b.dataset.example)),
				EXAMPLES.map((e) => e.field)
			);
			// Each says its kind by the CLI's own glyph, and what it shows.
			assert.deepEqual(
				(await page.locator(`${FIELD_FORM} [data-example] .name`).allInnerTexts()).map((t) => t.trim()),
				EXAMPLES.map((e) => `${glyphOf(e.verb)} ${e.field}`)
			);
			// Nothing else until it has a name: no kinds, no tabs, nothing to save.
			assert.equal(await page.locator(`${FIELD_FORM} .verb`).count(), 0);
			assert.equal(await page.locator(`${FIELD_FORM} [role="tab"]`).count(), 0);
			assert.equal(await page.locator(`${FIELD_FORM} button.commit`).isEnabled(), false);
			assert.deepEqual(app.errors, []);
		});
	});

	test('the kinds are named by what a field of each does, beside the CLI\'s glyph and word', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await page.locator('.bar button[data-track]').click();
			await page.locator(`${FIELD_FORM} input.field`).fill('tea');
			const said = await page.locator(`${FIELD_FORM} .verb`).evaluateAll((all) =>
				all.map((b) => [...b.children].map((c) => c.textContent.trim()))
			);
			assert.deepEqual(said, [
				['●', 'occurrence', 'occur'],
				['█', 'on/off', 'start'],
				['▞', 'switch', 'switch'],
				['¢', 'card', 'add']
			]);
			// A new field is Basics alone: the tabs come once it is saved.
			assert.equal(await page.locator(`${FIELD_FORM} [role="tab"]`).count(), 0);
			assert.equal(await page.locator(`${FIELD_FORM} [data-example]`).count(), 0, 'a named field still offers the examples');
		});
	});

	for (const example of EXAMPLES) {
		test(`the ${example.field} example fills the form, not the database, and saves to a field the bar logs`, async () => {
			await shared.solo(async (app) => {
				const { page } = app;
				const { saved } = await openField(page);
				const tall = () => page.locator(FIELD_FORM).evaluate((el) => el.getBoundingClientRect().height);
				await page.locator(`${FIELD_FORM} [data-example]`).first().waitFor();
				const was = await tall();
				await page.locator(`${FIELD_FORM} [data-example="${example.field}"]`).click();
				// The place keeps its size: nothing moves under the finger that tapped.
				await page.locator(`${FIELD_FORM} .verb`).first().waitFor();
				assert.equal(await tall(), was, 'the form changed height from the examples to Basics');

				// The form says what the field would be: its name, kind, states, keys.
				assert.equal(await page.locator(`${FIELD_FORM} input.field`).inputValue(), example.field);
				assert.equal(await page.locator(`${FIELD_FORM} .verb[aria-checked="true"]`).getAttribute('data-verb'), example.verb);
				assert.deepEqual(await page.locator(`${FIELD_FORM} .state input.name`).evaluateAll((all) => all.map((i) => i.value)), example.states ?? []);
				assert.deepEqual(
					await page.locator(`${FIELD_FORM} .basic-keys li`).evaluateAll((all) => all.map((li) => [li.querySelector('input.name').value, li.querySelector('select.kind').value])),
					(example.keys ?? []).map((k) => [k.name, k.kind])
				);
				assert.equal(await page.locator(`${FIELD_FORM} input.remainder`).inputValue(), example.remainder ?? '');
				const argv = trackArgv(
					example.field,
					example.verb,
					(example.states ?? []).map((name) => ({ name, color: '', description: '' })),
					'',
					(example.keys ?? []).map((k) => ({ ...k, value: '' })),
					example.remainder
				);
				assert.equal((await page.locator(`${FIELD_FORM} code.preview`).innerText()).trim(), shellJoin(argv));
				// And nothing is written yet.
				await app.idle();
				assert.deepEqual(await app.allEntries(), []);
				assert.equal((await app.run(['spec', 'list'])).stdout.includes(example.field), false, 'the tap declared it');

				// Saved, it is one `track`, and the widget stays on the field it made — with its tabs now.
				await page.locator(`${FIELD_FORM} button.commit`).click();
				const result = await saved;
				assert.equal(result.code, 0, result.stderr);
				assert.equal((await app.echoes()).at(-1), shellJoin(argv));
				await page.locator(`${FIELD_FORM} h2`).waitFor();
				assert.equal((await page.locator(`${FIELD_FORM} h2`).innerText()).trim(), example.field);
				assert.deepEqual(
					await page.locator(`${FIELD_FORM} [role="tab"]`).evaluateAll((all) => all.map((t) => t.dataset.tab)),
					['basics', 'states', 'keys', 'aliases', 'placement']
				);
				await until(() => marked(page), (m) => m.join() === `${glyphOf(example.verb)} ${example.field}`);
				await page.keyboard.press('Escape');
				await page.locator(FIELD_FORM).waitFor({ state: 'detached' });

				// A working field: the bar logs one entry of it.
				const [line, data] = LOGGED[example.field];
				await app.clock.forward(1000);
				await app.submit(line);
				assert.deepEqual(app.errors, []);
				const logged = (await app.entries()).filter((e) => e.field === example.field);
				assert.equal(logged.length, 1, JSON.stringify(await app.allEntries()));
				if (data) assert.deepEqual(logged[0].data, data);
			});
		});
	}

	test('the chips are the tracked fields in their own colour and glyph, and a tap goes to that field', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'coffee', '--verb', 'occur', '--color', '#884400']);
			await app.run(['track', 'sleep', '--verb', 'start']);
			await app.run(['track', 'mood', '--verb', 'switch', '--state', 'good']);
			await app.run(['track', 'person', '--verb', 'add']);
			await app.redraw();
			await openField(page, 'coffee');
			await page.locator(`${FIELD_FORM} h2`).waitFor();
			assert.deepEqual([...(await chipTexts(page))].sort(), ['+ new field', '¢ person', '█ sleep', '▞ mood', '● coffee'].sort());
			assert.deepEqual(await marked(page), ['● coffee']);
			assert.equal(
				await page.locator(`${WIDGET} .chips [data-chip="coffee"] span`).evaluate((el) => getComputedStyle(el).color),
				'rgb(136, 68, 0)'
			);

			await page.locator(`${WIDGET} .chips [data-chip="sleep"]`).click();
			await until(async () => (await page.locator(`${FIELD_FORM} h2`).innerText()).trim(), (t) => t === 'sleep');
			assert.deepEqual(await marked(page), ['█ sleep']);
			assert.equal(await page.locator(`${FIELD_FORM} .verb[aria-checked="true"]`).getAttribute('data-verb'), 'start');

			// Edits nobody saved are not dropped by a stray tap: the chip asks, and cancel keeps them.
			const h2 = async () => (await page.locator(`${FIELD_FORM} h2`).allInnerTexts()).join().trim();
			const asks = (answer) => page.locator('dialog.confirm').getByRole('button', { name: answer, exact: true }).click();
			await page.locator(`${FIELD_FORM} input.description`).fill('nights');
			await page.locator(`${WIDGET} .chips [data-chip="coffee"]`).click();
			assert.match(await page.locator('dialog.confirm p').innerText(), /^Leave sleep without saving/);
			await asks('cancel');
			assert.equal(await h2(), 'sleep');
			assert.equal(await page.locator(`${FIELD_FORM} input.description`).inputValue(), 'nights', 'cancel lost the edit');
			await page.locator(`${WIDGET} .chips [data-chip="coffee"]`).click();
			await asks('leave');
			await until(h2, (t) => t === 'coffee');
			assert.equal(JSON.parse((await app.run(['spec', 'show', 'sleep'])).stdout).description, null, 'leaving saved');

			// And «+ new field» is the way back to an empty form and its examples.
			await page.locator(`${WIDGET} .chips .new`).click();
			await page.locator(`${FIELD_FORM} [data-example]`).first().waitFor();
			assert.deepEqual(await marked(page), ['+ new field']);
			assert.equal(await page.locator(`${FIELD_FORM} input.field`).inputValue(), '');
			// Less the ones there is a field for already: a tap would re-declare it.
			assert.deepEqual(
				await page.locator(`${FIELD_FORM} [data-example]`).evaluateAll((all) => all.map((b) => b.dataset.example)),
				['alcohol', 'activity', 'emotion']
			);
			assert.deepEqual(app.errors, []);
		});
	});

	test('typing narrows the chips once there are more than a glance takes in', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			const names = ['tea', 'team', 'run', 'walk', 'read', 'nap'];
			for (const name of names) await app.run(['track', name, '--verb', 'occur']);
			await app.redraw();
			await openField(page);
			await until(() => chips(page).count(), (n) => n === names.length + 1);
			assert.equal(await page.locator(`${WIDGET} .chips .find`).count(), 0, 'six fields are a glance');

			await app.run(['track', 'swim', '--verb', 'occur']);
			await app.redraw();
			const find = page.locator(`${WIDGET} .chips .find`);
			// Whatever the case it is typed in.
			await find.fill('TEA');
			assert.deepEqual([...(await chipTexts(page))].sort(), ['+ new field', '● tea', '● team'].sort());
			await find.fill('');
			assert.equal(await chips(page).count(), names.length + 2);
		});
	});

	// A save takes a moment, and a reader may go to another field in it: they stay where they went.
	test('a chip pressed while a save runs is where the reader stays, the place still open', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'tea', '--verb', 'occur']);
			await app.run(['track', 'nap', '--verb', 'start']);
			await app.redraw();
			await openField(page, 'nap');
			await page.locator(`${FIELD_FORM} h2`).waitFor();
			await page.locator(`${WIDGET} .chips [data-chip="tea"]`).waitFor();
			const before = (await app.echoes()).length;
			// Done and the chip in one turn of the page, so the save is in flight when the chip lands.
			await page.evaluate((form) => {
				document.querySelector(`${form} button.commit`).click();
				document.querySelector('[data-chip="tea"]').click();
			}, FIELD_FORM);
			await until(async () => (await app.echoes()).length, (n) => n === before + 1);
			await app.idle();
			assert.equal((await page.locator(`${FIELD_FORM} h2`).allInnerTexts()).join().trim(), 'tea', 'the save took the place away, or the field back');
			assert.match((await app.echoes()).at(-1), /^spec set nap /);
		});
	});

	// The key a line's leftover words go to is a spec's to change as well as a new field's.
	test('the rest-of-line key of a saved field is read, changed and cleared in its one `spec set`', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'emotion', '--verb', 'occur', '-R', 'emotions']);
			const remainder = async () => JSON.parse((await app.run(['spec', 'show', 'emotion'])).stdout).remainder;
			for (const [typed, stored] of [['words', 'words'], ['', undefined]]) {
				const { saved } = await openField(page, 'emotion');
				const box = page.locator(`${FIELD_FORM} input.remainder`);
				assert.equal(await box.inputValue(), (await remainder()) ?? '');
				await box.fill(typed);
				await page.locator(`${FIELD_FORM} button.commit`).click();
				assert.equal((await saved).code, 0);
				assert.equal(await remainder(), stored);
			}
		});
	});

	/** A widget of `kind` in the page's first pane, as the chooser puts one. */
	const put = async (page, kind) => {
		const tab = await page.evaluate(() => window.datum.layout.current().root.tabs[0].content.children[0].tabs[0].id);
		await page.evaluate(([tab, kind]) => window.datum.shell.widgetInto(tab, kind), [tab, kind]);
		return page.locator(`section.pane[data-title="${kind}"]`);
	};

	test('`track` in the commands widget opens it on a new field, and writes no line', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			const track = (await put(page, 'command')).locator('[data-command="track"]');
			assert.equal(await track.getAttribute('data-act'), 'open');
			await track.click();
			await page.locator(`${FIELD_FORM} input.field`).waitFor();
			assert.equal(await page.locator('.prompt-bar input').first().inputValue(), '', 'the tap wrote a line as well');
			assert.deepEqual(await marked(page), ['+ new field']);
		});
	});

	// In a pane it is where someone chose to work: a field pill minted
	// beside it — a hold on a field in the fields widget — brings that field.
	test('in a pane, a field pill minted beside it brings that field, and a save stays', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await app.submit('track tea --verb occur');
			const pane = await put(page, 'field');
			await pane.locator(`${FIELD_FORM} input.field`).waitFor();

			await page.locator('.bar button[data-field="tea"]').click({ modifiers: ['Shift'] });
			await pane.locator(`${FIELD_FORM} h2`).waitFor();
			assert.equal((await pane.locator(`${FIELD_FORM} h2`).innerText()).trim(), 'tea');
			assert.equal(await page.locator('dialog[open]').count(), 0, 'a widget in view opened a place as well');

			// And the fields widget's `+` goes to the pane, not to a place of its own.
			await page.locator('.bar button[data-track]').click();
			await pane.locator(`${FIELD_FORM} input.field`).fill('nap');
			await pane.locator(`${FIELD_FORM} button[data-verb="start"]`).click();
			await pane.locator(`${FIELD_FORM} button.commit`).click();
			await until(async () => (await pane.locator(`${FIELD_FORM} h2`).allInnerTexts()).join().trim(), (t) => t === 'nap');
			assert.equal(JSON.parse((await app.run(['spec', 'show', 'nap'])).stdout).verb, 'start');
			assert.equal(await page.locator('dialog[open]').count(), 0);
			assert.deepEqual(app.errors, []);
		});
	});
});
