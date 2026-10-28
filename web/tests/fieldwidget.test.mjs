// The field widget (da-fu29p): the Track dialog and the spec widget as one.
// vividn, 2026-10-08, after a newcomer found the two confusing: «let's fully
// combine track/spec widgets into just 1, and make the appearance better
// when no fields exist … make it so that you can switch the field you're
// editing inside of it more easy and intuitive … It should also be clearer
// what "field" actually is and means.»
//
// So what is held here is what a newcomer meets: the sentence, the shapes
// first (da-uipyt), each with an «ex.» part opening a sheet of examples
// that fill the form and not the database (da-yqx0t), the `field:` line
// and its «fields» (da-zz9rn), a save that leaves a field the
// bar can log, and the sheet of fields that goes from one to the next.
// The form's own tabs are `track.test.mjs`'s, the place's coming and going
// `specplace.test.mjs`'s.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, withApp } from './support/app.mjs';
import { EXAMPLES, VERBS } from '../src/lib/track.ts';
import { DESKTOP, PHONE } from './support/layout.mjs';
import { FIELD_FORM, PANEL, leaveField, declared, openField } from './support/field.mjs';
import { until } from './support/until.mjs';
import { shellJoin } from '../src/lib/shellSplit.ts';

before(startApp);
after(stopApp);

/** The widget, wherever it is drawn: the place modal on a desktop, or a pane. */
const WIDGET = ':is(dialog.sheet[data-place="setup"], section.pane[data-title="setup"])';
/** The sheet «fields» opens, and the fields in it. */
const PICKER = 'dialog.sheet[data-picker]';
const pick = (page) => page.locator(`${FIELD_FORM} button.pick`);
const chips = (page) => page.locator(`${PICKER} .chips button`);
const chipTexts = async (page) => (await chips(page).allInnerTexts()).map((t) => t.trim());
const marked = async (page) => (await page.locator(`${PICKER} .chips [aria-current="true"]`).allInnerTexts()).map((t) => t.trim());
/** The field the form is on: its name line's h2. */
const heading = async (page) => (await page.locator(`${FIELD_FORM} h2`).allInnerTexts()).join().trim();

const TABS = ['basics', 'data', 'aliases'];
/** The shape's «ex.» part, which opens its examples. */
const more = (page, verb) => page.locator(`${FIELD_FORM} [data-examples="${verb}"]`);
const SHEET = 'dialog.sheet[data-catalog]';
const offered = (page) => page.locator(`${FIELD_FORM} [data-example]`).evaluateAll((all) => all.map((b) => b.dataset.example));
/** The examples `verb`'s «ex.» opens, the sheet shut again with Escape. */
async function examplesOf(page, verb) {
	await more(page, verb).click();
	await page.locator(SHEET).waitFor();
	const fields = await offered(page);
	await page.keyboard.press('Escape');
	await page.locator(SHEET).waitFor({ state: 'detached' });
	return fields;
}
/** Whether a text box has the focus: on a phone, the keyboard up. */
const typing = (page) => page.evaluate(() => !!document.activeElement?.matches('input, textarea, [contenteditable]'));

/** A line the bar logs each example with, once it is saved, and the data that line stores. */
const LOGGED = {
	coffee: ['coffee large', { size: 'large' }],
	mood: ['mood -- -3', { level: -3 }],
	headache: ['headache 7', { severity: 7 }],
	medication: ['medication ibuprofen 200mg', { name: 'ibuprofen', dose: '200mg' }],
	sleep: ['sleep', null],
	screentime: ['screentime', null],
	work: ['work', null],
	shower: ['shower', null],
	menstruation: ['menstruation', null],
	activity: ['activity meeting', null],
	environment: ['environment train', null],
	reading: ['reading dune', null],
	person: ['person vr520 vremya cantown', { id: 'vr520', name: 'vremya', city: 'cantown' }],
	book: ['book dune Dune Herbert', { id: 'dune', title: 'Dune', author: 'Herbert' }],
	place: ['place rosa Rosa Kastanienallee', { id: 'rosa', name: 'Rosa', address: 'Kastanienallee' }]
};

