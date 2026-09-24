// The pills a command takes, and the selection a tap makes, in node (da-r13j).
//
// `takesOf` reads the CLI's own usage lines — the ones below are copied
// from `datum help --json` — and `withPills` / `commandsUnder` decide from
// that what a press runs and which buttons are active. `picked` and
// `hidPill` are the selection, which lives nowhere but the `hid` pill. The
// same rules as a reader meets them are `selection.test.mjs`.

import test, { describe } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';

registerHooks({
	resolve(specifier, context, next) {
		if (/^\.\/.*\.js$/.test(specifier) && context.parentURL?.includes('/src/lib/'))
			return next(specifier.replace(/\.js$/, '.ts'), context);
		return next(specifier, context);
	}
});

const { COMMANDS, commandsUnder, fires, grammarOf, takesOf } = await import('../src/lib/commands.ts');
const { hidPill, picked, pickOf, withPills } = await import('../src/lib/selection.ts');

const entry = (name, usage, flags = [], aliases = []) => ({ name, aliases, usage, flags });
const T = { short: 't', long: 'time' };

/** A slice of `datum help --json`, verbatim where it matters. */
const GRAMMAR = grammarOf([
	entry('occur', ['datum occur <FIELD> [VALUE | KEY=VALUE]...'], [T]),
	entry('stop', ['datum stop <FIELD> [VALUE | KEY=VALUE]... [DUR]'], [T], ['end']),
	entry('tail', ['datum tail [FIELD]'], [T]),
	entry('now', ['datum now [-A] [--bare]']),
	entry('get', ['datum get <ID_OR_SLUG> [--pretty]']),
	entry('update', ['datum update <ID_OR_SLUG[,ID_OR_SLUG]...> [VALUE | KEY=VALUE]...']),
	entry('retime', [
		'datum retime [SLUG[,SLUG]...] [SHIFT]',
		'datum retime [SLUG[,SLUG]...] [-r] [-t T] [-y]... [-q]... [-d D]'
	]),
	entry('history', ['datum history <ID_OR_SLUG> [--version N | --diff A B]']),
	entry('delete', ['datum delete [ID_OR_SLUG[,ID_OR_SLUG]...] [--force] [--dry-run] [--strict]'], [], ['del']),
	entry('undo', ['datum undo [ID_OR_SLUG] [--to-version N]']),
	entry('flag', ['datum flag <FIELD> [COMMENT...]']),
	entry('reparse', ['datum reparse <HID>... [--fix]']),
	entry('conflicts', ['datum conflicts resolve <HID> (--take-local | --take-remote)']),
	// The field pill's own cases (da-8810n): a field at slot 1, a field
	// behind a SUBCOMMAND at slot 2, a field behind a FLAG (not a slot at
	// all), and `stats`, whose `<FIELD[:STATE]>` must not read as one.
	entry('stats', ['datum stats <FIELD[:STATE]> [SLICE] [--since TIME]']),
	entry('spec', ['datum spec show <FIELD> [--text]', 'datum spec list [--tag T]']),
	entry('grep', ['datum grep <PATTERN>... [-f FIELD]']),
	entry('count', ['datum count [FIELD] [--group-by KEY]'])
]);

const time = { id: 'c', kind: 'time', text: '-t 14:30' };
const one = hidPill(['abcde12345xy']);
const two = hidPill(['abcde12345xy', 'fghij']);
/** The field pill a hold on `mood` mints (da-8810n): the bare field name. */
const mood = { id: 'field', kind: 'field', text: 'mood', color: '#dc27c0' };

