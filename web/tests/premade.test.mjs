// The setup tab (da-asgtm): premade and manual setup in one tab again. vividn,
// 2026-11-09: «combine the premade and setup tabs back into one tab, which
// makes it easy for new people to get going, but also experienced users to
// add new fields, edit existing fields». The premade tab had been the
// examples' own (da-36whg) since 11-05.
//
// So: a first visit's tabs read welcome · setup · collection · dayview; setup
// with no field is «manual setup», the fields touched last beside it, and
// every example under its theme; a click opens its preview (the right half
// on a desktop, a sheet on a phone): its options, a day of it drawn by the
// CLI, Configure, which fills the form and writes nothing, and Start
// tracking, which sets it up as shown and goes to the collection. Each
// example, saved from the form, is a field the bar logs.
//
// The examples are premade.toml's, compiled into the CLI (da-rky8h): read here
// as the page reads them, `setup --premade --json`, through the same wasm.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { File as MemFile, OpenFile, WASI } from '../src/lib/vendor/browser_wasi_shim/index.js';
import { sharedApp, startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP, PHONE, exactly } from './support/layout.mjs';
import { FIELD_FORM, NAMING, PANEL, declared, fieldNamed, leaveField, nameField, setupCommand } from './support/field.mjs';
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

const SETUP = '.host.setup';
const row = (page, field) => page.locator(`${SETUP} button.row[data-example="${field}"]`);
/** The preview, inline on a desktop or in its sheet on a phone. */
const PREVIEW = '.preview:has([data-preview])';
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
/** The preview's band once the CLI has drawn `field` in it. */
const drawn = (page, field) => page.locator(`${PREVIEW} .band svg g.field[data-field="${field}"]`);

/** The setup tab, from a first visit's welcome, its catalog read. */
async function toSetup(page) {
	await rootTab(page, 'setup').click();
	await until(() => front(page), (name) => name === 'setup');
	await page.locator(`${SETUP} [data-example]`).first().waitFor();
}

