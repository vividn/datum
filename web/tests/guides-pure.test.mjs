// The guides (da-oefd0, #lib/guides.ts): every widget kind carries one in its
// registry entry, documenting all its features. vividn, 2026-11-05: «Each
// widget should carry with it in the code how to use it, and document all of
// its features». So every kind has a guide, every option its menu offers is
// named in it by its label, and no guide nor user doc (docs/README.md's «For
// using datum») says a retired word for a kind of data — read from one list,
// which the glossary (docs/concepts.md's table, and AGENTS.md's, the same)
// names as that word's old words.

import test, { before, describe } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolveLibAsSource } from './support/libhook.mjs';

const ROOT = new URL('../../', import.meta.url);
const read = (path) => readFileSync(new URL(path, ROOT), 'utf8');

/**
 * The retired words for a kind of data, each with the glossary's word now and
 * how it is said: `switch` and `card` as any word, `shape` and `value` only
 * of a field or an entry, since a JSON value and its shape are words of their
 * own.
 */
const RETIRED = [
	{ old: 'switch', now: 'change', said: /\bswitch(?:es|ed)?\b/i },
	{ old: 'card', now: 'record', said: /\bcards?\b/i },
	{ old: 'shape', now: 'field kind', said: /\b(?:field(?:'s)?|its|entry|entries'?) shapes?\b|\bshapes? of (?:an? |the |its )?(?:field|entr)/i },
	{ old: 'value', now: 'state', said: /\bvalue (?:fields?|kinds?)\b|\b(?:field(?:'s)?|entry(?:'s)?) values?\b/i }
];

/**
 * What a reader is told in `text`, a line each: no fenced block or code span,
 * which are what a terminal prints or is typed, nor a line telling an alias,
 * nor a row of the glossary's table, which says the old words to say what
 * they were.
 */
function prose(text) {
	const lines = [];
	let fenced = false;
	for (const line of text.split('\n')) {
		if (line.trimStart().startsWith('```')) fenced = !fenced;
		else if (!fenced && !line.includes('(alias: ') && !line.startsWith('|')) lines.push(line.replace(/(`+).+?\1/g, ''));
	}
	return lines;
}

/** Each retired word `text` says, as `old: line`. */
const retired = (text) => prose(text).flatMap((line) => RETIRED.filter(({ said }) => said.test(line)).map(({ old }) => `${old}: ${line.trim()}`));

let KINDS, guideHtml, guideOf, guided, PAGE;
before(async () => {
	resolveLibAsSource();
	({ KINDS } = await import('../src/lib/layout/registry.ts'));
	({ guideHtml, guideOf, guided, PAGE } = await import('../src/lib/guides.ts'));
});

test('the docs list the guide of every kind a person can open, and no other', () => {
	const shown = guided();
	assert.ok(shown.includes('kit') && shown.includes('widgetbar'), `${shown}`);
	assert.ok(!shown.includes('notice') && !shown.includes('debug') && !shown.includes('tutorial'), `${shown}`);
});

describe('the guides', () => {
	test('every kind has a guide, and so does the page', () => {
		for (const name of [PAGE, ...Object.keys(KINDS)]) {
			const guide = guideOf(name);
			assert.ok(guide?.text.trim(), `${name} has no guide`);
			// What it is for, then its features: more than a line.
			assert.ok(guide.text.split(/\n\s*\n/).length > 1, `${name}'s guide is one paragraph`);
		}
	});

	test('every option a kind\'s menu offers is named in its guide', () => {
		const unnamed = [];
		for (const [kind, { meta, guide }] of Object.entries(KINDS))
			for (const option of meta.options ?? [])
				if (!guide.toLowerCase().includes(option.label.toLowerCase())) unnamed.push(`${kind}: «${option.label}»`);
		assert.deepEqual(unnamed, []);
	});

	test('no guide says a retired word for a kind', () => {
		const said = [PAGE, ...Object.keys(KINDS)].flatMap((name) => retired(guideOf(name).text).map((s) => `${name} ${s}`));
		assert.deepEqual(said, []);
	});
});

// A guide links to a place by the page's own scheme (da-7s7xc): the sync
// guide's «Open Sync». syncguide.test.mjs presses it.
describe('a place link', () => {
	test('is a button for a place, and its text for a name no place has', () => {
		const html = guideHtml('[Open Sync](place:sync) or [Nowhere <b>](place:nowhere)');
		assert.match(html, /<button type="button" class="command" data-place="sync">Open Sync<\/button>/);
		assert.match(html, /or Nowhere &lt;b&gt;<\/p>$/);
		assert.equal((html.match(/<button/g) ?? []).length, 1, html);
	});

	test('in every guide names a place', () => {
		const links = [PAGE, ...Object.keys(KINDS)].flatMap((name) => [...guideOf(name).text.matchAll(/\]\(place:([^)]*)\)/g)].map((m) => ({ name, place: m[1] })));
		assert.ok(links.some((l) => l.name === 'sync' && l.place === 'sync'), 'the sync guide links to no place');
		for (const { name, place } of links) assert.match(guideHtml(`[x](place:${place})`), /data-place=/, `${name}'s guide links to ${place}, no place`);
	});
});

describe('the user docs', () => {
	test('say no retired word for a kind', () => {
		const contents = read('docs/README.md');
		const using = contents.slice(contents.indexOf('For using datum'), contents.indexOf('Design notes'));
		const docs = [...using.matchAll(/^- \[[^\]]+\]\(([^)]+\.md)\)/gm)].map((m) => m[1]);
		assert.ok(docs.length >= 5, `${docs.length} user docs`);
		const said = ['README.md', ...docs].flatMap((doc) => retired(read(`docs/${doc}`)).map((s) => `${doc} ${s}`));
		assert.deepEqual(said, []);
	});
});

describe('the retired words', () => {
	test('are the glossary\'s old words for the word now, in concepts.md and AGENTS.md alike', () => {
		for (const glossary of ['docs/concepts.md', 'AGENTS.md']) {
			const rows = read(glossary)
				.split('\n')
				.filter((line) => line.startsWith('| ') && line.includes('**'))
				.map((line) => line.split('|').map((cell) => cell.trim()));
			for (const { old, now } of RETIRED) {
				const row = rows.find((cells) => cells[2] === `**${now}**`);
				assert.ok(row, `${glossary} has no row for ${now}`);
				assert.match(row.at(-2), new RegExp(`\\b${old}\\b`), `${glossary} does not name ${old} an old word for ${now}`);
			}
		}
	});

	test('are found where they are said, and not where they are not', () => {
		assert.deepEqual(retired('A switch field is a change field.'), ['switch: A switch field is a change field.']);
		assert.deepEqual(retired('Every card'), ['card: Every card']);
		assert.deepEqual(retired("a field's shape"), ["shape: a field's shape"]);
		assert.deepEqual(retired("the field's value"), ["value: the field's value"]);
		assert.deepEqual(retired('a JSON shape, a key=value, `switch`\n```\nswitch\n```\n| switch |\n- `change` (alias: switch)'), []);
	});
});