describe('what the grammar says a command takes', () => {
	test('an id right after the name — one, or a comma list', () => {
		const id = (name) => GRAMMAR.get(name).id;
		assert.deepEqual(
			['get', 'undo', 'history', 'reparse', 'update', 'retime', 'delete', 'del'].map(id),
			['one', 'one', 'one', 'one', 'list', 'list', 'list', 'list']
		);
		// A FIELD is not an id, and neither is an id behind a subcommand.
		assert.deepEqual(['occur', 'flag', 'now', 'conflicts'].map(id), [false, false, false, false]);
	});

	test('-t from the flag table or from a usage line, aliases included', () => {
		const t = (name) => GRAMMAR.get(name).time;
		assert.deepEqual(['occur', 'end', 'tail', 'retime'].map(t), [true, true, true, true]);
		assert.deepEqual(['now', 'get', 'delete'].map(t), [false, false, false]);
	});

	test('takesOf reads one entry', () => {
		assert.deepEqual(takesOf(entry('history', ['datum history <ID_OR_SLUG> [--version N | --diff A B]'])), {
			id: 'one',
			time: false,
			field: false,
			more: false
		});
	});

	// da-8810n. The SLOT, not a boolean: a field pill's word has to land in
	// the positional the command keeps for it, and `spec show <FIELD>` keeps
	// the second.
	test('a FIELD is the positional it sits at — 1, or 2 behind a subcommand', () => {
		const field = (name) => GRAMMAR.get(name).field;
		assert.deepEqual(['occur', 'end', 'tail', 'flag', 'stats', 'count'].map(field), [1, 1, 1, 1, 1, 1]);
		assert.equal(field('spec'), 2, 'the field is behind `show`');
		// Not a positional at all: a bare word in `grep`'s place is a second
		// PATTERN, and `get`/`now` name no field.
		assert.deepEqual(['grep', 'get', 'now', 'delete'].map(field), [false, false, false, false]);
	});
});

// da-3lxol. Which taps RUN is read off the same grammar, never listed:
// an id command runs on a selected hid unless its usage has a word still
// to come after the id — `update`'s VALUE, `retime`'s SHIFT, which a time
// pill supplies and nothing else does.
describe('what a tap runs rather than fills', () => {
	const firing = (pills) => COMMANDS.map((c) => c.name).filter((n) => fires(GRAMMAR.get(n), pills));

	test('the word after the id is read as a word, a flag group is not', () => {
		const more = (name) => GRAMMAR.get(name).more;
		assert.deepEqual(['update', 'retime'].map(more), [true, true]);
		assert.deepEqual(['get', 'history', 'delete', 'undo', 'reparse'].map(more), [false, false, false, false, false]);
	});

	// Review 6642/6673: a stray closer must not take the depth below zero,
	// where whitespace stops splitting and the VALUE after it would be glued
	// onto the flag before it — read as a flag, and `more` lost.
	test('a stray closer does not glue the words after it into one', () => {
		assert.equal(takesOf(entry('x', ['datum x <HID> [--fix]] VALUE'])).more, true);
	});

	test('one hid: delete, get, history, undo run; update and retime fill', () => {
		assert.deepEqual(firing([one]), ['undo', 'delete', 'history', 'get']);
		assert.deepEqual(firing([two]), ['delete'], 'only a comma list takes two');
	});

	test('a time beside the hid completes retime, and only a command that takes -t', () => {
		assert.deepEqual(firing([one, time]), ['retime']);
	});

	test('nothing fires without an hid', () => {
		assert.deepEqual(firing([]), []);
		assert.deepEqual(firing([time]), []);
		assert.deepEqual(firing([mood]), []);
		assert.equal(fires(undefined, [one]), false, 'a command the grammar does not know');
	});
});