describe('a first visit', () => {
	// vividn, 2026-11-10 (da-thlic): «can we get that "manual setup" section more like in the design? Right now it is just a lone button».
	for (const [device, size] of [['desktop', DESKTOP], ['phone', PHONE]]) {
		test(`on a ${device}, manual setup is a card: its heading, what it is for, the button, and the fields touched last once there are any`, async () => {
			await withApp(async (app) => {
				const { page } = app;
				await toSetup(page);
				const card = page.locator(`${SETUP} ${FIELD_FORM} .manual`);
				assert.equal(await card.locator('h3').innerText(), 'MANUAL SETUP');
				assert.equal(await card.locator('p.for').innerText(), 'Make a field from scratch, or change one you already track.');
				assert.equal(await card.locator('button.track-something').innerText(), 'manual setup');
				assert.equal(await card.locator('.recent').count(), 0);
				const ground = await card.evaluate((el) => [getComputedStyle(el).backgroundColor, getComputedStyle(el).borderTopWidth]);
				const raised = await page.evaluate(() => {
					const probe = document.body.appendChild(Object.assign(document.createElement('i'), { style: 'background: var(--raised)' }));
					const color = getComputedStyle(probe).backgroundColor;
					probe.remove();
					return color;
				});
				assert.deepEqual(ground, [raised, '1px']);
				// One row that wraps on a desktop, stacked on a phone: the button beside the line, or under it.
				const box = (sel) => card.locator(sel).evaluate((el) => el.getBoundingClientRect().toJSON());
				const [line, button] = [await box('p.for'), await box('button.track-something')];
				assert.equal(await card.locator('.name-line').evaluate((el) => getComputedStyle(el).flexDirection), device === 'phone' ? 'column' : 'row');
				if (device === 'phone') assert.ok(button.top >= line.bottom, `the button is not under the line: ${button.top} < ${line.bottom}`);
				else assert.ok(button.top < line.bottom, `the button is not beside the line: ${button.top} >= ${line.bottom}`);
				// Two fields logged, a second apart as the page's clock is held: both are recent, newest first.
				for (const name of ['tea', 'nap']) {
					await app.clock.forward(1000);
					await app.run(['moment', name]);
				}
				await app.redraw();
				const recent = () => card.locator('[data-recent]').evaluateAll((all) => all.map((b) => b.dataset.recent));
				await until(recent, (r) => r.join() === 'nap,tea');
				if (device === 'phone') assert.ok((await box('.recent')).top >= (await box('button.track-something')).bottom, 'the recent row is not under the button');
				// The button still opens manual setup.
				await card.locator('button.track-something').click();
				await page.locator(`${NAMING} input.field`).waitFor();
				assert.deepEqual(app.errors, []);
			}, { ...size, home: null, desk: null });
		});

		// vividn, 2026-11-10 (da-gnba1): «When you're doing manual setup the premade ones don't currently disappear, leaving the entire setup widget cramped up at the top».
		test(`on a ${device}, a new name typed in manual setup takes the catalog and the recent fields away, and its × brings them back`, async () => {
			await withApp(async (app) => {
				const { page } = app;
				await app.run(['moment', 'tea']);
				await app.redraw();
				await toSetup(page);
				const [catalog, recent] = [page.locator(`${SETUP} .home h3.ask`), page.locator(`${SETUP} [data-recent]`)];
				await recent.first().waitFor();
				assert.equal(await catalog.innerText(), 'What do you want to track?');
				await nameField(page, 'hike');
				assert.equal(await fieldNamed(page), 'hike');
				assert.equal(await page.locator(`${SETUP} .home`).count(), 0);
				assert.equal(await recent.count(), 0);
				// The form has the pane: its foot is the pane's.
				const [form, pane] = await page
					.locator(`${SETUP} ${FIELD_FORM}`)
					.evaluate((el) => [el, el.parentElement].map((e) => Math.round(e.getBoundingClientRect().bottom)));
				assert.ok(form >= pane - 1, `the form's foot ${form} is above the pane's ${pane}`);
				await page.locator(`${FIELD_FORM} .name-line .pill button.clear`).click();
				await catalog.waitFor();
				await recent.first().waitFor();
				// A name there is opens that field, as it did.
				await nameField(page, 'tea');
				await until(() => page.locator(`${FIELD_FORM} h2`).innerText(), (t) => t.trim() === 'tea');
				assert.equal(await page.locator(`${SETUP} .home`).count(), 0);
				assert.deepEqual(app.errors, []);
			}, { ...size, home: null, desk: null });
		});
	}

	test('opens welcome · setup · collection · dayview, and setup lists every example under its theme, icon and name', async () => {
		await withApp(async (app) => {
			const { page } = app;
			assert.deepEqual(await page.locator('.root-bar [role="tab"]').allInnerTexts(), ['welcome', 'setup', 'collection', 'dayview']);
			await toSetup(page);
			// «manual setup» is over the catalog.
			assert.equal(await page.locator(`${SETUP} button.track-something`).innerText(), 'manual setup');
			const shown = await page.locator(`${SETUP} section[data-theme]`).evaluateAll((all) =>
				all.map((s) => [s.dataset.theme, [...s.querySelectorAll('[data-example]')].map((e) => e.dataset.example)])
			);
			assert.deepEqual(shown, THEMES.map((theme) => [theme, EXAMPLES.filter((e) => e.theme === theme).map((e) => e.field)]));
			// A row is its icon in its color and its name: no description, no kind.
			const rgb = (hex) => `rgb(${[1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)).join(', ')})`;
			const rows = await page.locator(`${SETUP} [data-example]`).evaluateAll((all) =>
				all.map((b) => [b.querySelector('.icon').textContent, getComputedStyle(b.querySelector('.icon')).color, b.textContent.trim()])
			);
			const listed = THEMES.flatMap((theme) => EXAMPLES.filter((e) => e.theme === theme));
			assert.deepEqual(rows, listed.map((e) => [e.symbol, rgb(e.color), `${e.symbol} ${e.field}`]));
			// On a desktop the first one's preview stands beside the list before any click.
			await drawn(page, listed[0].field).waitFor();
			assert.equal(await page.locator(`${PREVIEW} [data-preview]`).getAttribute('data-preview'), listed[0].field);
			// The chores kit is offered beside the chores.
			assert.equal(await page.locator(`${SETUP} section[data-theme="home & chores"] button.install`).count(), 1);
			assert.deepEqual(app.errors, []);
		}, { ...DESKTOP, home: null, desk: null });
	});

	test("weight's preview: lbs or kg, its label shown or not on the day the CLI draws, and Configure fills the form, writing nothing", async () => {
		await withApp(async (app) => {
			const { page } = app;
			await toSetup(page);
			await row(page, 'weight').click();
			const weight = EXAMPLES.find((e) => e.field === 'weight');
			await page.locator(`${PREVIEW} [data-preview="weight"]`).waitFor();
			assert.match(await page.locator(`${PREVIEW} .kind`).innerText(), /● moment/);
			assert.deepEqual(await page.locator(`${PREVIEW} [data-choice]`).allInnerTexts(), Object.keys(weight.choices));
			// The day's weighing, its value printed above it while the label is shown.
			const label = page.locator(`${PREVIEW} .band svg text.point-label`);
			await drawn(page, 'weight').waitFor();
			await until(() => label.allTextContents(), (t) => t.length === 1);
			await page.locator(`${PREVIEW} [data-label]`).click();
			await until(() => label.count(), (n) => n === 0);
			await page.locator(`${PREVIEW} [data-label]`).click();
			await page.locator(`${PREVIEW} [data-choice="kg"]`).click();
			assert.equal(await page.locator(`${PREVIEW} [data-choice="kg"]`).getAttribute('aria-pressed'), 'true');

			await page.locator(`${PREVIEW} button.configure`).click();
			await until(() => fieldNamed(page), (v) => v === 'weight');
			await page.locator(`${FIELD_FORM} [data-tab="data"]`).click();
			assert.deepEqual(await keyRows(page), [['-k', 'kg', 'true']]);
			await app.idle();
			assert.deepEqual(await app.allEntries(), []);
			// The form's × asks before it drops the unsaved field, and leaving is the catalog again, on weight.
			await page.locator(`${FIELD_FORM} .name-line .pill button.clear`).click();
			await page.locator('dialog.confirm').getByRole('button', { name: 'leave', exact: true }).click();
			await page.locator(`${PREVIEW} [data-preview="weight"]`).waitFor();
			assert.deepEqual(app.errors, []);
		}, { ...DESKTOP, home: null, desk: null });
	});

	test('on a phone a tap opens the preview as a sheet with ‹ back, and Configure opens the form filled, no keyboard, nothing written', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await toSetup(page);
			// Two columns on a phone, and no preview until a tap.
			const lefts = await page.locator(`${SETUP} section[data-theme="body"] [data-example]`).evaluateAll((all) => new Set(all.map((b) => Math.round(b.getBoundingClientRect().left))).size);
			assert.equal(lefts, 2);
			assert.equal(await page.locator(`${PREVIEW}`).count(), 0);
			await row(page, 'medication').click();
			await page.locator(`dialog ${PREVIEW} [data-preview="medication"]`).waitFor();
			await page.locator('dialog button.back').click();
			await page.locator(`${PREVIEW}`).waitFor({ state: 'detached' });
			await row(page, 'medication').click();
			await page.locator(`dialog ${PREVIEW} button.configure`).click();
			await until(() => fieldNamed(page), (v) => v === 'medication');
			assert.equal(await typing(page), false, 'the example opened the keyboard');
			assert.equal(await page.locator(`${FIELD_FORM} .verb[aria-checked="true"]`).getAttribute('data-verb'), 'occur');
			await page.locator(`${FIELD_FORM} [data-tab="data"]`).click();
			assert.deepEqual(await keyRows(page), [['-k', 'name', 'false'], ['-k', 'dose', 'true']]);
			await app.idle();
			assert.deepEqual(await app.allEntries(), []);
			assert.deepEqual(app.errors, []);
		}, { ...PHONE, home: null });
	});

	test("menstruation's other kind is one of its options, and Configure opens it as that kind", async () => {
		await withApp(async (app) => {
			const { page } = app;
			await toSetup(page);
			await row(page, 'menstruation').click();
			const menstruation = EXAMPLES.find((e) => e.field === 'menstruation');
			const [own, each] = [menstruation.kind, menstruation.as[0].kind];
			assert.deepEqual(await page.locator(`${PREVIEW} [data-variant]`).evaluateAll((all) => all.map((b) => b.dataset.variant)), [own, each]);
			await page.locator(`${PREVIEW} [data-variant="${each}"]`).click();
			await page.locator(`${PREVIEW} button.configure`).click();
			await until(() => fieldNamed(page), (v) => v === 'menstruation');
			assert.equal(await page.locator(`${FIELD_FORM} .verb[aria-checked="true"]`).getAttribute('data-verb'), 'allday');
			assert.deepEqual(app.errors, []);
		}, { ...DESKTOP, home: null, desk: null });
	});

	test('Start tracking sets weight up as shown, its label and place included, in one command, and goes to the collection', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await toSetup(page);
			await row(page, 'weight').click();
			await page.locator(`${PREVIEW} [data-choice="kg"]`).click();
			await page.locator(`${PREVIEW} button.start`).click();
			await until(() => front(page), (name) => name === 'collect');
			const spec = JSON.parse((await app.run(['setup', 'weight', '--json'])).stdout);
			const kg = EXAMPLES.find((e) => e.field === 'weight').choices.kg;
			assert.deepEqual([spec.labels, spec.keys, spec.y, spec.symbol_scale], [kg.labels, kg.spec.keys, kg.spec.y, kg.spec.symbol_scale]);
			const listed = JSON.parse((await app.run(['fields', '--json'])).stdout);
			assert.equal(listed.find((f) => f.field === 'weight')?.ignored, false, 'weight is not tracked');
			// One command: one undo takes back the spec and its TRACK row together.
			assert.equal((await app.run(['undo'])).code, 0);
			assert.deepEqual(await app.allEntries(), []);
			assert.deepEqual(app.errors, []);
		}, { ...DESKTOP, home: null, desk: null });
	});

	test('a field there is says so, Configure opens it, and the fields touched last are recent, newest first', async () => {
		await withApp(async (app) => {
			const { page } = app;
			// A second apart, as the page's clock is held.
			for (const line of [['track', 'coffee', '--verb', 'occur'], ['track', 'mood', '--verb', 'switch'], ['track', 'sleep', '--verb', 'start'], ['track', 'tea', '--verb', 'occur'], ['occur', 'coffee']]) {
				await app.clock.forward(1000);
				await app.run(line);
			}
			await app.redraw();
			await toSetup(page);
			const recent = () => page.locator(`${SETUP} [data-recent]`).evaluateAll((all) => all.map((b) => b.dataset.recent));
			await until(recent, (r) => r.join() === 'coffee,tea,sleep');
			await row(page, 'coffee').click();
			assert.equal(await row(page, 'coffee').getAttribute('data-taken'), 'true');
			assert.match(await page.locator(`${PREVIEW} .taken`).innerText(), /You track this already/);
			assert.equal(await page.locator(`${PREVIEW} button.start`).count(), 0);
			await page.locator(`${PREVIEW} button.configure`).click();
			await until(() => page.locator(`${FIELD_FORM} h2`).innerText(), (t) => t.trim() === 'coffee');
			await page.locator(`${FIELD_FORM} .name-line .pill button.clear`).click();
			await page.locator(`${SETUP} [data-recent="sleep"]`).click();
			await until(() => page.locator(`${FIELD_FORM} h2`).innerText(), (t) => t.trim() === 'sleep');
			assert.deepEqual(app.errors, []);
		}, { ...DESKTOP, home: null, desk: null });
	});

	// vremya on #1873: setup kept behind collection reads at the layout's epoch,
	// so a write on collection does not also run its read of the fields touched last.
	test('kept behind collection, the recent fields are read again once the page is quiet, not with the write', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.run(['track', 'coffee', '--verb', 'occur']);
			await app.redraw();
			await toSetup(page);
			await page.locator(`${SETUP} [data-recent="coffee"]`).waitFor();
			await rootTab(page, 'collection').click();
			await until(() => front(page), (name) => name === 'collect');
			const seen = await app.spyOnRunner();
			const touched = async () => (await seen()).filter((argv) => argv[0] === 'tail' && argv.includes('modify')).length;
			await app.clock.forward(1000);
			// Typed on the bar, which echoes nowhere here: collection has no CLI widget.
			const input = page.locator('.prompt-bar input');
			await input.fill('moment coffee');
			await input.press('Enter');
			await until(async () => (await app.entries()).filter((e) => e.field === 'coffee').length, (n) => n === 1);
			await app.idle();
			assert.equal(await touched(), 0, 'the write ran the recent fields read at once');
			// And once the page is quiet, it does.
			await app.clock.forward(1500);
			await until(touched, (n) => n === 1);
			assert.deepEqual(app.errors, []);
		}, { ...DESKTOP, home: null, desk: null });
	});
});

