// The commands widget's rules, in node, with no browser (da-ltux).
//
// `$lib/commands.ts` is pure but for its last function, and imports only
// a type, so node loads it as it is. What is checked here is the part a
// browser adds nothing to: the order the buttons come in, which verb a
// line is pending, what that verb leaves showing, what a press under it
// runs, and the arithmetic of the scroll rule. The widget as it is DRAWN
// — and the filtering as a reader sees it happen — is
// `commands.test.mjs`.

import test, { describe } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import {
	COMMANDS,
	VERBS,
	fieldsUnder,
	pendingVerb,
	declaredVerb,
	rowsIn,
	scrollsSideways,
	verbPress
} from '../src/lib/commands.ts';

/** vividn's order: Undo (with Redo beside it), the five verbs, then the rest. */
const ORDER = [
	'undo',
	'redo',
	'occur',
	'start',
	'end',
	'switch',
	'add',
	'retime',
	'update',
	'delete',
	'track',
	'ignore',
	'flag',
	'tail',
	'now',
	'stats',
	'fields',
	'count',
	'ids',
	'history',
	'get',
	'grep',
	'dayview',
	'nowview',
	'spec',
	'alias',
	'kit',
	'check',
	'db',
	'info',
	'config',
	'sync'
];

describe('the commands, in order', () => {
	test('Undo first, then the verbs, then the rest', () => {
		assert.deepEqual(
			COMMANDS.map((c) => c.name),
			ORDER
		);
		assert.equal(COMMANDS[0].label, 'Undo');
		assert.equal(COMMANDS[1].label, 'Redo');
	});

	// vividn, 2026-09-15 19:08: `note` is not a button here — it lives in a
	// field's long-press menu (da-py5w), beside the field it annotates.
	test('no note', () => {
		assert.equal(
			COMMANDS.find((c) => c.name === 'note'),
			undefined
		);
	});

	test('Undo and Redo run; the five verbs pend; everything else writes a line', () => {
		const pressOf = (name) => COMMANDS.find((c) => c.name === name).press;
		assert.deepEqual([pressOf('undo'), pressOf('redo')], ['run', 'run']);
		for (const verb of VERBS) assert.equal(pressOf(verb), 'verb', verb);
		for (const name of ['tail', 'db', 'sync']) assert.equal(pressOf(name), 'fill', name);
	});
});

describe('the verb a line is pending', () => {
	test('the first word, whole', () => {
		assert.equal(pendingVerb('occur '), 'occur');
		assert.equal(pendingVerb('occur'), 'occur');
		assert.equal(pendingVerb('  switch mood happy'), 'switch');
		assert.equal(pendingVerb('end work'), 'end');
	});

	test('anything else pends nothing', () => {
		for (const line of ['', '   ', 'tail', 'occurrence x', 'coffee occur', '-t 09:00'])
			assert.equal(pendingVerb(line), null, JSON.stringify(line));
	});
});

const point = { field: 'coffee', shape: { kind: 'point' } };
const record = { field: 'rec', shape: { kind: 'record' } };
const offBlock = { field: 'sleep', shape: { kind: 'state' } };
const onBlock = { field: 'work', shape: { kind: 'state' }, current: true };
const boolSwitch = { field: 'light', shape: { kind: 'states', states: ['false', 'true'] } };
const onSwitch = { field: 'mood', shape: { kind: 'states', states: ['bad', 'good'] }, current: 'good' };
const ALL = [point, record, offBlock, onBlock, boolSwitch, onSwitch];

const under = (verb) => fieldsUnder(ALL, verb).map((b) => b.field);

describe('what a pending verb leaves showing', () => {
	test('occur, switch and add are the fields of that verb', () => {
		assert.deepEqual(under('occur'), ['coffee']);
		assert.deepEqual(under('switch'), ['light', 'mood']);
		assert.deepEqual(under('add'), ['rec']);
	});

	// «Currently on/off» is `now --json`'s truth, which reaches a button as
	// `current` — so a block that is running is not something to start.
	test('start: a block that is off, or a switch with true among its states', () => {
		assert.deepEqual(under('start'), ['sleep', 'light']);
	});

	test('end: the blocks and switches that are on', () => {
		assert.deepEqual(under('end'), ['work', 'mood']);
	});

	test('no verb leaves the list whole', () => {
		assert.deepEqual(under(null), ALL.map((b) => b.field));
	});
});

