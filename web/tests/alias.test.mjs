// Aliases in the REPL.
//
// An alias is a name the user gives a command: `alias set cof 'occur
// coffee strength=strong'`, and from then on `cof` logs a strong coffee.
// The expansion is not this app's code — the browser runs the CLI, which
// expands it (da-5kbj, da-g8o8). So what these tests check is not "does the
// expansion work" (tests/parity.test.mjs holds that against the native
// binary, case by case) but the round trip in the browser: the row is
// written, it is read back on the very next line, and removing it stops
// the name expanding.
//
// Everything runs through `run(argv)`, the one seam, so these are the
// commands a person types and nothing behind them.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';

before(startApp);
after(stopApp);

/** `run`, asserting the command succeeded, and handing back its stdout. */
async function ok(run, argv) {
	const result = await run(argv);
	assert.equal(result.code, 0, `datum ${argv.join(' ')}: ${result.stderr}`);
	return result.stdout;
}

describe('aliases in the REPL', () => {
	test('an alias set in one line fires on the next, with no reload', async () => {
		await withApp(async ({ run, entries }) => {
			assert.equal(
				await ok(run, ['alias', 'set', 'cof', 'occur coffee strength=strong']),
				"✓ Alias 'cof' saved."
			);

			await ok(run, ['cof', '-t', '0915']);
			const [stored] = await entries();
			assert.equal(stored.field, 'coffee');
			assert.deepEqual(stored.data, { strength: 'strong' });
			// The provenance the CLI stamps: what was typed, and what it
			// became. Sync carries these columns, so the shape is not the
			// browser's to choose.
			assert.equal(stored.raw_input, 'cof -t 0915');
			assert.deepEqual(stored.alias_chain, [
				{ name: 'cof', expansion: 'occur coffee strength=strong' }
			]);
		});
	});

	// vividn's case, verbatim from datum v2: an alias carrying `--id
	// %comment%`, so two transactions on one day are two rows keyed by what
	// each was for, rather than one row per minute.
	test('an `--id %comment%` alias slugs the row on its own data', async () => {
		await withApp(async ({ run, entries }) => {
			await ok(run, ['alias', 'set', 'tx', 'occur tx --id %comment%']);
			await ok(run, ['tx', 'amount=40', 'comment=electricity', '-d', '2026-09-01']);
			await ok(run, ['tx', 'amount=12', 'comment=coffee', '-d', '2026-09-01']);

			const stored = await entries();
			assert.deepEqual(
				stored.map((e) => e.slug).sort(),
				['tx:coffee', 'tx:electricity'],
				'each row is filed under its comment, not under its minute'
			);
			assert.deepEqual(
				stored.map((e) => e.id_structure),
				['%comment%', '%comment%']
			);
		});
	});

	test('a field spec makes its own names commands', async () => {
		await withApp(async ({ run, entries }) => {
			await ok(run, ['spec', 'set', 'mood', '--default-verb', 'switch', '--state', 'happy']);

			// The bare field name, and the state name, both without a verb.
			await ok(run, ['mood', '-t', '0910']);
			await ok(run, ['happy', '-t', '0920']);

			const stored = await entries();
			assert.deepEqual(
				stored.map((e) => [e.field, e.state]),
				[
					['mood', 'happy'],
					['mood', true]
				],
				'newest first: `happy` switched mood, the bare name started it'
			);
		});
	});

	test('a chain expands through every hop, and a cycle is refused', async () => {
		await withApp(async ({ run, entries }) => {
			await ok(run, ['alias', 'set', 'a', 'b strength=strong']);
			await ok(run, ['alias', 'set', 'b', 'occur coffee']);
			await ok(run, ['a', '-t', '0915']);

			const [stored] = await entries();
			assert.equal(stored.field, 'coffee');
			assert.deepEqual(stored.alias_chain, [
				{ name: 'a', expansion: 'b strength=strong' },
				{ name: 'b', expansion: 'occur coffee' }
			]);

			await ok(run, ['alias', 'set', 'loop', 'knot']);
			await ok(run, ['alias', 'set', 'knot', 'loop']);
			const cycle = await run(['loop']);
			assert.notEqual(cycle.code, 0);
			assert.match(cycle.stderr, /Alias cycle: loop → knot → loop/);
		});
	});

	test('`alias list` prints the CLI’s lines, and `alias remove` unmakes one', async () => {
		await withApp(async ({ run }) => {
			assert.equal(await ok(run, ['alias', 'list']), 'No aliases defined.');

			await ok(run, ['alias', 'set', 'cof', 'occur coffee']);
			await ok(run, ['alias', 'set', 'tea', 'occur tea']);
			assert.equal(
				await ok(run, ['alias', 'list']),
				'  cof → datum occur coffee\n  tea → datum occur tea'
			);
			assert.equal(await ok(run, ['alias', 'show', 'cof']), 'cof → datum occur coffee');

			assert.equal(await ok(run, ['alias', 'remove', 'cof']), "✓ Alias 'cof' removed");
			assert.equal(await ok(run, ['alias', 'list']), '  tea → datum occur tea');

			// And the name stops expanding immediately — no reload.
			const gone = await run(['cof']);
			assert.notEqual(gone.code, 0, 'a removed alias must not still fire');

			const missing = await run(['alias', 'remove', 'cof']);
			assert.notEqual(missing.code, 0);
			assert.match(missing.stderr, /no user alias 'cof'/);
		});
	});

	test('a derived alias has no row to remove, and says which spec owns it', async () => {
		await withApp(async ({ run }) => {
			await ok(run, ['spec', 'set', 'sleep', '--default-verb', 'start']);
			const refused = await run(['alias', 'remove', 'sleep']);
			assert.notEqual(refused.code, 0);
			assert.match(refused.stderr, /derived alias from the 'sleep' field spec/);
		});
	});

	test('an alias survives a reload, because it is a row and not a variable', async () => {
		await withApp(async (app) => {
			await ok(app.run, ['alias', 'set', 'cof', 'occur coffee']);
			await app.reload();
			await ok(app.run, ['cof', '-t', '0915']);
			const [stored] = await app.entries();
			assert.equal(stored.field, 'coffee');
		});
	});
});
