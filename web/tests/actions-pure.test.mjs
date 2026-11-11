// datum's action URLs (da-uzl16), in node: what each link names, what a run
// may say, and how markdown with `act` draws them — a button, an embed's
// placeholder, or a refused run's words and why. textwidget.test.mjs and
// actlink.test.mjs press them in the app.

import test, { describe } from 'node:test';
import assert from 'node:assert/strict';
import { resolveLibAsSource } from './support/libhook.mjs';

resolveLibAsSource();
const { actionOf, refusal } = await import('../src/lib/actions.ts');
const { markdown, OUTWARD } = await import('../src/lib/markdown.ts');

const acting = (text) => markdown(text, { ...OUTWARD, act: () => true });

describe('an action link', () => {
	test('names its action in every spelling, and any other link names none', () => {
		const run = { kind: 'run', line: 'moment coffee' };
		for (const href of ['act:run?line=moment+coffee', '/act/run?line=moment%20coffee', 'https://datum.day/act/run?line=moment+coffee'])
			assert.deepEqual(actionOf(href), run, href);
		assert.deepEqual(actionOf('act:tab/my%20morning'), { kind: 'tab', name: 'my morning' });
		assert.deepEqual(actionOf('act:place/sync'), { kind: 'place', name: 'sync' });
		assert.deepEqual(actionOf('act:field/coffee'), { kind: 'field', name: 'coffee' });
		for (const href of ['act:run', 'act:run?line=+', 'act:delete/x', 'act:tab/a/b', 'https://example.com/act/tab/x', 'https://datum.day/tab/x', 'javascript:alert(1)'])
			assert.equal(actionOf(href), null, href);
	});

	test('a run takes a logging command on every side, and refuses anything else, saying so', () => {
		for (const line of ['moment "a , delete b"', 'moment coffee', 'on work -t -5', 'off work', 'change mood happy', 'record word x=1', 'remark coffee hot', 'allday outside -y', 'occur coffee', 'moment a , start b'])
			assert.equal(refusal({ kind: 'run', line }), null, line);
		for (const [line, word] of [
			['delete abc', 'delete'],
			['db purge', 'db'],
			['moment a , delete b', 'delete'],
			['sync', 'sync'],
			['kit install x', 'kit'],
			['coffee', 'coffee'],
			['-y moment a', '-y'],
			['moment a "," delete b', 'delete'],
			['moment a ,', 'nothing']
		])
			assert.ok(refusal({ kind: 'run', line })?.includes(word), `${line}: ${refusal({ kind: 'run', line })}`);
		assert.ok(refusal({ kind: 'run', line: 'moment a --db other' })?.includes('--db'));
	});

	// vremya on #2014: two logging flags write more than an entry, an alias and an entry replaced.
	test('a run carries only the flags that say what its entry is, refusing any other by name', () => {
		for (const line of [
			'moment coffee -t -1h milk=oat',
			'moment coffee -yy -q',
			'moment coffee -t10:00 -y',
			'moment coffee --time=10:00 --date -1',
			'on work -l home -L home --if-changed',
			'change mood happy -m -c "fine" -k mood_key',
			'record book -T title=Dune --dry-run --json',
			'moment jot -C -- see --alias x',
			'moment a -t -5 , off b -t -2'
		])
			assert.equal(refusal({ kind: 'run', line }), null, line);
		for (const [line, flag] of [
			['moment coffee --alias w --alias-overwrite', '--alias'],
			['moment note --id fixedslug v=2 --conflict replace', '--id'],
			['moment note v=2 --conflict=replace', '--conflict'],
			['moment note --update', '--update'],
			['moment coffee -u', '-u'],
			['moment coffee -yu', '-u'],
			['record book -K title Dune', '-K'],
			['moment x --setup', '--setup'],
			['moment a , moment b --merge', '--merge']
		])
			assert.ok(refusal({ kind: 'run', line })?.startsWith(`${flag} `), `${line}: ${refusal({ kind: 'run', line })}`);
		assert.equal(refusal({ kind: 'tab', name: 'x' }), null);
	});
});

describe('markdown with act', () => {
	test('draws an action as a button saying what it does, an embed as its placeholder, a refused run as its words and why', () => {
		const html = acting('[Coffee](act:run?line=moment+coffee) [Sync](act:place/sync) [Gone](act:run?line=delete+x)\n\n[coffee](act:field/coffee)');
		const buttons = [...html.matchAll(/<button type="button" class="command" data-action="([^"]+)" title="([^"]+)">(\w+)<\/button>/g)];
		assert.deepEqual(
			buttons.map(([, action, , label]) => [label, JSON.parse(action.replace(/&quot;/g, '"'))]),
			[
				['Coffee', { kind: 'run', line: 'moment coffee' }],
				['Sync', { kind: 'place', name: 'sync' }]
			]
		);
		assert.ok(buttons[0][2].includes('moment coffee'), 'the title says what it runs');
		assert.match(html, /<span class="refused">Gone \([^)]*delete[^)]*\)<\/span>/);
		assert.match(html, /<p><span data-embed="field" data-name="coffee"><\/span><\/p>/);
		assert.equal((html.match(/<button/g) ?? []).length, 2, html);
	});

	test('without act, no action is a link, and an unknown kind is its text either way', () => {
		assert.equal(markdown('[c](act:run?line=moment+coffee) [f](act:field/coffee)', OUTWARD), '<p>c f</p>');
		assert.equal(acting('[x](act:delete/y)'), '<p>x</p>');
		assert.equal(acting('[x](https://example.org)'), '<p><a href="https://example.org" target="_blank" rel="noopener">x</a></p>');
	});
});