describe('what a press runs with the pills', () => {
	test('the hid goes right after the name, -t after the words', () => {
		assert.deepEqual(withPills(GRAMMAR, ['retime', '-5'], [one, time]).argv, [
			'retime',
			'abcde12345xy',
			'-5',
			'-t',
			'14:30'
		]);
		assert.deepEqual(withPills(GRAMMAR, ['delete'], [two]).argv, ['delete', 'abcde12345xy,fghij']);
	});

	// `update A B note=x` takes B as a STATE (measured, vremya 09-17): several
	// hids are one comma-joined word, never words of their own.
	test('several hids are ONE word, joined by commas', () => {
		assert.deepEqual(withPills(GRAMMAR, ['update', 'note=x'], [two]).argv, ['update', 'abcde12345xy,fghij', 'note=x']);
	});

	// da-8810n. The field goes in the command's OWN field slot — after
	// `spec show`, not after `spec` — and BEFORE the other pills, since
	// `tail -t 14:30 mood` is not the command `tail mood -t 14:30` is.
	test('a field pill fills the field slot, wherever that is, ahead of the other pills', () => {
		assert.deepEqual(withPills(GRAMMAR, ['tail'], [mood]).argv, ['tail', 'mood']);
		assert.deepEqual(withPills(GRAMMAR, ['spec', 'show'], [mood]).argv, ['spec', 'show', 'mood']);
		assert.deepEqual(withPills(GRAMMAR, ['stats'], [mood]).argv, ['stats', 'mood']);
		assert.deepEqual(withPills(GRAMMAR, ['tail'], [mood, time]).argv, ['tail', 'mood', '-t', '14:30']);
	});

	// THE SILENT ONE. datum's line is field-first, so `occur coffee` with a
	// `mood` pill on the bar would log `mood` as coffee's STATE — a wrong
	// row, not an error. The slot is taken, so the pill is refused and stays.
	test('a command that already names a field does not get a second one', () => {
		const { argv, used } = withPills(GRAMMAR, ['occur', 'coffee'], [mood, time]);
		assert.deepEqual(argv, ['occur', 'coffee', '-t', '14:30']);
		assert.deepEqual(used, [time], 'the field pill was spent on a filled slot');
		assert.deepEqual(withPills(GRAMMAR, ['spec', 'show', 'coffee'], [mood]).argv, [
			'spec',
			'show',
			'coffee'
		]);
	});

	// And refused on a line with a FLAG in it, where the slot rule cannot
	// tell a flag's value from a field: `datum help --json` gives no flag
	// arity, so placing one would risk `tail -n 20 coffee` — two fields.
	// The refusal is visible (the pill stays on the bar); a misplacement
	// would be a wrong row. Canary deepseek raised it on PR #62.
	test('and refused on a line carrying a flag, which the slot rule cannot read', () => {
		const { argv, used } = withPills(GRAMMAR, ['tail', '-n', '20'], [mood]);
		assert.deepEqual(argv, ['tail', '-n', '20']);
		assert.deepEqual(used, []);
	});

	// The same refusal the hid has (review 1498), for the same reason.
	test('a first word the grammar does not know gets no field pill either', () => {
		const { argv, used } = withPills(GRAMMAR, ['coffee'], [mood, time]);
		assert.deepEqual(argv, ['coffee', '-t', '14:30']);
		assert.deepEqual(used, [time]);
		assert.deepEqual(withPills(null, ['tail'], [mood]).argv, ['tail']);
	});

	test('a pill the command does not take is not given, and not spent', () => {
		const { argv, used } = withPills(GRAMMAR, ['occur', 'coffee'], [one, time]);
		assert.deepEqual(argv, ['occur', 'coffee', '-t', '14:30']);
		assert.deepEqual(used, [time]);
		assert.deepEqual(withPills(GRAMMAR, ['undo'], [time]).argv, ['undo']);
	});

	// Review 1498: `coffee <hid>` logs the hid as coffee's STATE. A first
	// word the grammar does not know gets the time cursor, never the hid.
	test('a first word it does not know — or no grammar — gets every pill but the hid', () => {
		for (const [argv, want] of [
			[['mood', 'happy'], ['mood', 'happy', '-t', '14:30']],
			[['coffee'], ['coffee', '-t', '14:30']],
			[['myalias', 'x'], ['myalias', 'x', '-t', '14:30']]
		]) {
			const { argv: got, used } = withPills(GRAMMAR, argv, [one, time]);
			assert.deepEqual(got, want, argv.join(' '));
			assert.deepEqual(used, [time], 'the hid is not spent');
		}
		assert.deepEqual(withPills(null, ['get'], [one, time]).argv, ['get', '-t', '14:30']);
	});
});

