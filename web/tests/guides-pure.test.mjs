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

// A guide links to a place by datum's action links (da-7s7xc, da-uzl16): the
// sync guide's «Open Sync», `act:place/sync`. syncguide.test.mjs presses it.
describe('a place link', () => {
	test('is a button for a place, and its text for a name no place has', () => {
		const html = guideHtml('[Open Sync](act:place/sync) or [Nowhere <b>](act:place/nowhere)');
		assert.match(html, /<button type="button" class="command" data-action="\{&quot;kind&quot;:&quot;place&quot;,&quot;name&quot;:&quot;sync&quot;\}" title="[^"]*">Open Sync<\/button>/);
		assert.match(html, /or Nowhere &lt;b&gt;<\/p>$/);
		assert.equal((html.match(/<button/g) ?? []).length, 1, html);
	});

	test('in every guide names a place', () => {
		const links = [PAGE, ...Object.keys(KINDS)].flatMap((name) => [...guideOf(name).text.matchAll(/\]\(act:place\/([^)]*)\)/g)].map((m) => ({ name, place: m[1] })));
		assert.ok(links.some((l) => l.name === 'sync' && l.place === 'sync'), 'the sync guide links to no place');
		for (const { name, place } of links) assert.match(guideHtml(`[x](act:place/${place})`), /data-action=/, `${name}'s guide links to ${place}, no place`);
	});
});

// The vocabulary is said whole (da-uzl16): the text widget's guide and the docs
// name every action and embed with its example.
test('the text widget guide and docs/actions.md list every action and embed', async () => {
	const { KINDS_OF_ACTION } = await import('../src/lib/actions.ts');
	const { EMBEDS } = await import('../src/lib/markdown.ts');
	const kinds = Object.keys(KINDS_OF_ACTION);
	assert.deepEqual(kinds, ['tab', 'place', 'run', 'field']);
	for (const embed of EMBEDS) assert.ok(kinds.includes(embed), `the embed ${embed} is not in the vocabulary`);
	const docs = read('docs/actions.md');
	for (const { example } of Object.values(KINDS_OF_ACTION)) {
		assert.ok(guideOf('text').text.includes(example), `the guide lacks ${example}`);
		assert.ok(docs.includes(example), `docs/actions.md lacks ${example}`);
	}
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

// da-6i6dx: the setup widget's guide speaks the form's words — its kinds and
// its data, and a new field's save, one `setup F --track` and one Undo
// (da-2pbvd). Read here since da-7dflk: Help then a tap on the widget tours it.
test("the setup widget's guide is in the words its form uses", () => {
	const text = KINDS.setup.guide;
	assert.ok(text.includes('setup F --track'), "it names the command a new field's Save runs");
	// A field's kind is the glossary's word (da-kjkpx), the old «shape» gone; its keys are its data.
	assert.doesNotMatch(text, /\bshapes?\b|\bkeys\b/);
});

// da-7dflk: every kind has a tour, each step a sentence on one part of it,
// inviting only what the runner can see done, and naming a real other kind
// where it is about one. A kind no browser test opens alone (gated by a
// database, or hidden) tours only its widget whole, which is always drawn.
test('every kind has a tour; a kind no test opens alone tours its widget whole', async () => {
	const { WHOLE } = await import('../src/lib/layout/registry.ts');
	const opened = guided().filter((kind) => kind !== 'kit');
	for (const [kind, { tour }] of Object.entries(KINDS)) {
		assert.ok(tour.length > 0, `${kind} has no tour`);
		for (const step of tour) {
			assert.ok(step.say.trim().length > 0 && step.at.trim().length > 0, `${kind}: ${JSON.stringify(step)}`);
			assert.ok(!step.do || ['tap', 'hold', 'drag', 'scroll', 'type'].includes(step.do), `${kind}: ${step.do}`);
			assert.ok(!step.with || (step.with !== kind && KINDS[step.with]), `${kind}: with ${step.with}`);
		}
		if (!opened.includes(kind)) assert.deepEqual(tour.map((s) => s.at), [WHOLE], `${kind} tours a part no test opens`);
	}
});
