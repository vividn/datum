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
//
// The examples are premade.toml's, compiled into the CLI (da-rky8h): read here
// as the page reads them, `setup --premade --json`, through the same wasm.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { File as MemFile, OpenFile, WASI } from '../src/lib/vendor/browser_wasi_shim/index.js';
import { sharedApp, startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP, PHONE, exactly } from './support/layout.mjs';
import { FIELD_FORM, PANEL, declared, fieldNamed, leaveField } from './support/field.mjs';
import { resolveLibAsSource } from './support/libhook.mjs';
import { exec } from './support/process.mjs';
import { until } from './support/until.mjs';
import { VERBS, chosen } from '../src/lib/track.ts';

resolveLibAsSource();
const { Sink } = await import('../src/lib/wasiFs.ts');
/** premade.toml, as `setup --premade --json` prints it from the wasm the page runs. */
const { themes: THEMES, examples: EXAMPLES } = (() => {
	const module = new WebAssembly.Module(readFileSync(new URL('../src/lib/wasm/datum-cli.wasm', import.meta.url)));
	const [stdout, stderr] = [new Sink(), new Sink()];
	const wasi = new WASI(['datum', 'setup', '--premade', '--json'], ['TZ=UTC'], [new OpenFile(new MemFile([])), stdout, stderr], { debug: false });
	assert.equal(exec(module, wasi), 0, stderr.text());
	return JSON.parse(stdout.text());
})();

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
/** The key rows `spec` draws: each key it declares, then the one taking the rest of the line. */
const rowsOf = (spec) => [
	...spec.keys.map((k) => [k.in_slug ? '-K' : '-k', k.name, String(k.kind !== 'required')]),
	...(spec.remainder ? [[spec.string_remainder ? '-SR' : '-R', spec.remainder, null]] : [])
];

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
			assert.deepEqual(shown, THEMES.map((theme) => [theme, EXAMPLES.filter((e) => e.theme === theme).map((e) => e.field)]));
			// Each with its icon in its color, its name and its kind's glyph, and what it shows.
			const rgb = (hex) => `rgb(${[1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)).join(', ')})`;
			const rows = await page.locator(`${PREMADE} [data-example]`).evaluateAll((all) =>
				all.map((b) => [b.querySelector('.icon').textContent, getComputedStyle(b.querySelector('.icon')).color, b.querySelector('.name').textContent, b.querySelector('.what').textContent])
			);
			// In the file's order, theme by theme.
			const listed = THEMES.flatMap((theme) => EXAMPLES.filter((e) => e.theme === theme));
			assert.deepEqual(rows, listed.map((e) => [e.symbol, rgb(e.color), `${e.field} ${VERBS.find((v) => v.verb === e.kind).glyph}`, e.shows]));
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
			await until(() => fieldNamed(page), (v) => v === 'medication');
			assert.equal(await typing(page), false, 'the example opened the keyboard');
			assert.equal(await page.locator(`${FIELD_FORM} .verb[aria-checked="true"]`).getAttribute('data-verb'), 'occur');
			await page.locator(`${FIELD_FORM} [data-tab="data"]`).click();
			assert.deepEqual(await keyRows(page), [['-k', 'name', 'false'], ['-k', 'dose', 'true']]);
			// Setup itself offers no examples.
			assert.equal(await page.locator(`${FIELD_FORM} [data-examples], ${FIELD_FORM} [data-example]`).count(), 0);
			await app.idle();
			assert.deepEqual(await app.allEntries(), []);
			assert.deepEqual(app.errors, []);
		}, { ...PHONE, home: null });
	});

	test('steps is listed under body, and a tap opens setup with the all-day kind chosen', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await toPremade(page);
			assert.equal(await page.locator(`${PREMADE} section[data-theme="body"] [data-example="steps"]`).count(), 1);
			await card(page, 'steps').click();
			await until(() => front(page), (name) => name === 'setup');
			await until(() => fieldNamed(page), (v) => v === 'steps');
			assert.equal(await page.locator(`${FIELD_FORM} .verb[aria-checked="true"]`).getAttribute('data-verb'), 'allday');
			await app.idle();
			assert.deepEqual(await app.allEntries(), []);
			assert.deepEqual(app.errors, []);
		}, { ...DESKTOP, home: null, desk: null });
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
			await until(() => fieldNamed(page), (v) => v === 'weight');
			await page.locator(`${FIELD_FORM} [data-tab="data"]`).click();
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

