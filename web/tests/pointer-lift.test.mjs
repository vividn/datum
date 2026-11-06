// A gesture hears its lift wherever the pointer lifts (docs/web-conventions.md
// #31, da-ukyq4): a lift away from the element that heard the press need not
// reach it, and a gesture that missed one stays pressed (da-yn6my's fingers,
// #1769's close drag). So every `onpointerup=` in web/src is on the window,
// beside a press that captures the pointer, or beside a `pointerleave` that
// clears a hold timer, and beside an `onpointercancel=` too.
//
// It launches no browser and reads no build: a scan of text, in milliseconds.

import test from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';

const src = new URL('../src/', import.meta.url);
const files = readdirSync(src, { recursive: true, encoding: 'utf8' })
	.filter((f) => f.endsWith('.svelte'))
	.sort()
	.map((f) => ({ f, text: readFileSync(new URL(f, src), 'utf8').replace(/<!--[\s\S]*?-->/g, (c) => c.replace(/[^\n]/g, '')) }));

/** The start tag holding the attribute at `at`: from its `<name` to the first `>` that is no `=>` and no ` > `. */
function tagAround(text, at) {
	const start = text.slice(0, at).search(/<[\w:-]+\s[^<]*$/);
	const name = /^<([\w:-]+)/.exec(text.slice(start))?.[1] ?? '';
	const end = text.slice(at).search(/(?<![= ])>|(?<= )>(?! )/);
	return { name, attrs: text.slice(start, at + end + 1) };
}

/** Whether the press `expr` captures: inline, or in a top-level `function` of the file it calls. */
function captures(text, expr) {
	if (expr.includes('setPointerCapture')) return true;
	return [...expr.matchAll(/\b([A-Za-z_$][\w$]*)\b/g)].some(([, name]) => {
		const body = new RegExp(`\\n\\tfunction ${name}\\([\\s\\S]*?\\n\\t}`).exec(text)?.[0] ?? '';
		return body.includes('setPointerCapture');
	});
}

const lifts = files.flatMap(({ f, text }) =>
	[...text.matchAll(/\bonpointerup=/g)].map((m) => {
		const { name, attrs } = tagAround(text, m.index);
		const press = /onpointerdown=\{([^\n]*)\}/.exec(attrs)?.[1] ?? '';
		const how =
			name === 'svelte:window' || name === 'svelte:document'
				? 'window'
				: captures(text, press)
					? 'capture'
					: /\bonpointerleave=/.test(attrs)
						? 'leave'
						: null;
		return { where: `${f}:${text.slice(0, m.index).split('\n').length} <${name}>`, how, cancel: /\bonpointercancel=/.test(attrs) };
	})
);

test('every onpointerup= hears its lift wherever the pointer lifts', () => {
	// The idioms are there to be found: a scan matching nothing would pass for the wrong reason.
	for (const how of ['window', 'capture', 'leave']) assert.ok(lifts.some((l) => l.how === how), `found no lift by ${how}`);
	assert.deepEqual(
		lifts.filter((l) => !l.how).map((l) => l.where),
		[],
		'capture the pointer at the press (setPointerCapture), listen on <svelte:window>, or clear a hold on pointerleave'
	);
});

test('every onpointerup= clears on pointercancel too', () => {
	assert.deepEqual(
		lifts.filter((l) => !l.cancel).map((l) => l.where),
		[],
		'give the element an onpointercancel= beside its onpointerup='
	);
});