test('the chores kit button shows what the kit brings; cancel installs nothing, install installs it and opens its chores tab', async () => {
	await withApp(async (app) => {
		const { page } = app;
		const kits = async () => (await app.run(['kit', 'list'])).stdout;
		await toSetup(page);
		const install = page.locator(`${SETUP} button.install`);
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
		await until(() => front(page), (name) => name === 'chores');
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

/** The setup widget opened on `example` as Configure opens it, once any place before it has gone. */
async function openExample(page, example) {
	if (await page.locator(FIELD_FORM).count()) await leaveField(page);
	await page.locator('.field-form').waitFor({ state: 'detached' });
	const saved = page.evaluate((e) => window.datum.field(e.field, e.kind, { example: e }), example);
	saved.catch(() => {});
	return { saved };
}

// vividn, 2026-11-11 (da-ebz6k): «hitting "start tracking" on a premade thing ends up switching to collection
// which is good, but it leaves the modal open, which it should close». On a phone the preview's sheet, on a
// desktop the setup modal: whatever is open goes as collect comes to the front, with the new field's button on it.
describe('start tracking lands on collect with nothing open over it', () => {
	/** The dialogs open, but the tour's sentence: a sheet, the setup modal, the form's naming modal. */
	const open = (page) => page.locator('dialog[open], [role="dialog"]:not(.tour *)').count();
	/** Whether `field`'s button is drawn on the collect widget in front. */
	const button = (page, field) =>
		page.locator(`[data-widget="widgetbar"] button[data-field="${field}"]`).evaluateAll((all) => all.some((b) => b.getBoundingClientRect().width > 0));
	async function landed(page, field, at = 'collect') {
		await until(() => front(page), (name) => name === at);
		await until(() => open(page), (n) => n === 0);
		await until(() => button(page, field), (drawn) => drawn);
	}
	const ON_SETUP = { names: ['welcome', 'setup', 'collect', 'dayview'], active: 1 };
	/** The setup place, which on a desktop with no setup tab is a modal over the page. */
	const toModal = (page) => void page.evaluate(() => window.datum.field()).catch(() => {});

	for (const [device, size] of [['desktop', DESKTOP], ['phone', PHONE]])
		test(`on a ${device}, a premade's Start tracking from the setup tab`, async () => {
			await withApp(async (app) => {
				const { page } = app;
				await toSetup(page);
				await row(page, 'weight').click();
				if (device === 'phone') await page.locator('dialog[open] [data-preview="weight"]').waitFor();
				await page.locator(`${PREVIEW} button.start`).click();
				await landed(page, 'weight');
				assert.deepEqual(app.errors, []);
			}, { ...size, home: null, desk: null });
		});

	test("on a desktop, a premade's Start tracking from the setup modal", async () => {
		await withApp(async (app) => {
			const { page } = app;
			toModal(page);
			await page.locator('dialog.sheet[open][data-place="setup"] button.row[data-example="weight"]').click();
			await page.locator(`${PREVIEW} button.start`).click();
			await landed(page, 'weight');
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	// Over a page with a collect widget on it already, the tour stays there (tour.test.mjs).
	for (const [where, options, at] of [
		["a phone's setup tab", { ...PHONE, home: ON_SETUP, desk: null, tour: true }, 'collect'],
		["a desktop's setup modal", { ...DESKTOP, tour: true }, 'main']
	])
		test(`a first field typed by hand, from ${where}`, async () => {
			await withApp(async (app) => {
				const { page } = app;
				if (where.includes('modal')) toModal(page);
				await page.locator(FIELD_FORM).waitFor();
				await nameField(page, 'tea');
				await page.locator(`${FIELD_FORM} .verb[data-verb="occur"]`).click();
				await setupCommand(page);
				await landed(page, 'tea', at);
				assert.deepEqual(app.errors, []);
			}, options);
		});
});

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
				await setupCommand(page);
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
