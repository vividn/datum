// What the closed output's tab says: six characters, or the failure (da-k5kwu).
import test from 'node:test';
import assert from 'node:assert/strict';
import { tabWord } from '../src/lib/tabWord.ts';

const w = (...texts) => texts.map((text) => ({ text, color: `#${text}` }));
const said = ({ parts, more }) => parts.map((p) => p.text).join(',') + (more ? '+' : '');

for (const [words, failed, want] of [
	[[], false, ''],
	[w('UPDATE'), false, 'UPDATE'],
	[w('UPDATE', 'SWITCH'), false, 'UPD,SW'],
	[w('UPDATE', 'SWITCH', 'CREATE'), false, 'UP,S,C'],
	[w('UPDATE', 'SWITCH', 'CREATE', 'DELETE'), false, 'U,S,C+'],
	[w('UPDATE', 'SWITCH', 'CREATE', 'DELETE', 'RETIME'), false, 'U,S,C+'],
	[[], true, 'ERROR!'],
	[w('UPDATE'), true, 'ERROR!'],
	[w('UPDATE', 'FAILED', 'SWITCH'), false, 'FAILED'],
	[w('UPDATE', 'FAILED'), true, 'FAILED']
])
	test(`${words.map((x) => x.text).join(' ') || 'nothing'}${failed ? ', failed,' : ''} says «${want}»`, () => {
		const out = tabWord(words, failed);
		assert.equal(said(out), want);
		assert.ok(want === '' || want.length === 6, 'not six characters');
	});

test('each part keeps its own action\'s color', () => {
	assert.deepEqual(
		tabWord(w('UPDATE', 'SWITCH'), false).parts.map((p) => p.color),
		['#UPDATE', '#SWITCH']
	);
});