describe('the field widget', () => {
	const shared = sharedApp(DESKTOP);

	test('with no fields: what a field is, the four shapes first, a name box, and every tab', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await openField(page);
			await page.locator(FIELD_FORM).waitFor();
			// It says what a field is, before anything else is asked.
			assert.ok((await page.locator(`${WIDGET} p.what`).innerText()).trim());
			// No «+ new field», and nothing to edit yet, so no «fields».
			assert.equal(await page.locator(`${WIDGET} button.new`).count(), 0);
			assert.equal(await pick(page).count(), 0);
			assert.equal(await page.locator(`${FIELD_FORM} input.field`).inputValue(), '');
			// The shapes come first, above the name, each named and lined as the welcome has it.
			const said = await page.locator(`${FIELD_FORM} .verb`).evaluateAll((all) =>
				all.map((b) => [...b.children].map((c) => c.textContent.trim()))
			);
			assert.deepEqual(said, VERBS.map((v) => [v.glyph, v.what, v.line]));
			const top = async (sel) => (await page.locator(`${FIELD_FORM} ${sel}`).first().boundingBox()).y;
			assert.ok((await top('.verb')) < (await top('input.field')), 'the name comes before the shapes');
			// None chosen, and every tab there before a save, none of them required.
			assert.equal(await page.locator(`${FIELD_FORM} .verb[aria-checked="true"]`).count(), 0);
			assert.deepEqual(await page.locator(`${FIELD_FORM} [role="tab"]`).evaluateAll((all) => all.map((t) => t.dataset.tab)), TABS);
			// The examples wait behind each shape's «ex.».
			assert.equal(await page.locator(`${FIELD_FORM} [data-example]`).count(), 0);
			assert.equal(await page.locator(`${FIELD_FORM} button.commit`).isEnabled(), false);
			assert.deepEqual(app.errors, []);
		});
	});

	// vividn, 2026-10-19: «after selecting an example don't enter text edit mode and open the keyboard, that is
	// jarring. In fact, maybe don't enter that either after selecting a data shape» (da-zz9rn).
	test('on a phone: the name line, description first, no keyboard after a shape or an example, and a field saved tracked', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const { saved } = await openField(page);
			// `field:`, its box asking what to track, and nothing to edit yet.
			const line = page.locator(`${FIELD_FORM} .name-line`);
			assert.equal((await line.locator('.label').innerText()).trim(), 'field:');
			assert.ok(await line.locator('input.field').getAttribute('placeholder'), 'the box asks nothing');
			assert.equal(await page.locator(`${WIDGET} button.new`).count(), 0, 'a «+ new field» line');
			assert.equal(await pick(page).count(), 0);
			// No tracked / ignored control, no command preview, no paragraph explaining the data.
			for (const gone of ['[data-tracking]', 'button.info', '.preview', 'p.about']) {
				assert.equal(await page.locator(`${FIELD_FORM} ${gone}`).count(), 0, gone);
			}
			// The description is the first thing on Basics.
			const panel = page.locator(`${FIELD_FORM} [data-panel="basics"]`);
			const tops = await panel.evaluate((el) => [...el.children].map((c) => [c.getBoundingClientRect().top, c.querySelector('input.description') !== null]));
			assert.equal(tops.find(([top]) => top === Math.min(...tops.map(([t]) => t)))[1], true, 'the description is not first');

			// Neither a shape nor an example puts the focus in a text box.
			await page.locator(`${FIELD_FORM} button[data-verb="start"]`).click();
			assert.equal(await typing(page), false, 'a shape opened the keyboard');
			await more(page, 'occur').click();
			await page.locator(`${SHEET} [data-example="coffee"]`).click();
			await page.locator(SHEET).waitFor({ state: 'detached' });
			assert.equal(await page.locator(`${FIELD_FORM} input.field`).inputValue(), 'coffee');
			assert.equal(await typing(page), false, 'an example opened the keyboard');

			// Saved, it is tracked: one `setup F --track`. (A phone's CLI widget is folded away: the save says what it ran.)
			await page.locator(`${FIELD_FORM} button.commit`).click();
			const result = await saved;
			assert.equal(result.code, 0, result.stderr);
			assert.equal(declared(shellJoin(result.argv)).line, 'setup coffee --from-json --track');
			assert.equal(JSON.parse((await app.run(['fields', '--json'])).stdout).find((f) => f.field === 'coffee')?.ignored, false);
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	test('on a phone, «fields» beside the name opens the fields, and one goes to that field', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.run(['track', 'tea', '--verb', 'occur']);
			await app.run(['track', 'nap', '--verb', 'start']);
			await app.redraw();
			await openField(page);
			await page.locator(`${FIELD_FORM} input.field`).waitFor();
			const button = pick(page);
			assert.equal((await button.innerText()).trim(), 'fields');
			// At 390 px the box's placeholder takes the line, and «fields» wraps under it (da-qimwi).
			const [box, name] = [await button.boundingBox(), await page.locator(`${FIELD_FORM} input.field`).boundingBox()];
			assert.ok(box.y >= name.y + name.height && box.x < name.x, 'the button is not under the name');
			await button.click();
			await page.locator(PICKER).waitFor();
			assert.deepEqual([...(await chipTexts(page))].sort(), ['● tea', '█ nap'].sort());
			await page.locator(`${PICKER} [data-chip="nap"]`).click();
			await page.locator(PICKER).waitFor({ state: 'detached' });
			await until(() => heading(page), (t) => t === 'nap');
			// On a field, the line is `field: nap` over its shapes, the button still beside it.
			const top = async (sel) => (await page.locator(`${FIELD_FORM} ${sel}`).first().boundingBox()).y;
			assert.ok((await top('.name-line')) < (await top('.verb')), 'a field\'s name is not over its shapes');
			assert.equal((await page.locator(`${FIELD_FORM} .name-line .label`).innerText()).trim(), 'field:');
			// On a field, the sheet's first entry goes back to a new one.
			await pick(page).click();
			assert.deepEqual(await marked(page), ['█ nap']);
			assert.equal((await chips(page).first().innerText()).trim(), '+ new field');
			await page.locator(`${PICKER} button.new`).click();
			await page.locator(`${FIELD_FORM} input.field`).waitFor();
			assert.equal(await page.locator(`${FIELD_FORM} input.field`).inputValue(), '');
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	// vividn, 2026-10-15: «divide the shape button into a little extra chunk to the side (maybe 20-30%), and
	// write ex. centered there … Open up examples in a separate dialog box. Use nice icons for each».
	test('on a phone each shape\'s «ex.» part opens a sheet of its own examples, each with its icon', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const { saved } = await openField(page);
			for (const { verb, what } of VERBS) {
				const ex = more(page, verb);
				assert.equal((await ex.innerText()).trim(), 'ex.');
				const part = await ex.boundingBox();
				const card = await page.locator(`${FIELD_FORM} [data-verb="${verb}"]`).boundingBox();
				const share = part.width / (part.width + card.width);
				assert.ok(share >= 0.2 && share <= 0.3, `${verb}'s ex. is ${share} of its card`);
				assert.equal(Math.round(part.y), Math.round(card.y), 'the ex. part is not beside the shape');
				assert.equal(await ex.evaluate((el) => getComputedStyle(el).textAlign), 'center');

				await ex.click();
				await page.locator(SHEET).waitFor();
				assert.equal((await page.locator(`${SHEET} .sheet-head > span`).innerText()).trim(), `examples of ${what}`);
				const mine = EXAMPLES.filter((e) => e.verb === verb);
				assert.ok(mine.length >= 3, `${verb} has ${mine.length} examples`);
				assert.deepEqual(await offered(page), mine.map((e) => e.field));
				// Each with its icon in its colour, its name, and what it shows.
				const rows = await page.locator(`${SHEET} [data-example]`).evaluateAll((all) =>
					all.map((b) => [b.querySelector('.icon').textContent, getComputedStyle(b.querySelector('.icon')).color, b.querySelector('.name').textContent, b.querySelector('.what').textContent])
				);
				const rgb = (hex) => `rgb(${[1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)).join(', ')})`;
				assert.deepEqual(rows, mine.map((e) => [e.glyph, rgb(e.color), e.field, e.shows]));
				await page.keyboard.press('Escape');
				await page.locator(SHEET).waitFor({ state: 'detached' });
			}
			// Nothing was chosen by looking.
			assert.equal(await page.locator(`${FIELD_FORM} .verb[aria-checked="true"]`).count(), 0);

			// A tap fills the form — name, glyph, states and their colours — and shuts the sheet.
			const environment = EXAMPLES.find((e) => e.field === 'environment');
			await more(page, 'switch').click();
			await page.locator(`${SHEET} [data-example="environment"]`).click();
			await page.locator(SHEET).waitFor({ state: 'detached' });
			assert.equal(await page.locator(`${FIELD_FORM} input.field`).inputValue(), 'environment');
			assert.equal(await page.locator(`${FIELD_FORM} .tile[aria-label^="the field's symbol:"]`).getAttribute('data-glyph'), '🌳');
			assert.deepEqual(await page.locator(`${FIELD_FORM} .state input.name`).evaluateAll((all) => all.map((i) => i.value)), Object.keys(environment.states));
			await page.locator(`${FIELD_FORM} button.commit`).click();
			const { line, spec } = declared(shellJoin((await saved).argv));
			assert.equal(line, 'setup environment --from-json --track');
			assert.deepEqual([spec.verb, spec.color, spec.allowed_states], ['switch', environment.color, Object.keys(environment.states)]);
			assert.deepEqual(spec.state_colors, Object.fromEntries(Object.entries(environment.states).filter(([, color]) => color)));
			assert.equal(environment.states.outside, '#9ad48a');
			assert.equal(environment.states.airplane, '#ece8df');
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	for (const example of EXAMPLES) {
		test(`the ${example.field} example fills the form, not the database, and saves to a field the bar logs`, async () => {
			await shared.solo(async (app) => {
				const { page } = app;
				const { saved } = await openField(page);
				const tall = () => page.locator(FIELD_FORM).evaluate((el) => el.getBoundingClientRect().height);
				// Typed before it, and not the example's: the example is the whole field.
				await page.locator(`${FIELD_FORM} input.description`).fill('left over');
				await more(page, example.verb).click();
				const was = await tall();
				await page.locator(`${FIELD_FORM} [data-example="${example.field}"]`).click();
				// The place keeps its size: nothing moves under the finger that tapped.
				assert.equal(await tall(), was, 'the form changed height from the examples to Basics');
				assert.equal(await page.locator(`${FIELD_FORM} [data-example]`).count(), 0, 'the examples stayed open');

				// The form says what the field would be: its name, ready to retype, its shape, states, data.
				assert.equal(await page.locator(`${FIELD_FORM} input.field`).inputValue(), example.field);
				assert.equal(await typing(page), false, 'the example opened the keyboard');
				assert.equal(await page.locator(`${FIELD_FORM} input.description`).inputValue(), '', 'what was typed before stayed');
				assert.equal(await page.locator(`${FIELD_FORM} .verb[aria-checked="true"]`).getAttribute('data-verb'), example.verb);
				assert.deepEqual(await page.locator(`${FIELD_FORM} .state input.name`).evaluateAll((all) => all.map((i) => i.value)), Object.keys(example.states ?? {}));
				// A card's keys are on Basics; any other shape's a line says, which goes to the Data tab (da-i9upr).
				if (example.verb !== 'add') {
					const said = (await page.locator(`${FIELD_FORM} .data-line .ask`).innerText()).trim();
					const names = (example.keys ?? []).map((k) => k.name);
					assert.ok(said && names.every((n) => said.includes(n)), `${said} names ${names}`);
					await page.locator(`${FIELD_FORM} button[data-to-tab="data"]`).click();
				}
				assert.deepEqual(
					await page
						.locator(`${PANEL} ul.data li.key`)
						.evaluateAll((all) => all.map((li) => [li.querySelector('.mode').textContent, li.querySelector('input.name').value, li.querySelector('.eq').getAttribute('aria-pressed')])),
					(example.keys ?? []).map((k) => [k.in_slug ? '-K' : '-k', k.name, String(k.kind !== 'required')])
				);
				// Nothing is written yet.
				await app.idle();
				assert.deepEqual(await app.allEntries(), []);
				assert.equal((await app.run(['setup'])).stdout.includes(example.field), false, 'the tap declared it');

				// Saved, it is ONE `setup F` with the whole form, which writes its TRACK row too
				// (da-2pbvd); the widget stays on the field it made.
				const before = (await app.echoes()).length;
				await page.locator(`${FIELD_FORM} button.commit`).click();
				const result = await saved;
				assert.equal(result.code, 0, result.stderr);
				const echoes = (await app.echoes()).slice(before);
				assert.equal(echoes.length, 1, `one command, not a stream: ${echoes.join(' | ')}`);
				assert.equal(declared(echoes[0]).line, `setup ${example.field} --from-json --track`);
				const shown = JSON.parse((await app.run(['setup', example.field, '--json'])).stdout);
				assert.equal(shown.color, example.color);
				assert.equal(shown.description, null);
				assert.equal(shown.symbol, example.glyph);
				assert.deepEqual(shown.state_colors, example.states ?? {});
				assert.equal(shown.state_lookup_field, example.lookup);
				assert.equal(shown.never_false, example.neverFalse);
				await page.locator(`${FIELD_FORM} h2`).waitFor();
				assert.equal((await page.locator(`${FIELD_FORM} h2`).innerText()).trim(), example.field);
				assert.deepEqual(await page.locator(`${FIELD_FORM} [role="tab"]`).evaluateAll((all) => all.map((t) => t.dataset.tab)), TABS);
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

				// Undo takes back the entry, and one more the field whole: its spec and TRACK row (da-2pbvd).
				for (const _ of [1, 2]) assert.equal((await app.run(['undo'])).code, 0);
				assert.match((await app.run(['setup', example.field, '--json'])).stdout, /No spec defined/);
				assert.deepEqual(await app.allEntries(), []);
			});
		});
	}

	test('the sheet of fields is every tracked field in its own colour and glyph, and a tap goes to that field', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'coffee', '--verb', 'occur', '--color', '#884400']);
			await app.run(['track', 'sleep', '--verb', 'start']);
			await app.run(['track', 'mood', '--verb', 'switch', '--add-state', 'good']);
			await app.run(['track', 'person', '--verb', 'add']);
			await app.redraw();
			await openField(page, 'coffee');
			await page.locator(`${FIELD_FORM} h2`).waitFor();
			await pick(page).click();
			assert.deepEqual([...(await chipTexts(page))].sort(), ['+ new field', '¢ person', '█ sleep', '▞ mood', '● coffee'].sort());
			assert.deepEqual(await marked(page), ['● coffee']);
			assert.equal(
				await page.locator(`${PICKER} [data-chip="coffee"] span`).evaluate((el) => getComputedStyle(el).color),
				'rgb(136, 68, 0)'
			);

			await page.locator(`${PICKER} [data-chip="sleep"]`).click();
			await until(() => heading(page), (t) => t === 'sleep');
			assert.equal(await page.locator(`${FIELD_FORM} .verb[aria-checked="true"]`).getAttribute('data-verb'), 'start');

			// A spec's edits are saved by going to another field, with nothing asked (da-hda4j).
			await page.locator(`${FIELD_FORM} input.description`).fill('nights');
			await pick(page).click();
			await page.locator(`${PICKER} [data-chip="coffee"]`).click();
			await until(() => heading(page), (t) => t === 'coffee');
			assert.equal(await page.locator('dialog.confirm').count(), 0, 'leaving a spec asked');
			assert.equal(JSON.parse((await app.run(['setup', 'sleep', '--json'])).stdout).description, 'nights', 'leaving did not save');

			// And «+ new field» in the sheet is the way back to an empty form and its examples,
			// less the ones there is a field for already: a tap would re-declare it.
			await pick(page).click();
			await page.locator(`${PICKER} button.new`).click();
			await page.locator(`${FIELD_FORM} input.field`).waitFor();
			assert.equal(await page.locator(`${FIELD_FORM} input.field`).inputValue(), '');
			assert.deepEqual(await examplesOf(page, 'occur'), ['headache', 'medication']);
			assert.deepEqual(await examplesOf(page, 'start'), ['screentime', 'work', 'shower', 'menstruation']);
			assert.deepEqual(await examplesOf(page, 'add'), ['book', 'place']);

			// A new field is not made by leaving it: the pick asks, and cancel keeps what was typed and the sheet.
			const asks = (answer) => page.locator('dialog.confirm').getByRole('button', { name: answer, exact: true }).click();
			await page.locator(`${FIELD_FORM} input.field`).fill('tea');
			await pick(page).click();
			await page.locator(`${PICKER} [data-chip="coffee"]`).click();
			await page.locator('dialog.confirm').waitFor();
			await asks('cancel');
			assert.equal(await page.locator(`${FIELD_FORM} input.field`).inputValue(), 'tea', 'cancel lost the edit');
			assert.equal(await page.locator(PICKER).isVisible(), true, 'cancel shut the sheet');
			await page.locator(`${PICKER} [data-chip="coffee"]`).click();
			await asks('leave');
			await until(() => heading(page), (t) => t === 'coffee');
			assert.match((await app.run(['setup', 'tea', '--json'])).stdout, /No spec defined/, 'leaving made the new field');
			assert.deepEqual(app.errors, []);
		});
	});

	test('typing narrows the sheet of fields once there are more than a glance takes in', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			const names = ['tea', 'team', 'run', 'walk', 'read', 'nap'];
			for (const name of names) await app.run(['track', name, '--verb', 'occur']);
			await app.redraw();
			await openField(page);
			await pick(page).click();
			await until(() => chips(page).count(), (n) => n === names.length);
			assert.equal(await page.locator(`${PICKER} .find`).count(), 0, 'six fields are a glance');

			await app.run(['track', 'swim', '--verb', 'occur']);
			await app.redraw();
			const find = page.locator(`${PICKER} .find`);
			// Whatever the case it is typed in.
			await find.fill('TEA');
			assert.deepEqual([...(await chipTexts(page))].sort(), ['● tea', '● team'].sort());
			await find.fill('');
			assert.equal(await chips(page).count(), names.length + 1);
		});
	});

	// Every tab is there before a save (da-uipyt), so what is set on one is saved with the field.
	test('a new field\'s tabs are saved with it, in one `setup F --track` that one Undo takes back (da-2pbvd)', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			const { saved } = await openField(page);
			await page.locator(`${FIELD_FORM} button[data-verb="switch"]`).click();
			await page.locator(`${FIELD_FORM} input.field`).fill('mood');
			await page.locator(`${FIELD_FORM} .states + button.add`).click();
			await page.locator(`${FIELD_FORM} .state input.name`).fill('good');
			await page.locator(`${FIELD_FORM} .state button.more`).click();
			await page.locator(`${FIELD_FORM} .state input.meaning`).fill('a good day');
			await page.locator(`${FIELD_FORM} input.description`).fill('how it feels');
			const before = (await app.echoes()).length;
			await page.locator(`${FIELD_FORM} button.commit`).click();
			assert.equal((await saved).code, 0);
			const echoes = (await app.echoes()).slice(before);
			assert.equal(echoes.length, 1, echoes.join(' | '));
			assert.match(echoes[0], /^setup mood --from-json .* --track$/);
			const shown = JSON.parse((await app.run(['setup', 'mood', '--json'])).stdout);
			assert.equal(shown.description, 'how it feels');
			assert.deepEqual(shown.state_descriptions, { good: 'a good day' });
			assert.deepEqual(shown.allowed_states, ['good']);
			assert.equal((await app.allEntries()).filter((e) => e.field === 'mood').length, 1, 'no TRACK row');
			assert.equal((await app.run(['undo'])).code, 0);
			assert.match((await app.run(['setup', 'mood', '--json'])).stdout, /No spec defined/, 'one Undo left the spec');
			assert.deepEqual(await app.allEntries(), [], 'one Undo left a row');
		});
	});

	// A save takes a moment, and a reader may go to another field in it: they stay where they went.
	test('a field picked while a save runs is where the reader stays, the place still open', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'tea', '--verb', 'occur']);
			await app.run(['track', 'nap', '--verb', 'start']);
			await app.redraw();
			await openField(page, 'nap');
			await page.locator(`${FIELD_FORM} h2`).waitFor();
			await page.locator(`${FIELD_FORM} input.description`).fill('short');
			await pick(page).click();
			await page.locator(`${PICKER} [data-chip="tea"]`).waitFor();
			const before = (await app.echoes()).length;
			// Save and the pick in one turn of the page, so the save is in flight when the pick lands.
			await page.evaluate((form) => {
				document.querySelector(`${form} button.commit`).click();
				document.querySelector('[data-chip="tea"]').click();
			}, FIELD_FORM);
			await until(async () => (await app.echoes()).length, (n) => n === before + 1);
			await app.idle();
			assert.equal((await page.locator(`${FIELD_FORM} h2`).allInnerTexts()).join().trim(), 'tea', 'the save took the place away, or the field back');
			assert.match((await app.echoes()).at(-1), /^setup nap /);
		});
	});

	// The key a line's leftover words go to is a spec's to change as well as
	// a new field's: a row's mode (da-zqnt3).
	test('the rest-of-line row of a saved field is read, renamed, made one string and removed, each in its one `setup F`', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'emotion', '--verb', 'occur', '-R', 'emotions']);
			const route = async () => {
				const { remainder, string_remainder } = JSON.parse((await app.run(['setup', 'emotion', '--json'])).stdout);
				return [remainder, string_remainder];
			};
			const row = page.locator(`${FIELD_FORM} ul.data li.key[data-mode^="R"], ${FIELD_FORM} ul.data li.key[data-mode="SR"]`);
			for (const [change, stored] of [
				[() => row.locator('input.name').fill('words'), ['words', undefined]],
				[() => row.locator('.mode').click(), ['words', true]],
				[() => row.locator('.remove').click(), [undefined, undefined]]
			]) {
				const { saved } = await openField(page, 'emotion');
				await page.locator(`${FIELD_FORM} [data-tab="data"]`).click();
				assert.equal(await row.locator('input.name').inputValue(), (await route())[0]);
				await change();
				await page.locator(`${FIELD_FORM} button.commit`).click();
				assert.equal((await saved).code, 0);
				assert.deepEqual(await route(), stored);
			}
		});
	});

	/** A widget of `kind` in the page's first pane, as the chooser puts one. */
	const put = async (page, kind) => {
		const tab = await page.evaluate(() => window.datum.layout.current().root.tabs[0].content.children[0].tabs[0].id);
		await page.evaluate(([tab, kind]) => window.datum.shell.widgetInto(tab, kind), [tab, kind]);
		return page.locator(`section.pane[data-title="${kind}"]`);
	};

	// da-i9upr, vividn, 2026-10-19: «move the placement to the basics tab and show what the data looks like
	// displayed … completely remove the states tab … for card, embed the data widget directly on the basics».
	// da-srrqd, vividn, 2026-10-22: the samples «should actually be inside the placement configuration tool».
	test('Basics draws each shape as it is logged in its lane on the band; a switch\'s states start at off; a card\'s keys are there', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			const { saved } = await openField(page);
			const at = (sel) => page.locator(`${FIELD_FORM} [data-panel="basics"] ${sel}`);
			const own = at('.placement .band g.field[data-field="thing"]');
			/** Wait for the band to draw the field's sample as `ok` says: its blocks' states, and how many dots. */
			const drawn = async (what, ok) => {
				for (const end = Date.now() + 5000; ; await page.waitForTimeout(50)) {
					const seen = {
						blocks: await own.locator('rect.block').evaluateAll((all) => all.map((b) => b.dataset.state)),
						dots: await own.locator('.point-fg').count()
					};
					if (ok(seen)) return;
					if (Date.now() > end) assert.fail(`never drawn: ${what}, but ${JSON.stringify(seen)}`);
				}
			};
			const off = at('.states li.off button.off-state');
			const struck = async () => [await off.getAttribute('aria-pressed'), await off.isDisabled()];
			await page.locator(`${FIELD_FORM} input.field`).fill('thing');
			// No shape yet, nothing to draw.
			assert.equal(await at('.placement').count(), 0);
			assert.deepEqual(await page.locator(`${FIELD_FORM} [role="tab"]`).evaluateAll((all) => all.map((t) => t.dataset.tab)), TABS);

			await page.locator(`${FIELD_FORM} button[data-verb="occur"]`).click();
			await drawn('the dots of an occurrence', ({ blocks, dots }) => dots === 8 && blocks.length === 0);
			assert.equal(await at('.placement .box').count(), 0, 'no box at rest (da-r6wwe)');

			await page.locator(`${FIELD_FORM} button[data-verb="start"]`).click();
			await drawn('on with off between', ({ blocks, dots }) => dots === 0 && blocks.length === 4 && blocks.every((s) => s === 'on'));

			// A switch starts with off alone, which cannot go until there is another state.
			await page.locator(`${FIELD_FORM} button[data-verb="switch"]`).click();
			assert.deepEqual(await struck(), ['false', true]);
			await at('.states + button.add').click();
			await at('.state input.name').fill('calm');
			await drawn('calm with off between', ({ blocks }) => blocks.length === 4 && blocks.every((s) => s === 'calm'));
			assert.equal(await off.isDisabled(), false);
			await off.click();
			await drawn('calm with no off: one stretch', ({ blocks }) => blocks.length === 1 && blocks[0] === 'calm');
			// The line names the keys and goes to the Data tab.
			await page.locator(`${FIELD_FORM} button[data-to-tab="data"]`).click();
			assert.equal(await page.locator(PANEL).getAttribute('data-panel'), 'data');
			await page.locator(`${FIELD_FORM} [data-tab="basics"]`).click();

			// A card has no time: no placement, so no sample, and the Data tab's rows, typed, on Basics.
			await page.locator(`${FIELD_FORM} button[data-verb="add"]`).click();
			for (const gone of ['.placement', '.data-line', '.states']) assert.equal(await at(gone).count(), 0, gone);
			await at('ul.data + button.add').click();
			assert.equal(await at('ul.data li.key select.type').count(), 1, 'not the Data tab\'s rows');

			// Saved as a switch, off disabled is `never_false`.
			await page.locator(`${FIELD_FORM} button[data-verb="switch"]`).click();
			await page.locator(`${FIELD_FORM} button.commit`).click();
			assert.equal((await saved).code, 0);
			const shown = JSON.parse((await app.run(['setup', 'thing', '--json'])).stdout);
			assert.deepEqual([shown.allowed_states, shown.never_false], [['calm'], true]);
			assert.deepEqual(app.errors, []);
		});
	});

	// da-ooflv, vividn, 2026-10-28: «let's bring the data and aliases tabs back on the setup widget, but no swipe action
	// between them». A tap on a tab or an arrow key in its bar changes it; a swipe never does (fieldswipe.test.mjs).
	test('basics, data and aliases are tabs, each shown by a tap or the arrow keys; a card\'s data is on basics and on data', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			const { saved } = await openField(page);
			await page.locator(`${FIELD_FORM} input.field`).fill('thing');
			await page.locator(`${FIELD_FORM} button[data-verb="switch"]`).click();
			const tab = (name) => page.locator(`${FIELD_FORM} [role="tab"][data-tab="${name}"]`);
			const shown = () => page.locator(PANEL).getAttribute('data-panel');
			/** What each tab holds, which no other tab shows. */
			const inside = { basics: 'input.description', data: 'ul.data + button.add', aliases: 'input.autoalias' };
			assert.equal(await shown(), 'basics');
			for (const name of ['data', 'aliases', 'basics']) {
				await tab(name).click();
				assert.equal(await shown(), name);
				assert.equal(await tab(name).getAttribute('aria-selected'), 'true');
				for (const [other, sel] of Object.entries(inside)) {
					assert.equal(await page.locator(`${FIELD_FORM} ${sel}`).count(), other === name ? 1 : 0, `on ${name}, ${other}'s ${sel}`);
				}
			}
			// The arrows walk the bar, the focus with them, and stop at either end.
			await tab('basics').focus();
			for (const [key, name] of [['ArrowRight', 'data'], ['ArrowRight', 'aliases'], ['ArrowRight', 'aliases'], ['ArrowLeft', 'data']]) {
				await page.keyboard.press(key);
				assert.equal(await shown(), name, key);
				assert.equal(await page.evaluate(() => document.activeElement?.dataset.tab), name, `${key}: the focus`);
			}
			// A card: its rows on basics, where another shape's line to data stands, and on data.
			await page.locator(`${FIELD_FORM} button[data-verb="add"]`).click();
			for (const name of ['basics', 'data']) {
				await tab(name).click();
				assert.equal(await page.locator(`${PANEL} ul.data + button.add`).count(), 1, `a card's data is not on ${name}`);
			}
			await leaveField(page);
			assert.equal(await saved, null);
			assert.deepEqual(app.errors, []);
		});
	});

	// da-puzij, vividn, 2026-10-22: «the individual states for switch data don't need individual symbols, just color is
	// sufficient. also, just put a ... button to the left of x button … remove the up and down arrows, and put reorg handles
	// like there are on the data tab … have the "off" state at the top look the same as the rest of the states».
	for (const [name, size] of [['a desktop', DESKTOP], ['a phone', PHONE]])
		test(`on ${name}, a switch's states are handle · color · name · … · ✕, off's ✕ strikes it, and a save keeps all of it`, async () => {
			await withApp(async (app) => {
				const { page } = app;
				await app.run(['track', 'mood', '--verb', 'switch', '--states', 'a,b,c', '--state-description', 'a=first']);
				const { saved } = await openField(page, 'mood');
				const rows = page.locator(`${FIELD_FORM} .states li.state`);
				const names = () => rows.locator('input.name').evaluateAll((all) => all.map((i) => i.value));
				const off = page.locator(`${FIELD_FORM} .states li.off`);
				const strike = off.locator('button.off-state');
				/** The parts of a row, left to right, each its class and the middle of its box. */
				const parts = (row) =>
					row.evaluate((li) =>
						[...li.children].filter((c) => c.getBoundingClientRect().width).map((c) => {
							const r = c.getBoundingClientRect();
							return { part: c.className.split(' ')[0], x: r.left, y: r.top + r.height / 2 };
						})
					);

				// One line each, in order: no symbol in the row, no description under it.
				const row = await parts(rows.first());
				assert.deepEqual(row.map((p) => p.part), ['handle', 'swatch', 'name', 'remove', 'remove']);
				assert.ok(row.every((p) => p.x >= row[0].x && Math.abs(p.y - row[0].y) <= 2), JSON.stringify(row));
				assert.deepEqual(await rows.first().locator(':scope > button:not(.handle)').evaluateAll((all) => all.map((b) => b.textContent.trim())), ['', '…', '✕']);
				for (const gone of ['.tile', 'input.meaning']) assert.equal(await rows.locator(gone).count(), 0, gone);
				// Off: the same columns, its swatch and ✕ under theirs, «off» not to be edited, no handle and no ….
				const offRow = await parts(off);
				assert.deepEqual(offRow.map((p) => p.part), ['swatch', 'name', 'remove']);
				assert.deepEqual([offRow[0].x, offRow[2].x], [row[1].x, row[4].x]);
				assert.deepEqual([await off.locator('input.name').inputValue(), await off.locator('input.name').isDisabled()], ['off', true]);
				assert.equal(await off.locator('.swatch').evaluate((s) => getComputedStyle(s).backgroundColor), 'rgba(0, 0, 0, 0)');

				// … opens the description and, under «advanced», the symbol; pressed again, it shuts.
				const more = page.locator(`${FIELD_FORM} [aria-label="the options of a"]`);
				await more.click();
				assert.equal(await more.getAttribute('aria-expanded'), 'true');
				assert.equal(await rows.first().locator('input.meaning').inputValue(), 'first');
				assert.equal((await rows.first().locator('.advanced').innerText()).trim().toLowerCase(), 'advanced');
				assert.equal(await rows.first().locator('.tile[aria-label^="the symbol of a:"]').count(), 1);
				await rows.first().locator('input.meaning').fill('the first');
				await more.click();
				assert.equal(await rows.locator('input.meaning').count(), 0);
				await rows.nth(1).locator('.swatch[aria-expanded]').click();
				const color = await page.locator(`${FIELD_FORM} .state .set .swatch`).nth(3).getAttribute('data-color');
				await page.locator(`${FIELD_FORM} .state .set .swatch`).nth(3).click();

				// A drag on c's handle to a's row puts c first; the arrow keys move it back down one, keeping the focus.
				const handle = (s) => page.locator(`${FIELD_FORM} [aria-label="move ${s}: drag, or the arrow keys"]`);
				// In view first: a phone's form is shorter with the bar's output open
				// (da-mwp4m), and a drag off the visible box lands on nothing.
				await handle('c').scrollIntoViewIfNeeded();
				const [from, to] = [await handle('c').boundingBox(), await rows.first().boundingBox()];
				await page.mouse.move(from.x + from.width / 2, from.y + from.height / 2);
				await page.mouse.down();
				await page.mouse.move(from.x + from.width / 2, to.y + 2, { steps: 5 });
				await page.mouse.up();
				assert.deepEqual(await names(), ['c', 'a', 'b']);
				await handle('c').focus();
				await page.keyboard.press('ArrowDown');
				assert.deepEqual(await names(), ['a', 'c', 'b']);
				assert.equal(await handle('c').evaluate((el) => el === document.activeElement), true, 'the moved handle lost the focus');

				// Off's ✕ strikes the whole line, and again brings it back.
				const across = () => off.evaluate((li) => getComputedStyle(li, '::after').borderTopStyle);
				assert.deepEqual([await strike.getAttribute('aria-pressed'), await across()], ['false', 'none']);
				await strike.click();
				assert.deepEqual([await strike.getAttribute('aria-pressed'), await across()], ['true', 'solid']);
				await strike.click();
				assert.equal(await strike.getAttribute('aria-pressed'), 'false');
				await strike.click();

				await page.locator(`${FIELD_FORM} button.commit`).click();
				assert.equal((await saved).code, 0);
				const shown = JSON.parse((await app.run(['setup', 'mood', '--json'])).stdout);
				assert.deepEqual(shown.allowed_states, ['a', 'c', 'b']);
				assert.deepEqual(shown.state_colors, { b: color });
				assert.deepEqual(shown.state_descriptions, { a: 'the first' });
				assert.equal(shown.never_false, true);

				// It cannot strike off while `false` is a state, nor while there is no other.
				const { saved: left } = await openField(page, 'mood');
				await strike.click();
				await page.locator(`${FIELD_FORM} .states + button.add`).click();
				await rows.last().locator('input.name').fill('false');
				assert.equal(await strike.isDisabled(), true);
				assert.match(await strike.getAttribute('title'), /\bfalse\b/, 'the title names the state in the way');
				for (const s of ['false', 'a', 'c', 'b']) await page.locator(`${FIELD_FORM} [aria-label="remove ${s}"]`).click();
				assert.equal(await strike.isDisabled(), true);
				assert.ok(await strike.getAttribute('title'), 'a disabled ✕ says why');
				await leaveField(page);
				assert.equal(await left, null);
				assert.deepEqual(app.errors, []);
			}, size);
		});

	// da-ua9gv: a state dragged past a taller one (its … open) swapped with it on every move, the taller row's bottom
	// edge below the pointer again after each swap. It passes a neighbor once the pointer reaches the neighbor's middle.
	for (const [name, size, finger] of [['a mouse on a desktop', DESKTOP, false], ['a finger on a phone', PHONE, true]])
		test(`with ${name}, a state dragged past a taller one moves once and lands under the pointer`, async () => {
			await withApp(async (app) => {
				const { page } = app;
				await app.run(['track', 'mood', '--verb', 'switch', '--states', 'a,b,c']);
				const { saved } = await openField(page, 'mood');
				const rows = page.locator(`${FIELD_FORM} .states li.state`);
				const names = () => rows.locator('input.name').evaluateAll((all) => all.map((i) => i.value).join(''));
				await page.locator(`${FIELD_FORM} [aria-label="the options of b"]`).click();
				const grip = await page.locator(`${FIELD_FORM} [aria-label="move a: drag, or the arrow keys"]`).boundingBox();
				const [a, b] = [await rows.nth(0).boundingBox(), await rows.nth(1).boundingBox()];
				assert.ok(b.height > 2 * a.height, `b's … makes it the taller: ${a.height} and ${b.height}`);

				// From a's handle down to near b's bottom, the order read after every move.
				const [x, y] = [grip.x + grip.width / 2, grip.y + grip.height / 2];
				const end = b.y + b.height - 4;
				const cdp = finger && (await page.context().newCDPSession(page));
				const touch = (type, at) => cdp.send('Input.dispatchTouchEvent', { type, touchPoints: at === undefined ? [] : [{ x, y: at }] });
				const seen = [await names()];
				if (finger) await touch('touchStart', y);
				else {
					await page.mouse.move(x, y);
					await page.mouse.down();
				}
				for (let i = 1; i <= 30; i++) {
					const at = y + ((end - y) * i) / 30;
					await (finger ? touch('touchMove', at) : page.mouse.move(x, at));
					seen.push(await names());
				}
				await (finger ? touch('touchEnd') : page.mouse.up());
				if (cdp) await cdp.detach();

				assert.deepEqual(seen.filter((s, i) => s !== seen[i - 1]), ['abc', 'bac'], seen.join(' '));
				const landed = await rows.nth(1).boundingBox();
				assert.ok(landed.y <= end && end <= landed.y + landed.height, `a's row ${JSON.stringify(landed)} is under ${end}`);
				await leaveField(page);
				assert.equal(await saved, null);
				assert.deepEqual(app.errors, []);
			}, size);
		});

	// vremya, for vividn, 2026-10-19: «put them back at the very bottom of the basics tab» (da-i9upr).
	test('a field\'s Basics ends with tracked or ignored, whose button runs `ignore F` or `track F` and stays', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'tea', '--verb', 'occur']);
			await app.redraw();
			await openField(page, 'tea');
			const tracking = page.locator(`${FIELD_FORM} [data-tracking]`);
			// Its state, and the button naming the command it runs.
			const said = async () => [await tracking.getAttribute('data-tracking'), (await tracking.locator('button').innerText()).trim()];
			assert.deepEqual(await said(), ['tracked', 'ignore']);
			assert.equal(
				await page.locator(`${FIELD_FORM} [data-panel="basics"]`).evaluate((el) => el.lastElementChild?.dataset.tracking),
				'tracked',
				'not at the very bottom'
			);
			await page.locator(`${FIELD_FORM} input.description`).fill('kept');
			for (const [verb, now] of [
				['ignore', ['ignored', 'track']],
				['track', ['tracked', 'ignore']]
			]) {
				// The app's clock is held, and two rows of one field at one instant collide.
				await app.clock.forward(1000);
				const before = (await app.echoes()).length;
				await tracking.locator('button').click();
				await until(said, (seen) => seen[0] === now[0]);
				assert.deepEqual(await said(), now);
				assert.deepEqual((await app.echoes()).slice(before), [`${verb} tea`]);
			}
			// The form stayed, with what was typed in it.
			assert.equal(await heading(page), 'tea');
			assert.equal(await page.locator(`${FIELD_FORM} input.description`).inputValue(), 'kept');
			assert.deepEqual(app.errors, []);
		});
	});

	test('`track` in the commands widget opens it on a new field, and writes no line', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			const track = (await put(page, 'command')).locator('[data-command="track"]');
			assert.equal(await track.getAttribute('data-act'), 'open');
			await track.click();
			await page.locator(`${FIELD_FORM} input.field`).waitFor();
			assert.equal(await page.locator('.prompt-bar input').first().inputValue(), '', 'the tap wrote a line as well');
			assert.equal(await page.locator(`${FIELD_FORM} input.field`).inputValue(), '');
		});
	});

	// In a pane it is where someone chose to work: a field pill minted
	// beside it — a hold on a field in the fields widget — brings that field.
	test('in a pane, a field pill minted beside it brings that field, and a save stays', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await app.submit('track tea --verb occur');
			const pane = await put(page, 'setup');
			await pane.locator(`${FIELD_FORM} input.field`).waitFor();

			await page.locator('.bar button[data-field="tea"]').click({ modifiers: ['Shift'] });
			await pane.locator(`${FIELD_FORM} h2`).waitFor();
			assert.equal((await pane.locator(`${FIELD_FORM} h2`).innerText()).trim(), 'tea');
			assert.equal(await page.locator('dialog[open]').count(), 0, 'a widget in view opened a place as well');

			// And a new field goes to the pane, not to a place of its own.
			await page.evaluate(() => void window.datum.field());
			await pane.locator(`${FIELD_FORM} input.field`).fill('nap');
			await pane.locator(`${FIELD_FORM} button[data-verb="start"]`).click();
			await pane.locator(`${FIELD_FORM} button.commit`).click();
			await until(async () => (await pane.locator(`${FIELD_FORM} h2`).allInnerTexts()).join().trim(), (t) => t === 'nap');
			assert.equal(JSON.parse((await app.run(['setup', 'nap', '--json'])).stdout).verb, 'start');
			assert.equal(await page.locator('dialog[open]').count(), 0);
			assert.deepEqual(app.errors, []);
		});
	});

	// vividn, 2026-10-19: «there should probably be an explicit rename button though» (da-qimwi).
	test('«rename» on a field asks the new name, shows the dry run, runs `rename`, and the panes show the new name', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'mood', '--verb', 'switch', '--add-state', 'good']);
			await app.run(['track', 'tea', '--verb', 'occur']);
			// Not at the TRACK row's instant, whose slug it would share.
			await app.clock.forward(60_000);
			await app.run(['switch', 'mood', 'good']);
			await app.clock.forward(60_000);
			await app.run(['switch', 'mood', 'bad']);
			await app.redraw();
			const tail = page.locator('section.pane[data-title="tail"] tbody');
			await until(() => tail.innerText(), (t) => t.includes('mood'));
			// No rename on a new field: nothing to rename yet.
			await openField(page);
			await page.locator(`${FIELD_FORM} input.field`).waitFor();
			assert.equal(await page.locator(`${FIELD_FORM} button.rename`).count(), 0);
			await leaveField(page);
			await openField(page, 'mood');
			await page.locator(`${FIELD_FORM} h2`).waitFor();
			const button = page.locator(`${FIELD_FORM} button.rename`);
			assert.equal((await button.innerText()).trim(), 'rename');
			const [box, name] = [await button.boundingBox(), await page.locator(`${FIELD_FORM} h2`).boundingBox()];
			assert.ok(Math.abs(box.y + box.height / 2 - (name.y + name.height / 2)) < 4, 'rename is not beside the name');

			// A name that is taken is refused by the dry run, and asked again saying why.
			const ask = page.locator('dialog.ask');
			await button.click();
			assert.equal(await ask.locator('input').inputValue(), 'mood');
			await ask.locator('input').fill('tea');
			await ask.locator('button[value]').click();
			await until(() => ask.locator('span').innerText(), (t) => t.includes('tea') && t.endsWith('rename mood to:') && t !== 'rename mood to:');
			assert.equal(await ask.locator('input').inputValue(), 'tea');
			await ask.locator('input').fill('feeling');
			await ask.locator('button[value]').click();
			// The dry run's counts — the two switches and the TRACK row — and nothing moved yet.
			const confirm = page.locator('dialog.confirm');
			await confirm.waitFor();
			const counted = await confirm.locator('p').innerText();
			for (const fact of [/\bmood\b/, /\bfeeling\b/, /\b3\b/]) assert.match(counted, fact);
			assert.equal((await app.entries()).filter((e) => e.field === 'mood').length, 2);
			await confirm.locator('button[value="rename"]').click();

			// The form is on the new name, and the panes show it with its entries.
			await until(() => heading(page), (t) => t === 'feeling');
			assert.deepEqual((await app.entries()).map((e) => e.field).sort(), ['feeling', 'feeling']);
			await until(() => tail.innerText(), (t) => t.includes('feeling') && !t.includes('mood'));
			await page.locator('.bar button[data-field="feeling"]').waitFor();
			assert.equal(await page.locator('.bar button[data-field="mood"]').count(), 0);
			// One undo takes it all back.
			assert.equal((await app.run(['undo'])).code, 0);
			assert.deepEqual((await app.entries()).map((e) => e.field), ['mood', 'mood']);
			assert.deepEqual(app.errors, []);
		});
	});

	// jasper on #944: at 390 px the placeholder was cut beside «fields» (da-qimwi).
	test('on a phone the name line keeps its placeholder whole, and a field\'s rename stands beside «fields»', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.run(['track', 'tea', '--verb', 'occur']);
			await app.redraw();
			await openField(page);
			const input = page.locator(`${FIELD_FORM} input.field`);
			await input.waitFor();
			await pick(page).waitFor();
			const [needs, has] = await input.evaluate((el) => {
				const cs = getComputedStyle(el);
				const ctx = document.createElement('canvas').getContext('2d');
				ctx.font = cs.font;
				return [ctx.measureText(el.placeholder).width, el.clientWidth - parseFloat(cs.paddingLeft) - parseFloat(cs.paddingRight)];
			});
			assert.ok(needs <= has, `the placeholder needs ${needs}px and has ${has}px`);
			await page.screenshot({ path: process.env.DATUM_SHOT ? `${process.env.DATUM_SHOT}/new.png` : undefined });
			await leaveField(page);

			await openField(page, 'tea');
			await page.locator(`${FIELD_FORM} h2`).waitFor();
			const [fields, rename] = [await pick(page).boundingBox(), await page.locator(`${FIELD_FORM} button.rename`).boundingBox()];
			assert.ok(Math.abs(fields.y - rename.y) < 2 && rename.x > fields.x, 'rename does not stand next to «fields»');
			await page.screenshot({ path: process.env.DATUM_SHOT ? `${process.env.DATUM_SHOT}/field.png` : undefined });
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});
});

