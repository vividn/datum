// The layout tree's operations, in node, with no browser.
//
// `$lib/layout/tree.ts` is pure and imports only types, so node loads it
// as it is (node strips the types). What is checked here is the canon the
// file promises: after any operation — and after any SEQUENCE of them —
// there is no split with one child, no pane with no tabs, the ratios sum
// to 1, `active` names a tab, and the last pane is still there.

import test, { describe } from 'node:test';
import assert from 'node:assert/strict';
import {
	EMPTY_KIND,
	addTab,
	allIds,
	canCloseTab,
	closeTab,
	findPane,
	findSplit,
	isCanonical,
	newId,
	newPane,
	newSplit,
	newTab,
	newWidget,
	normalize,
	setActive,
	setBar,
	setRatios,
	setWidget,
	splitPane,
	splitTab
} from '../src/lib/layout/tree.ts';

const widgetTab = (kind) => newTab(newWidget(kind));

/** A root pane with one widget tab. */
const single = (kind = 'tail') => ({ version: 1, name: 'test', root: newPane([widgetTab(kind)]) });

/** A root pane whose one tab is a row split of two widget panes. */
function pair() {
	const left = newPane([widgetTab('repl')]);
	const right = newPane([widgetTab('tail')]);
	return {
		layout: { version: 1, name: 'test', root: newPane([newTab(newSplit('row', [left, right]))]) },
		left,
		right
	};
}

const kinds = (pane) => pane.tabs.map((t) => t.content.kind);

describe('splitPane', () => {
	test('the pane becomes one child of a new split, beside a new empty pane', () => {
		const before = single('tail');
		const after = splitPane(before, before.root.id, 'row');
		assert.ok(isCanonical(after));
		const split = after.root.tabs[0].content;
		assert.equal(split.direction, 'row');
		assert.deepEqual(split.ratios, [0.5, 0.5]);
		assert.equal(split.children[0].id, before.root.id, 'the pane keeps its id');
		assert.deepEqual(kinds(split.children[1]), [EMPTY_KIND]);
	});

	test('all the tabs travel together', () => {
		let layout = single('tail');
		layout = addTab(layout, layout.root.id);
		layout = splitPane(layout, layout.root.id, 'column');
		const split = layout.root.tabs[0].content;
		assert.deepEqual(kinds(split.children[0]), ['tail', EMPTY_KIND]);
	});

	test('joins a split already running that way rather than nesting', () => {
		const { layout, left } = pair();
		const after = splitPane(layout, left.id, 'row');
		const split = after.root.tabs[0].content;
		assert.equal(split.children.length, 3);
		assert.equal(split.children[0].id, left.id);
		assert.deepEqual(kinds(split.children[1]), [EMPTY_KIND]);
		assert.deepEqual(split.ratios, [0.25, 0.25, 0.5]);
	});

	test('nests when the split runs the other way', () => {
		const { layout, left } = pair();
		const after = splitPane(layout, left.id, 'column');
		const outer = after.root.tabs[0].content;
		assert.equal(outer.children.length, 2);
		const inner = outer.children[0].tabs[0].content;
		assert.equal(inner.direction, 'column');
		assert.equal(inner.children[0].id, left.id);
		assert.ok(isCanonical(after));
	});
});

describe('splitTab', () => {
	test('is refused with one tab', () => {
		const layout = single('tail');
		assert.equal(splitTab(layout, layout.root.id, 0, 'row'), layout);
	});

	test("that tab's content becomes a split of [its old content, an empty pane]", () => {
		let layout = single('tail');
		layout = addTab(layout, layout.root.id);
		const after = splitTab(layout, layout.root.id, 0, 'column');
		assert.ok(isCanonical(after));
		const split = after.root.tabs[0].content;
		assert.equal(split.direction, 'column');
		assert.deepEqual(kinds(split.children[0]), ['tail']);
		assert.deepEqual(kinds(split.children[1]), [EMPTY_KIND]);
		assert.deepEqual(kinds(after.root).slice(1), [EMPTY_KIND], 'the other tab is untouched');
	});
});

