// A gesture hears its lift wherever the pointer lifts (docs/web-conventions.md
// #31, da-ukyq4): a lift away from the element that heard the press need not
// reach it, and a gesture that missed one stays pressed (da-yn6my's fingers,
// #1769's close drag). So every `onpointerup=` in web/src is on the window,
// beside a press that captures the pointer, or beside a `pointerleave` that
// clears a hold timer, and beside an `onpointercancel=` too. So is a lift an
// action adds to an element, `node.addEventListener('pointerup', …)` (da-c4u45):
// its file captures, or the same receiver hears `pointerleave`, and
// `pointercancel` either way. A bare `addEventListener` is the window's.
//
// It launches no browser and reads no build: a scan of text, in milliseconds.

import test from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';

const src = new URL('../src/', import.meta.url);
const sources = readdirSync(src, { recursive: true, encoding: 'utf8' })
	.filter((f) => f.endsWith('.svelte') || f.endsWith('.ts'))
	.sort()
	.map((f) => ({ f, text: readFileSync(new URL(f, src), 'utf8').replace(/<!--[\s\S]*?-->/g, (c) => c.replace(/[^\n]/g, '')) }));
const files = sources.filter(({ f }) => f.endsWith('.svelte'));

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

const line = (text, at) => text.slice(0, at).split('\n').length;

/**
 * Each `<receiver>.addEventListener('pointerup', …)` in `text`, the receiver no window. It captures
 * when a pointer handler the same receiver adds does: the call's own line, or the named function's body.
 */
function actionLifts(f, text) {
	const adds = (r, type) =>
		new RegExp(`(?<![\\w$.])${r.replace(/[.$]/g, '\\$&')}\\.addEventListener\\(\\s*['"]${type}['"][^\\n]*`, 'g');
	const on = (r, type) => adds(r, type).test(text);
	const captures = (r) =>
		[...text.matchAll(adds(r, 'pointer\\w+'))].some(([call]) => {
			const name = /,\s*([A-Za-z_$][\w$]*)\s*[,)]/.exec(call)?.[1];
			const body = name && new RegExp(`^([\\t ]*)function ${name}\\([\\s\\S]*?\\n\\1}`, 'm').exec(text)?.[0];
			return call.includes('setPointerCapture') || !!body?.includes('setPointerCapture');
		});
	return [...text.matchAll(/(?<![\w$.])([A-Za-z_$][\w$.]*)\.addEventListener\(\s*['"]pointerup['"]/g)]
		.filter(([, r]) => !['window', 'document', 'globalThis', 'self'].includes(r))
		.map((m) => ({
			where: `${f}:${line(text, m.index)} ${m[1]}`,
			how: captures(m[1]) ? 'capture' : on(m[1], 'pointerleave') ? 'leave' : null,
			cancel: on(m[1], 'pointercancel')
		}));
}

const actions = sources.flatMap(({ f, text }) => actionLifts(f, text));

const lifts = actions.concat(files.flatMap(({ f, text }) =>
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
		return { where: `${f}:${line(text, m.index)} <${name}>`, how, cancel: /\bonpointercancel=/.test(attrs) };
	})
));

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

test("an action's pointerup on an element is read, and held to the idioms", () => {
	const read = actions.map((l) => l.where.split(':')[0]);
	for (const f of ['lib/grip.ts', 'lib/hold.ts']) assert.ok(read.includes(f), `read no lift in ${f}: ${read}`);
	// hold.ts is a hold: without its pointerleave, a lift elsewhere would leave the timer running.
	const hold = sources.find(({ f }) => f === 'lib/hold.ts').text;
	const leaveless = hold.replace(/^.*addEventListener\('pointerleave'.*\n/m, '');
	assert.notEqual(leaveless, hold);
	assert.deepEqual(actionLifts('hold.ts', leaveless).map((l) => l.how), [null]);
	const cancelless = hold.replace(/^.*addEventListener\('pointercancel'.*\n/m, '');
	assert.deepEqual(actionLifts('hold.ts', cancelless).map((l) => l.cancel), [false]);
	// grip.ts captures in a handler its node adds; a second receiver in the file that does not is not excused by it.
	const grip = sources.find(({ f }) => f === 'lib/grip.ts').text;
	assert.deepEqual(actionLifts('grip.ts', grip).map((l) => l.how), ['capture']);
	const other = `${grip}\nother.addEventListener('pointerup', onPointerCancel);\nother.addEventListener('pointercancel', onPointerCancel);\n`;
	assert.deepEqual(actionLifts('grip.ts', other).map((l) => l.how), ['capture', null]);
});
