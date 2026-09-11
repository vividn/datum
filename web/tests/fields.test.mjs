// The fields widget (da-c7u8): every field `datum fields` lists, Track +
// first, each tracked field shaped by its kind and filled by what it is
// now, the untracked ones dim after them.
//
// The logic is pure (`$lib/fields.ts`) and node loads it as it is; the
// rest is the built app, driven through the bottom bar's prompt — the
// way a person types — and asserted as computed colours, radii and
// roles, never as "different from before".

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { inkOver, lookOf, pressArgv, shapeFor, buttonsFor } from '../src/lib/fields.ts';
import { startApp, stopApp, withApp } from './support/app.mjs';

const spec = (field, rest = {}) => ({ field, ...rest });

describe('the logic', () => {
	test('`occur` is a placeholder, so the log decides; start / switch / add are declarations', () => {
		assert.deepEqual(shapeFor(spec('sleep', { default_verb: 'occur' }), [true, false]), { kind: 'state' });
		assert.deepEqual(shapeFor(spec('coffee', { default_verb: 'occur' })), { kind: 'point' });
		assert.deepEqual(shapeFor(spec('work', { default_verb: 'start' })), { kind: 'state' });
		assert.deepEqual(shapeFor(spec('note', { default_verb: 'add' })), { kind: 'record' });
		assert.deepEqual(shapeFor(undefined, ['b', 'a']), { kind: 'states', states: ['a', 'b'] });
	});

	test('tracked fields first, each group in the CLI order; `now` says what is on', () => {
		const widgets = buttonsFor(
			[
				{ field: 'a', spec: null, states: [] },
				{ field: 'b', spec: spec('b', { default_verb: 'start' }), states: [] },
				{ field: 'c', spec: spec('c', { default_verb: 'switch' }), states: ['x'] }
			],
			[{ field: 'b', state: true }, { field: 'c', state: 'x' }, { field: 'a', state: false }]
		);
		assert.deepEqual(widgets.map((w) => [w.field, w.tracked, w.current]), [
			['b', true, true],
			['c', true, 'x'],
			['a', false, undefined]
		]);
	});

	test("the ink is the CLI's YIQ rule", () => {
		// crates/datum-commands/src/render/colors.rs `contrast_fg`, and datum-parity's tests of it.
		assert.equal(inkOver('#ffff00'), '#000000');
		assert.equal(inkOver('#000080'), '#ffffff');
		assert.equal(inkOver('#808080'), '#ffffff', 'brightness 128 is not above 128');
		assert.equal(inkOver('#818181'), '#000000');
	});

	test('a point is filled; a block fills while on; a switch takes its state colour inside the field outline', () => {
		const w = (shape, rest = {}) => ({ field: 'f', shape, tracked: true, stateColors: {}, ...rest });
		assert.deepEqual(lookOf(w({ kind: 'point' }, { color: '#ffff00' })), { fill: '#ffff00', line: '#ffff00', text: '#000000', label: 'f' });
		assert.deepEqual(lookOf(w({ kind: 'point' }, { color: '#ffff00', current: 'x' })).fill, '#ffff00', 'never coloured by a state');
		assert.deepEqual(lookOf(w({ kind: 'state' }, { color: '#0000ff' })), { fill: null, line: '#0000ff', text: '#0000ff', label: 'f' });
		assert.deepEqual(lookOf(w({ kind: 'state' }, { color: '#0000ff', current: true })).fill, '#0000ff');
		const mood = w({ kind: 'states', states: ['good', 'meh'] }, { color: '#800080', stateColors: { good: '#00ff00' }, current: 'good' });
		assert.deepEqual(lookOf(mood), { fill: '#00ff00', line: '#800080', text: '#800080', label: 'f' });
		assert.deepEqual(lookOf(mood, 'state'), { fill: '#00ff00', line: '#800080', text: '#000000', label: 'good' });
		assert.equal(lookOf({ ...mood, current: 'meh' }).fill, '#800080', 'a state without a colour takes the field colour');
		assert.deepEqual(lookOf(w({ kind: 'record' })), { fill: null, line: '#7c7cff', text: '#7c7cff', label: 'f' }, 'no colour: the neutral');
	});

	test('a block that is on ends; a record is `add`', () => {
		const w = (shape, current) => ({ field: 'f', shape, tracked: true, stateColors: {}, current });
		assert.deepEqual(pressArgv(w({ kind: 'state' })), ['start', 'f']);
		assert.deepEqual(pressArgv(w({ kind: 'state' }, true), { minutesAgo: 5 }), ['end', 'f', '-t', '-5']);
		assert.deepEqual(pressArgv(w({ kind: 'record' })), ['add', 'f']);
	});
});

