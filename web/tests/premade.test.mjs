// The premade widget (da-36whg): the examples left the setup widget for a tab of
// their own, between welcome and setup, grouped by theme. vividn, 2026-11-05:
// «let's move the examples out of the setup pane, and instead they should get
// their own widget/layout to the left of setup … They will be organized by
// theme rather than data kind», and «examples should be it's completely own
// layout. So a tab in between welcome and setup, maybe named premade».
//
// So: a first visit's tabs read welcome · premade · setup; premade lists every
// example under its theme; a tap opens setup with that example's kind and keys
// filled and writes nothing; one whose field exists says so and opens it; and
// each example, saved from setup, is a field the bar logs.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP, PHONE, exactly } from './support/layout.mjs';
import { FIELD_FORM, PANEL, declared, leaveField } from './support/field.mjs';
import { until } from './support/until.mjs';
import { EXAMPLES, THEMES, VERBS, chosen } from '../src/lib/track.ts';

before(startApp);
after(stopApp);

const PREMADE = '.host.premade';
const card = (page, field) => page.locator(`${PREMADE} [data-example="${field}"] button.card`);
const rootTab = (page, name) => page.locator('.root-bar [role="tab"]', { hasText: exactly(name) });
const front = (page) => page.evaluate(() => window.datum.layout.current().root.tabs[window.datum.layout.current().root.active].layout);
/** Whether a text box has the focus: on a phone, the keyboard up. */
const typing = (page) => page.evaluate(() => !!document.activeElement?.matches('input, textarea, [contenteditable]'));
/** The key rows on the form's data tab: mode, name, and whether it is optional (none for the rest of the line). */
const keyRows = (page) =>
	page
		.locator(`${PANEL} ul.data li.key`)
		.evaluateAll((all) => all.map((li) => [li.querySelector('.mode').textContent, li.querySelector('input.name').value, li.querySelector('.eq')?.getAttribute('aria-pressed') ?? null]));
const rowsOf = (keys = []) => keys.map((k) => [k.rest ? `-${k.rest}` : k.in_slug ? '-K' : '-k', k.name, k.rest ? null : String(k.kind !== 'required')]);

/** Premade's tab, from a first visit's welcome. */
async function toPremade(page) {
	await rootTab(page, 'premade').click();
	await until(() => front(page), (name) => name === 'premade');
	await page.locator(`${PREMADE} [data-example]`).first().waitFor();
}

