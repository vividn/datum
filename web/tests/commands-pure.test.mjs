// The commands widget's rules, in node, with no browser (da-ltux).
//
// `#lib/commands.ts` is pure but for its last function, and imports only
// a type, so node loads it as it is. What is checked here is the part a
// browser adds nothing to: the order the buttons come in, which verb a
// line is pending, what that verb leaves showing, what a press under it
// runs, and the arithmetic of the scroll rule. The widget as it is DRAWN
// — and the filtering as a reader sees it happen — is
// `commands.test.mjs`.

import test, { describe } from 'node:test';
import assert from 'node:assert/strict';
import { resolveLibAsSource } from './support/libhook.mjs';
import { VERBS as TRACK_VERBS } from '../src/lib/track.ts';
import {
	BACK,
	BASE,
	COMMANDS,
	ICONS,
	MENUS,
	iconOf,
	LOG_COMMANDS,
	commandsUnder,
	fieldsUnder,
	grammarOf,
	pendingCommand,
	drills,
	opensField,
	fieldCommand,
	flagLine,
	flagsOf,
	lanesOf,
	pressKind,
	typesKeys,
	rowsIn,
	subcommandsOf,
	suggestionOf,
	roleOf,
	argumentsOf,
	pendingPress,
	withFlag
} from '../src/lib/commands.ts';

/**
 * The base row and its submenus (da-fg4w4), vividn, 2026-11-03: «undo write
 * read setup kit db sync settings. Write and read expand into a submenu»,
 * `setup` become a `fields` submenu (13:31), with no nowview, dayview or ids
 * button anywhere and conflicts under sync.
 */
const ROW = ['undo', 'write', 'read', 'fields', 'kit', 'db', 'sync', 'settings'];
const SUBMENUS = {
	write: ['moment', 'on', 'off', 'change', 'record', 'remark', 'flag', 'update', 'retime', 'delete', 'revert'],
	read: ['tail', 'now', 'stats', 'get', 'history', 'grep', 'fields', 'count', 'info', 'check'],
	fields: ['setup', 'track', 'ignore', 'alias', 'rename'],
	settings: ['config', 'help'],
	sync: ['sync', 'conflicts']
};
const ORDER = [...new Set(['undo', 'help', ...SUBMENUS.write, ...SUBMENUS.fields, ...SUBMENUS.read, 'kit', 'db', 'sync', 'conflicts', 'config'])];

describe('the commands, in order', () => {
	test('the base row is eight: undo, then the four submenus around kit, db and sync', () => {
		assert.deepEqual(
			BASE.map((c) => c.name),
			ROW
		);
		assert.deepEqual(
			BASE.filter((c) => c.press === 'menu').map((c) => c.name),
			['write', 'read', 'fields', 'settings']
		);
	});

	test('each submenu holds its commands, in order', () => {
		assert.deepEqual(Object.fromEntries(Object.entries(MENUS).map(([name, commands]) => [name, commands.map((c) => c.name)])), SUBMENUS);
	});

	test('flat, a loaded pill sees every command once, and none of nowview, dayview, ids or view', () => {
		assert.deepEqual(
			COMMANDS.map((c) => c.name),
			ORDER
		);
		for (const name of ['nowview', 'dayview', 'ids', 'view'])
			assert.ok(![...BASE, ...COMMANDS, ...Object.values(MENUS).flat()].some((c) => c.name === name), name);
	});

	// da-8ru0m: a tap under a field pill writes the pill into the line before the words.
	test('remark fills', () => {
		assert.deepEqual(COMMANDS.find((c) => c.name === 'remark'), {
			name: 'remark',
			label: 'remark',
			press: 'fill',
			next: 'type'
		});
	});

	test('undo runs; the five log commands pend; everything else writes a line', () => {
		const pressOf = (name) => COMMANDS.find((c) => c.name === name).press;
		assert.equal(pressOf('undo'), 'run');
		assert.equal(pressOf('help'), 'help');
		for (const verb of LOG_COMMANDS) assert.equal(pressOf(verb), 'pend', verb);
		for (const name of ['tail', 'db', 'kit', 'track']) assert.equal(pressOf(name), 'fill', name);
	});

	// da-6du3o: one button, sync, opening its place rather than drilling into nineteen.
	test('sync is one button that opens its place, and so is conflicts', () => {
		assert.deepEqual(BASE.find((c) => c.name === 'sync'), { name: 'sync', label: 'sync', press: 'place' });
		assert.equal(COMMANDS.find((c) => c.name === 'conflicts').press, 'place');
	});
});