const FIXTURE = [
	'spec set coffee --default-verb occur --color orange',
	'spec set work --default-verb start --color blue',
	// `--state good=green` would name a state `good=green`: the colours are `--state-color`.
	'spec set mood --default-verb switch --state good --state bad --state-color good=green --state-color bad=red --color purple',
	'spec set note --default-verb add',
	'occur untracked'
];

const ORANGE = 'rgb(255, 165, 0)';
const BLUE = 'rgb(0, 0, 255)';
const PURPLE = 'rgb(128, 0, 128)';
const GREEN = 'rgb(0, 128, 0)';
const CLEAR = 'rgba(0, 0, 0, 0)';

/** Type `line` into the bottom bar's prompt and wait for the REPL to echo it. */
async function type(page, line) {
	const before = await page.locator('.repl .echo').count();
	const input = page.locator('.prompt-bar input');
	await input.fill(line);
	await input.press('Enter');
	await page.waitForFunction((n) => document.querySelectorAll('.repl .echo').length > n, before);
}

/** What the widget's entry for `field` looks like, as computed. */
const look = (page, field) =>
	page.locator(`.bar [data-field="${field}"]`).evaluate((el) => {
		const cs = getComputedStyle(el);
		return {
			tag: el.tagName,
			role: el.getAttribute('role'),
			bg: cs.backgroundColor,
			border: cs.borderTopColor,
			color: cs.color,
			radius: parseFloat(cs.borderTopLeftRadius),
			underline: cs.textDecorationLine,
			underlineColor: cs.textDecorationColor
		};
	});

/** Wait until `field`'s computed `prop` is `value`. */
const until = (page, field, prop, value, timeout = 2000) =>
	page.waitForFunction(
		([f, p, v]) => {
			const el = document.querySelector(`.bar [data-field="${f}"]`);
			return el && getComputedStyle(el)[p] === v;
		},
		[field, prop, value],
		{ timeout }
	);

