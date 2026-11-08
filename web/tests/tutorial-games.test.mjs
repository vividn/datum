// The tutorial's games: Start and Reset, the game widget, and the fields they
// track and ignore.
// One of 4 files split from tutorial.test.mjs by topic, so each stays well
// inside test.mjs's 300 s file cap (da-5rpcr); the helpers they share are
// support/tutorial.mjs.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { PHONE } from './support/layout.mjs';
import { cuedRoom, settled, startSwitch, statuses } from './support/tutorial.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

// da-uzyih.4864, vividn: «a start over button, that just resets the game
// and makes any states that are currently active for that game false, but
// do not delete any data. Do not prompt about the starting over». Since
// da-uzyih.4910 (vividn, 2026-10-29: «get rid of the "Undo" and "Start Over"
// buttons … After the game is over the start button can read "Reset" which
// restarts the game to it's prestarted state») it is Start's, once a game is
// over: no game has an Undo or a Start over.
test('a game over, Start reads «Reset»: the neutral scene again, its fields’ active states ended, nothing deleted or asked; no Undo, no Start over', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			const active = () => app.run(['now', '--json']).then((r) => JSON.parse(r.stdout).active.map((a) => `${a.field}=${a.state}`).sort());
			const listed = async (fields) => (await Promise.all(fields.map((f) => app.run(['tail', f, '--json', '-n', '50']).then((r) => JSON.parse(r.stdout).rows)))).flat();
			// A field of no game's, running all along.
			assert.equal((await app.run(['start', 'other', '-t', '-5'])).code, 0);

			for (const tab of ['moment', 'on-off', 'change']) {
				const lesson = await page.evaluate((tab) => window.datum.tutorial.lessons.find((l) => l.name === tab), tab);
				const pane = `[data-pane="${tab}-lesson"]`;
				const game = page.locator(`${pane} .game`);
				const named = (name) => page.getByRole('button', { name, exact: true });
				// The game's pane and its fields' pane: the bar's own Undo, under its `❯`, is not the game's.
				const panes = page.locator(`${pane}, [data-pane="${tab}-fields"]`);
				const gone = async (when) => {
					assert.equal(await panes.count(), 2, `${tab}: both panes`);
					for (const name of ['Start over', '↶ Undo', 'Undo']) assert.equal(await panes.getByRole('button', { name, exact: true }).count(), 0, `${tab}, ${when}: no ${name}`);
				};
				await page.locator('.root-bar [role="tab"]', { hasText: tab }).click();
				await gone('at rest');
				if (tab === 'change') await startSwitch(page);
				else await named('Start').click();
				await named('End now').waitFor();
				await settled(page, pane);
				await gone('mid-game');
				await app.clock.forward(1000);
				await named('End now').click();
				await page.locator(`${pane} .game[data-phase="done"][aria-busy="false"]`).waitFor();
				await gone('over');
				assert.equal(await game.getByRole('button').allInnerTexts().then((t) => t.join()), 'Reset', `${tab}: over, Start reads Reset`);

				// After the game, the reader logs on: Reset ends what is active of the game's fields.
				await app.clock.forward(1000);
				const logs = { moment: [], 'on-off': [['start', 'tv'], ['start', 'reading']], change: [['change', 'room', 'kitchen']] }[tab];
				for (const argv of logs) assert.equal((await app.run(argv)).code, 0);
				const states = { moment: [], 'on-off': ['reading=true', 'tv=true'], change: ['room=kitchen'] }[tab];
				assert.deepEqual((await active()).filter((a) => !a.startsWith('other')), states, `${tab}: the states logged after it active`);
				const before = (await listed(lesson.fields)).map((row) => row.hid);

				await app.clock.forward(1000);
				await named('Reset').click();
				await game.and(page.locator('[aria-busy="true"]')).waitFor({ state: 'detached' });
				assert.equal(await game.getAttribute('data-phase'), 'neutral', `${tab}: the game at rest again`);
				if (tab === 'change') await game.locator('.cue').waitFor();
				else await named('Start').waitFor();
				assert.equal(await named('Reset').count(), 0, `${tab}: at rest, no Reset`);
				assert.equal(await page.locator('[role="dialog"], [role="alertdialog"]').count(), 0, `${tab}: nothing asked`);
				assert.deepEqual(await active(), ['other=true'], `${tab}: none of the game's states active, another field's untouched`);
				const after = (await listed(lesson.fields)).map((row) => row.hid);
				assert.ok(before.every((h) => after.includes(h)), `${tab}: nothing deleted`);
				// An end a state, or room's IGNORED, room being never_false.
				assert.equal(after.length, before.length + states.length, `${tab}: one write a state`);
			}
			assert.deepEqual(app.errors, []);
		},
		{ ...PHONE, query: '?db=tutorial' }
	);
});