// da-y6jfb, da-fg4w4: every command a circle with its own mark in it.
describe('the icons', () => {
	const drawn = [...BASE, ...COMMANDS, ...Object.values(MENUS).flat(), BACK];

	test('every button has one, and no two share it', () => {
		const marks = new Map(drawn.map((c) => [`${c.press === 'menu' ? 'menu ' : ''}${c.name}`, iconOf(c)]));
		assert.deepEqual([...marks].filter(([, icon]) => !icon), [], 'a button with no mark');
		assert.equal(new Set(marks.values()).size, marks.size, 'two buttons share a mark');
	});

	test('a subcommand wears its command\'s', () => {
		assert.equal(iconOf({ name: 'db backup', label: 'backup', press: 'backups' }), ICONS.db);
	});

	test('a verb wears the glyph the CLI prints for the row it writes', () => {
		// track.ts's table is the one track.test.mjs holds to the CLI's render.
		for (const { verb, glyph } of TRACK_VERBS) assert.equal(ICONS[{ occur: 'moment', start: 'on', switch: 'change', add: 'record' }[verb]], glyph, verb);
	});
});

// da-uzyih.4898, vividn, 2026-10-29: the setup widget's shape buttons «should probably keep 1 liners»; what more there
// is to say of each is docs/basics.md (da-uzyih.4908).
describe('the shapes', () => {
	test('a shape button says its shape in a line', () => {
		for (const { verb, line } of TRACK_VERBS) assert.ok(!/[.!?]\s/.test(line), `${verb}'s button says more than a line: ${line}`);
	});
});

describe('the command a line is pending', () => {
	test('the first word, whole', () => {
		assert.equal(pendingCommand('moment '), 'moment');
		assert.equal(pendingCommand('moment'), 'moment');
		assert.equal(pendingCommand('occur coffee'), 'moment', 'its old word is the same command pending');
		assert.equal(pendingCommand('  change mood happy'), 'change');
		assert.equal(pendingCommand('off work'), 'off');
	});

	test('an old word pends its command, as the CLI runs it', () => {
		const old = { occur: 'moment', start: 'on', end: 'off', stop: 'off', switch: 'change', add: 'record', card: 'record' };
		for (const [word, command] of Object.entries(old)) assert.equal(pendingCommand(`${word} x`), command, word);
	});

	test('anything else pends nothing', () => {
		for (const line of ['', '   ', 'tail', 'occurrence x', 'coffee moment', '-t 09:00'])
			assert.equal(pendingCommand(line), null, JSON.stringify(line));
	});
});

const point = { field: 'coffee', shape: { kind: 'moment' }, required: [] };
const record = { field: 'rec', shape: { kind: 'record' }, required: [] };
const offBlock = { field: 'sleep', shape: { kind: 'on/off' }, required: [] };
const onBlock = { field: 'work', shape: { kind: 'on/off' }, current: true, required: [] };
const boolSwitch = { field: 'light', shape: { kind: 'change', states: ['false', 'true'] }, required: [] };
const onSwitch = { field: 'mood', shape: { kind: 'change', states: ['bad', 'good'] }, current: 'good', required: [] };
const ALL = [point, record, offBlock, onBlock, boolSwitch, onSwitch];

const under = (verb) => fieldsUnder(ALL, verb).map((b) => b.field);

