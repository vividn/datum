// What a gesture inspects, in node (da-pzzoz).
//
// The double tap's two numbers, which modifiers an inspect selects WITH,
// which mouse events are the desk's inspect at all, and the four words a
// tooltip says about an entry drawn as more than one mark. The same rules
// as a reader meets them are `inspect.test.mjs` in the browser.

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

const { DOUBLE_MS, DOUBLE_PX, NO_TAP, doubled, inspectClick, inspectMods, tapOf, tipOf } =
	await import('../src/lib/inspect.ts');

const at = (x, y, ms) => ({ clientX: x, clientY: y, timeStamp: ms });
const HID = 'b17ew04s0c85';

describe('two taps on the same mark are one gesture', () => {
	test('near enough and soon enough; and neither on its own', () => {
		const first = tapOf(at(100, 50, 1000), HID);
		assert.equal(doubled(first, at(100, 50, 1000 + DOUBLE_MS - 1), HID), true);
		assert.equal(doubled(first, at(100 + DOUBLE_PX - 1, 50, 1100), HID), true, 'inside the reach');
		assert.equal(doubled(first, at(100, 50, 1000 + DOUBLE_MS), HID), false, 'too late');
		assert.equal(doubled(first, at(100 + DOUBLE_PX, 50, 1100), HID), false, 'too far across');
		assert.equal(doubled(first, at(100, 50 + DOUBLE_PX, 1100), HID), false, 'too far down');
	});

	// vividn's gesture is «if you double tap a color» — ONE colour, twice.
	// Marks are stacked a lane apart, and in the nowview on a phone two
	// fields' now-chunks sit well inside DOUBLE_PX of each other: «near
	// enough» alone read two quick taps on two DIFFERENT fields as one
	// double tap and inspected the second, which is how `fieldsorder`'s
	// «a finger on the bar holds the order until it leaves» — mood, then
	// sleep — went red.
	test('a second tap on a NEIGHBOUR’s mark is not the same gesture', () => {
		const first = tapOf(at(100, 50, 1000), HID);
		assert.equal(doubled(first, at(100, 52, 1100), 'd9plbzg641as'), false, 'a lane away is another entry');
		assert.equal(doubled(first, at(100, 52, 1100), null), false, 'and ground names no entry at all');
	});

	test('the first tap of all is never the second of one', () => {
		assert.equal(doubled(NO_TAP, at(0, 0, 0), HID), false);
		assert.equal(doubled(NO_TAP, at(0, 0, 0), null), false);
	});

	// The numbers the day strip read «the whole day back» with from
	// da-ukcf until da-8rqb9 gave that meaning to a button. They are a
	// finger's numbers, not that gesture's, so they outlived it: the
	// nowview has always recognised its own double tap with them.
	test('the numbers are a finger’s, and unchanged', () => {
		assert.equal(DOUBLE_MS, 350);
		assert.equal(DOUBLE_PX, 30);
	});
});

describe('what a picture’s inspect selects WITH', () => {
	// Shift picks a RANGE in the tail and on a document; on a picture shift
	// IS the inspect, and one modifier cannot be both.
	test('shift is stripped, ctrl and ⌘ are not', () => {
		assert.deepEqual(inspectMods({ ctrlKey: false, metaKey: false }), {
			shiftKey: false,
			ctrlKey: false,
			metaKey: false
		});
		assert.deepEqual(inspectMods({ ctrlKey: true, metaKey: false }), {
			shiftKey: false,
			ctrlKey: true,
			metaKey: false
		});
		assert.deepEqual(inspectMods({ ctrlKey: false, metaKey: true }), {
			shiftKey: false,
			ctrlKey: false,
			metaKey: true
		});
	});
});

describe('the desk’s inspect: right click or shift click', () => {
	const mouse = (type, extra = {}) => ({ type, button: 0, shiftKey: false, ...extra });

	test('the right button is asked as contextmenu, and ONLY there', () => {
		assert.equal(inspectClick(mouse('contextmenu', { button: 2 })), true);
		// One right press fires three events. Inspecting on more than one of
		// them would select the entry and then deselect it again.
		assert.equal(inspectClick(mouse('pointerup', { button: 2 })), false);
		assert.equal(inspectClick(mouse('mousedown', { button: 2 })), false);
	});

	test('shift+click is the left button with shift, and a plain click is neither', () => {
		assert.equal(inspectClick(mouse('pointerup', { shiftKey: true })), true);
		assert.equal(inspectClick(mouse('pointerup')), false, 'a plain click still means TIME');
		assert.equal(inspectClick(mouse('pointerup', { button: 2, shiftKey: true })), false);
	});
});

describe('what the tooltip says about an entry', () => {
	const mark = (extra = {}) => ({ field: null, state: null, from: null, to: null, at: null, ...extra });

	test('the field, the state and the span the entry’s marks cover', () => {
		const tip = tipOf([
			mark({ field: 'work', state: 'coding', from: '2026-09-10T12:00:00.000Z', to: '2026-09-10T12:30:00.000Z' })
		]);
		assert.equal(tip.field, 'work');
		assert.equal(tip.state, 'coding');
		assert.equal(tip.from, Date.parse('2026-09-10T12:00:00.000Z'));
		assert.equal(tip.to, Date.parse('2026-09-10T12:30:00.000Z'));
	});

	// The now chunk carries `data-id` and NOTHING else (da-6hg6v), so the
	// four words are gathered ACROSS the marks that name the entry rather
	// than read off whichever one the finger landed on. A double tap on the
	// colour must say as much as one on the band it continues.
	test('several marks, one entry: the chunk borrows the band’s words', () => {
		const tip = tipOf([
			mark(),
			mark({ field: 'work', state: 'coding', from: '2026-09-10T12:00:00.000Z', to: '2026-09-10T12:30:00.000Z' })
		]);
		assert.equal(tip.field, 'work');
		assert.equal(tip.state, 'coding');
	});

	test('a dot’s one instant is both its edges', () => {
		const tip = tipOf([mark({ field: 'coffee', state: '∅', at: '2026-09-10T09:15:00.000Z' })]);
		assert.equal(tip.from, tip.to);
		assert.equal(tip.from, Date.parse('2026-09-10T09:15:00.000Z'));
	});

	test('a mark with no instants at all says none, and no marks says nothing', () => {
		assert.deepEqual(tipOf([mark({ field: 'work' })]), { field: 'work', state: '', from: null, to: null });
		assert.equal(tipOf([]), null);
	});
});
