// Sync conflicts on the page (da-6cutb): the commands widget's «conflicts»
// button, red with their count while `datum conflicts --json` lists any,
// and the conflicts widget a typed `conflicts` opens, where each is
// resolved by one `datum conflicts resolve` line run through the bar.
//
// vividn, 2026-10-08: «there should be a button "conflicts" in the commands
// menu. If there are any conflicts, it should become red for the user to
// then directly deal with» and «yes there should be a conflicts widget that
// get opened by the conflicts command».
//
// ONE flow on one app: `debug conflicts 2` (da-4dvk7) makes three — two
// entries whose state two devices edited, and one deleted here and edited
// there. What a resolve stores is the CLI's (`conflicts_cmd.rs`); this
// holds the page to composing the line and to reading the answer.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP, along, draft, widgetPane } from './support/layout.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const PAGE = JSON.stringify(along('column', widgetPane('repl', 'cli'), widgetPane('cmd', 'command')));

test('the button counts them in red, the widget resolves them, and the last one turns it plain', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			const button = page.locator('button[data-command="conflicts"]');
			/** The button's words, and whether it is drawn in the palette's error red. */
			const drawn = () =>
				button.evaluate((el) => {
					const probe = document.body.appendChild(document.createElement('i'));
					probe.style.color = 'var(--error)';
					const red = getComputedStyle(probe).color;
					probe.remove();
					const { color, borderTopColor } = getComputedStyle(el);
					return { text: el.textContent.trim(), red: color === red && borderTopColor === red };
				});
			const widget = page.locator('dialog.sheet[open][data-place="conflicts"]');
			const entries = widget.locator('section[data-hid]');
			const last = () => page.evaluate(() => window.datum.bar.last());
			const stateOf = async (hid) => JSON.parse((await app.run(['get', hid, '--json'])).stdout).docs[0].state;
			/** Tap Resolve on `entry` and wait for the bar's receipt of the line it showed. */
			const resolve = async (entry) => {
				const line = await entry.locator('.run code').textContent();
				await entry.getByRole('button', { name: 'Resolve' }).click();
				await until(last, (run) => run?.line === line);
				return last();
			};

			assert.deepEqual(await drawn(), { text: 'conflicts', red: false }, 'no conflicts: plain');

			await app.submit('debug conflicts 2');
			await until(drawn, (b) => b.text === 'conflicts 3');
			assert.equal((await drawn()).red, true, 'three conflicts: red');

			await app.submit('conflicts');
			await widget.waitFor();
			await until(() => entries.count(), (n) => n === 3);

			// The CLI's own listing: the letters, devices and values it prints.
			const listed = JSON.parse((await app.run(['conflicts', '--json'])).stdout);
			const [first, second] = listed.filter((e) => e.conflicts.state);
			const gone = listed.find((e) => e.conflicts.deleted);
			const sides = await widget.locator(`section[data-hid="${first.hid}"] [data-key="state"] .side`).allTextContents();
			assert.equal(sides.length, 2);
			first.conflicts.state.forEach((side, i) => {
				const text = sides[i].replace(/\s+/g, ' ').trim();
				assert.ok(text.startsWith(`${'AB'[i]} ${JSON.stringify(side.value)} ${side.device} · `), text);
				assert.match(text, / · \w+ \d+, \d\d:\d\d$/, 'and when');
			});

			// One by a strategy: both values, as one array.
			const one = widget.locator(`section[data-hid="${first.hid}"]`);
			await one.locator('[data-how="merge"]').click();
			let ran = await resolve(one);
			assert.equal(ran.line, `conflicts resolve ${first.hid} --merge`);
			assert.equal(ran.result.code, 0, ran.result.stderr);
			await until(drawn, (b) => b.text === 'conflicts 2');
			assert.deepEqual(await stateOf(first.hid), ['edited here', 'edited there']);

			// One by a pick: a tap on side A writes its letter into the key's box.
			const two = widget.locator(`section[data-hid="${second.hid}"]`);
			await two.locator('[data-key="state"] [data-side="A"]').click();
			assert.equal(await two.locator('[data-key="state"] input').inputValue(), 'a');
			ran = await resolve(two);
			assert.equal(ran.line, `conflicts resolve ${second.hid} state=a`);
			assert.equal(ran.result.code, 0, ran.result.stderr);
			await until(drawn, (b) => b.text === 'conflicts 1');
			assert.equal(await stateOf(second.hid), second.conflicts.state[0].value);
			assert.equal((await drawn()).red, true, 'one left: still red');

			// A delete against an edit: `--merge` is the CLI's to refuse, in its words.
			const three = widget.locator(`section[data-hid="${gone.hid}"]`);
			await three.locator('[data-how="merge"]').click();
			ran = await resolve(three);
			assert.notEqual(ran.result.code, 0);
			assert.match(await three.locator('.refused').textContent(), /cannot merge true with false.*pick that key instead/);
			assert.equal(await entries.count(), 1, 'refused: still listed');

			// «keep as shown» is no flag at all, and the entry stays deleted.
			await three.getByRole('radio', { name: 'keep as shown' }).click();
			ran = await resolve(three);
			assert.equal(ran.line, `conflicts resolve ${gone.hid}`);
			assert.equal(ran.result.code, 0, ran.result.stderr);
			await until(drawn, (b) => b.text === 'conflicts');
			assert.equal((await drawn()).red, false, 'none left: plain');
			await widget.getByText('No sync conflicts.').waitFor();
			assert.equal((await app.run(['conflicts', '--json'])).stdout.trim(), '[]');
			assert.equal((await app.run(['get', gone.hid])).code, 1, 'the deleted entry stays deleted');
		},
		{ ...DESKTOP, prepare: (context) => context.addInitScript(draft, PAGE) }
	);
});
