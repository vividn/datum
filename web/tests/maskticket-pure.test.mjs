// What the scrollback and the recall history keep of a typed `sync chain
// join` (da-l9kdr): `<ticket>` wherever `syncCommand` would read a
// ticket, `--db` anywhere in the line included — never the ticket.

import test from 'node:test';
import assert from 'node:assert/strict';
import { resolveLibAsSource } from './support/libhook.mjs';

resolveLibAsSource();
const { maskTicket } = await import('../src/lib/sync.ts');

const T = 'MFRGGZDFMZTWQ2LKNNWG23TPOBYXE43UOV3HO6DZPJQWEY3EMVTGO2DJNJVWY3LONZXXA4LSON2HK5TXPB4XU';
const LINK = `https://datum.day/sync/join#${T}`;

test('the ticket or link is <ticket>, wherever --db stands', () => {
	assert.equal(maskTicket(`sync chain join ${T}`), 'sync chain join <ticket>');
	assert.equal(maskTicket(`sync chain join ${LINK}`), 'sync chain join <ticket>');
	assert.equal(maskTicket(`sync chain join --db work ${T}`), 'sync chain join --db work <ticket>');
	assert.equal(maskTicket(`sync --db work chain join ${T}`), 'sync --db work chain join <ticket>');
	assert.equal(maskTicket(`--db=work sync chain join '${LINK}'`), `--db=work sync chain join '<ticket>'`);
	assert.equal(maskTicket(`sync chain join ${T} ${LINK}`), 'sync chain join <ticket> <ticket>');
});

test('nothing else is touched', () => {
	assert.equal(maskTicket('sync chain join c'), 'sync chain join <ticket>');
	assert.equal(maskTicket('sync chain join --help'), 'sync chain join --help');
	assert.equal(maskTicket('sync chain join'), 'sync chain join');
	assert.equal(maskTicket(`note diary ${T}`), `note diary ${T}`);
});