describe('addTab / setActive / closeTab', () => {
	test('a new empty tab is added and made active', () => {
		const layout = single('tail');
		const after = addTab(layout, layout.root.id);
		assert.deepEqual(kinds(after.root), ['tail', EMPTY_KIND]);
		assert.equal(after.root.active, 1);
	});

	test('setActive lands in range', () => {
		const one = single('tail');
		const layout = addTab(one, one.root.id);
		assert.equal(setActive(layout, layout.root.id, 7).root.active, 1);
		assert.equal(setActive(layout, layout.root.id, -3).root.active, 0);
	});

	test('closing a tab keeps the active one where it can', () => {
		let layout = single('a');
		layout = addTab(layout, layout.root.id);
		layout = setWidget(layout, layout.root.tabs[1].id, newWidget('b'));
		layout = addTab(layout, layout.root.id);
		layout = setWidget(layout, layout.root.tabs[2].id, newWidget('c'));
		layout = setActive(layout, layout.root.id, 2);
		const after = closeTab(layout, layout.root.id, 0);
		assert.deepEqual(kinds(after.root), ['b', 'c']);
		assert.equal(kinds(after.root)[after.root.active], 'c');
	});

	test('the last pane cannot be closed', () => {
		const layout = single('tail');
		assert.equal(canCloseTab(layout, layout.root.id), false);
		assert.equal(closeTab(layout, layout.root.id, 0), layout);
	});

	test('a pane left with no tabs leaves its split, and the split unnests', () => {
		const { layout, left, right } = pair();
		assert.equal(canCloseTab(layout, right.id), true);
		const after = closeTab(layout, right.id, 0);
		assert.ok(isCanonical(after));
		assert.deepEqual(kinds(after.root), ['repl'], 'the survivor’s tabs are the root’s tabs now');
		assert.equal(findPane(after, left.id), undefined, 'the survivor pane itself is gone');
		assert.equal(findSplit(after, layout.root.tabs[0].content.id), undefined);
	});

	test('unnesting a survivor with several tabs hands them all up', () => {
		const { layout, left, right } = pair();
		const withTabs = addTab(layout, left.id);
		const after = closeTab(withTabs, right.id, 0);
		assert.deepEqual(kinds(after.root), ['repl', EMPTY_KIND]);
		assert.ok(isCanonical(after));
	});

	test('closes collapse all the way up', () => {
		const { layout, left } = pair();
		const deeper = splitPane(layout, left.id, 'column');
		const inner = deeper.root.tabs[0].content.children[0].tabs[0].content;
		const emptyPane = inner.children[1];
		const once = closeTab(deeper, emptyPane.id, 0);
		// The unnest hands `left`'s tabs up into the pane that wrapped it, so
		// it is that pane — the outer split's first child — that holds the
		// REPL now, under its own id.
		const holder = once.root.tabs[0].content.children[0];
		assert.deepEqual(kinds(holder), ['repl']);
		const after = closeTab(once, holder.id, 0);
		assert.ok(isCanonical(after));
		assert.deepEqual(kinds(after.root), ['tail']);
	});
});

describe('setRatios', () => {
	test('renormalizes to a sum of 1', () => {
		const { layout } = pair();
		const split = layout.root.tabs[0].content;
		const after = findSplit(setRatios(layout, split.id, [3, 1]), split.id);
		assert.deepEqual(after.ratios, [0.75, 0.25]);
	});

	test('clamps to the floors, giving the weight back', () => {
		const { layout } = pair();
		const split = layout.root.tabs[0].content;
		const after = findSplit(setRatios(layout, split.id, [0.95, 0.05], [0.1, 0.2]), split.id);
		assert.deepEqual(after.ratios.map((r) => Math.round(r * 1000) / 1000), [0.8, 0.2]);
	});

	test('keeps what it had when the floors cannot all fit', () => {
		const { layout } = pair();
		const split = layout.root.tabs[0].content;
		const after = findSplit(setRatios(layout, split.id, [0.9, 0.1], [0.7, 0.7]), split.id);
		assert.deepEqual(after.ratios, [0.5, 0.5]);
	});
});

