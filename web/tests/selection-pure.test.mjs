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

const { COMMANDS, commandsUnder, grammarOf, takesOf } = await import('../src/lib/commands.ts');
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
	entry('conflicts', ['datum conflicts resolve <HID> (--take-local | --take-remote)'])
]);

const time = { id: 'c', kind: 'time', text: '-t 14:30' };
const one = hidPill(['abcde12345xy']);
const two = hidPill(['abcde12345xy', 'fghij']);

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
			time: false
		});
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
