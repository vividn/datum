// §16's guard: a wait's cap is a hang-stop, not a speed assertion.
//
// §16 as it stood forbade EVERY `timeout:`, the one correct use included, so
// it could not be mechanised and so it was not enforced at all: a bare
// `{ timeout: 5000 }` landed in `daystrip.test.mjs` on 2026-09-15, three days
// after the cleanup (f1ef87e5) that removed nine of them, and sat there beside
// a legitimate 400 for a week (da-q0se9). §16 now names the second shape; this
// reads the suite's own source and holds it to the two.
//
// It launches no browser and reads no build — it is a scan of text, and runs
// in milliseconds.

import test, { describe } from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';

/** The suite's one tier (§16), playwright's own default. */
const TIER = 30000;

// How far past a cap we look for the `.catch(` that would excuse it. Argued,
// not tuned: one line is the real distance in both surviving shapes, and a
// dozen leaves room for a playwright call that closes over several lines
// first. A chained `await` longer than that would want a parser rather than a
// scan — and erring long only makes this STRICTER, never laxer, because an
// unrelated `throw` drawn into the window refuses the excuse. The failure mode
// is a red that a reader fixes, never a silent green.
const WINDOW = 12;

const dir = new URL('./', import.meta.url);
const files = readdirSync(dir, { recursive: true, encoding: 'utf8' })
	.filter((f) => f.endsWith('.mjs'))
	.sort();

/**
 * The text of the call whose name starts at `at` in `lines[0]`, through the
 * `)` that closes it. Parens inside a string or a template literal count as
 * code, so a literal holding an unbalanced one would end the call early; no
 * site in this suite has one, and the boundary is where a scan would have to
 * become a parser.
 */
function call(lines, at) {
	let depth = 0;
	let opened = false;
	let text = '';
	for (const [n, line] of lines.entries()) {
		for (let k = n === 0 ? at : 0; k < line.length; k++) {
			text += line[k];
			if (line[k] === '(') (depth++, (opened = true));
			else if (line[k] === ')') depth--;
			if (opened && depth === 0) return text;
		}
		text += '\n';
	}
	return text;
}

// The second legitimate shape: the awaited event is OPTIONAL, so its
// TimeoutError is caught and DISCARDED. A rethrow is what tells the two apart,
// and the test is per line — `if (e.name !== 'TimeoutError') throw e` names it
// on the throwing line (the throw is for everything else), where a catch that
// re-raises with a better message does not.
function excused(lines, at) {
	const window = lines.slice(at, at + WINDOW);
	const opens = window.findIndex((l) => l.includes('.catch('));
	if (opens === -1) return false;
	const body = call(window.slice(opens), window[opens].indexOf('.catch(')).split('\n');
	if (!body.some((l) => l.includes('TimeoutError'))) return false;
	return body.every((l) => !/\bthrow\b/.test(l) || l.includes('TimeoutError'));
}

// The hand-rolled poller takes its cap POSITIONALLY — `until(read, ok, ms)` —
// so there is no `timeout:` to find, and the first draft of this file merely
// disclaimed the hole, asserting that no caller had ever overridden the 30 s
// default. That was FALSE: `tailcolumns.test.mjs` passed 5000 twice, the same
// magnitude as the `daystrip` cap this change deletes, sitting in the blind
// spot of a guard whose comment said the blind spot was empty (canary kimi-k3
// on PR #76). A disclaimer nobody checked is how the rule got into this state
// in the first place, so the scan grew instead.
//
// What it finds is the shape that occurs: a bare number as the call's LAST
// argument. A cap held in a named constant, or computed, would still slip
// past — said here rather than claimed away.

/** Every wait's cap in the suite's source — `timeout:`, and `until`'s third. */
function caps() {
	const found = [];
	for (const file of files) {
		const lines = readFileSync(new URL(file, dir), 'utf8').split('\n');
		lines.forEach((line, i) => {
			const trimmed = line.trimStart();
			if (trimmed.startsWith('//') || trimmed.startsWith('*') || trimmed.startsWith('/*')) return;
			// A `//` that is not part of a `://` starts a trailing comment.
			// Cutting at a bare `indexOf('//')` would truncate a line at a
			// URL instead, and a cap after it would go unseen — a guard's
			// miss is the one direction that must not be cheap.
			const comment = line.search(/(?<![:/])\/\//);
			const code = comment === -1 ? line : line.slice(0, comment);
			for (const m of code.matchAll(/\btimeout:\s*([0-9_]+)/g)) {
				found.push({ file, line: i + 1, value: Number(m[1].replaceAll('_', '')), ok: excused(lines, i) });
			}
			for (const m of code.matchAll(/\buntil\(/g)) {
				const last = call(lines.slice(i), m.index).match(/,\s*([0-9_]+)\s*\)$/);
				if (last) found.push({ file, line: i + 1, value: Number(last[1].replaceAll('_', '')), ok: false });
			}
		});
	}
	return found;
}

describe("§16: a wait's cap is a hang-stop", () => {
	const found = caps();

	// The denominator, both halves of it. A green here that examined no files
	// or matched no caps is the scan having broken, which is far likelier than
	// the suite having gone clean — so it says the numbers out loud and fails
	// on either zero rather than passing quietly.
	test('the scan has something to say', () => {
		console.log(`wait-caps: ${files.length} files examined, ${found.length} caps found`);
		assert.ok(files.length > 20, `only ${files.length} .mjs files under tests/ — the walk is broken`);
		assert.ok(
			found.length > 0,
			'no `timeout:` anywhere under tests/ — the suite is more likely to have outgrown this scan than to be clean'
		);
	});

	test('every cap is at the tier, or a caught and discarded TimeoutError', () => {
		const bad = found.filter((c) => c.value < TIER && !c.ok);
		assert.deepEqual(
			bad.map((c) => `${c.file}:${c.line}: timeout: ${c.value}`),
			[],
			`a cap under ${TIER}ms asserts speed by accident (§16). Delete it and let playwright's own ` +
				'default stand; if the test really means "and quickly", assert the elapsed number instead.'
		);
	});
});
