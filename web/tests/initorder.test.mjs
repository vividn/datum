// No component reads a top-level binding before its declaration during init
// (da-1oi8w). The check is tests/support/init-order.mjs; this runs it over the
// tree, on #1445's fault as it stood (a copy of 486aeb63a's file), and on each
// thing the check tells apart.

import test from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { findings, say } from './support/init-order.mjs';

const src = join(import.meta.dirname, '..', 'src');
const read = (body) => findings(`<script lang="ts">\n${body}\n</script>\n`).map((f) => f.name);

test('no component in src reaches a binding before its declaration during init', () => {
	const files = readdirSync(src, { recursive: true }).filter((f) => f.endsWith('.svelte'));
	assert.ok(files.length > 50, `read ${files.length} components`);
	const all = files.flatMap((f) => findings(readFileSync(join(src, f), 'utf8'), f).map((x) => say(f, x)));
	assert.deepEqual(all, []);
});

test("#1445's NowviewWidget fails on HANDLE_PX, through the effect, flow and clearFar", () => {
	const old = readFileSync(join(import.meta.dirname, 'fixtures', 'NowviewWidget-486aeb63a.svelte.txt'), 'utf8');
	const found = findings(old).filter((f) => f.name === 'HANDLE_PX');
	assert.equal(found.length, 1);
	assert.equal(found[0].declaredAt, 1567);
	assert.match(found[0].path.join(' '), /\$effect\.pre.* flow .* clearFar /);
});

for (const [what, body, names] of [
	['a $state initializer', 'let a = $state(b); const b = 1;', ['b']],
	['a top-level call', 'f(); function f() { return b; } const b = 1;', ['b']],
	['an $effect.pre body', '$effect.pre(() => { b; }); const b = 1;', ['b']],
	['a function handed to untrack', '$effect.pre(() => { untrack(f); }); function f() { b; } const b = 1;', ['b']],
	['a read of a $derived, its initializer', 'const d = $derived(b); $effect.pre(() => { d; }); const b = 1;', ['b']],
	['a read of a $derived.by, its function', 'const d = $derived.by(() => b); const e = d; const b = 1;', ['b']],
	['a function declared inside the one called', 'f(); function f() { const g = () => b; g(); } const b = 1;', ['b']],
	['a call of a const function before it', 'g(); const g = () => {};', ['g']],
	['a destructured default', 'let { x = b } = $props(); const b = 1;', ['b']],
	['a write', 'f(); function f() { b = 2; } let b = 1;', ['b']],
	['a guard on what is not known to be falsy', 'let a = $state(1); $effect.pre(() => { if (a) b; }); const b = 1;', ['b']],
	['a guard once its let has been written', 'let a = $state(); a = 1; $effect.pre(() => { if (a) b; }); const b = 1;', ['b']],
	['what precedes an await', 'f(); async function f() { b; await 0; } const b = 1;', ['b']],
	['an await that may not run', 'f(); async function f(c) { if (c) await 0; b; } const b = 1;', ['b']]
]) {
	test(`it finds ${what}`, () => assert.deepEqual([...new Set(read(body))], names));
}

for (const [what, body] of [
	['a $effect body', '$effect(() => { b; }); const b = 1;'],
	['a function nothing calls', 'function f() { b; } const b = 1;'],
	['a $derived nothing reads during init', 'const d = $derived(b); const b = 1;'],
	['a $derived.by nothing reads during init', 'const d = $derived.by(() => b); const b = 1;'],
	['a listener', 'el.addEventListener("x", () => b); const b = 1;'],
	['a timer naming a local function', 'setTimeout(f, 0); function f() { b; } const b = 1;'],
	['onMount', 'onMount(() => b); const b = 1;'],
	['a read after its declaration', 'const b = 1; $effect.pre(() => { b; });'],
	['a parameter of the same name', 'f(0); function f(b) { b; } const b = 1;'],
	['a local of the same name', 'f(); function f() { const b = 2; b; } const b = 1;'],
	['a key and a property of the same name', 'const o = { b: 1 }; o.b; const b = 1;'],
	['a type of the same name', 'const o = 1 as unknown as typeof b; const b = 1;'],
	['a guard on a let not bound yet', 'let el = $state(); $effect.pre(() => { if (!el) return; b; }); const b = 1;'],
	['an && on a let not bound yet', 'let el; $effect.pre(() => { el && b; }); const b = 1;'],
	['a ?. on a let not bound yet', 'let el; $effect.pre(() => { if (el?.a) b; }); const b = 1;'],
	['what follows a plain await', 'f(); async function f() { await 0; b; } const b = 1;']
]) {
	test(`it leaves ${what}`, () => assert.deepEqual(read(body), []));
}