describe('what a pending command leaves showing', () => {
	test('moment, switch and add are the fields of that verb', () => {
		assert.deepEqual(under('moment'), ['coffee']);
		assert.deepEqual(under('change'), ['light', 'mood']);
		assert.deepEqual(under('record'), ['rec']);
	});

	// A block takes start and end whatever its state (da-vpzbv): re-logging
	// one is how a forgotten start or end is recorded. A switch still reads
	// `current` — `now --json`'s truth — for `end`.
	test('start: every block, and a switch with true among its states', () => {
		assert.deepEqual(under('on'), ['sleep', 'work', 'light']);
	});

	test('end: every block, and the switches that are on', () => {
		assert.deepEqual(under('off'), ['sleep', 'work', 'mood']);
	});

	test('no verb leaves the list whole', () => {
		assert.deepEqual(under(null), ALL.map((b) => b.field));
	});
});

// da-57j14: a loaded field narrows the LOG_COMMANDS by the same rule a pending
// verb narrows the fields with (`commandFits`). Every verb names its field at
// slot 1, as `datum help --json` says, so the pill itself hides none.
const GRAMMAR = grammarOf(
	['moment', 'on', 'off', 'change', 'record'].map((name) => ({
		name,
		aliases: [],
		usage: [`datum ${name} <FIELD>`],
		flags: []
	}))
);
const pill = (b) => ({ id: 'field', kind: 'field', text: b.field });
const verbsUnder = (pills) =>
	commandsUnder(GRAMMAR, pills, ALL)
		.map((c) => c.name)
		.filter((name) => LOG_COMMANDS.includes(name));

describe('what a loaded field leaves of the verbs', () => {
	test('a moment field leaves moment, and not start, end or switch', () => {
		assert.deepEqual(verbsUnder([pill(point)]), ['moment']);
	});

	// `end` only while it is on: off, there is nothing to end.
	test('a switch with true among its states offers switch, start and end', () => {
		assert.deepEqual(verbsUnder([pill(boolSwitch)]), ['on', 'change']);
		const on = { ...boolSwitch, current: 'true' };
		const verbs = commandsUnder(GRAMMAR, [pill(on)], [on]).map((c) => c.name);
		assert.deepEqual(verbs.filter((name) => LOG_COMMANDS.includes(name)), ['on', 'off', 'change']);
	});

	// da-tu30m: beside an hid the field is CONTEXT and narrows no command.
	// Invisible with the real grammar — no verb takes an id, so the hid
	// hides all five anyway — so here each verb takes one too.
	test('a context field (beside an hid) narrows no verb', () => {
		const byId = grammarOf(
			LOG_COMMANDS.map((name) => ({ name, aliases: [], usage: [`datum ${name} <FIELD>`, `datum ${name} <ID_OR_SLUG>`], flags: [] }))
		);
		const hid = { id: 'hid', kind: 'hid', text: 'abcde' };
		const verbs = (pills) =>
			commandsUnder(byId, pills, ALL)
				.map((c) => c.name)
				.filter((name) => LOG_COMMANDS.includes(name));
		assert.deepEqual(verbs([pill(point)]), ['moment'], 'the argument field narrows');
		assert.deepEqual(verbs([pill(point), hid]), LOG_COMMANDS);
	});

	// da-vpzbv, vividn: «show both start and end … regardless of the current
	// state». Running or stopped, a block offers both, and each press runs.
	test('a block field offers start and end, running or stopped', () => {
		for (const block of [onBlock, offBlock]) {
			assert.deepEqual(verbsUnder([pill(block)]), ['on', 'off'], block.field);
			for (const verb of ['on', 'off']) {
				assert.deepEqual(pendingPress(verb, block), [verb, block.field]);
				assert.equal(pressKind(block, verb), 'run', `${verb} ${block.field}`);
			}
		}
	});

	test('clearing the pill brings every verb back', () => {
		assert.deepEqual(verbsUnder([]), LOG_COMMANDS);
	});

	test('a field with no button yet keeps every verb: it has no shape to narrow by', () => {
		assert.deepEqual(verbsUnder([{ id: 'field', kind: 'field', text: 'brandnew' }]), LOG_COMMANDS);
	});

	// The property: for every field kind × verb, the verb shows under the
	// field exactly when the field shows under the verb.
	test('the two directions agree for every field and every verb', () => {
		for (const b of ALL)
			for (const verb of LOG_COMMANDS)
				assert.equal(
					verbsUnder([pill(b)]).includes(verb),
					fieldsUnder(ALL, verb).includes(b),
					`${verb} × ${b.field}`
				);
	});
});

