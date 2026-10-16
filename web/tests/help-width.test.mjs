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
import { until } from './support/until.mjs';

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

// THE HELP PLACE TOO (da-nrjze): it asks for its page at its own width in
// characters, and again when that changes, so a page reads in the lines the
// CLI wrapped it to — none wrapped a second time by the browser, which is
// what a line drawn `pre` wider than the place would be under `pre-wrap`.

/** Each of the Help place's texts, its folds opened, wider than the place drawn unwrapped: its widest line. */
const wrapped = (page) =>
	page.evaluate(() => {
		const host = document.querySelector('.host[data-widget="help"]');
		for (const fold of host.querySelectorAll('details')) fold.open = true;
		return [...host.querySelectorAll('pre')].flatMap((pre) => {
			pre.style.whiteSpace = 'pre';
			const [wide, room] = [pre.scrollWidth, pre.clientWidth];
			pre.style.whiteSpace = '';
			const widest = pre.textContent.split('\n').reduce((a, b) => (b.length > a.length ? b : a), '');
			return wide > room ? [`${wide}px > ${room}px: ${widest}`] : [];
		});
	});

for (const name of ['occur', undefined, 'logging']) {
	const title = `datum help${name ? ` ${name}` : ''}`;
	test(`the Help place's ${title} on a ${PHONE.width}px phone is wrapped by the CLI alone`, async () => {
		await withApp(
			async (app) => {
				await app.page.evaluate((n) => window.datum.help(n), name);
				const asked = app.page.locator('.host[data-widget="help"] h3.asked');
				await until(() => asked.innerText().catch(() => ''), (t) => t.trim() === title);
				await until(() => wrapped(app.page), (over) => over.length === 0);
				if (name !== 'occur') return;
				// Its example rows stack as the REPL's do.
				const intro = (await app.page.locator('.host[data-widget="help"] pre.help').innerText()).split('\n');
				const row = intro.indexOf('  datum occur coffee');
				assert.ok(row > 0, intro.join('\n'));
				assert.equal(intro[row + 1], '      a coffee, now');
			},
			{ viewport: PHONE }
		);
	});
}