describe('a first visit', () => {
	test('opens welcome · premade · setup · collection · dayview, and premade lists every example under its theme', async () => {
		await withApp(async (app) => {
			const { page } = app;
			assert.deepEqual(await page.locator('.root-bar [role="tab"]').allInnerTexts(), ['welcome', 'premade', 'setup', 'collection', 'dayview']);
			await toPremade(page);
			const shown = await page.locator(`${PREMADE} section[data-theme]`).evaluateAll((all) =>
				all.map((s) => [s.dataset.theme, [...s.querySelectorAll('[data-example]')].map((e) => e.dataset.example)])
			);
			assert.deepEqual(shown, THEMES.map(({ theme, examples }) => [theme, examples.map((e) => e.field)]));
			// Each with its icon in its color, its name and its kind's glyph, and what it shows.
			const rgb = (hex) => `rgb(${[1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)).join(', ')})`;
			const rows = await page.locator(`${PREMADE} [data-example]`).evaluateAll((all) =>
				all.map((b) => [b.querySelector('.icon').textContent, getComputedStyle(b.querySelector('.icon')).color, b.querySelector('.name').textContent, b.querySelector('.what').textContent])
			);
			assert.deepEqual(rows, EXAMPLES.map((e) => [e.glyph, rgb(e.color), `${e.field} ${VERBS.find((v) => v.verb === e.verb).glyph}`, e.shows]));
			// The chores kit is offered beside the chores.
			assert.equal(await page.locator(`${PREMADE} section[data-theme="home & chores"] button.install`).count(), 1);
			assert.deepEqual(app.errors, []);
		}, { ...DESKTOP, home: null, desk: null });
	});

	test('on a phone a tap opens setup with the example filled, no keyboard, and nothing written', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await toPremade(page);
			await card(page, 'medication').click();
			await until(() => front(page), (name) => name === 'setup');
			const field = page.locator(`${FIELD_FORM} input.field`);
			await until(() => field.inputValue(), (v) => v === 'medication');
			assert.equal(await typing(page), false, 'the example opened the keyboard');
			assert.equal(await page.locator(`${FIELD_FORM} .verb[aria-checked="true"]`).getAttribute('data-verb'), 'occur');
			await page.locator(`${FIELD_FORM} button[data-to-tab="data"]`).click();
			assert.deepEqual(await keyRows(page), [['-k', 'name', 'false'], ['-k', 'dose', 'true']]);
			// Setup itself offers no examples.
			assert.equal(await page.locator(`${FIELD_FORM} [data-examples], ${FIELD_FORM} [data-example]`).count(), 0);
			await app.idle();
			assert.deepEqual(await app.allEntries(), []);
			assert.deepEqual(app.errors, []);
		}, { ...PHONE, home: null });
	});

	test("weight's lbs or kg names its key, and an example already a field says «set up» and opens it", async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.run(['track', 'coffee', '--verb', 'occur']);
			await app.redraw();
			await toPremade(page);
			// The card is its choices; kg makes the key `kg=`.
			assert.deepEqual(await page.locator(`${PREMADE} [data-example="weight"] [data-choice]`).allInnerTexts(), ['lbs', 'kg']);
			await page.locator(`${PREMADE} [data-example="weight"] [data-choice="kg"]`).click();
			await until(() => front(page), (name) => name === 'setup');
			await until(() => page.locator(`${FIELD_FORM} input.field`).inputValue(), (v) => v === 'weight');
			await page.locator(`${FIELD_FORM} button[data-to-tab="data"]`).click();
			assert.deepEqual(await keyRows(page), [['-k', 'kg', 'true']]);

			await rootTab(page, 'premade').click();
			await until(() => front(page), (name) => name === 'premade');
			const coffee = page.locator(`${PREMADE} [data-example="coffee"]`);
			assert.equal(await coffee.getAttribute('data-taken'), 'true');
			assert.equal((await coffee.locator('.what').innerText()).trim(), 'set up');
			await card(page, 'coffee').click();
			await until(() => front(page), (name) => name === 'setup');
			await until(() => page.locator(`${FIELD_FORM} h2`).innerText(), (t) => t.trim() === 'coffee');
			assert.deepEqual(app.errors, []);
		}, { ...DESKTOP, home: null, desk: null });
	});
});

test('the chores kit button shows what the kit brings; cancel installs nothing, install installs it', async () => {
	await withApp(async (app) => {
		const { page } = app;
		const kits = async () => (await app.run(['kit', 'list'])).stdout;
		await toPremade(page);
		const install = page.locator(`${PREMADE} button.install`);
		const dialog = page.locator('dialog.confirm');
		await install.click();
		await dialog.waitFor();
		assert.match(await dialog.innerText(), /chores/, 'the dry run is not shown');
		await dialog.getByRole('button', { name: 'cancel', exact: true }).click();
		await dialog.waitFor({ state: 'detached' });
		await app.idle();
		assert.doesNotMatch(await kits(), /chores/);

		await install.click();
		await dialog.getByRole('button', { name: 'install', exact: true }).click();
		await dialog.waitFor({ state: 'detached' });
		await until(kits, (listed) => /^chores\s/m.test(listed));
		assert.deepEqual(app.errors, []);
	}, { ...DESKTOP, home: null, desk: null });
});

/** A line the bar logs each example with, once it is saved, and the data that line stores. */
const LOGGED = {
	sleep: ['sleep', null],
	weight: ['weight 160', { lbs: 160 }],
	water: ['water 250', { ml: 250 }],
	medication: ['medication ibuprofen 200mg', { name: 'ibuprofen', dose: '200mg' }],
	headache: ['headache', null],
	coffee: ['coffee', null],
	meal: ['snack food=toast', { food: 'toast' }],
	alcohol: ['alcohol', null],
	tobacco: ['tobacco', null],
	cannabis: ['cannabis', null],
	mood: ['mood -- -3', { level: -3 }],
	emotion: ['emotion calm tired', { words: ['calm', 'tired'] }],
	energy: ['energy 7', { level: 7 }],
	run: ['run 5', { distanceKm: 5 }],
	workout: ['workout swim', null],
	work: ['work', null],
	project: ['project datum', null],
	meeting: ['meeting', null],
	room: ['room kitchen', null],
	environment: ['environment train', null],
	laundry: ['laundry', null],
	plants: ['plants', null],
	makebed: ['makebed', null],
	dishes: ['dishes', null],
	trash: ['trash', null],
	vacuum: ['vacuum', null],
	phone: ['phone', null],
	watch: ['watch dune', null],
	reading: ['reading dune', null],
	floss: ['floss', null],
	stretch: ['stretch', null],
	vitamins: ['vitamins', null],
	person: ['person vr520 vremya', { id: 'vr520', name: 'vremya' }],
	book: ['book dune Dune Herbert', { id: 'dune', title: 'Dune', author: 'Herbert' }],
	movie: ['movie dune Dune 2021', { id: 'dune', title: 'Dune', year: 2021 }],
	series: ['series severance Severance 2', { id: 'severance', title: 'Severance', season: 2 }],
	word: ['word correr es verb run', { id: 'correr', lang: 'es', pos: 'verb', meaning: 'run' }]
};

