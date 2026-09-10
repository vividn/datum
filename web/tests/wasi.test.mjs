// `?wasi`: the real datum CLI behind the seam (da-1vat, route C).
//
// The CLI built for wasm32-wasip1, one process per command, its database in
// OPFS. What these hold it to is what makes it the CLI rather than a
// likeness: its own output and exit codes, its own errors, a database that
// survives a reload and passes `datum check`, and — under both lock designs
// — two tabs that each see the other's rows.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';

before(startApp);
after(stopApp);

const plain = (text) => text.replace(/\x1b\[[0-9;]*m/g, '');

describe('the wasi CLI', () => {
	test('logs, reads back through a reload, and passes check', async () => {
		await withApp(
			async (app) => {
				const logged = await app.run(['occur', 'coffee', '-t', '0915']);
				assert.equal(logged.code, 0, logged.stderr);
				assert.match(plain(logged.stdout), /^CREATE [0-9a-z]{5} coffee:\d{4}-\d\d-\d\dT09:15:00\.000Z$/m);

				await app.reload();
				const tail = await app.run(['tail']);
				assert.equal(tail.code, 0, tail.stderr);
				assert.match(plain(tail.stdout), /09:15:00 +coffee/);

				const check = await app.run(['check']);
				assert.equal(check.code, 0, check.stdout + check.stderr);
			},
			{ query: '?wasi' }
		);
	});

	test('an unknown command is the CLI’s own error', async () => {
		await withApp(
			async (app) => {
				const result = await app.run(['nosuch']);
				assert.equal(result.code, 1);
				assert.equal(result.stderr, "Error: Invalid argument: unknown subcommand: 'nosuch'");
			},
			{ query: '?wasi' }
		);
	});

	for (const query of ['?wasi', '?wasi=command']) {
		test(`two tabs share one database (${query})`, async () => {
			await withApp(
				async (first) => {
					const second = await first.newTab();
					const [a, b] = await Promise.all([
						first.run(['occur', 'coffee', '-t', '0700']),
						second.run(['occur', 'tea', '-t', '0800'])
					]);
					assert.equal(a.code, 0, a.stderr);
					assert.equal(b.code, 0, b.stderr);
					for (const tab of [first, second]) {
						const tail = plain((await tab.run(['tail'])).stdout);
						assert.match(tail, /coffee[\s\S]*tea/);
					}
				},
				{ query }
			);
		});
	}
});