// A test that throws with a place open fails with what it threw, not with
// the save's «Target page closed» as its page is closed under it (da-w3y9q).
test('a test that throws with the place open fails as itself', async () => {
	await assert.rejects(
		withApp(async ({ page }) => {
			await openField(page, 'coffee');
			await page.locator(FIELD_FORM).waitFor();
			throw new Error('the real assertion');
		}),
		/the real assertion/
	);
});

// A spec saves when it is left, and its bottom is Revert · Save · Redo (da-hda4j), vividn, 2026-10-26: «if you
// navigate away from a spec, maybe we should autosave it? But we change the bottom to have [Revert] and [Save]
// buttons, clicking revert goes back to the unsaved settings, or if already at a save checkpoint undoes the spec to
// the previous save (maybe we also then need a [redo] button for undoing that revert??)», and how far: «whole saved
// history».
const described = async (app, field) => JSON.parse((await app.run(['setup', field, '--json'])).stdout).description;
const button = (page, name) => page.locator(`${FIELD_FORM} .buttons button.${name}`);
const description = (page) => page.locator(`${FIELD_FORM} input.description`);

/** The widget opened, then `field` picked in its own list of fields: reached by the person themselves (da-wz84y). */
async function reach(page, field) {
	await openField(page);
	await pick(page).click();
	await page.locator(`${PICKER} [data-chip="${field}"]`).click();
	await until(() => heading(page), (t) => t === field);
	await description(page).waitFor();
}

