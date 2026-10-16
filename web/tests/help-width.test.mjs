// HELP AT THE WIDTH IT IS SHOWN IN (da-nt53d).
//
// The pages are written at 73 columns, and a phone's REPL holds about 43:
// `help occur` was a 597px `pre` in a 351px column, read by scrolling the
// whole history sideways. The REPL now asks for help at its own width in
// characters (`COLUMNS`, `helpEnv` in $lib/panes.ts) and the CLI fits the
// page to it (`datum_parse::help::fit`): no line wider than the pane, and a
// table's columns still lined up.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';

before(startApp);
after(stopApp);

const PHONE = { width: 390, height: 844 };

/** The newest entry's output: its lines, how wide it is, the room it has, and the history's sideways scroll. */
const newest = (page) =>
	page.evaluate(() => {
		const entry = [...document.querySelectorAll('.repl .entry')].at(-1);
		const pre = entry.querySelector('pre');
		const body = entry.closest('.body');
		return {
			lines: pre.textContent.split('\n'),
			wide: pre.scrollWidth,
			room: entry.clientWidth,
			sideways: body.scrollWidth - body.clientWidth
		};
	});

for (const line of ['help occur', 'help']) {
	test(`${line} on a ${PHONE.width}px phone reads without scrolling sideways`, async () => {
		await withApp(
			async (app) => {
				await app.submit(line);
				await app.idle();
				const { lines, wide, room, sideways } = await newest(app.page);

				assert.ok(lines.length > 10, `${line} printed ${lines.length} lines`);
				assert.ok(wide <= room, `${line} is ${wide}px wide in a ${room}px pane`);
				assert.equal(sideways, 0, `the history scrolls ${sideways}px sideways`);
				// Still a table: each row's description starts at its column.
				const [a, b] = line === 'help' ? ['add ', 'note '] : ['--show <MODE>', '--json'];
				const at = (flag) => {
					const row = lines.find((l) => l.includes(flag));
					return row.length - row.slice(row.indexOf(flag) + flag.length).trimStart().length;
				};
				assert.equal(at(a), at(b), `${a} and ${b} line up:\n${lines.join('\n')}`);
			},
			{ viewport: PHONE, query: 'repl' }
		);
	});
}