// da-lxqqz, vividn: «when a field is selected, then the start/end/moment
// buttons should … auto trigger entry (unless there are extra keys to be
// filled in…». Required keys only: one with a default does not stop it.
describe('a verb under a field pill', () => {
	const keyed = { ...point, required: ['amount'] };
	const fields = [{ ...point, required: [] }, keyed, { ...onBlock, required: [] }];

	test('moment, start and end run a field that needs nothing typed', () => {
		assert.deepEqual(fieldCommand('moment', [pill(point)], fields), { field: pill(point), runs: true });
		for (const verb of ['on', 'off']) assert.equal(fieldCommand(verb, [pill(onBlock)], fields)?.runs, true, verb);
	});

	test('a key with no default fills instead, with the pill to go on the line', () => {
		assert.deepEqual(fieldCommand('moment', [pill(keyed)], [keyed]), { field: pill(keyed), runs: false });
	});

	// da-o4jaa, vividn: «generally speaking I don't save any extra_data in this one».
	test('off runs whatever keys the field declares; on still fills', () => {
		const block = { ...onBlock, required: ['project'] };
		assert.deepEqual(fieldCommand('off', [pill(block)], [block]), { field: pill(block), runs: true });
		assert.equal(fieldCommand('on', [pill(block)], [block])?.runs, false);
	});

	test('no field pill, another verb, an unknown field, or a context field: the press as it was', () => {
		assert.equal(fieldCommand('moment', [], fields), null);
		for (const verb of ['change', 'record', 'tail']) assert.equal(fieldCommand(verb, [pill(point)], fields), null, verb);
		assert.equal(fieldCommand('moment', [{ id: 'field', kind: 'field', text: 'brandnew' }], fields), null);
		assert.equal(fieldCommand('moment', [pill(point), { id: 'hid', kind: 'hid', text: 'abcde' }], fields), null);
	});
});

describe('what a press under a verb runs', () => {
	test('`<verb> <field>`, at once', () => {
		assert.deepEqual(pendingPress('moment', point), ['moment', 'coffee']);
		assert.deepEqual(pendingPress('on', offBlock), ['on', 'sleep']);
		assert.deepEqual(pendingPress('off', onSwitch), ['off', 'mood']);
		// A switch with `true` among its states is startable AS a block.
		assert.deepEqual(pendingPress('on', boolSwitch), ['on', 'light']);
	});

	// The two argvs that are not commands: the field's own courtesy takes
	// over and still ends in the pending verb.
	test('change on a many-stated field, and record on a record, ask first', () => {
		assert.equal(pendingPress('change', onSwitch), null);
		assert.equal(pendingPress('record', record), null);
	});
});