describe('in the app', () => {
	before(startApp);
	after(stopApp);

	test('Track + first and largest, then each tracked field by its kind, the untracked dim after', async () => {
		await withApp(async ({ page }) => {
			for (const line of FIXTURE) await type(page, line);
			await page.locator('.bar [data-field="untracked"]').waitFor();

			const order = await page.locator('.bar').evaluate((bar) =>
				[...bar.querySelectorAll('button.track, [data-field]')].map((el) => el.dataset.field ?? 'Track +')
			);
			assert.deepEqual(order, ['Track +', 'coffee', 'mood', 'note', 'work', 'untracked']);
			const track = await page.locator('.bar button.track').evaluate((el) => ({
				height: el.getBoundingClientRect().height,
				bg: getComputedStyle(el).backgroundColor,
				text: el.textContent.trim()
			}));
			assert.equal(track.text, 'Track +');
			assert.equal(track.bg, 'rgb(124, 124, 255)');
			const heights = await page.locator('.bar button.widget').evaluateAll((all) => all.map((el) => el.getBoundingClientRect().height));
			assert.ok(heights.every((h) => track.height > h), `Track + is ${track.height}px, the fields ${heights}`);

			const coffee = await look(page, 'coffee');
			assert.equal(coffee.bg, ORANGE);
			assert.ok(coffee.radius >= 12, `coffee's radius is ${coffee.radius}`);
			assert.equal(coffee.color, 'rgb(0, 0, 0)', "orange's ink, by the CLI's rule");

			const work = await look(page, 'work');
			assert.deepEqual([work.bg, work.border, work.color], [CLEAR, BLUE, BLUE]);
			assert.ok(work.radius <= 4, `work's radius is ${work.radius}`);

			assert.equal((await look(page, 'mood')).border, PURPLE);

			const note = await look(page, 'note');
			assert.notEqual(note.role, null);
			assert.notEqual(note.role, 'button');
			assert.equal(note.underline, 'underline');
			assert.equal(note.underlineColor, 'rgb(124, 124, 255)', 'no colour: the neutral');

			const untracked = page.locator('.bar .untracked[data-field="untracked"]');
			assert.equal(await untracked.locator('button').getAttribute('aria-label'), 'track untracked');
			await untracked.locator('button').click();
			assert.equal(await page.locator('form.add input').first().inputValue(), 'untracked');
		});
	});

	test('a block fills while it is on, a switch takes its state colour inside the field outline', async () => {
		await withApp(async ({ page }) => {
			for (const line of FIXTURE) await type(page, line);
			await type(page, 'start work');
			await until(page, 'work', 'backgroundColor', BLUE);
			await type(page, 'end work');
			await until(page, 'work', 'backgroundColor', CLEAR);
			assert.equal((await look(page, 'work')).border, BLUE);

			await type(page, 'switch mood good');
			await until(page, 'mood', 'backgroundColor', GREEN);
			assert.equal((await look(page, 'mood')).border, PURPLE);
		});
	});

	test('a press runs what its shape means, with the bar’s pills after it', async () => {
		await withApp(async (app) => {
			const { page } = app;
			for (const line of FIXTURE) await type(page, line);
			await page.locator('.bar [data-field="coffee"]').click();
			await page.waitForFunction(() => document.querySelector('.prompt-bar input').value === 'occur coffee');
			await page.waitForFunction(() => document.querySelector('.prompt-bar .receipt').textContent.startsWith('CREATE '));
			await app.idle();
			assert.equal((await app.tailRows()).rows.at(-1).field, 'coffee');

			await type(page, 'start work');
			await until(page, 'work', 'backgroundColor', BLUE);
			await page.locator('.bar [data-field="work"]').click();
			await until(page, 'work', 'backgroundColor', CLEAR);
			assert.equal((await app.echoes()).at(-1), 'end work');
			await app.idle();
			const ended = (await app.tailRows()).rows.at(-1);
			assert.deepEqual([ended.field, ended.state], ['work', false]);

			const seen = await app.spyOnRunner();
			await page.evaluate(() => window.datum.bar.setPills([{ id: 'when', text: '-t 09:00', kind: 'time' }]));
			await page.locator('.bar [data-field="coffee"]').click();
			await page.waitForFunction(() => window.__datumSeen.some((a) => a[0] === 'occur'));
			assert.deepEqual((await seen()).find((a) => a[0] === 'occur'), ['occur', 'coffee', '-t', '09:00']);
		});
	});

	test('a record fills the prompt with its line, focused, and runs nothing', async () => {
		await withApp(async (app) => {
			const { page } = app;
			for (const line of FIXTURE) await type(page, line);
			const echoes = (await app.echoes()).length;
			await page.locator('.bar [data-field="note"]').click();
			await page.waitForFunction(() => {
				const input = document.querySelector('.prompt-bar input');
				return input.value === 'add note ' && document.activeElement === input;
			});
			assert.equal((await app.echoes()).length, echoes);
		});
	});

	test('Track + is there on an empty database, the hint one line beside it', async () => {
		await withApp(async ({ page }) => {
			const track = await page.locator('.bar button.track').boundingBox();
			const hint = await page.locator('.bar .hint.empty').boundingBox();
			assert.ok(Math.abs(hint.y + hint.height / 2 - (track.y + track.height / 2)) < 4, 'the hint is not beside Track +');
			assert.ok(hint.height < 24, `the hint is ${hint.height}px tall`);
		});
	});

	for (const viewport of [
		{ width: 390, height: 844 },
		{ width: 320, height: 664 },
		{ width: 1280, height: 800 }
	]) {
		test(`at ${viewport.width}x${viewport.height} the widget wraps, every button ≥ 24px, Track + first in the Tab order`, async () => {
			await withApp(async ({ page }) => {
				for (const line of [...FIXTURE, 'occur tea', 'occur water', 'occur run']) await type(page, line);
				await page.locator('.bar [data-field="run"]').waitFor();
				const bar = await page.locator('.bar').evaluate((el) => ({
					width: el.scrollWidth <= el.clientWidth,
					rows: new Set([...el.children].map((c) => Math.round(c.getBoundingClientRect().top))).size,
					heights: [...el.querySelectorAll('button')].map((b) => b.getBoundingClientRect().height)
				}));
				assert.ok(bar.width, 'the widget scrolls sideways rather than wrapping');
				if (viewport.width < 500) assert.ok(bar.rows > 1, 'the widget did not wrap');
				for (const h of bar.heights) assert.ok(h >= 24, `a button is ${h}px tall`);

				const first = await page.locator('.bar').evaluate((el) => {
					const focusable = [...el.querySelectorAll('button, input, [tabindex]')].filter((b) => !b.disabled && b.tabIndex >= 0);
					return focusable[0]?.textContent.trim();
				});
				assert.equal(first, 'Track +');
			}, { viewport });
		});
	}
});
