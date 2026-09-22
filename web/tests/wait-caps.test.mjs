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

// There is no search window, and the reasoning that once justified one is
// worth keeping as a warning: it said that erring long "only makes this
// STRICTER, never laxer, because an unrelated `throw` drawn into the window
// refuses the excuse". That is false in the other direction — an unrelated
// HANDLER drawn into the window EXCUSES a bare cap — and a bare 5 s cap seven
// lines above an unrelated `.then` went green on exactly that argument. The
// excuse is anchored to the call now, not to a distance.

const dir = new URL('./', import.meta.url);
const files = readdirSync(dir, { recursive: true, encoding: 'utf8' })
	.filter((f) => f.endsWith('.mjs'))
	.sort();

/**
 * A line with its comment gone: nothing for a whole-line one, and the tail cut
 * at the first `//` that is not part of a `://`. Cutting at a bare
 * `indexOf('//')` would truncate at a URL instead, and a cap after it would go
 * unseen — a guard's miss is the one direction that must not be cheap.
 */
function code(line) {
	const trimmed = line.trimStart();
	if (trimmed.startsWith('//') || trimmed.startsWith('*') || trimmed.startsWith('/*')) return '';
	const at = line.search(/(?<![:/])\/\//);
	return at === -1 ? line : line.slice(0, at);
}

/**
 * The text of the call whose name starts at `at` in `lines[0]`, through the
 * `)` that closes it, comments removed as it goes — so neither a trailing
 * `5000 // ms` nor an unbalanced paren inside a comment can defeat what reads
 * the result (canary kimi-k3 on PR #76: a cap written with a trailing comment
 * was not merely unexcused, it was never COUNTED).
 *
 * Parens inside a string or a template literal still count as code, so a
 * literal holding an unbalanced one would end the call early; no site in this
 * suite has one, and that is where a scan would have to become a parser.
 */
function call(lines, at) {
	let depth = 0;
	let opened = false;
	let text = '';
	for (const [n, line] of lines.entries()) {
		const src = code(line);
		for (let k = n === 0 ? at : 0; k < src.length; k++) {
			text += src[k];
			if (src[k] === '(') (depth++, (opened = true));
			else if (src[k] === ')') depth--;
			if (opened && depth === 0) return text;
		}
		text += '\n';
	}
	return text;
}

/** A call's top-level arguments, as text — commas inside nesting are not separators. */
function args(text) {
	const inner = text.slice(text.indexOf('(') + 1, -1);
	const out = [];
	let depth = 0;
	let cur = '';
	for (const ch of inner) {
		if ('([{'.includes(ch)) depth++;
		else if (')]}'.includes(ch)) depth--;
		if (ch === ',' && depth === 0) (out.push(cur), (cur = ''));
		else cur += ch;
	}
	out.push(cur);
	return out;
}

/**
 * Walking forward from (n, k), the position just past the `)` that closes the
 * call ENCLOSING that point — the first `)` met at depth zero.
 */
function enclosingClose(lines, n, k) {
	let depth = 0;
	for (let i = n; i < lines.length; i++) {
		const src = code(lines[i]);
		for (let j = i === n ? k : 0; j < src.length; j++) {
			if (src[j] === '(') depth++;
			else if (src[j] === ')') {
				if (depth === 0) return [i, j + 1];
				depth--;
			}
		}
	}
	return null;
}

// A `throw` inside the handler is allowed only where it is GUARDED — the line
// tests the error's name and re-raises everything that is not the timeout, as
// `if (e.name !== 'TimeoutError') throw e` does.
//
// This asked whether the line MENTIONED TimeoutError, which is the mistake
// this whole branch is about, one turn further down: a `.catch` re-raising a
// better message excused its cap as soon as the message happened to contain
// the word — and `tailcolumns.test.mjs`'s diagnostic is exactly a rethrow with
// a message, so this was one edit away from laundering the cap it was written
// to refuse (canary deepseek-v4-pro on PR #81 named the neighbourhood of it).
// Stripping strings does not work either, because the legitimate site's own
// `'TimeoutError'` IS a string. The condition is what differs, so test that.
const GUARDED = /\bif\b.*!==?\s*['"]TimeoutError['"]/;

// The second legitimate shape: the awaited event is OPTIONAL, so the wait's
// rejection is caught and DISCARDED. What tells it from a hang-stop is the
// RETHROW, not the error's class — and the class cannot be the test, because
// the hand-rolled `until` throws a plain Error and so can never name a
// TimeoutError at all. That is what the first version of this rule got wrong:
// it demanded the literal, and so refused `fieldsorder.test.mjs`'s wait, which
// is expected to expire and whose rejection handler is empty.
//
// ⚑ The handler must be chained onto the SAME call. This searched FORWARD a
// fixed number of lines for any `.catch(`/`.then(` and never asked whose it
// was, so a neighbour's rejection handler LAUNDERED a bare cap — a silent
// miss, in the guard against silent misses (vremya on PR #81, with the two
// statements seven lines apart, which is ordinary spacing in a test file).
// A smaller window would still bleed; it would only need the statements
// closer. So there is no window: find where the capped call CLOSES, and
// require the chain to be the first thing after it.
//
// The handler is `.catch`'s only argument, or `.then`'s SECOND, and it must
// exist and not re-raise. The throw test is per line, because
// `if (e.name !== 'TimeoutError') throw e` throws only for everything else,
// where a catch that re-raises with a better message does not.
//
// What this does NOT check is whether the discard DISCRIMINATES. A bare
// `.catch(() => {})` hides a real failure as well as a timeout, and this
// excuses it. §16 asks for the careful form; the guard enforces the shape.
function excused(lines, n, k) {
	const close = enclosingClose(lines, n, k);
	if (!close) return false;
	const [cn, ck] = close;
	const rest = [code(lines[cn]).slice(ck), ...lines.slice(cn + 1).map(code)];
	let at = 0;
	while (at < rest.length && rest[at].trim() === '') at++;
	if (at === rest.length) return false;
	const col = rest[at].length - rest[at].trimStart().length;
	const how = rest[at].slice(col).match(/^\.(catch|then)\(/);
	if (!how) return false;
	const handler = args(call(rest.slice(at), col))[how[1] === 'catch' ? 0 : 1];
	if (!handler || !handler.trim()) return false;
	return handler.split('\n').every((l) => !/\bthrow\b/.test(l) || GUARDED.test(l));
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
function scan(lines, file = '') {
	const found = [];
	{
		lines.forEach((line, i) => {
			const src = code(line);
			for (const m of src.matchAll(/\btimeout:\s*([0-9_]+)/g)) {
				const value = Number(m[1].replaceAll('_', ''));
				found.push({ file, line: i + 1, how: `timeout: ${value}`, value, ok: excused(lines, i, m.index) });
			}
			for (const m of src.matchAll(/\buntil\(/g)) {
				const last = call(lines.slice(i), m.index).match(/,\s*([0-9_]+)\s*\)$/);
				if (!last) continue;
				const value = Number(last[1].replaceAll('_', ''));
				// The positional arm asks `excused` too. It did not at first —
				// it assumed no `until` cap could be legitimate — and that
				// assumption was wrong within one file of being written:
				// `fieldsorder.test.mjs` waits for something it asserts does
				// NOT happen, and discards the rejection in `.then`'s second
				// argument. Same shape as `layout-floors`, different spelling.
				found.push({ file, line: i + 1, how: `until(…, ${value})`, value, ok: excused(lines, i, m.index + m[0].length) });
			}
		});
	}
	return found;
}

/** Every cap in the suite's source. */
function caps() {
	return files.flatMap((file) => scan(readFileSync(new URL(file, dir), 'utf8').split('\n'), file));
}

// Every shape the rule must tell apart, run through the same `scan` a file
// goes through. These lived in scratch files that only the author ever ran,
// which makes a control a story rather than a check — and three defects on
// this branch were found by someone else constructing a shape nobody had
// written down (kimi-k3's trailing comment, vremya's neighbouring handler,
// deepseek-v4-pro's rethrown message). They are tests now.
const REFUSE = 'refuse';
const EXCUSE = 'excuse';

// ⚑ Every number below is INTERPOLATED, never written inline. That is what
// keeps this file out of its own denominator without a self-exclusion: the
// scan wants `timeout:` followed by digits and `until(` closing on digits, and
// `${ms(400)}` is neither until it is evaluated. A guard that skipped its own
// source would have the one blind spot it exists to forbid.
const ms = (n) => String(n);

const SHAPES = [
	[REFUSE, 'a bare playwright cap', `page.waitForFunction(f, null, { timeout: ${ms(400)} });`],
	[REFUSE, 'a bare 5s cap', `page.waitForFunction(f, undefined, { timeout: ${ms(5000)} });`],
	[REFUSE, 'a cap after an https:// on the same line', `page.goto('https://x.test/a', { timeout: ${ms(2000)} });`],
	[REFUSE, 'an inline positional cap', `until(read, ok, ${ms(5000)});`],
	[REFUSE, 'a positional cap spread over lines', `until(\n\tread,\n\tok,\n\t${ms(5000)}\n);`],
	[REFUSE, 'a positional cap behind a trailing comment', `until(\n\ty,\n\tkept,\n\t${ms(5000)} // ms\n);`],
	[
		REFUSE,
		'a catch that re-raises a better message',
		`p.waitForFunction(f, a, { timeout: ${ms(10000)} }).catch((e) => {\n\tthrow new Error(SAID, { cause: e });\n});`
	],
	[
		REFUSE,
		'a catch re-raising a message that NAMES TimeoutError',
		`p.waitForFunction(f, a, { timeout: ${ms(400)} }).catch(() => {\n\tthrow new Error('TimeoutError: never settled');\n});`
	],
	[
		REFUSE,
		'a then whose rejection handler re-raises',
		`p.waitForFunction(f, null, { timeout: ${ms(400)} }).then(\n\t() => {},\n\t(e) => {\n\t\tthrow e;\n\t}\n);`
	],
	[REFUSE, 'a then with no rejection handler at all', `p.waitForFunction(f, null, { timeout: ${ms(400)} }).then(() => {});`],
	[
		REFUSE,
		'an unguarded rethrow on its own line',
		`p.waitForFunction(f, null, { timeout: ${ms(400)} }).catch((e) => {\n\tif (e.name !== TIMEOUT) {\n\t\tthrow e;\n\t}\n});`
	],
	[
		REFUSE,
		'a bare cap whose only nearby handler belongs to the NEXT statement',
		`until(read, ok, ${ms(5000)});\n\n\n\nuntil(a, b, ${ms(400)}).then(\n\t() => {},\n\t() => {}\n);`
	],
	[
		REFUSE,
		'a bare cap whose only nearby handler belongs to the PREVIOUS one',
		`until(a, b, ${ms(400)}).then(\n\t() => {},\n\t() => {}\n);\n\n\n\npage.waitForFunction(f, null, { timeout: ${ms(5000)} });`
	],
	[
		EXCUSE,
		'a guarded catch — the layout-floors shape',
		`p.waitForFunction(f, a, { timeout: ${ms(400)} }).catch((e) => {\n\tif (e.name !== 'TimeoutError') throw e;\n});`
	],
	[
		EXCUSE,
		"a then's second argument, empty — the fieldsorder shape",
		`until(\n\tread,\n\tok,\n\t// a comment between the predicate and the number\n\t${ms(1000)}\n).then(\n\t() => assert.fail(SAID),\n\t() => {}\n);`
	],
	[EXCUSE, 'a positional cap at the tier', `until(read, ok, ${ms(30000)});`],
	[EXCUSE, 'a playwright cap at the tier', `page.waitForFunction(f, null, { timeout: ${ms(30000)} });`],
	// A KNOWN LIMIT, asserted so it cannot drift silently: the guard checks
	// that the rejection is DISCARDED, not that the discard DISCRIMINATES. A
	// bare `.catch(() => {})` hides a real failure as well as a timeout and is
	// excused here. §16 asks for the careful form; this enforces the shape.
	[
		EXCUSE,
		'a bare non-discriminating discard (a known limit, not an endorsement)',
		`p.waitForFunction(f, null, { timeout: ${ms(400)} }).catch(() => {});`
	]
];

/** What the rule says about one snippet: `refuse` if any cap in it is unexcused. */
const verdictOn = (src) => (scan(src.split('\n')).some((c) => c.value < TIER && !c.ok) ? REFUSE : EXCUSE);

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

	test('the rule tells every known shape apart', () => {
		assert.deepEqual(
			SHAPES.map(([, name, src]) => `${verdictOn(src)}: ${name}`),
			SHAPES.map(([want, name]) => `${want}: ${name}`)
		);
	});

	test('every cap is at the tier, or a caught and discarded TimeoutError', () => {
		const bad = found.filter((c) => c.value < TIER && !c.ok);
		assert.deepEqual(
			bad.map((c) => `${c.file}:${c.line}: ${c.how}`),
			[],
			`a cap under ${TIER}ms asserts speed by accident (§16). Delete it and let playwright's own ` +
				'default stand; if the test really means "and quickly", assert the elapsed number instead.'
		);
	});
});