describe('which commands are active under the pills', () => {
	/** The commands drawn, among those the slice above describes. */
	const active = (pills) =>
		commandsUnder(GRAMMAR, pills)
			.map((c) => c.name)
			.filter((name) => GRAMMAR.has(name));

	test('no pills, or no grammar and no hid: all of them, in the usual order', () => {
		assert.deepEqual(commandsUnder(GRAMMAR, []), COMMANDS);
		assert.deepEqual(commandsUnder(null, [time]), COMMANDS);
		// No grammar says who takes an id, so nothing is offered one.
		assert.deepEqual(commandsUnder(null, [one]), []);
	});

	test('one hid: the id commands, the rest hidden', () => {
		assert.deepEqual(active([one]), ['undo', 'retime', 'update', 'delete', 'history', 'get']);
		assert.equal(active([one]).includes('occur'), false);
	});

	test('two hids: what takes a comma list', () => {
		assert.deepEqual(active([two]), ['retime', 'update', 'delete']);
	});

	test('a -t pill: what takes -t', () => {
		const names = active([time]);
		for (const n of ['occur', 'end', 'retime', 'tail']) assert.ok(names.includes(n), n);
		for (const n of ['now', 'get', 'delete', 'undo']) assert.ok(!names.includes(n), n);
	});

	// da-8810n, and the trap the bead names: `fits` FAILS OPEN, so a kind it
	// has not been taught leaves every command visible and the filter
	// silently does nothing. What is held here is the HIDING.
	test('a field pill: what names a field — and `spec`, which the spec editor is reached by', () => {
		const names = active([mood]);
		for (const n of ['occur', 'end', 'tail', 'flag', 'stats', 'count', 'spec'])
			assert.ok(names.includes(n), `${n} takes a field and was hidden`);
		for (const n of ['get', 'now', 'delete', 'update', 'retime', 'grep', 'conflicts'])
			assert.ok(!names.includes(n), `${n} takes no field and was not hidden`);
	});

	test('a field pill and a -t pill compose: both filters apply', () => {
		const names = active([mood, time]);
		for (const n of ['occur', 'end', 'tail']) assert.ok(names.includes(n), n);
		// `flag` names a field and takes no -t; `retime` takes -t and names
		// no field. Each pill hides the other's.
		for (const n of ['flag', 'retime']) assert.ok(!names.includes(n), n);
	});
});

describe('the selection a tap makes', () => {
	const order = ['a1', 'b2', 'c3', 'd4', 'e5'];
	const tap = (current, hid, pick, anchor = null, locked = false) => picked(current, hid, pick, order, anchor, locked);

	test('a plain tap replaces; the same one again clears', () => {
		assert.deepEqual(tap(['a1', 'b2'], 'c3', 'replace'), ['c3']);
		assert.deepEqual(tap(['c3'], 'c3', 'replace'), []);
	});

	test('ctrl toggles; shift takes the range from the last tap', () => {
		assert.deepEqual(tap(['a1'], 'c3', 'toggle'), ['a1', 'c3']);
		assert.deepEqual(tap(['a1', 'c3'], 'a1', 'toggle'), ['c3']);
		assert.deepEqual(tap(['a1'], 'd4', 'range', 'a1'), ['a1', 'b2', 'c3', 'd4']);
		assert.deepEqual(tap(['e5'], 'b2', 'range', 'd4'), ['b2', 'c3', 'd4']);
	});

	test('a locked pill: a plain tap toggles, a range adds', () => {
		assert.equal(pickOf({ shiftKey: false, ctrlKey: false, metaKey: false }, true), 'toggle');
		assert.equal(pickOf({ shiftKey: false, ctrlKey: false, metaKey: true }, false), 'toggle');
		assert.deepEqual(tap(['e5'], 'c3', 'range', 'b2', true), ['e5', 'b2', 'c3']);
	});

	test('a short hid and a whole one are the same entry', () => {
		assert.deepEqual(picked(['abcde12345xy'], 'abcde', 'replace', [], null, false), []);
	});

	test('the pill: its text the hids, one ✕-able section each', () => {
		assert.equal(two.text, 'abcde12345xy,fghij');
		assert.deepEqual(
			two.sections.map((s) => s.text),
			['abcde', 'fghij']
		);
		assert.deepEqual(two.sections[0].rest, { text: 'fghij' });
		assert.deepEqual(one.sections, [{ text: 'abcde' }]);
		assert.equal(hidPill([]), null);
	});
});