/** The setup widget opened on `example` as premade opens it, once any place before it has gone. */
async function openExample(page, example) {
	if (await page.locator(FIELD_FORM).count()) await leaveField(page);
	await page.locator('.field-form').waitFor({ state: 'detached' });
	const saved = page.evaluate((e) => window.datum.field(e.field, e.verb, { example: e }), example);
	saved.catch(() => {});
	return { saved };
}

describe('each example', () => {
	const shared = sharedApp(DESKTOP);

	test('has a line the bar logs it with', () => assert.deepEqual(Object.keys(LOGGED).sort(), EXAMPLES.map((e) => e.field).sort()));

	for (const example of EXAMPLES.map((e) => chosen(e, Object.keys(e.choices ?? {})[0]))) {
		test(`${example.field} fills setup, not the database, and saves to a field the bar logs`, async () => {
			await shared.solo(async (app) => {
				const { page } = app;
				const { saved } = await openExample(page, example);
				await until(() => page.locator(`${FIELD_FORM} input.field`).inputValue(), (v) => v === example.field);
				assert.equal(await page.locator(`${FIELD_FORM} .verb[aria-checked="true"]`).getAttribute('data-verb'), example.verb);
				assert.deepEqual(await page.locator(`${FIELD_FORM} .state input.name`).evaluateAll((all) => all.map((i) => i.value)), Object.keys(example.states ?? {}));
				if (example.verb !== 'add') await page.locator(`${FIELD_FORM} button[data-to-tab="data"]`).click();
				assert.deepEqual(await keyRows(page), rowsOf(example.keys));
				await app.idle();
				assert.deepEqual(await app.allEntries(), []);

				// Saved, it is ONE `setup F --track` with the whole form (da-2pbvd).
				const before = (await app.echoes()).length;
				await page.locator(`${FIELD_FORM} button.commit`).click();
				const result = await saved;
				assert.equal(result.code, 0, result.stderr);
				const echoes = (await app.echoes()).slice(before);
				assert.equal(echoes.length, 1, `one command, not a stream: ${echoes.join(' | ')}`);
				assert.equal(declared(echoes[0]).line, `setup ${example.field} --from-json --track`);
				const shown = JSON.parse((await app.run(['setup', example.field, '--json'])).stdout);
				assert.equal(shown.color, example.color);
				assert.equal(shown.symbol, example.glyph);
				assert.deepEqual(shown.state_colors, example.states ?? {});
				assert.equal(shown.state_lookup_field, example.lookup);
				assert.equal(shown.never_false, example.neverFalse);
				if (example.aliases)
					assert.deepEqual(
						shown.derived_aliases.map((a) => [a.state, a.expands_to]),
						example.aliases.map((a) => [a.state, a.expands_to])
					);
				await leaveField(page);
				await page.locator(FIELD_FORM).waitFor({ state: 'detached' });

				// A working field: the bar logs one entry of it.
				const [line, data] = LOGGED[example.field];
				await app.clock.forward(1000);
				await app.submit(line);
				assert.deepEqual(app.errors, []);
				const logged = (await app.entries()).filter((e) => e.field === example.field);
				assert.equal(logged.length, 1, JSON.stringify(await app.allEntries()));
				if (data) assert.deepEqual(logged[0].data, data);

				// Undo takes back the entry, and one more the field whole.
				for (const _ of [1, 2]) assert.equal((await app.run(['undo'])).code, 0);
				assert.match((await app.run(['setup', example.field, '--json'])).stdout, /No spec defined/);
				assert.deepEqual(await app.allEntries(), []);
			});
		});
	}
});