describe('a spec saves when it is left, and Revert and Redo walk its saved history', () => {
	const desktop = sharedApp(DESKTOP);
	/** Which of Revert, Save and Redo can be pressed. */
	const enabled = async (page) => Object.fromEntries(await Promise.all(['revert', 'commit', 'redo'].map(async (b) => [b, await button(page, b).isEnabled()])));

	test('going to another field saves the edits, and so does closing the place', async () => {
		await desktop.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'tea', '--verb', 'occur']);
			await app.run(['track', 'nap', '--verb', 'start']);
			await app.redraw();
			await openField(page, 'tea');
			await page.locator(`${FIELD_FORM} h2`).waitFor();
			await description(page).fill('green');
			await pick(page).click();
			await page.locator(`${PICKER} [data-chip="nap"]`).click();
			await until(() => heading(page), (t) => t === 'nap');
			await until(() => described(app, 'tea'), (d) => d === 'green');
			// The heading follows at once; the form is nap's once it is read.
			await until(() => description(page).inputValue(), (v) => v === '');

			await description(page).fill('short');
			await page.evaluate(() => window.datum.shell.leavePlace());
			await page.locator('dialog.sheet[data-place="setup"]').waitFor({ state: 'detached' });
			await until(() => described(app, 'nap'), (d) => d === 'short');
			assert.deepEqual(app.errors, []);
		});
	});

	test('Revert drops unsaved edits; at a save point it steps back through every version, the CLI\'s too, and Redo forward', async () => {
		await desktop.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'tea', '--verb', 'occur']);
			const spec = JSON.parse((await app.run(['setup', 'tea', '--json'])).stdout);
			for (const said of ['one', 'two']) await app.run(['setup', 'tea', '--from-json', JSON.stringify({ ...spec, description: said })]);
			// Reached by the person's own pick, so its Save stays (da-wz84y).
			await app.redraw();
			await reach(page, 'tea');
			assert.equal(await description(page).inputValue(), 'two');
			assert.deepEqual(await enabled(page), { revert: true, commit: false, redo: false }, 'at the newest save');

			// Unsaved edits: Revert drops them and writes nothing.
			const before = (await app.echoes()).length;
			await description(page).fill('three');
			assert.deepEqual(await enabled(page), { revert: true, commit: true, redo: false }, 'with edits');
			await button(page, 'revert').click();
			await until(() => description(page).inputValue(), (v) => v === 'two');
			assert.equal((await app.echoes()).length, before, 'dropping the edits ran something');
			assert.equal(await described(app, 'tea'), 'two');

			// At a save point: back a version a press, to the first, as `undo SPEC --to-version N`.
			const { hid } = JSON.parse((await app.run(['history', 'spec:tea', '--json'])).stdout)[0];
			for (const [shown, version] of [['one', 2], ['', 1]]) {
				await button(page, 'revert').click();
				await until(() => description(page).inputValue(), (v) => v === shown);
				assert.equal((await app.echoes()).at(-1), `undo ${hid} --to-version ${version}`);
				assert.equal(await described(app, 'tea'), shown || null);
			}
			assert.deepEqual(await enabled(page), { revert: false, commit: false, redo: true }, 'at the first version');

			// Redo walks forward again, up to where the walk began.
			for (const [shown, version] of [['one', 2], ['two', 3]]) {
				await button(page, 'redo').click();
				await until(() => description(page).inputValue(), (v) => v === shown);
				assert.equal((await app.echoes()).at(-1), `undo ${hid} --to-version ${version}`);
				assert.equal(await described(app, 'tea'), shown);
			}
			assert.deepEqual(await enabled(page), { revert: true, commit: false, redo: false }, 'back where the walk began');

			// A save is a new save point: the walk ahead of it is gone.
			await button(page, 'revert').click();
			await until(() => description(page).inputValue(), (v) => v === 'one');
			await description(page).fill('four');
			await button(page, 'commit').click();
			await until(() => described(app, 'tea'), (d) => d === 'four');
			await until(() => enabled(page), (e) => e.revert && !e.commit && !e.redo);
			assert.equal(await description(page).inputValue(), 'four');
			assert.deepEqual(app.errors, []);
		});
	});
});

