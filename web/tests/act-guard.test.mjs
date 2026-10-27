// The double outline is decided in ONE place (da-y0ocz): a button says what
// its tap does with `use:act={kind}` ($lib/act.ts), the action writes
// `data-act`, and the double is drawn on `[data-act='run']` and nowhere
// else. vividn: «like hooking up a button to do something automatically gives
// it a double line». This holds the source to it — so a component can no
// longer draw a double border by hand, or claim a kind without the action.
// `act-taps-*.test.mjs` hold the look to what a tap then does.
//
// It launches no browser and reads no build: a scan of text, in milliseconds.

import test, { describe } from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';

const src = new URL('../src/', import.meta.url);
const files = readdirSync(src, { recursive: true, encoding: 'utf8' })
	.filter((f) => /\.(svelte|ts|css)$/.test(f) && !f.includes('wasm'))
	.sort()
	.map((f) => ({ f, text: readFileSync(new URL(f, src), 'utf8') }));

const noComments = (css) => css.replace(/\/\*[\s\S]*?\*\//g, '');
const styleOf = ({ f, text }) =>
	f.endsWith('.css') ? text : [...text.matchAll(/<style[^>]*>([\s\S]*?)<\/style>/g)].map((m) => m[1]).join('\n');
const markupOf = ({ f, text }) =>
	f.endsWith('.svelte') ? text.replace(/<style[^>]*>[\s\S]*?<\/style>/g, '').replace(/<!--[\s\S]*?-->/g, '') : '';

/** Every innermost `selector { body }` in a stylesheet, `@media` unwrapped. */
const rules = (css) =>
	[...noComments(css).matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => ({ selector: m[1].trim(), body: m[2] }));

describe('the double outline is the action’s alone', () => {
	test('a double border is drawn only on [data-act=run]', () => {
		const doubles = files.flatMap((file) =>
			rules(styleOf(file))
				.filter((r) => /\bdouble\b/.test(r.body))
				.map((r) => ({ f: file.f, ...r }))
		);
		// The rules this guards are there to be found: a scan matching
		// nothing would pass for the wrong reason.
		assert.ok(doubles.length >= 8, `found only ${doubles.length} double-border rules`);
		const stray = doubles.filter((r) => !r.selector.split(',').every((s) => s.includes("[data-act='run']")));
		assert.deepEqual(
			stray.map((r) => `${r.f}: ${r.selector}`),
			[],
			'draw a double border under [data-act=\'run\'] and give the button use:act'
		);
	});

	// Its sibling mark (da-vteda): a tap that opens a modal wears a `›`, and so
	// does one that takes the person back to another pane (`data-away`, da-wz84y).
	test('a chevron is drawn only on [data-act=open] or [data-away]', () => {
		const chevrons = files.flatMap((file) =>
			rules(styleOf(file))
				.filter((r) => /content:[^;]*›/.test(r.body))
				.map((r) => ({ f: file.f, ...r }))
		);
		assert.ok(chevrons.length >= 1, 'found no chevron rule');
		const stray = chevrons.filter((r) => !r.selector.split(',').every((s) => s.includes("[data-act='open']") || s.trim() === '[data-away]::after'));
		assert.deepEqual(
			stray.map((r) => `${r.f}: ${r.selector}`),
			[],
			"draw a chevron only under [data-act='open'] and give the button use:act={'open'}"
		);
	});

	test('no component writes data-act, or the old act class, by hand', () => {
		const hand = files.flatMap((file) => {
			const markup = markupOf(file);
			return [
				...[...markup.matchAll(/\bdata-act\s*=/g)].map(() => 'data-act='),
				...[...markup.matchAll(/\bclass:act\b/g)].map(() => 'class:act'),
				...[...markup.matchAll(/\bclass="[^"]*\bact\b[^"]*"/g)].map((m) => m[0])
			].map((what) => `${file.f}: ${what}`);
		});
		assert.deepEqual(hand, [], 'say the kind with use:act={kind}');
		const uses = files.reduce((n, file) => n + [...markupOf(file).matchAll(/\buse:act=\{/g)].length, 0);
		assert.ok(uses >= 15, `found only ${uses} use:act`);
	});

	test('only $lib/act.ts sets the attribute from script', () => {
		const writers = files
			.filter((file) => file.f !== 'lib/act.ts' && /dataset\.act\b|['"]data-act['"]/.test(file.text))
			.map((file) => file.f);
		assert.deepEqual(writers, []);
		assert.ok(/dataset\.act\b/.test(files.find((file) => file.f === 'lib/act.ts').text));
	});

	// da-jozgv: every control that opens a place or a sheet wears the `›`.
	// The walks (act-taps-*) hold the controls they can reach to it; this
	// holds the ones they cannot — the documents' `+N` chips appear only
	// under an answer — by what the tap calls: `openPlace(`, or a Sheet's
	// `open = true`. Handlers named elsewhere are the walks' to catch.
	test('a button whose tap opens a place or a sheet says open', () => {
		const opens = /openPlace\(|\(open = true\)/;
		const openers = files.flatMap((file) =>
			[...markupOf(file).matchAll(/<button\b([^>]*?\bon:click=\{[^}]*\})[^>]*>/gs)]
				.filter((m) => opens.test(m[1]))
				.map((m) => ({ f: file.f, tag: m[0] }))
		);
		assert.ok(openers.length >= 3, `found only ${openers.length} openers`);
		assert.deepEqual(
			openers.filter((o) => !o.tag.includes("use:act={'open'}")).map((o) => `${o.f}: ${o.tag.replace(/\s+/g, ' ')}`),
			[],
			"a tap that opens a place or a sheet wears use:act={'open'}"
		);
	});
});
