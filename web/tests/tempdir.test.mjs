// The browser has no temp directory (da-uhur).
//
// A WASI process is given one preopened directory, `/data`, and nothing
// that answers as a place to put a temporary file. So the first query whose
// ephemeral table or sorter outgrows SQLite's page cache asks the VFS for a
// temp path, is answered `SQLITE_IOERR_GETTEMPPATH` (6410), and the command
// dies with "disk I/O error" — on a database that is merely large. `stats`
// did exactly that on an imported multi-year database, because the zone
// timeline runs a window function over every entry.
//
// Nothing about this can fail natively, where a temp directory always
// exists, so the test has to be here. The fix is `PRAGMA temp_store =
// MEMORY` on the wasm build (`Db::configure_connection` in datum-core):
// with the ephemeral table in memory there is nothing left to ask the VFS
// for.
//
// The size below is measured, not guessed: 19,500 entries still succeeded
// before the fix and 20,000 failed, so 20,000 is the smallest round number
// that actually reproduced. A smaller database would leave the test green
// with the fix reverted.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';

before(startApp);
after(stopApp);

const SPILL_ENTRIES = 20_000;

/**
 * Fill the database with `n` entries across four zones, through `db restore`.
 *
 * The row shape is datum's own: one entry is logged, backed up, and the
 * line that comes back is the template every seeded row is cloned from. So
 * there is no schema written down here to go stale, and the header carries
 * the schema version the restore gates on.
 *
 * Both the generating and the restoring happen inside the page: the JSONL
 * is ~30 MB, and there is no reason to move it through the test runner.
 */
async function seed({ page, run }, n) {
	const logged = await run(['occur', 'coffee', '-t', '0700']);
	assert.equal(logged.code, 0, logged.stderr);
	// The header, the TRACK document declaring `coffee` (da-c8mh) and the
	// entry itself; the template below is that entry.
	const backup = (await run(['db', 'backup', '-'])).stdout.split('\n');
	assert.equal(backup.length, 3, `expected a header and two rows: ${backup.join('\n')}`);
	backup.splice(1, 1);

	return page.evaluate(
		async ([header, template, n]) => {
			const row = JSON.parse(template);
			// Four stretches, so the timeline the query builds is a history
			// with moves in it rather than one flat zone.
			const zones = ['UTC', 'America/Chicago', 'Europe/Berlin', 'Asia/Tokyo'];
			const lines = [header];
			for (let i = 0; i < n; i++) {
				const utc = new Date(Date.UTC(2020, 0, 1) + i * 600_000).toISOString().replace('.000Z', 'Z');
				lines.push(
					JSON.stringify({
						...row,
						hid: i.toString(16).padStart(12, '0'),
						slug: `coffee:${utc}`,
						content_hash: i.toString(16).padStart(64, '0'),
						occur_time: {
							...row.occur_time,
							utc,
							timezone: zones[Math.floor((i * zones.length) / n)]
						}
					})
				);
			}
			return window.datum.runQuiet(['db', 'restore', '-'], lines.join('\n') + '\n');
		},
		[backup[0], backup[1], n]
	);
}

describe('a database too big to sort in cache', () => {
	test('`stats` reads all of it instead of asking for a temp directory', async () => {
		await withApp(async (app) => {
			const restored = await seed(app, SPILL_ENTRIES);
			assert.equal(restored.code, 0, restored.stderr);

			const stats = await app.run(['stats', 'coffee']);
			// The error this replaces: "Error code 6410: VFS is unable to
			// determine a suitable directory for temporary files".
			assert.equal(stats.code, 0, stats.stderr);
			// The count is the point: `stats` walked every seeded row (and
			// the one the template was taken from), so the window function
			// over the whole table really ran.
			assert.match(stats.stdout, new RegExp(`over ${SPILL_ENTRIES + 1} entries`));
		});
	});
});