// da-2chdp, vividn, 2026-11-05: «nowview, game (should be called game widget) and collect widget». A lesson's game
// is the `game` kind, its pane named «game widget»; a layout saved while the game was the `tutorial` kind's still
// runs its game.
test('a lesson’s game is a pane named «game widget» that runs a round; a tab saved with the old tutorial game pane still runs it', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			const pane = page.locator('[data-pane="moment-lesson"]');
			/** The moment tab's game pane's kind and title, and a round played in it to its score. */
			const round = async () => {
				await page.locator('.root-bar [role="tab"]', { hasText: /^moment$/ }).click();
				await pane.locator('.game').waitFor();
				const kind = await page.locator('.host:has(> [data-pane="moment-lesson"])').getAttribute('data-widget');
				const title = await pane.getAttribute('data-title');
				await pane.getByRole('button', { name: 'Start', exact: true }).click();
				await pane.getByRole('button', { name: 'End now', exact: true }).waitFor();
				// The begin's TRACKED landed before the clock moves: run after it, it would share End now's instant.
				await settled(page, '[data-pane="moment-lesson"]');
				await app.clock.forward(1000);
				await pane.getByRole('button', { name: 'End now', exact: true }).click();
				await pane.locator('.game[data-phase="done"][aria-busy="false"] .result').waitFor();
				await pane.getByRole('button', { name: 'Reset', exact: true }).click();
				await pane.getByRole('button', { name: 'Start', exact: true }).waitFor();
				return [kind, title];
			};
			assert.deepEqual(await round(), ['game', 'game widget']);

			// The tab as a layout saved before the game was its own kind: the game pane the `tutorial` kind's.
			const old = await page.evaluate(() => JSON.parse(JSON.stringify(window.datum.layouts.kitLayout('moment')).replaceAll('"kind":"game"', '"kind":"tutorial"')));
			// The about, the discussion (da-zb1dr), and the game pane, the tutorial kind again.
			assert.equal(JSON.stringify(old).match(/"kind":"tutorial"/g).length, 3, 'the game pane is the tutorial kind again');
			await page.evaluate((l) => window.datum.layouts.saveLayout(l), old);
			await app.reload();
			assert.deepEqual(await round(), ['tutorial', 'tutorial']);
			assert.deepEqual(app.errors, []);
		},
		{ ...PHONE, query: '?db=tutorial' }
	);
});

// da-uzyih.4882, vividn, 2026-10-27 18:22: «the fields on the other tutorial
// pages shoudl also be IGNORED after the game ends, and TRACKED when it
// begins». Each game is played twice, ended by its time and by «End now»: its
// fields TRACKED once it has begun and IGNORED once it has ended, read back
// from `datum now --json -A`. Reset then leaves them IGNORED and writes
// nothing. The switch game begins again, after Reset brings its cue back
// (da-uzyih.4880), by a tap on `room` while it is IGNORED: the collect widget
// draws an IGNORED field, and a tap on it logs.
test('each game’s fields are TRACKED once it begins and IGNORED once it ends, by its time and by «End now»', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			const count = async (fields) =>
				(await Promise.all(fields.map((f) => app.run(['tail', f, '--json', '-n', '200']).then((r) => JSON.parse(r.stdout).rows.length)))).reduce((a, b) => a + b);
			for (const tab of ['moment', 'on-off', 'change']) {
				const { fields } = await page.evaluate((tab) => window.datum.tutorial.lessons.find((l) => l.name === tab), tab);
				const pane = `[data-pane="${tab}-lesson"]`;
				const game = page.locator(`${pane} .game`);
				const all = (status) => Object.fromEntries(fields.map((f) => [f, status]));
				await page.locator('.root-bar [role="tab"]', { hasText: tab }).click();
				for (const how of ['time', 'End now']) {
					if ((await game.getAttribute('data-phase')) === 'done') {
						await page.getByRole('button', { name: 'Reset', exact: true }).click();
						await settled(page, pane);
					}
					if (tab === 'change') await startSwitch(page);
					else await page.getByRole('button', { name: 'Start', exact: true }).click();
					await settled(page, pane);
					assert.deepEqual(await statuses(app, fields), all('TRACKED'), `${tab}, before ${how}: TRACKED once it begins`);

					// The clock is held: an IGNORED at the TRACKED's instant would collide with it.
					await app.clock.forward(1000);
					if (how === 'time') await app.clock.forward(3_600_000);
					else await page.getByRole('button', { name: how, exact: true }).click();
					await page.locator(`${pane} .game[data-phase="done"][aria-busy="false"]`).waitFor();
					assert.deepEqual(await statuses(app, fields), all('IGNORED'), `${tab}, ended by ${how}: IGNORED`);
					assert.equal(await game.locator('.failed').count(), 0, `${tab}, ${how}: nothing failed`);
					await app.clock.forward(1000);
				}
				const before = await count(fields);
				await page.getByRole('button', { name: 'Reset', exact: true }).click();
				await settled(page, pane);
				assert.deepEqual(await statuses(app, fields), all('IGNORED'), `${tab}: Reset leaves them IGNORED`);
				assert.equal(await count(fields), before, `${tab}: and writes nothing`);
			}
			assert.deepEqual(app.errors, []);
		},
		{ ...PHONE, query: '?db=tutorial' }
	);
});

