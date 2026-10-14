// The field widget (da-fu29p): the Track dialog and the spec widget as one.
// vividn, 2026-10-08, after a newcomer found the two confusing: «let's fully
// combine track/spec widgets into just 1, and make the appearance better
// when no fields exist … make it so that you can switch the field you're
// editing inside of it more easy and intuitive … It should also be clearer
// what "field" actually is and means.»
//
// So what is held here is what a newcomer meets: the sentence, the shapes
// first (da-uipyt), each with a ▾ of examples that fill the form and not
// the database, tracked or ignored under the name, a save that leaves a
// field the bar can log, and the chips that go from one field to the next.
// The form's own tabs are `track.test.mjs`'s, the place's coming and going
// `specplace.test.mjs`'s.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, withApp } from './support/app.mjs';
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

const TABS = ['basics', 'states', 'data', 'aliases', 'placement'];
/** The shape's ▾, which opens its examples. */
const more = (page, verb) => page.locator(`${FIELD_FORM} [data-examples="${verb}"]`);
const offered = (page) => page.locator(`${FIELD_FORM} [data-example]`).evaluateAll((all) => all.map((b) => b.dataset.example));
/** The name box has the focus, its whole text selected. */
const picked = (page) =>
	page.locator(`${FIELD_FORM} input.field`).evaluate((el) => el === document.activeElement && el.selectionStart === 0 && el.selectionEnd === el.value.length);

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
			await page.locator('.bar button[data-track]').click();
			await page.locator(FIELD_FORM).waitFor();
			assert.equal(
				(await page.locator(`${WIDGET} p.what`).innerText()).trim(),
				'A field is one thing you track: mood, coffee, sleep. Each time you log it, that\'s an entry.'
			);
			assert.deepEqual(await chipTexts(page), ['+ new field']);
			assert.deepEqual(await marked(page), ['+ new field']);
			assert.equal(await page.locator(`${FIELD_FORM} input.field`).inputValue(), '');
			// The shapes come first, above the name, each named and lined as the welcome has it.
			const said = await page.locator(`${FIELD_FORM} .verb`).evaluateAll((all) =>
				all.map((b) => [...b.children].map((c) => c.textContent.trim()))
			);
			assert.deepEqual(said, [
				['●', 'occurrence', 'a single point in time (no state)'],
				['█', 'on/off', 'on for a while, then off (state: true/false)'],
				['▞', 'switch', 'one state until the next (custom states)'],
				['¢', 'card', 'keys and values, with no time']
			]);
			const top = async (sel) => (await page.locator(`${FIELD_FORM} ${sel}`).first().boundingBox()).y;
			assert.ok((await top('.verb')) < (await top('input.field')), 'the name comes before the shapes');
			// None chosen, and every tab there before a save, none of them required.
			assert.equal(await page.locator(`${FIELD_FORM} .verb[aria-checked="true"]`).count(), 0);
			assert.deepEqual(await page.locator(`${FIELD_FORM} [role="tab"]`).evaluateAll((all) => all.map((t) => t.dataset.tab)), TABS);
			// The examples wait behind each shape's ▾.
			assert.equal(await page.locator(`${FIELD_FORM} [data-example]`).count(), 0);
			assert.equal(await page.locator(`${FIELD_FORM} button.commit`).isEnabled(), false);
			assert.deepEqual(app.errors, []);
		});
	});

	test('a shape chosen puts the focus in the name box with its text selected', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await page.locator('.bar button[data-track]').click();
			await page.locator(`${FIELD_FORM} button[data-verb="start"]`).click();
			assert.equal(await picked(page), true, 'the name box did not take the focus');
			await page.keyboard.type('nap');
			assert.equal(await page.locator(`${FIELD_FORM} input.field`).inputValue(), 'nap');
			// Another shape: the name is selected, so typing replaces it.
			await page.locator(`${FIELD_FORM} button[data-verb="occur"]`).click();
			assert.equal(await picked(page), true, 'the name was not selected');
			assert.equal(await page.locator(`${FIELD_FORM} button.commit`).isEnabled(), true);
		});
	});

	test('each shape\'s ▾ opens its own examples, and again shuts them', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await page.locator('.bar button[data-track]').click();
			for (const { verb } of VERBS) {
				await more(page, verb).click();
				assert.equal(await more(page, verb).getAttribute('aria-expanded'), 'true');
				const mine = EXAMPLES.filter((e) => e.verb === verb);
				assert.ok(mine.length >= 3, `${verb} has ${mine.length} examples`);
				assert.deepEqual(await offered(page), mine.map((e) => e.field));
				// Each in its colour and glyph, and what it shows under it.
				assert.deepEqual(
					(await page.locator(`${FIELD_FORM} [data-example] .name`).allInnerTexts()).map((t) => t.trim()),
					mine.map((e) => `${e.glyph ?? glyphOf(verb)} ${e.field}`)
				);
				assert.deepEqual(await page.locator(`${FIELD_FORM} [data-example] .what`).allInnerTexts(), mine.map((e) => e.shows));
			}
			await more(page, 'add').click();
			assert.deepEqual(await offered(page), []);
			// Nothing was chosen by looking.
			assert.equal(await page.locator(`${FIELD_FORM} .verb[aria-checked="true"]`).count(), 0);
		});
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

				// The form says what the field would be: its name, ready to retype, its kind, states, keys.
				assert.equal(await page.locator(`${FIELD_FORM} input.field`).inputValue(), example.field);
				assert.equal(await picked(page), true, 'the name box did not take the focus');
				assert.equal(await page.locator(`${FIELD_FORM} input.description`).inputValue(), '', 'what was typed before stayed');
				assert.equal(await page.locator(`${FIELD_FORM} .verb[aria-checked="true"]`).getAttribute('data-verb'), example.verb);
				assert.deepEqual(await page.locator(`${FIELD_FORM} .state input.name`).evaluateAll((all) => all.map((i) => i.value)), Object.keys(example.states ?? {}));
				assert.deepEqual(
					await page
						.locator(`${FIELD_FORM} ul.data li.key`)
						.evaluateAll((all) => all.map((li) => [li.querySelector('.mode').textContent, li.querySelector('input.name').value, li.querySelector('.eq').getAttribute('aria-pressed')])),
					(example.keys ?? []).map((k) => [k.in_slug ? '-K' : '-k', k.name, String(k.kind !== 'required')])
				);
				const argv = trackArgv(
					example.field,
					example.verb,
					Object.entries(example.states ?? {}).map(([name, color]) => ({ name, color, description: '' })),
					example.color,
					(example.keys ?? []).map((k) => ({ ...k, value: '' }))
				);
				// What `track` cannot say — a glyph, a state card, never false — is a `spec set` after it.
				const extra = example.glyph || example.lookup || example.neverFalse;
				assert.equal((await page.locator(`${FIELD_FORM} code.preview`).innerText()).trim(), shellJoin(argv));
				// And nothing is written yet.
				await app.idle();
				assert.deepEqual(await app.allEntries(), []);
				assert.equal((await app.run(['spec', 'list'])).stdout.includes(example.field), false, 'the tap declared it');

				// Saved, it is its `track` and, where the form says more, one `spec set`; the widget stays on the field it made.
				const before = (await app.echoes()).length;
				await page.locator(`${FIELD_FORM} button.commit`).click();
				const result = await saved;
				assert.equal(result.code, 0, result.stderr);
				const echoes = (await app.echoes()).slice(before);
				assert.equal(echoes[0], shellJoin(argv));
				assert.equal(echoes.length, extra ? 2 : 1, echoes.join(' | '));
				if (extra) assert.match(echoes[1], new RegExp(`^spec set ${example.field} --from-json `));
				const shown = JSON.parse((await app.run(['spec', 'show', example.field])).stdout);
				assert.equal(shown.color, example.color);
				assert.equal(shown.description, null);
				assert.equal(shown.glyph, example.glyph);
				assert.equal(shown.state_lookup_field, example.lookup);
				assert.equal(shown.never_false, example.neverFalse);
				await page.locator(`${FIELD_FORM} h2`).waitFor();
				assert.equal((await page.locator(`${FIELD_FORM} h2`).innerText()).trim(), example.field);
				assert.deepEqual(await page.locator(`${FIELD_FORM} [role="tab"]`).evaluateAll((all) => all.map((t) => t.dataset.tab)), TABS);
				await until(() => marked(page), (m) => m.join() === `${example.glyph ?? glyphOf(example.verb)} ${example.field}`);
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
			await page.locator(`${FIELD_FORM} input.field`).waitFor();
			assert.deepEqual(await marked(page), ['+ new field']);
			assert.equal(await page.locator(`${FIELD_FORM} input.field`).inputValue(), '');
			// Less the ones there is a field for already: a tap would re-declare it.
			await more(page, 'occur').click();
			assert.deepEqual(await offered(page), ['headache', 'medication']);
			await more(page, 'start').click();
			assert.deepEqual(await offered(page), ['screentime', 'work', 'shower']);
			await more(page, 'add').click();
			assert.deepEqual(await offered(page), ['book', 'place']);
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

	/** Whether `field` is ignored, as `fields --json` lists it. */
	const ignoredIn = async (app, field) =>
		JSON.parse((await app.run(['fields', '--json'])).stdout).find((f) => f.field === field)?.ignored;
	const tracking = (page) => page.locator(`${FIELD_FORM} [data-tracking][aria-checked="true"]`).getAttribute('data-tracking');
	const explained = async (page) => (await page.locator(`${FIELD_FORM} .explain`).innerText()).trim();
	// A clock that moves: under the held one, a field's `track` row and its `ignore` row are one instant, one slug.
	const MOVING = { ...DESKTOP, clock: null };

	// vividn, 2026-10-14: «it should be able to change a field to be IGNORED or TRACKED (maybe with an
	// explanation of what that means. Probably right under the field name».
	test('tracked or ignored, under the name: a new field saved ignored is its `track` and then `ignore`', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const { saved } = await openField(page);
			await page.locator(`${FIELD_FORM} button[data-verb="occur"]`).click();
			await page.keyboard.type('tea');
			assert.equal(await tracking(page), 'tracked');
			assert.equal(await explained(page), 'in the now view and the field list');
			const top = async (sel) => (await page.locator(`${FIELD_FORM} ${sel}`).first().boundingBox()).y;
			assert.ok((await top('input.field')) < (await top('[data-tracking]')), 'it is not under the name');
			assert.ok((await top('[data-tracking]')) < (await top('[role="tab"]')), 'it is not above the tabs');

			await page.locator(`${FIELD_FORM} [data-tracking="ignored"]`).click();
			assert.equal(await tracking(page), 'ignored');
			assert.equal(
				await explained(page),
				'left out of the now view and the field list; its entries and spec stay as they are, and logging it again tracks it'
			);
			assert.equal((await page.locator(`${FIELD_FORM} code.preview`).innerText()).trim(), 'track tea --verb occur\nignore tea');
			const before = (await app.echoes()).length;
			await page.locator(`${FIELD_FORM} button.commit`).click();
			assert.equal((await saved).code, 0);
			assert.deepEqual((await app.echoes()).slice(before), ['track tea --verb occur', 'ignore tea']);
			assert.equal(await ignoredIn(app, 'tea'), true);
			assert.equal(JSON.parse((await app.run(['spec', 'show', 'tea'])).stdout).verb, 'occur', 'the spec was not written');
			assert.deepEqual(app.errors, []);
		}, MOVING);
	});

	test('a field is ignored from here and tracked again, each with Done', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.run(['track', 'coffee', '--verb', 'occur']);
			await app.redraw();
			for (const [from, to] of [['tracked', 'ignored'], ['ignored', 'tracked']]) {
				const { saved } = await openField(page, 'coffee');
				await page.locator(`${FIELD_FORM} h2`).waitFor();
				assert.equal(await tracking(page), from);
				await page.locator(`${FIELD_FORM} [data-tracking="${to}"]`).click();
				const before = (await app.echoes()).length;
				await page.locator(`${FIELD_FORM} button.commit`).click();
				assert.equal((await saved).code, 0);
				const echoes = (await app.echoes()).slice(before);
				assert.match(echoes[0], /^spec set coffee --from-json /);
				assert.deepEqual(echoes.slice(1), [`${to === 'ignored' ? 'ignore' : 'track'} coffee`]);
				assert.equal(await ignoredIn(app, 'coffee'), to === 'ignored');
			}
			// Left as it was, Done writes neither.
			const { saved } = await openField(page, 'coffee');
			await page.locator(`${FIELD_FORM} h2`).waitFor();
			const before = (await app.echoes()).length;
			await page.locator(`${FIELD_FORM} button.commit`).click();
			assert.equal((await saved).code, 0);
			assert.equal((await app.echoes()).slice(before).length, 1);
			assert.deepEqual(app.errors, []);
		}, MOVING);
	});

	// Every tab is there before a save (da-uipyt), so what is set on one is saved with the field.
	test('a new field\'s tabs are saved with it: its `track`, then the `spec set` of the rest over what it wrote', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			const { saved } = await openField(page);
			await page.locator(`${FIELD_FORM} button[data-verb="switch"]`).click();
			await page.keyboard.type('mood');
			await page.locator(`${FIELD_FORM} [data-tab="states"]`).click();
			await page.locator(`${FIELD_FORM} .states + button.add`).click();
			await page.locator(`${FIELD_FORM} .state input.name`).fill('good');
			await page.locator(`${FIELD_FORM} .state input.description`).fill('a good day');
			await page.locator(`${FIELD_FORM} [data-tab="basics"]`).click();
			await page.locator(`${FIELD_FORM} input.description`).fill('how it feels');
			const before = (await app.echoes()).length;
			await page.locator(`${FIELD_FORM} button.commit`).click();
			assert.equal((await saved).code, 0);
			const echoes = (await app.echoes()).slice(before);
			assert.equal(echoes[0], 'track mood --verb switch --state good');
			assert.match(echoes[1], /^spec set mood --from-json /);
			assert.equal(echoes.length, 2);
			const shown = JSON.parse((await app.run(['spec', 'show', 'mood'])).stdout);
			assert.equal(shown.description, 'how it feels');
			assert.deepEqual(shown.state_descriptions, { good: 'a good day' });
			assert.deepEqual(shown.allowed_states, ['good']);
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

	// The key a line's leftover words go to is a spec's to change as well as
	// a new field's: a row's mode (da-zqnt3).
	test('the rest-of-line row of a saved field is read, renamed, made one string and removed, each in its one `spec set`', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'emotion', '--verb', 'occur', '-R', 'emotions']);
			const route = async () => {
				const { remainder, string_remainder } = JSON.parse((await app.run(['spec', 'show', 'emotion'])).stdout);
				return [remainder, string_remainder];
			};
			const row = page.locator(`${FIELD_FORM} ul.data li.key[data-mode^="R"], ${FIELD_FORM} ul.data li.key[data-mode="SR"]`);
			for (const [change, stored] of [
				[() => row.locator('input.name').fill('words'), ['words', undefined]],
				[() => row.locator('.mode').click(), ['words', true]],
				[() => row.locator('.remove').click(), [undefined, undefined]]
			]) {
				const { saved } = await openField(page, 'emotion');
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
