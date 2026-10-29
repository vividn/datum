// What a gesture inspects, in node (da-pzzoz).
//
// Which press traveled (da-xg1vi), which modifiers an inspect selects
// WITH, and the four words a tooltip says about an entry drawn as more
// than one mark. The same rules
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

const { CLICK_SLOP, inspectMods, pressTraveled, unwatchPresses, watchPresses } = await import(
	'../src/lib/inspect.ts'
);

// da-xg1vi, vividn: «in general panning around shouldn't cause data to be
// selected or any tool tips to pop up». What a picture asks before it does
// either: did the press in hand, or the one just lifted, travel?
describe('a press that traveled is no click', () => {
	unwatchPresses();
	const page = new EventTarget();
	watchPresses(page);
	const fire = (type, x = 0, y = 0, pointerId = 1) =>
		page.dispatchEvent(Object.assign(new Event(type), { pointerId, clientX: x, clientY: y, isPrimary: pointerId === 1 }));

	test('within the slop it is still; past it, it traveled until the next press', () => {
		fire('pointerdown', 100, 50);
		fire('pointermove', 100 + CLICK_SLOP, 50);
		fire('pointerup', 100 + CLICK_SLOP, 50);
		assert.equal(pressTraveled(), false, 'a click may wobble');
		fire('pointerdown', 100, 50);
		fire('pointermove', 100, 50 + CLICK_SLOP + 1);
		fire('pointermove', 100, 50);
		fire('pointerup', 100, 50);
		assert.equal(pressTraveled(), true, 'a scroll that came back is still a scroll');
		fire('pointerdown', 100, 50);
		assert.equal(pressTraveled(), false, 'the next press starts afresh');
		fire('pointerup', 100, 50);
	});

	test('a press the browser took, and a second finger, traveled', () => {
		fire('pointerdown', 100, 50);
		fire('pointercancel', 100, 50);
		assert.equal(pressTraveled(), true, 'a cancel is the browser’s scroll');
		fire('pointerdown', 100, 50, 1);
		fire('pointerdown', 200, 50, 2);
		assert.equal(pressTraveled(), true, 'a pinch is no click');
		fire('pointerup', 100, 50, 1);
		fire('pointerup', 200, 50, 2);
	});

	test('a lift the document never heard does not outlive its press', () => {
		fire('pointerdown', 100, 50);
		fire('pointerdown', 100, 50);
		assert.equal(pressTraveled(), false, 'the primary pointer starts afresh');
		fire('pointerup', 100, 50);
	});

	test('a pointer that is not down moves nothing', () => {
		fire('pointerdown', 100, 50);
		fire('pointerup', 100, 50);
		fire('pointermove', 400, 400);
		assert.equal(pressTraveled(), false, 'a mouse hovering after its click');
	});
});

describe('what a picture’s inspect selects WITH', () => {
	// Shift picks a RANGE in the tail and on a document; on a picture
	// shift+drag is a range of TIME, and one modifier cannot be both.
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