/** A line the bar logs each example with, once it is saved, the data that line stores, and the state where it stores one. */
const LOGGED = {
	sleep: ['sleep', null],
	weight: ['weight 160', { lbs: 160 }],
	water: ['water 250', { ml: 250 }],
	medication: ['medication ibuprofen 200mg', { name: 'ibuprofen', dose: '200mg' }],
	headache: ['headache', null],
	shower: ['shower', null],
	menstruation: ['menstruation', null],
	sitting: ['sitting', null],
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
	steps: ['steps 8000', null, 8000],
	work: ['work', null],
	project: ['project datum', null],
	meeting: ['meeting', null],
	activity: ['activity meeting', null],
	room: ['room kitchen', null],
	environment: ['environment train', null],
	laundry: ['laundry', null],
	plants: ['plants', null],
	makebed: ['makebed', null],
	dishes: ['dishes', null],
	trash: ['trash', null],
	vacuum: ['vacuum', null],
	phone: ['phone', null],
	screentime: ['screentime', null],
	watch: ['watch dune', null],
	reading: ['reading dune', null],
	floss: ['floss', null],
	stretch: ['stretch', null],
	vitamins: ['vitamins', null],
	person: ['person vr520 vremya', { id: 'vr520', name: 'vremya' }],
	place: ['place rosa Rosa Kastanienallee', { id: 'rosa', name: 'Rosa', address: 'Kastanienallee' }],
	book: ['book dune Dune Herbert', { id: 'dune', title: 'Dune', author: 'Herbert' }],
	movie: ['movie dune Dune 2021', { id: 'dune', title: 'Dune', year: 2021 }],
	series: ['series severance Severance 2', { id: 'severance', title: 'Severance', season: 2 }],
	word: ['word correr es verb run', { id: 'correr', lang: 'es', pos: 'verb', meaning: 'run' }]
};

/** The setup widget opened on `example` as premade opens it, once any place before it has gone. */
async function openExample(page, example) {
	if (await page.locator(FIELD_FORM).count()) await leaveField(page);
	await page.locator('.field-form').waitFor({ state: 'detached' });
	const saved = page.evaluate((e) => window.datum.field(e.field, e.kind, { example: e }), example);
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
				await until(() => fieldNamed(page), (v) => v === example.field);
				const { spec } = example;
				assert.equal(await page.locator(`${FIELD_FORM} .verb[aria-checked="true"]`).getAttribute('data-verb'), example.kind);
				assert.deepEqual(await page.locator(`${FIELD_FORM} .state input.name`).evaluateAll((all) => all.map((i) => i.value)), spec.allowed_states ?? []);
				if (example.kind !== 'add') await page.locator(`${FIELD_FORM} [data-tab="data"]`).click();
				assert.deepEqual(await keyRows(page), rowsOf(spec));
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
				// What it stores is the example's spec, its place and mood's faces included.
				const shown = JSON.parse((await app.run(['setup', example.field, '--json'])).stdout);
				for (const key of ['color', 'symbol', 'state_colors', 'state_lookup_field', 'never_false', 'symbol_by', 'y', 'height', 'symbol_scale', 'keys'])
					assert.deepEqual(shown[key], spec[key], key);
				assert.deepEqual(
					shown.derived_aliases.map((a) => [a.state, a.expands_to]),
					spec.derived_aliases.map((a) => [a.state, a.expands_to])
				);
				await leaveField(page);
				await page.locator(FIELD_FORM).waitFor({ state: 'detached' });

				// A working field: the bar logs one entry of it.
				const [line, data, state] = LOGGED[example.field];
				await app.clock.forward(1000);
				await app.submit(line);
				assert.deepEqual(app.errors, []);
				const logged = (await app.entries()).filter((e) => e.field === example.field);
				assert.equal(logged.length, 1, JSON.stringify(await app.allEntries()));
				if (data) assert.deepEqual(logged[0].data, data);
				// An all-day entry's count is its state, on the day (da-uykff).
				if (state !== undefined) assert.equal(logged[0].state, state);

				// Undo takes back the entry, and one more the field whole.
				for (const _ of [1, 2]) assert.equal((await app.run(['undo'])).code, 0);
				assert.match((await app.run(['setup', example.field, '--json'])).stdout, /No spec defined/);
				assert.deepEqual(await app.allEntries(), []);
			});
		});
	}
});
