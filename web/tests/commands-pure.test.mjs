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
import { resolveLibAsSource } from './support/libhook.mjs';
import {
	COMMANDS,
	VERBS,
	commandsUnder,
	fieldsUnder,
	grammarOf,
	pendingVerb,
	declaredVerb,
	drills,
	flagsOf,
	lanesOf,
	pressKind,
	rowsIn,
	subcommandsOf,
	verbPress,
	withFlag
} from '../src/lib/commands.ts';

/** vividn's order: Undo (its redo an icon on it, da-zq91a), Help where Redo was (da-vjwid), the five verbs, then the rest. */
const ORDER = [
	'undo',
	'help',
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
	});

	// vividn, 2026-09-15 19:08: `note` is not a button here — it lived in a
	// field's long-press menu (da-py5w), beside the field it annotates.
	// da-e78x6 removed that menu and did NOT move `note` here: a `fill`
	// press puts `note ` on the line, and a field pill stops fitting the
	// field slot the moment the comment's words are typed after it
	// (`fieldSlotOpen`, $lib/selection.ts), so the button would compose
	// `note too strong` and log a field called `too`. `note` is typed.
	test('no note', () => {
		assert.equal(
			COMMANDS.find((c) => c.name === 'note'),
			undefined
		);
	});

	test('Undo runs; the five verbs pend; everything else writes a line', () => {
		const pressOf = (name) => COMMANDS.find((c) => c.name === name).press;
		assert.equal(pressOf('undo'), 'run');
		assert.equal(pressOf('help'), 'help');
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

// da-57j14: a loaded field narrows the VERBS by the same rule a pending
// verb narrows the fields with (`verbFits`). Every verb names its field at
// slot 1, as `datum help --json` says, so the pill itself hides none.
const GRAMMAR = grammarOf(
	['occur', 'start', 'stop', 'switch', 'add'].map((name) => ({
		name,
		aliases: name === 'stop' ? ['end'] : [],
		usage: [`datum ${name} <FIELD>`],
		flags: []
	}))
);
const pill = (b) => ({ id: 'field', kind: 'field', text: b.field });
const verbsUnder = (pills) =>
	commandsUnder(GRAMMAR, pills, ALL)
		.map((c) => c.name)
		.filter((name) => VERBS.includes(name));

describe('what a loaded field leaves of the verbs', () => {
	test('an occur field leaves occur, and not start, end or switch', () => {
		assert.deepEqual(verbsUnder([pill(point)]), ['occur']);
	});

	// `end` only while it is on: off, there is nothing to end.
	test('a switch with true among its states offers switch, start and end', () => {
		assert.deepEqual(verbsUnder([pill(boolSwitch)]), ['start', 'switch']);
		const on = { ...boolSwitch, current: 'true' };
		const verbs = commandsUnder(GRAMMAR, [pill(on)], [on]).map((c) => c.name);
		assert.deepEqual(verbs.filter((name) => VERBS.includes(name)), ['start', 'end', 'switch']);
	});

	// da-tu30m: beside an hid the field is CONTEXT and narrows no command.
	// Invisible with the real grammar — no verb takes an id, so the hid
	// hides all five anyway — so here each verb takes one too.
	test('a context field (beside an hid) narrows no verb', () => {
		const byId = grammarOf(
			VERBS.map((name) => ({ name, aliases: [], usage: [`datum ${name} <FIELD>`, `datum ${name} <ID_OR_SLUG>`], flags: [] }))
		);
		const hid = { id: 'hid', kind: 'hid', text: 'abcde' };
		const verbs = (pills) =>
			commandsUnder(byId, pills, ALL)
				.map((c) => c.name)
				.filter((name) => VERBS.includes(name));
		assert.deepEqual(verbs([pill(point)]), ['occur'], 'the argument field narrows');
		assert.deepEqual(verbs([pill(point), hid]), VERBS);
	});

	test('clearing the pill brings every verb back', () => {
		assert.deepEqual(verbsUnder([]), VERBS);
	});

	test('a field with no button yet keeps every verb: it has no shape to narrow by', () => {
		assert.deepEqual(verbsUnder([{ id: 'field', kind: 'field', text: 'brandnew' }]), VERBS);
	});

	// The property: for every field kind × verb, the verb shows under the
	// field exactly when the field shows under the verb.
	test('the two directions agree for every field and every verb', () => {
		for (const b of ALL)
			for (const verb of VERBS)
				assert.equal(
					verbsUnder([pill(b)]).includes(verb),
					fieldsUnder(ALL, verb).includes(b),
					`${verb} × ${b.field}`
				);
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

// What a tap on a field DOES, which its outline is drawn from (da-z8z40):
// the bead's table, row by row. `run` is the double outline.
describe('what a field tap does', () => {
	test('with no verb: a point and a block run, a switch opens, a record fills', () => {
		assert.equal(pressKind(point, null), 'run');
		assert.equal(pressKind(offBlock, null), 'run');
		assert.equal(pressKind(onBlock, null), 'run');
		assert.equal(pressKind(onSwitch, null), 'open');
		assert.equal(pressKind(boolSwitch, null), 'open');
		assert.equal(pressKind(record, null), 'fill');
	});

	test('under a verb, every field it leaves runs — but where verbPress asks first', () => {
		for (const verb of VERBS)
			for (const button of fieldsUnder(ALL, verb))
				assert.equal(
					pressKind(button, verb),
					verbPress(verb, button) ? 'run' : button.shape.kind === 'record' ? 'fill' : 'open',
					`${verb} ${button.field}`
				);
		assert.equal(pressKind(onSwitch, 'switch'), 'open');
		assert.equal(pressKind(record, 'add'), 'fill');
		assert.equal(pressKind(boolSwitch, 'start'), 'run');
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

	// da-vu7pi: item `i` to row `i % rows`, so each row packs on its own
	// and the first few items still start the rows.
	test('the items are dealt into the rows by index, and no row is left empty', () => {
		const items = ['Undo', 'Redo', 'occur', 'start', 'end', 'switch', 'add'];
		assert.deepEqual(lanesOf(items, 1), [items]);
		assert.deepEqual(lanesOf(items, 2), [
			['Undo', 'occur', 'end', 'add'],
			['Redo', 'start', 'switch']
		]);
		assert.deepEqual(lanesOf(items, 3), [
			['Undo', 'start', 'add'],
			['Redo', 'end'],
			['occur', 'switch']
		]);
		assert.deepEqual(lanesOf(['Track +', 'coffee'], 3), [['Track +'], ['coffee']]);
		assert.deepEqual(lanesOf([], 2), []);
	});
});

// The kind itself: pickable, and with the one-widget layout every pickable
// kind has (da-y2f6's rule, held generally by `tree.test.mjs`). `command`
// is the id da-zqop freed by renaming the old one-line widget to `custom`.
describe('the `command` kind', () => {
	resolveLibAsSource(import.meta.url);

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

// da-803mk: which commands drill, and what the bar is while one is open —
// read off `help --json`'s `subcommands` and `bare`, never off usage lines.
describe('subcommands', () => {
	const sub = (...names) => names.map((name) => ({ name }));
	const TREE = grammarOf([
		{ name: 'db', aliases: [], usage: ['datum db setup', 'datum db backup <PATH | ->'], flags: [], subcommands: sub('setup', 'backup'), bare: false },
		{ name: 'sync', aliases: [], usage: ['datum sync [--push]'], flags: [], subcommands: sub('login'), bare: true },
		{ name: 'spec', aliases: [], usage: ['datum spec show <FIELD>'], flags: [], subcommands: sub('show'), bare: false },
		// Two forms, no subcommands: `add` fills as it always did.
		{ name: 'add', aliases: [], usage: ['datum add <FIELD>', 'datum add <FIELD> <DUR>'], flags: [] }
	]);
	const field = { id: 'field', kind: 'field', text: 'mood' };

	test('a command with subcommands drills; alternate forms do not; spec under a field opens its spec instead', () => {
		assert.deepEqual(['db', 'sync', 'spec', 'add', 'tail'].map((n) => drills(TREE, n, [])), [true, true, true, false, false]);
		assert.equal(drills(TREE, 'spec', [field]), false, 'da-ybai9: the modal, not the subcommands');
		assert.equal(drills(TREE, 'db', [field]), true);
	});

	test('< Back and Help lead; a command that runs bare is offered as itself; each subcommand fills its whole line', () => {
		const bar = (name) => subcommandsOf(TREE, name).map((c) => [c.name, c.label, c.press]);
		assert.deepEqual(bar('db'), [
			['back', '< Back', 'back'],
			['help', 'Help', 'help'],
			['db setup', 'setup', 'fill'],
			// da-9ow8m: it opens «Backups…» rather than fill the line.
			['db backup', 'backup', 'sheet']
		]);
		assert.deepEqual(bar('sync').slice(2), [
			['sync', 'sync', 'fill'],
			['sync login', 'login', 'fill']
		]);
	});
});

// da-st2xl: holding a command opens its flags — the CLI's, in a curated
// order, each knowing whether a value follows it.
describe('flags', () => {
	const f = (short, long, takes_value = false) => ({ short, long, takes_value, help: `${long} help` });
	// The CLI's order, scrambled against the curated one on purpose.
	const TREE = grammarOf([
		{
			name: 'occur',
			aliases: [],
			usage: ['datum occur <FIELD>'],
			flags: [f('y', 'yesterday'), f('q', 'quick'), f('d', 'date', true), f('t', 'time', true), f('z', 'timezone', true), f('c', 'comment', true), f(null, 'id', true), f('m', 'moment')]
		}
	]);

	test('< Back and Help lead; the curated flags first, the rest in the CLI order; a value flag is typed next', () => {
		assert.deepEqual(
			flagsOf(TREE, 'occur').map((c) => [c.name, c.press, c.next ?? null]),
			[
				['back', 'back', null],
				['help', 'help', null],
				['-t', 'flag', 'type'],
				['-d', 'flag', 'type'],
				['-y', 'flag', 'tap'],
				['-q', 'flag', 'tap'],
				['-c', 'flag', 'type'],
				['-m', 'flag', 'tap'],
				['-z', 'flag', 'type'],
				['--id', 'flag', 'type']
			]
		);
		assert.equal(flagsOf(TREE, 'occur')[2].help, 'time help');
	});

	test('a flag appends to its own command\'s line, and starts any other line afresh', () => {
		assert.equal(withFlag('occur ', 'occur', '-q'), 'occur -q ');
		assert.equal(withFlag('occur -q ', 'occur', '-t'), 'occur -q -t ');
		assert.equal(withFlag('occur', 'occur', '-t'), 'occur -t ');
		// PR #216: after a run, and under another command.
		assert.equal(withFlag('', 'occur', '-q'), 'occur -q ');
		assert.equal(withFlag('tail ', 'occur', '-q'), 'occur -q ');
		assert.equal(withFlag('occurrence ', 'occur', '-q'), 'occur -q ');
	});
});