// What a tap on a field DOES, which its outline is drawn from (da-z8z40):
// the bead's table, row by row. `run` is the double outline.
describe('what a field tap does', () => {
	test('with no verb: a record fills, and every other field opens in collect (da-w6437)', () => {
		assert.equal(pressKind(point, null), 'show');
		assert.equal(pressKind(offBlock, null), 'show');
		assert.equal(pressKind(onBlock, null), 'show');
		assert.equal(pressKind(onSwitch, null), 'show');
		assert.equal(pressKind(boolSwitch, null), 'show');
		assert.equal(pressKind(record, null), 'fill');
	});

	// da-gra0z, vividn 2026-11-04: «you can still record with 1 tap unless
	// there is some extra data or whatever needed» — one click (collect's
	// default, and a tutorial game's tap, da-uzyih.4940) logs a moment or a
	// block and opens a switch's states; two clicks open every one.
	test('one click logs a moment or block and opens a switch’s states; two clicks open every one', () => {
		const tea = { ...point, field: 'tea', required: ['amount'] };
		const walk = { ...offBlock, field: 'walk', required: ['route'] };
		const walking = { ...onBlock, field: 'walk', required: ['route'] };
		for (const [button, one] of [
			[point, 'run'],
			[offBlock, 'run'],
			[onBlock, 'run'],
			[tea, 'fill'],
			[walk, 'fill'],
			// `end` takes no keys (da-o4jaa).
			[walking, 'run'],
			[onSwitch, 'show'],
			[boolSwitch, 'show'],
			[record, 'fill']
		]) {
			assert.equal(pressKind(button, null, true), one, `one click on ${button.field}`);
			assert.equal(pressKind(button, null), button === record ? 'fill' : 'show', `two clicks on ${button.field}`);
		}
	});

	// da-uzyih.4941: a pending verb is answered before one click is asked.
	test('one click changes nothing under a verb', () => {
		for (const verb of LOG_COMMANDS)
			for (const button of fieldsUnder(ALL, verb)) assert.equal(pressKind(button, verb, true), pressKind(button, verb), `${verb} ${button.field}`);
		assert.equal(pressKind(onSwitch, 'change', true), 'show');
	});

	test('under a verb, every field it leaves runs — but where pendingPress asks first', () => {
		for (const verb of LOG_COMMANDS)
			for (const button of fieldsUnder(ALL, verb))
				assert.equal(
					pressKind(button, verb),
					pendingPress(verb, button) ? 'run' : button.shape.kind === 'record' ? 'fill' : 'show',
					`${verb} ${button.field}`
				);
		assert.equal(pressKind(onSwitch, 'change'), 'show');
		assert.equal(pressKind(record, 'record'), 'fill');
		assert.equal(pressKind(boolSwitch, 'on'), 'run');
	});

	// da-chml5, vividn: «moment/start/end run at once with a double outline,
	// unless there are required keys only, in which case fill the line».
	test('a field with a required key fills under a verb — but end still runs, and a bare tap opens it', () => {
		const tea = { ...point, field: 'tea', required: ['amount'] };
		const shift = { ...offBlock, required: ['project'] };
		const on = { ...onBlock, required: ['project'] };
		assert.equal(pressKind(tea, null), 'show');
		assert.equal(pressKind(tea, 'moment'), 'fill');
		assert.equal(pressKind(shift, null), 'show');
		assert.equal(pressKind(shift, 'on'), 'fill');
		assert.equal(pressKind(on, null), 'show');
		assert.equal(pressKind(on, 'off'), 'run');
		assert.equal(pressKind({ ...onSwitch, required: ['why'] }, null), 'show', 'the ring still opens');
		assert.equal(typesKeys('change', { ...onSwitch, required: ['why'] }), true, 'and its state fills');
		assert.equal(typesKeys('change', onSwitch), false);
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
		const items = ['Undo', 'Redo', 'moment', 'on', 'off', 'change', 'record'];
		assert.deepEqual(lanesOf(items, 1), [items]);
		assert.deepEqual(lanesOf(items, 2), [
			['Undo', 'moment', 'off', 'record'],
			['Redo', 'on', 'change']
		]);
		assert.deepEqual(lanesOf(items, 3), [
			['Undo', 'on', 'record'],
			['Redo', 'off'],
			['moment', 'change']
		]);
		assert.deepEqual(lanesOf(['+', 'coffee'], 3), [['+'], ['coffee']]);
		assert.deepEqual(lanesOf([], 2), []);
	});
});

