// What the scrollback and the recall history keep of a typed `sync chain
// connect` (da-l9kdr): `<ticket>` wherever `syncCommand` would read a
// ticket, `--db` anywhere in the line included — never the ticket.

import test from 'node:test';
import assert from 'node:assert/strict';
import { resolveLibAsSource } from './support/libhook.mjs';

resolveLibAsSource();
const { maskTicket } = await import('../src/lib/sync.ts');

const T = 'MFRGGZDFMZTWQ2LKNNWG23TPOBYXE43UOV3HO6DZPJQWEY3EMVTGO2DJNJVWY3LONZXXA4LSON2HK5TXPB4XU';
const LINK = `https://datum.day/sync/join#${T}`;

test('the ticket or link is <ticket>, wherever --db stands', () => {
	assert.equal(maskTicket(`sync chain connect ${T}`), 'sync chain connect <ticket>');
	assert.equal(maskTicket(`sync chain connect ${LINK}`), 'sync chain connect <ticket>');
	assert.equal(maskTicket(`sync chain connect --db work ${T}`), 'sync chain connect --db work <ticket>');
	assert.equal(maskTicket(`sync --db work chain connect ${T}`), 'sync --db work chain connect <ticket>');
	assert.equal(maskTicket(`--db=work sync chain connect '${LINK}'`), `--db=work sync chain connect '<ticket>'`);
	assert.equal(maskTicket(`sync chain connect ${T} ${LINK}`), 'sync chain connect <ticket> <ticket>');
});

test('nothing else is touched', () => {
	assert.equal(maskTicket('sync chain connect c'), 'sync chain connect <ticket>');
	assert.equal(maskTicket('sync chain connect --help'), 'sync chain connect --help');
	assert.equal(maskTicket('sync chain connect'), 'sync chain connect');
	assert.equal(maskTicket(`note diary ${T}`), `note diary ${T}`);
});