// vividn, 2026-10-27 15:35: «The fields widget should always be active, even
// when the game is not going, so that you can experiment with logging data
// outside the game». A tap before Start and one after the game logs as any
// tap does; neither starts a game or is scored. The switch game starts by a
// switch of room to the room its cue names, so its two taps are of
// other rooms.
test('the fields log outside a game too, and no game starts or scores by it', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			const count = (field) => app.run(['tail', field, '--json', '-n', '50']).then((r) => JSON.parse(r.stdout).rows.length);
			const tap = async (tab, field, state) => {
				const before = await count(field);
				await page.locator(`[data-pane="${tab}-fields"] .fields button[data-field="${field}"]`).click();
				if (state) await page.locator(`[data-pane="${tab}-fields"] .fields .room button[data-state="${state}"]`).click();
				await until(() => count(field), (n) => n === before + 1);
			};
			/** A room neither the cue's, where it is drawn (at rest alone, da-uzyih.4880), nor `held`. */
			const another = async (held) => {
				const cue = (await page.locator('[data-pane="change-lesson"] .game .cue').count()) ? await cuedRoom(page) : null;
				return ['kitchen', 'bedroom', 'bathroom'].find((r) => r !== cue && r !== held);
			};
			await page.evaluate(() => (window.datum.tutorial.cafe.round.seed = 7));
			for (const { tab, first, then } of [
				{ tab: 'moment', first: async () => ['tea'], then: async () => ['coffee'] },
				{ tab: 'on-off', first: async () => ['tv'], then: async () => ['phone'] },
				{ tab: 'change', first: async () => ['room', await another()], then: async (held) => ['room', await another(held)] }
			]) {
				await page.locator('.root-bar [role="tab"]', { hasText: tab }).click();
				const game = page.locator(`[data-pane="${tab}-lesson"] .game`);
				const result = () => game.locator('.result').innerText();
				// At their own instants: the clock is held, and two writes at one would collide.
				await app.clock.forward(1000);
				await tap(tab, ...(await first()));
				assert.equal(await game.getAttribute('data-phase'), 'neutral', `${tab}: a tap before Start starts no game`);

				await app.clock.forward(1000);
				const held = tab === 'change' ? await startSwitch(page) : await page.getByRole('button', { name: 'Start', exact: true }).click();
				await settled(page, `[data-pane="${tab}-lesson"]`);
				await app.clock.forward(500);
				await page.getByRole('button', { name: 'End now', exact: true }).click();
				await game.locator('.result').waitFor();
				const scored = await result();
				if (tab === 'moment') assert.match(scored, /accuracy\s+—/, 'nothing passed and the tap before Start is not scored');
				else assert.match(scored, /^accuracy\s+\d+%$/, scored);

				await app.clock.forward(1000);
				await tap(tab, ...(await then(held)));
				assert.equal(await game.getAttribute('data-phase'), 'done', `${tab}: a tap after the game starts none`);
				assert.equal(await result(), scored, `${tab}: and is not scored`);
			}
			assert.deepEqual(app.errors, []);
		},
		{ ...PHONE, query: '?db=tutorial' }
	);
});

test('Reset refuses outside the tutorial database, and ends nothing there', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			// A field of datum's that shares a lesson field's name.
			assert.equal((await app.run(['start', 'tv'])).code, 0);
			const refused = await page.evaluate(() =>
				window.datum.tutorial.startOver(['tv']).then(
					() => null,
					(e) => e.message
				)
			);
			assert.match(refused ?? 'it ran', /\btutorial\b/, 'the refusal names the database it is for');
			const { active } = JSON.parse((await app.run(['now', '--json'])).stdout);
			assert.deepEqual(active.map((a) => a.field), ['tv'], "datum's state is still on");
		},
		PHONE
	);
});

test('Reset ends a state only as it read it: one the reader ended meanwhile is not ended twice (da-v2d29)', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			for (const field of ['tv', 'phone']) assert.equal((await app.run(['start', field])).code, 0);
			// The page's clock is held: the states have lasted a while.
			await app.clock.forward(120_000);
			// The reader turns the TV off between Reset's read and its end.
			await page.evaluate(async () => {
				const previous = window.datum.setRunner(async (argv, ...rest) => {
					if (argv[0] === 'end' && argv[1] === 'tv') {
						const { code, stderr } = await previous(['end', 'tv', '-t', '-1'], ...rest);
						if (code !== 0) throw new Error(`the reader's end: ${stderr}`);
					}
					return previous(argv, ...rest);
				});
				try {
					await window.datum.tutorial.startOver(['tv', 'phone']);
				} finally {
					window.datum.setRunner(previous);
				}
			});
			const { active } = JSON.parse((await app.run(['now', '--json'])).stdout);
			assert.deepEqual(active, [], 'both are off');
			const { stdout } = await app.run(['check', 'integrity']);
			assert.doesNotMatch(stdout, /repeated_state/, 'and the TV is ended once');
			assert.deepEqual(app.errors, []);
		},
		{ ...PHONE, query: '?db=tutorial' }
	);
});
