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
	test('a spec\'s verb wins, whatever the log says; a spec-less field falls back to the log', () => {
		// occur is a declaration too: a field logged with `true`/`false` under
		// an occur spec still stays a point (da-u8n7), not a block.
		assert.deepEqual(shapeFor(spec('sleep', { default_verb: 'occur' }), [true, false]), { kind: 'point' });
		assert.deepEqual(shapeFor(spec('coffee', { default_verb: 'occur' })), { kind: 'point' });
		assert.deepEqual(shapeFor(spec('work', { default_verb: 'start' })), { kind: 'state' });
		assert.deepEqual(shapeFor(spec('note', { default_verb: 'add' })), { kind: 'record' });
		// No spec: the log is the only evidence. A boolean log is a block...
		assert.deepEqual(shapeFor(undefined, [true, false]), { kind: 'state' });
		// ...and named states are a menu.
		assert.deepEqual(shapeFor(undefined, ['b', 'a']), { kind: 'states', states: ['a', 'b'] });
	});

	test('every field in the CLI order, a spec or not; `ignored` is the CLI\'s; `now` says what is on', () => {
		const widgets = buttonsFor(
			[
				{ field: 'a', spec: null, states: [true], color: '#111111', state_colors: {}, ignored: false },
				{ field: 'b', spec: spec('b', { default_verb: 'start' }), states: [], color: '#222222', state_colors: {}, ignored: true },
				{ field: 'c', spec: spec('c', { default_verb: 'switch' }), states: ['x'], color: '#333333', state_colors: { x: '#444444' }, ignored: false }
			],
			[{ field: 'b', state: true }, { field: 'c', state: 'x' }, { field: 'a', state: false }]
		);
		assert.deepEqual(widgets.map((w) => [w.field, w.shape.kind, w.ignored, w.current]), [
			['a', 'state', false, undefined],
			['b', 'state', true, true],
			['c', 'states', false, 'x']
		]);
	});

	test("the ink is the CLI's YIQ rule", () => {
		// crates/datum-commands/src/render/colors.rs `contrast_fg`, and datum-parity's tests of it.
		assert.equal(inkOver('#ffff00'), '#000000');
		assert.equal(inkOver('#000080'), '#ffffff');
		assert.equal(inkOver('#808080'), '#ffffff', 'brightness 128 is not above 128');
		assert.equal(inkOver('#818181'), '#000000');
	});

	test('a point is filled; a block fills while on; a switch takes its state colour inside the field outline; a label is always the fill’s ink', () => {
		const w = (shape, rest = {}) => ({ field: 'f', shape, tracked: true, color: '#7c7cff', stateColors: {}, ...rest });
		assert.deepEqual(lookOf(w({ kind: 'point' }, { color: '#ffff00' })), { fill: '#ffff00', line: '#ffff00', text: '#000000', label: 'f' });
		assert.deepEqual(lookOf(w({ kind: 'point' }, { color: '#ffff00', current: 'x' })).fill, '#ffff00', 'never coloured by a state');
		assert.deepEqual(lookOf(w({ kind: 'state' }, { color: '#0000ff' })), { fill: null, line: '#0000ff', text: '#0000ff', label: 'f' });
		assert.deepEqual(lookOf(w({ kind: 'state' }, { color: '#0000ff', current: true })).fill, '#0000ff');
		const mood = w({ kind: 'states', states: ['good', 'meh'] }, { color: '#800080', stateColors: { good: '#00ff00' }, current: 'good' });
		assert.deepEqual(lookOf(mood), { fill: '#00ff00', line: '#800080', text: '#000000', label: 'f' });
		assert.deepEqual(lookOf(mood, 'state'), { fill: '#00ff00', line: '#800080', text: '#000000', label: 'good' }, 'the variant picks the word, not the ink');
		assert.equal(lookOf({ ...mood, current: 'meh' }).fill, '#800080', 'a state without a colour takes the field colour');
		assert.deepEqual(lookOf(w({ kind: 'record' })), { fill: null, line: '#7c7cff', text: '#7c7cff', label: 'f' }, 'a record is outlined and inked in its (resolved) field colour');

		// da-qnvh: the text was the field's colour and the fill falls back to
		// the field's colour, so the two collapsed — the name of the field
		// printed in exactly its own background. Every colour it could be.
		// `color` is always a resolved `#rrggbb` in production (da-yiyj) — no
		// longer an "uncoloured" case for `lookOf` itself to handle.
		for (const color of ['#7c7cff', '#e8820c', '#00ff00', '#000080']) {
			const painted = lookOf(w({ kind: 'states', states: ['on'] }, { color, current: 'on' }));
			assert.notEqual(painted.text, painted.fill, `a switch in ${color} is painted on itself`);
		}
	});

	test('a block that is on ends; a record is `add`', () => {
		const w = (shape, current) => ({ field: 'f', shape, tracked: true, stateColors: {}, current });
		assert.deepEqual(pressArgv(w({ kind: 'state' })), ['start', 'f']);
		assert.deepEqual(pressArgv(w({ kind: 'state' }, true), { minutesAgo: 5 }), ['end', 'f', '-t', '-5']);
		assert.deepEqual(pressArgv(w({ kind: 'record' })), ['add', 'f']);
	});

	test('an occur spec presses `occur`, even once it has been logged with a boolean state (da-u8n7)', () => {
		const shape = shapeFor(spec('jumps', { default_verb: 'occur' }), [true]);
		assert.deepEqual(shape, { kind: 'point' });
		assert.deepEqual(pressArgv({ field: 'jumps', shape, tracked: true, stateColors: {} }), ['occur', 'jumps']);
	});
});

