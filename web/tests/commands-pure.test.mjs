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
	fieldVerb,
	flagLine,
	flagsOf,
	lanesOf,
	pressKind,
	typesKeys,
	rowsIn,
	subcommandsOf,
	verbPress,
	withFlag
} from '../src/lib/commands.ts';

/**
 * vividn's sections (da-0mepr), in order, each in the order he named them:
 * Undo (its redo an icon on it, da-zq91a) and Help where Redo was
 * (da-vjwid), the verbs, setup, the edits, analysis, the db. What he did
 * not name follows in its kind: `flag` logs, `config` sets up.
 */
const SECTIONS = [
	['undo', ['undo', 'help']],
	['log', ['occur', 'start', 'end', 'switch', 'add', 'note', 'flag']],
	['setup', ['track', 'ignore', 'spec', 'alias', 'kit', 'view', 'config']],
	['edit', ['delete', 'update', 'retime']],
	[
		'analysis',
		['tail', 'now', 'stats', 'get', 'check', 'info', 'fields', 'count', 'ids', 'history', 'grep', 'dayview', 'nowview']
	],
	['db', ['sync', 'db', 'conflicts']]
];
const ORDER = SECTIONS.flatMap(([, names]) => names);

describe('the commands, in order', () => {
	test('Undo first, then the verbs, then the other sections', () => {
		assert.deepEqual(
			COMMANDS.map((c) => c.name),
			ORDER
		);
		assert.equal(COMMANDS[0].label, 'Undo');
	});

	test('every command is in its section, and the sections come in order', () => {
		assert.deepEqual(
			COMMANDS.map((c) => [c.name, c.section]),
			SECTIONS.flatMap(([section, names]) => names.map((name) => [name, section]))
		);
	});

	// da-8ru0m: back on the bar, in the logging section — a tap under a
	// field pill writes the pill into the line before the words.
	test('note fills, in the logging section', () => {
		assert.deepEqual(COMMANDS.find((c) => c.name === 'note'), {
			name: 'note',
			label: 'note',
			press: 'fill',
			next: 'type',
			section: 'log'
		});
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

const point = { field: 'coffee', shape: { kind: 'point' }, required: [] };
const record = { field: 'rec', shape: { kind: 'record' }, required: [] };
const offBlock = { field: 'sleep', shape: { kind: 'state' }, required: [] };
const onBlock = { field: 'work', shape: { kind: 'state' }, current: true, required: [] };
const boolSwitch = { field: 'light', shape: { kind: 'states', states: ['false', 'true'] }, required: [] };
const onSwitch = { field: 'mood', shape: { kind: 'states', states: ['bad', 'good'] }, current: 'good', required: [] };
const ALL = [point, record, offBlock, onBlock, boolSwitch, onSwitch];

const under = (verb) => fieldsUnder(ALL, verb).map((b) => b.field);

describe('what a pending verb leaves showing', () => {
	test('occur, switch and add are the fields of that verb', () => {
		assert.deepEqual(under('occur'), ['coffee']);
		assert.deepEqual(under('switch'), ['light', 'mood']);
		assert.deepEqual(under('add'), ['rec']);
	});

	// A block takes start and end whatever its state (da-vpzbv): re-logging
	// one is how a forgotten start or end is recorded. A switch still reads
	// `current` — `now --json`'s truth — for `end`.
	test('start: every block, and a switch with true among its states', () => {
		assert.deepEqual(under('start'), ['sleep', 'work', 'light']);
	});

	test('end: every block, and the switches that are on', () => {
		assert.deepEqual(under('end'), ['sleep', 'work', 'mood']);
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

	// da-vpzbv, vividn: «show both start and end … regardless of the current
	// state». Running or stopped, a block offers both, and each press runs.
	test('a block field offers start and end, running or stopped', () => {
		for (const block of [onBlock, offBlock]) {
			assert.deepEqual(verbsUnder([pill(block)]), ['start', 'end'], block.field);
			for (const verb of ['start', 'end']) {
				assert.deepEqual(verbPress(verb, block), [verb, block.field]);
				assert.equal(pressKind(block, verb), 'run', `${verb} ${block.field}`);
			}
		}
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

// da-lxqqz, vividn: «when a field is selected, then the start/end/occur
// buttons should … auto trigger entry (unless there are extra keys to be
// filled in…». Required keys only: one with a default does not stop it.
describe('a verb under a field pill', () => {
	const keyed = { ...point, required: ['amount'] };
	const fields = [{ ...point, required: [] }, keyed, { ...onBlock, required: [] }];

	test('occur, start and end run a field that needs nothing typed', () => {
		assert.deepEqual(fieldVerb('occur', [pill(point)], fields), { field: pill(point), runs: true });
		for (const verb of ['start', 'end']) assert.equal(fieldVerb(verb, [pill(onBlock)], fields)?.runs, true, verb);
	});

	test('a key with no default fills instead, with the pill to go on the line', () => {
		assert.deepEqual(fieldVerb('occur', [pill(keyed)], [keyed]), { field: pill(keyed), runs: false });
	});

	// da-o4jaa, vividn: «generally speaking I don't save any extra_data in this one».
	test('end runs whatever keys the field declares; start still fills', () => {
		const block = { ...onBlock, required: ['project'] };
		assert.deepEqual(fieldVerb('end', [pill(block)], [block]), { field: pill(block), runs: true });
		assert.equal(fieldVerb('start', [pill(block)], [block])?.runs, false);
	});

	test('no field pill, another verb, an unknown field, or a context field: the press as it was', () => {
		assert.equal(fieldVerb('occur', [], fields), null);
		for (const verb of ['switch', 'add', 'tail']) assert.equal(fieldVerb(verb, [pill(point)], fields), null, verb);
		assert.equal(fieldVerb('occur', [{ id: 'field', kind: 'field', text: 'brandnew' }], fields), null);
		assert.equal(fieldVerb('occur', [pill(point), { id: 'hid', kind: 'hid', text: 'abcde' }], fields), null);
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
		assert.equal(pressKind(onSwitch, null), 'show');
		assert.equal(pressKind(boolSwitch, null), 'show');
		assert.equal(pressKind(record, null), 'fill');
	});

	test('under a verb, every field it leaves runs — but where verbPress asks first', () => {
		for (const verb of VERBS)
			for (const button of fieldsUnder(ALL, verb))
				assert.equal(
					pressKind(button, verb),
					verbPress(verb, button) ? 'run' : button.shape.kind === 'record' ? 'fill' : 'show',
					`${verb} ${button.field}`
				);
		assert.equal(pressKind(onSwitch, 'switch'), 'show');
		assert.equal(pressKind(record, 'add'), 'fill');
		assert.equal(pressKind(boolSwitch, 'start'), 'run');
	});

	// da-chml5, vividn: «occur/start/end run at once with a double outline,
	// unless there are required keys only, in which case fill the line».
	test('a field with a required key fills, bare or under a verb — but end still runs', () => {
		const tea = { ...point, field: 'tea', required: ['amount'] };
		const shift = { ...offBlock, required: ['project'] };
		const on = { ...onBlock, required: ['project'] };
		assert.equal(pressKind(tea, null), 'fill');
		assert.equal(pressKind(tea, 'occur'), 'fill');
		assert.equal(pressKind(shift, null), 'fill');
		assert.equal(pressKind(shift, 'start'), 'fill');
		assert.equal(pressKind(on, null), 'run', 'a tap on a running block ends it');
		assert.equal(pressKind(on, 'end'), 'run');
		assert.equal(pressKind({ ...onSwitch, required: ['why'] }, null), 'show', 'the ring still opens');
		assert.equal(typesKeys('switch', { ...onSwitch, required: ['why'] }), true, 'and its state fills');
		assert.equal(typesKeys('switch', onSwitch), false);
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
			// da-9ow8m: it opens «Backups» rather than fill the line.
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
		},
		{ name: 'stop', aliases: ['end'], usage: ['datum stop <FIELD>'], flags: [f('t', 'time', true), { ...f('L', 'last-state', true), value_optional: true }] }
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

	// da-bbmpx: a field tap under `occur -q ` runs `occur -q coffee`.
	test('a line of flags is what a field tap runs; a positional or a flag waiting for its value is not', () => {
		const occur = TREE.get('occur');
		assert.deepEqual(flagLine(['occur'], occur), ['occur']);
		assert.deepEqual(flagLine(['occur', '-q'], occur), ['occur', '-q']);
		assert.deepEqual(flagLine(['occur', '-t', '-8', '-qq', '--id', 'x'], occur), ['occur', '-t', '-8', '-qq', '--id', 'x']);
		assert.equal(flagLine(['occur', 'half-typed'], occur), null);
		assert.equal(flagLine(['occur', '-q', '-t'], occur), null);
		assert.equal(flagLine(['occur', '-q', '--time'], occur), null);
		// No grammar: a value cannot be told from a positional, nor a value
		// flag from a switch, so only a bare verb is taken (PR #219, kimi-k3).
		assert.equal(flagLine(['occur', '-t', '1400'], undefined), null);
		assert.equal(flagLine(['occur', '-t'], undefined), null);
		assert.deepEqual(flagLine(['occur'], undefined), ['occur']);
		// da-69mzq: `-L`'s value only ever comes attached, so a bare one waits for nothing.
		const stop = TREE.get('stop');
		assert.deepEqual(flagLine(['stop', '-L'], stop), ['stop', '-L']);
		assert.deepEqual(flagLine(['stop', '--last-state', '-t', '-8'], stop), ['stop', '--last-state', '-t', '-8']);
		assert.deepEqual(flagLine(['stop', '-L=done'], stop), ['stop', '-L=done']);
		assert.equal(flagLine(['stop', '-L', 'done'], stop), null);
		assert.equal(flagLine(['stop', '-L', '-t'], stop), null);
	});
});