// The kind itself: pickable, and with the one-widget layout every pickable
// kind has (da-y2f6's rule, held generally by `tree.test.mjs`). `command`
// is the id da-zqop freed by renaming the old one-line widget to `custom`.
describe('the `command` kind', () => {
	resolveLibAsSource();

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
		{ name: 'db', aliases: [], usage: ['datum db migrate', 'datum db backup <PATH | ->'], flags: [], subcommands: sub('migrate', 'backup'), bare: false },
		{ name: 'conflicts', aliases: [], usage: ['datum conflicts [list] [--json]'], flags: [], subcommands: sub('resolve'), bare: true },
		// da-ez40f: `setup` has no subcommands, its field the first word.
		{ name: 'setup', aliases: [], usage: ['datum setup [--tag T]', 'datum setup <FIELD> [--json]'], flags: [], subcommands: [], bare: true },
		// Two forms, no subcommands: `add` fills as it always did.
		{ name: 'record', aliases: ['add', 'card'], usage: ['datum record <FIELD>', 'datum record <FIELD> <DUR>'], flags: [] }
	]);

	test('a command with subcommands drills; alternate forms do not; setup opens the setup widget instead, and track does not', () => {
		assert.deepEqual(['db', 'conflicts', 'setup', 'record', 'tail'].map((n) => drills(TREE, n)), [true, true, false, false, false]);
		// da-hbrfg: the spec is a place, so that tap OPENS, and draws the `›`.
		assert.deepEqual([opensField('setup'), opensField('db')], [true, false]);
		// da-fg4w4, vividn: «track shouldn't open the setup widget, it should just put the word track in the command line».
		assert.equal(opensField('track'), false);
	});

	test('back and Help lead; a command that runs bare is offered as itself; each subcommand fills its whole line', () => {
		const bar = (name) => subcommandsOf(TREE, name).map((c) => [c.name, c.label, c.press]);
		assert.deepEqual(bar('db'), [
			['back', 'back', 'back'],
			['help', 'help', 'help'],
			['db migrate', 'migrate', 'fill'],
			// da-9ow8m: it opens «Backups» rather than fill the line.
			['db backup', 'backup', 'backups']
		]);
		assert.deepEqual(bar('conflicts').slice(2), [
			['conflicts', 'conflicts', 'fill'],
			['conflicts resolve', 'resolve', 'fill']
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
			name: 'moment',
			aliases: [],
			usage: ['datum moment <FIELD>'],
			flags: [f('y', 'yesterday'), f('q', 'quick'), f('d', 'date', true), f('t', 'time', true), f('z', 'timezone', true), f('c', 'comment', true), f(null, 'id', true), f('m', 'moment')]
		},
		{ name: 'off', aliases: ['end', 'stop'], usage: ['datum off <FIELD>'], flags: [f('t', 'time', true), f('L', 'ensure-last-state', true)] }
	]);

	test('back and Help lead; the curated flags first, the rest in the CLI order; a value flag is typed next', () => {
		assert.deepEqual(
			flagsOf(TREE, 'moment').map((c) => [c.name, c.press, c.next ?? null]),
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
		assert.equal(flagsOf(TREE, 'moment')[2].help, 'time help');
	});

	test('a flag appends to its own command\'s line, and starts any other line afresh', () => {
		assert.equal(withFlag('moment ', 'moment', '-q'), 'moment -q ');
		assert.equal(withFlag('moment -q ', 'moment', '-t'), 'moment -q -t ');
		assert.equal(withFlag('moment', 'moment', '-t'), 'moment -t ');
		// PR #216: after a run, and under another command.
		assert.equal(withFlag('', 'moment', '-q'), 'moment -q ');
		assert.equal(withFlag('tail ', 'moment', '-q'), 'moment -q ');
		assert.equal(withFlag('occurrence ', 'moment', '-q'), 'moment -q ');
	});

	// da-bbmpx: a field tap under `moment -q ` runs `moment -q coffee`.
	test('a line of flags is what a field tap runs; a positional or a flag waiting for its value is not', () => {
		const moment = TREE.get('moment');
		assert.deepEqual(flagLine(['moment'], moment), ['moment']);
		assert.deepEqual(flagLine(['moment', '-q'], moment), ['moment', '-q']);
		assert.deepEqual(flagLine(['moment', '-t', '-8', '-qq', '--id', 'x'], moment), ['moment', '-t', '-8', '-qq', '--id', 'x']);
		assert.equal(flagLine(['moment', 'half-typed'], moment), null);
		assert.equal(flagLine(['moment', '-q', '-t'], moment), null);
		assert.equal(flagLine(['moment', '-q', '--time'], moment), null);
		// No grammar: a value cannot be told from a positional, nor a value
		// flag from a switch, so only a bare verb is taken (PR #219, kimi-k3).
		assert.equal(flagLine(['moment', '-t', '1400'], undefined), null);
		assert.equal(flagLine(['moment', '-t'], undefined), null);
		assert.deepEqual(flagLine(['moment'], undefined), ['moment']);
		// da-n86g3: `-L` takes its word, as v2's did, so a bare one waits for it.
		const stop = TREE.get('off');
		assert.equal(flagLine(['off', '-L'], stop), null);
		assert.deepEqual(flagLine(['off', '-L', 'done'], stop), ['off', '-L', 'done']);
		assert.deepEqual(flagLine(['off', '-L=done'], stop), ['off', '-L=done']);
	});
});