describe('what a press under a verb runs', () => {
	test('`<verb> <field>`, at once', () => {
		assert.deepEqual(verbPress('occur', point), ['occur', 'coffee']);
		assert.deepEqual(verbPress('start', offBlock), ['start', 'sleep']);
		assert.deepEqual(verbPress('end', onSwitch), ['end', 'mood']);
		// A switch with `true` among its states is startable AS a block.
		assert.deepEqual(verbPress('start', boolSwitch), ['start', 'light']);
	});

	// The two argvs that are not commands: the field's own courtesy takes
	// over and still ends in the pending verb.
	test('switch on a many-stated field, and add on a record, ask first', () => {
		assert.equal(verbPress('switch', onSwitch), null);
		assert.equal(verbPress('add', record), null);
	});

	// The `+` at the end of a filtered list opens the Track modal on the
	// pending verb (da-94q1's four). `end` is not one a field can be BORN
	// with, and a block you are ending is a block — so it declares `start`.
	test('the `+` opens the Track modal on the pending verb, with end declaring start', () => {
		assert.deepEqual(VERBS.map(declaredVerb), ['occur', 'start', 'start', 'switch', 'add']);
	});
});

describe('the scroll rule', () => {
	// Rows of 33px buttons with 6.4px between them: one row needs 33, two
	// need 72.4, three 111.8, four 151.2.
	test('rows are counted from the box the layout gave the bar', () => {
		assert.equal(rowsIn(40, 33), 1);
		assert.equal(rowsIn(72, 33), 1);
		assert.equal(rowsIn(73, 33), 2);
		assert.equal(rowsIn(112, 33), 3);
		assert.equal(rowsIn(160, 33), 4);
	});

	test('a box too short for one row still has one, and an unmeasured button one too', () => {
		assert.equal(rowsIn(0, 33), 1);
		assert.equal(rowsIn(400, 0), 1);
	});

	test('one or two rows scroll sideways; three or more wrap', () => {
		assert.deepEqual([1, 2, 3, 4].map(scrollsSideways), [true, true, false, false]);
	});
});

// The kind itself: pickable, and with the one-widget layout every pickable
// kind has (da-y2f6's rule, held generally by `tree.test.mjs`). `command`
// is the id da-zqop freed by renaming the old one-line widget to `custom`.
describe('the `command` kind', () => {
	registerHooks({
		resolve(specifier, context, next) {
			if (specifier.endsWith('.svelte')) return { url: 'data:text/javascript,export default {}', shortCircuit: true };
			if (!/^(\.|\$lib\/)/.test(specifier)) return next(specifier, context);
			const lib = new URL('../src/lib/', import.meta.url);
			const url = new URL(specifier.replace(/^\$lib\//, lib.href), context.parentURL);
			return next(url.href.replace(/\.js$/, '.ts'), context);
		}
	});

	test('exists, is pickable, and has a built-in layout of its own', async () => {
		const { KINDS, PICKABLE, titleOf } = await import('../src/lib/layout/registry.ts');
		const { BUILTIN, BUILTIN_NAMES } = await import('../src/lib/layout/defaults.ts');
		assert.ok(KINDS.command, 'no `command` kind');
		assert.equal(titleOf('command'), 'command');
		assert.ok(PICKABLE.includes('command'));
		assert.ok(BUILTIN_NAMES.includes('command'));
		assert.equal(BUILTIN.command(false).root.tabs[0].content.kind, 'command');
		// And the widget it took the id back from is still reachable.
		assert.equal(BUILTIN.custom(false).root.tabs[0].content.kind, 'custom');
	});
});