// The bottom row by how the person came (da-wz84y), vividn, 2026-10-26: «maybe it should depend on how you got there?
// Like if you navigate yourself to the sync widget, stay on the field, probably flash some sort of field saved
// underneath the save button or something», and «I guess there could be a "cancel" button, that always goes back to
// where you came from. And then revert always just reverts to the last state without going back. If you navigate
// there on your own, maybe no cancel button. I guess to follow our convention if hitting save or cancel takes you
// somewhere else it should have a little chevron on it, right?»
describe('the bottom row by how the person came', () => {
	const desktop = sharedApp(DESKTOP);
	/** The row as drawn: each button's words, and whether it wears the chevron. */
	const row = (page) =>
		page.locator(`${FIELD_FORM} .buttons button`).evaluateAll((all) => all.map((b) => [b.textContent.trim(), getComputedStyle(b, '::after').content.includes('›')]));
	const place = (page) => page.locator('dialog.sheet[data-place="setup"]');
	const saved = (page) => page.locator(`${FIELD_FORM} .buttons .saved`);

	test('reached by its own list: Revert · Save · Redo and no Cancel; Save stays and says «saved», which fades; Revert stays', async () => {
		await desktop.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'tea', '--verb', 'occur']);
			await app.run(['track', 'nap', '--verb', 'start']);
			await app.redraw();
			await reach(page, 'tea');
			assert.deepEqual(await row(page), [['Revert', false], ['Save', false], ['Redo', false]]);

			await description(page).fill('green');
			const rowBox = () => page.locator(`${FIELD_FORM} .buttons`).boundingBox();
			const still = await rowBox();
			await button(page, 'commit').click();
			await saved(page).waitFor();
			assert.equal((await saved(page).innerText()).trim(), 'saved');
			// Its line is kept under the row always: the row does not move when it shows.
			assert.deepEqual(await rowBox(), still, 'the row moved for «saved»');
			await until(() => described(app, 'tea'), (d) => d === 'green');
			assert.equal(await place(page).isVisible(), true, 'Save left the place');
			assert.equal(await heading(page), 'tea');
			await saved(page).waitFor({ state: 'detached' });

			// At the save point Revert steps back a version, and stays.
			await button(page, 'revert').click();
			await until(() => description(page).inputValue(), (v) => v === '');
			assert.equal(await place(page).isVisible(), true, 'Revert left the place');

			// Said of tea's save, it is not said of the next field picked.
			await description(page).fill('black');
			await button(page, 'commit').click();
			await saved(page).waitFor();
			await pick(page).click();
			await page.locator(`${PICKER} [data-chip="nap"]`).click();
			await until(() => heading(page), (t) => t === 'nap');
			assert.equal(await saved(page).count(), 0, '«saved» followed to nap');
			assert.deepEqual(app.errors, []);
		});
	});

	test('brought from another pane: Cancel › · Revert · Save › · Redo; Revert stays, Cancel drops the edits and goes back, Save saves and goes back', async () => {
		await desktop.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'tea', '--verb', 'occur']);
			await app.redraw();
			await openField(page, 'tea', undefined, true);
			await description(page).waitFor();
			assert.deepEqual(await row(page), [['Cancel', true], ['Revert', false], ['Save', true], ['Redo', false]]);

			await description(page).fill('green');
			await button(page, 'revert').click();
			await until(() => description(page).inputValue(), (v) => v === '');
			assert.equal(await place(page).isVisible(), true, 'Revert left the place');

			const before = (await app.echoes()).length;
			await description(page).fill('green');
			await button(page, 'cancel').click();
			await place(page).waitFor({ state: 'detached' });
			await app.idle();
			assert.equal((await app.echoes()).length, before, 'Cancel ran something');
			assert.equal(await described(app, 'tea'), null);

			await openField(page, 'tea', undefined, true);
			await description(page).fill('green');
			await button(page, 'commit').click();
			await place(page).waitFor({ state: 'detached' });
			assert.equal(await described(app, 'tea'), 'green');

			// A pick in its own list is reaching it: the row is the person's own again.
			await openField(page, '', undefined, true);
			await pick(page).click();
			await page.locator(`${PICKER} [data-chip="tea"]`).click();
			await until(() => row(page), (r) => r.length === 3);
			assert.deepEqual(await row(page), [['Revert', false], ['Save', false], ['Redo', false]]);
			assert.deepEqual(app.errors, []);
		});
	});

	test('a new field keeps Cancel · Save; brought, its Cancel wears the chevron and goes back, and its Save stays', async () => {
		await desktop.solo(async (app) => {
			const { page } = app;
			await openField(page, undefined, undefined, true);
			await page.locator(`${FIELD_FORM} input.field`).waitFor();
			assert.deepEqual(await row(page), [['Cancel', true], ['Save', false]]);
			await button(page, 'cancel').click();
			await place(page).waitFor({ state: 'detached' });
			assert.deepEqual(app.errors, []);
		});
	});

	test('on a phone, brought from the main tab, Save and Cancel go back to it', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const front = () => page.locator('.root-bar [role="tab"][aria-selected="true"]').evaluate((b) => b.textContent.replace('•', '').trim());
			await app.run(['track', 'tea', '--verb', 'occur']);
			await app.redraw();
			const home = await front();
			for (const leave of ['cancel', 'commit']) {
				await openField(page, 'tea', undefined, true);
				await description(page).waitFor();
				await until(front, (t) => t === 'setup');
				assert.deepEqual(await row(page), [['Cancel', true], ['Revert', false], ['Save', true], ['Redo', false]]);
				await description(page).fill(leave);
				await button(page, leave).click();
				await until(front, (t) => t === home);
			}
			assert.equal(await described(app, 'tea'), 'commit');
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});
});