// da-h3jcu: the command the pills make ready leads the command pane, by the
// field's shape as a bare tap reads it, dashed while something is missing.
describe('the ready command', () => {
	const colored = (b, color, stateColors = {}) => ({ ...b, color, stateColors });
	const FIELDS = [
		colored(point, '#aa0000'),
		colored(record, '#00aa00'),
		colored(offBlock, '#0000aa'),
		colored(onBlock, '#aaaa00'),
		colored(onSwitch, '#00aaaa', { bad: '#111111', good: '#222222' }),
		colored({ ...point, field: 'pill', required: ['dose'] }, '#aa00aa')
	];
	const field = (text) => ({ id: 'field', kind: 'field', text });
	const state = (text, color) => ({ id: 'state', kind: 'state', text, ...(color ? { color } : {}) });
	const ready = (pills) => {
		const s = suggestionOf(pills, FIELDS);
		return s && { label: s.label, ready: s.ready, color: s.color, argv: s.argv };
	};

	test('a moment field alone is ready, in its color', () => {
		assert.deepEqual(ready([field('coffee')]), { label: 'moment coffee', ready: true, color: '#aa0000', argv: ['moment'] });
	});

	test('a block starts, or ends while it is on', () => {
		assert.deepEqual(ready([field('sleep')]), { label: 'on sleep', ready: true, color: '#0000aa', argv: ['on'] });
		assert.deepEqual(ready([field('work')]), { label: 'off work', ready: true, color: '#aaaa00', argv: ['off'] });
	});

	test('a switch waits for its state, then runs to it in the state color', () => {
		assert.deepEqual(ready([field('mood')]), { label: 'change → ?', ready: false, color: '#00aaaa', argv: ['change'] });
		assert.deepEqual(ready([field('mood'), state('bad')]), { label: 'change → bad', ready: true, color: '#111111', argv: ['change'] });
		assert.equal(ready([field('mood'), state('meh', '#333333')]).color, '#333333', "the pill's own color first");
	});

	test('a record, or a key with no default, is dashed', () => {
		assert.equal(ready([field('rec')]).ready, false);
		assert.equal(ready([field('pill')]).ready, false);
	});

	test('no field, an unknown one, or one an hid makes context suggests nothing', () => {
		assert.equal(ready([]), null);
		assert.equal(ready([{ id: 't', kind: 'time', text: '-t 09:00' }]), null);
		assert.equal(ready([field('nope')]), null);
		assert.equal(ready([field('coffee'), { id: 'hid', kind: 'hid', text: 'abc' }]), null);
	});

	// collect's state pill (da-w6437) stands right after the field's, and a
	// press places both: `switch` is `switch mood bad`.
	test('a state pill is an argument, placed after the field', () => {
		assert.equal(roleOf(state('bad'), [field('mood'), state('bad')]), 'argument');
		assert.deepEqual(argumentsOf([field('mood'), state('bad')]).map((p) => p.kind), ['field', 'state']);
	});
});