describe('setBar / setWidget', () => {
	test('setBar moves the bar', () => {
		const layout = single('tail');
		const after = setBar(layout, layout.root.id, { edge: 'left', align: 'end' });
		assert.deepEqual(after.root.bar, { edge: 'left', align: 'end' });
	});

	test('setWidget fills an empty tab', () => {
		const one = single('tail');
		let layout = addTab(one, one.root.id);
		const empty = layout.root.tabs[1];
		layout = setWidget(layout, empty.id, newWidget('nowview', { x: 1 }));
		assert.equal(layout.root.tabs[1].content.kind, 'nowview');
		assert.deepEqual(layout.root.tabs[1].content.settings, { x: 1 });
	});
});

describe('normalize', () => {
	test('is idempotent', () => {
		const messy = {
			version: 1,
			name: 'messy',
			root: {
				id: 'r',
				active: 9,
				bar: { edge: 'top', align: 'start' },
				tabs: [
					{
						id: 't',
						content: {
							id: 's',
							direction: 'row',
							ratios: [2, 2, 2],
							children: [
								{ id: 'p1', active: 0, bar: { edge: 'top', align: 'start' }, tabs: [] },
								{
									id: 'p2',
									active: 0,
									bar: { edge: 'top', align: 'start' },
									tabs: [{ id: 'w', content: { id: 'x', kind: 'tail', settings: {} } }]
								}
							]
						}
					}
				]
			}
		};
		const once = normalize(messy);
		assert.ok(isCanonical(once));
		assert.deepEqual(kinds(once.root), ['tail']);
		assert.deepEqual(normalize(once), once);
	});

	test('an emptied root gets a fresh empty tab', () => {
		const empty = { version: 1, name: 'e', root: { ...newPane(), tabs: [] } };
		const after = normalize(empty);
		assert.deepEqual(kinds(after.root), [EMPTY_KIND]);
	});
});

describe('any sequence of operations', () => {
	test('leaves the tree canonical, and never reuses an id', () => {
		let layout = single('repl');
		const seen = new Set(allIds(layout));
		let previous = new Set(seen);
		let seed = 7;
		const rand = () => (seed = (seed * 48271) % 2147483647) / 2147483647;
		const pick = (list) => list[Math.floor(rand() * list.length)];
		const everyPane = (l) => {
			const out = [];
			const walk = (p) => {
				out.push(p);
				for (const t of p.tabs) if ('children' in t.content) t.content.children.forEach(walk);
			};
			walk(l.root);
			return out;
		};
		for (let i = 0; i < 1000; i++) {
			const pane = pick(everyPane(layout));
			const dir = rand() < 0.5 ? 'row' : 'column';
			switch (Math.floor(rand() * 6)) {
				case 0:
					layout = splitPane(layout, pane.id, dir);
					break;
				case 1:
					layout = splitTab(layout, pane.id, Math.floor(rand() * pane.tabs.length), dir);
					break;
				case 2:
					layout = addTab(layout, pane.id);
					break;
				case 3:
					layout = closeTab(layout, pane.id, Math.floor(rand() * pane.tabs.length));
					break;
				case 4: {
					const tab = pick(pane.tabs);
					if (!('children' in tab.content))
						layout = setWidget(layout, tab.id, newWidget(pick(['tail', 'nowview', 'repl'])));
					break;
				}
				default:
					layout = setActive(layout, pane.id, Math.floor(rand() * 4));
			}
			assert.ok(isCanonical(layout), `not canonical after op ${i}`);
			const current = new Set(allIds(layout));
			for (const id of current) {
				if (!previous.has(id)) assert.ok(!seen.has(id), `id ${id} came back after op ${i}`);
				seen.add(id);
			}
			previous = current;
		}
		assert.ok(everyPane(layout).length >= 1, 'the last pane survived');
	});

	test('newId is unique across a thousand mints', () => {
		const ids = new Set(Array.from({ length: 1000 }, newId));
		assert.equal(ids.size, 1000);
		for (const id of ids) assert.match(id, /^[0-9a-z]{8}$/);
	});
});