// Hex, not CSS colour names: `crates/datum-commands/src/render/colors.rs`'s
// `resolve_rgb` only understands `#rrggbb` (or a legacy ANSI-256 number) —
// the same model both the CLI and the widget now share (da-yiyj) — so a name
// like `orange` would silently fall through to `md5(field)`.
const FIXTURE = [
	'spec set coffee --default-verb occur --color #ffa500',
	'spec set work --default-verb start --color #0000ff',
	// `--state good=green` would name a state `good=green`: the colours are `--state-color`.
	'spec set mood --default-verb switch --state good --state bad --state-color good=#008000 --state-color bad=#ff0000 --color #800080',
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

/**
 * Every label in the widget, with the contrast its ink actually has.
 *
 * Contrast, not colour: the bug this measures (da-qnvh) was a chip drawn
 * in two colours that were the same colour, and every assertion the suite
 * had said the right name for each of them. Only the ratio between them
 * said anything was wrong.
 *
 * `behind` walks UP for the first ancestor that paints. A chip's own
 * background is `transparent` whenever it is outlined, and reading the
 * element alone measures the label against nothing and calls it fine —
 * which is most of how this was missed.
 */
const contrasts = (page) =>
	page.locator('.bar').evaluate((bar) => {
		const parse = (color) => (color.match(/[\d.]+/g) ?? []).map(Number);
		const channel = (c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
		const luminance = ([r, g, b]) =>
			0.2126 * channel(r / 255) + 0.7152 * channel(g / 255) + 0.0722 * channel(b / 255);
		const behind = (el) => {
			for (let node = el; node; node = node.parentElement) {
				const bg = parse(getComputedStyle(node).backgroundColor);
				if (bg[3] === undefined || bg[3] > 0) return bg.slice(0, 3);
			}
			return [0, 0, 0];
		};
		const ratio = (fg, bg) => {
			const [light, dark] = [luminance(fg), luminance(bg)].sort((a, b) => b - a);
			return (light + 0.05) / (dark + 0.05);
		};
		return [...bar.querySelectorAll('button.track, [data-field]')].map((el) => {
			const fg = parse(getComputedStyle(el).color).slice(0, 3);
			const bg = behind(el);
			return {
				label: el.dataset.field ?? el.textContent.trim(),
				fg: `rgb(${fg})`,
				bg: `rgb(${bg})`,
				contrast: Math.round(ratio(fg, bg) * 100) / 100
			};
		});
	});

/** The measurements as the bead writes them, for a failure to print. */
const table = (rows) =>
	rows.map((r) => `  ${r.label.padEnd(10)} ${r.fg.padEnd(20)} ${r.bg.padEnd(20)} ${r.contrast}`).join('\n');

/** Wait until the bar's `field` button has computed `prop` of `value`. */
const untilFieldStyle = (page, field, prop, value) =>
	page.waitForFunction(
		([f, p, v]) => {
			const el = document.querySelector(`.bar [data-field="${f}"]`);
			return el && getComputedStyle(el)[p] === v;
		},
		[field, prop, value]
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
			// No spec colour: resolved server-side to md5("note") (da-yiyj), not
			// the app's accent — `crates/datum-commands/src/render/colors.rs`.
			assert.equal(note.underlineColor, 'rgb(170, 214, 83)');

			// No spec, logged once: a point, a button like any other.
			assert.equal((await look(page, 'untracked')).tag, 'BUTTON');
		});
	});

	// da-d08c, the three cases as measured: whether a field has a spec says
	// nothing about whether it gets a button — its track/ignore status does.
	test('track tea and occur walk are buttons; an ignored run with a spec is not, and Track + un-ignores it', async () => {
		await withApp(async (app) => {
			const { page } = app;
			for (const line of ['track tea', 'occur walk', 'spec set run --default-verb occur', 'ignore run']) await type(page, line);
			await page.locator('.bar button[data-field="walk"]').waitFor();
			const drawn = () => page.locator('.bar [data-field]').evaluateAll((all) => all.map((el) => `${el.tagName}:${el.dataset.field}`));
			assert.deepEqual(await drawn(), ['BUTTON:tea', 'BUTTON:walk']);

			await page.locator('.bar button.track').click();
			await page.locator('.bar [data-ignored="run"]').click();
			await page.locator('.bar button[data-field="run"]').waitFor();
			assert.equal((await app.echoes()).at(-1), 'track run');
			assert.deepEqual(await drawn(), ['BUTTON:run', 'BUTTON:tea', 'BUTTON:walk']);
		});
	});

	test('a field that arrives by `db restore -` is a button after that command, with no reload', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await type(page, 'occur walk');
			await page.locator('.bar button[data-field="walk"]').waitFor();
			// The backup's own header and walk's own row, re-keyed as a field
			// the page has never seen — what an import from elsewhere brings.
			const [header, ...rows] = (await app.run(['db', 'backup', '-'])).stdout.trim().split('\n');
			const walk = JSON.parse(rows.find((l) => l.includes('"walk"')));
			const swim = { ...walk, hid: 'swim0000', slug: walk.slug.replace('walk', 'swim'), field: 'swim', content_hash: '0'.repeat(64), raw_input: 'occur swim' };
			// Typed, as a person would: the page's own command, so its own beat refreshes the bar.
			const [chooser] = await Promise.all([page.waitForEvent('filechooser'), type(page, 'db restore -')]);
			await chooser.setFiles({ name: 'swim.jsonl', mimeType: 'application/jsonl', buffer: Buffer.from(`${header}\n${JSON.stringify(swim)}\n`) });
			await page.locator('.bar button[data-field="swim"]').waitFor();
			assert.equal((await app.entries()).some((e) => e.field === 'swim'), true);
		});
	});

	test('a block fills while it is on, a switch takes its state colour inside the field outline', async () => {
		await withApp(async ({ page }) => {
			for (const line of FIXTURE) await type(page, line);
			await type(page, 'start work');
			await untilFieldStyle(page, 'work', 'backgroundColor', BLUE);
			await type(page, 'end work');
			await untilFieldStyle(page, 'work', 'backgroundColor', CLEAR);
			assert.equal((await look(page, 'work')).border, BLUE);

			await type(page, 'switch mood good');
			await untilFieldStyle(page, 'mood', 'backgroundColor', GREEN);
			assert.equal((await look(page, 'mood')).border, PURPLE);
		});
	});

	test('a press runs what its shape means, with the bar’s pills after it', async () => {
		await withApp(async (app) => {
			const { page } = app;
			for (const line of FIXTURE) await type(page, line);
			await page.locator('.bar [data-field="coffee"]').click();
			// Ghosted beside the prompt, not typed into it (da-jprg).
			await page.waitForFunction(
				() => document.querySelector('.prompt-bar .ghost')?.textContent === 'occur coffee'
			);
			await page.waitForFunction(() => document.querySelector('.prompt-bar .receipt').textContent.startsWith('CREATE '));
			await app.idle();
			assert.equal((await app.tailRows()).rows.at(-1).field, 'coffee');

			await type(page, 'start work');
			await untilFieldStyle(page, 'work', 'backgroundColor', BLUE);
			await page.locator('.bar [data-field="work"]').click();
			await untilFieldStyle(page, 'work', 'backgroundColor', CLEAR);
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

	// The four kinds, with NO colours set except the one state colour that
	// tells a coloured state from an uncoloured one. The default case is
	// the broken one: a field tracked today has no `state_colors`, and a
	// switch in a state then filled itself with the field's colour and
	// wrote the label in that same colour (da-qnvh).
	const PLAIN = [
		'spec set pushups --default-verb occur',
		'spec set work --default-verb start',
		'spec set mood --default-verb switch --state good --state bad',
		'spec set energy --default-verb switch --state high --state low --state-color high=#008000',
		'start work',
		'switch mood good',
		'switch energy high',
		'occur untracked'
	];

	test('every field chip clears 4.5:1 against what is behind it, the uncoloured switch included', async () => {
		await withApp(async ({ page }) => {
			for (const line of PLAIN) await type(page, line);
			await page.locator('.bar [data-field="untracked"]').waitFor();
			// No spec colour on `work`: resolved server-side to md5("work"),
			// not the app's accent (da-yiyj).
			await untilFieldStyle(page, 'work', 'backgroundColor', 'rgb(103, 233, 44)');

			const on = await contrasts(page);
			assert.deepEqual(
				on.map((r) => r.label),
				['Track +', 'energy', 'mood', 'pushups', 'untracked', 'work'],
				'all four kinds have to be in the measurement'
			);
			for (const row of on) {
				assert.ok(row.contrast >= 4.5, `${row.label} is at ${row.contrast}:1\n${table(on)}`);
			}

			// And outlined: a block that is off is the one chip whose fill is
			// on nothing, so its label is measured against the pane.
			await type(page, 'end work');
			await untilFieldStyle(page, 'work', 'backgroundColor', CLEAR);
			const off = await contrasts(page);
			for (const row of off.filter((r) => !r.dim)) {
				assert.ok(row.contrast >= 4.5, `${row.label} is at ${row.contrast}:1 with work off\n${table(off)}`);
			}
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

// The pane the widget lives in keeps the height its layout gives it
// (da-mbz2). It used to size itself to its content — the only kind that
// did — so every field tracked pushed the rest of the page up a little,
// and da-d08c, which draws a button for EVERY field, would have made that
// a shove. Now the box is the split's to decide and the bar scrolls
// inside it.
describe("the fields pane's box", () => {
	before(startApp);
	after(stopApp);

	/** The pane's border box, as the reader sees it. */
	const boxOf = (page) =>
		page.locator('section[data-pane="bar"]').evaluate((el) => {
			const { width, height } = el.getBoundingClientRect();
			return { width: Math.round(width), height: Math.round(height) };
		});

	const TEN = ['alfa', 'bravo', 'charlie', 'delta', 'echo', 'foxtrot', 'golf', 'hotel', 'india', 'juliett'];

	test('tracking ten more fields does not change it', async () => {
		await withApp(async ({ page }) => {
			await type(page, 'spec set coffee --default-verb occur');
			await page.locator('.bar [data-field="coffee"]').waitFor();
			const before = await boxOf(page);

			for (const field of TEN) await type(page, `spec set ${field} --default-verb occur`);
			await page.locator('.bar [data-field="juliett"]').waitFor();

			assert.deepEqual(await boxOf(page), before, 'the pane grew with its buttons');
		});
	});

	// A phone, where the pane is short enough that a handful of fields
	// overflows it — the shape da-d08c makes ordinary.
	test('a button past the bottom of the pane is scrolled to and pressed', async () => {
		await withApp(async ({ page }) => {
			for (const field of TEN) await type(page, `spec set ${field} --default-verb occur`);
			await page.locator('.bar [data-field="juliett"]').waitFor();

			const body = page.locator('section[data-pane="bar"] .body');
			assert.ok(
				await body.evaluate((el) => el.scrollHeight > el.clientHeight + 1),
				'the bar fits its pane — there is nothing this test can prove'
			);

			const echoes = await page.locator('.repl .echo').count();
			// Playwright refuses to click what is out of view or covered, and
			// scrolls its scrollable ancestor to reach it: the press is the
			// assertion that the button is reachable, not clipped away.
			await page.locator('.bar button.widget').last().click();
			await page.waitForFunction((n) => document.querySelectorAll('.repl .echo').length > n, echoes);
			assert.ok(await body.evaluate((el) => el.scrollTop > 0), 'the pane never scrolled to reach it');
		}, { viewport: { width: 390, height: 640 } });
	});
});
