// What a press on a fields-widget chip does (da-c7u8): the argv it runs,
// or the line it leaves in the prompt for a person to finish.
//
// The five: `fields` (the pure logic, and the widget as it is drawn),
// `fields-press` (what a press runs), `fields-fit` (at a screen's size,
// and its ink), `fields-box` (the pane's own box) and `fields-states` (a
// state field's states). There was a sixth, `fields-bubbles`, over the
// hold panel's `-t` / `dur` bubbles; da-e78x6 removed the panel and the
// file with it. What more than one of them needs is
// `./support/fields.mjs`; a helper one file uses stays in that file.
//
// Budget (da-lt0k): node's `--test-timeout` is per TEST and a FILE is a
// test, so 120 s is this file's whole budget — `before(startApp)`, the
// Chromium it launches, and every test in it. One file held all of this,
// reached 32 tests, and `test timed out after 120000ms` took main's Web
// job red on four of five pushes; a cancelled file hides every other
// file's failures behind it. Measured node 22, at concurrency 3 on the
// 24-core build box under a load average of 95: fields 47s, press 60s,
// fit 63s, states 53s, box 36s (and bubbles, now gone, 35s) — the one
// file was 215s serial. A group that costs a minute wants a file of its own, not another
// `describe` in one of these.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp } from './support/app.mjs';
import { BLUE, CLEAR, FIXTURE, type, untilFieldStyle } from './support/fields.mjs';

describe('a press', () => {
	before(startApp);
	after(stopApp);

	// ONE APP for the file (da-6b3dj, sharedApp in support/app.mjs): `solo`
	// empties the database each test wrote, and the REPL is read from its end.
	const shared = sharedApp();

	// da-emze's bug, as da-c8mh settles it. A colour needs a spec to live
	// in, and the spec used to be minted with `occur` — so colouring a
	// start/stop field turned it into a one-tap occur button. A colour is
	// still not a verb: `track --color` takes the verb the LOG implies and
	// writes it, so the button stays a block, and an explicit verb changes it.
	test('colouring a start/stop field leaves it a block; naming a verb is what changes it', async () => {
		// The shape is on the chip as its class, so the wait is for the bar
		// to have caught up with the command — not for a beat to have gone by.
		const untilShape = (page, kind) =>
			page.waitForFunction(
				(k) => document.querySelector('.bar [data-field="sleep"]')?.classList.contains(k),
				kind
			);
		await shared.solo(async (app) => {
			const { page } = app;
			for (const line of ['start sleep -t -60', 'end sleep -t -1', 'track sleep --color #0000ff']) await type(page, line);
			await untilFieldStyle(page, 'sleep', 'borderTopColor', BLUE);
			assert.equal(JSON.parse((await app.run(['spec', 'show', 'sleep'])).stdout).verb, 'start');

			await untilShape(page, 'state');
			await app.pressWidget('sleep');
			assert.equal((await app.echoes()).at(-1), 'start sleep', 'a colour made it a point');

			await type(page, 'end sleep');
			await type(page, 'spec set sleep --verb occur');
			await untilShape(page, 'point');
			await app.pressWidget('sleep');
			assert.equal((await app.echoes()).at(-1), 'occur sleep', 'an explicit verb was not honoured');
		});
	});

	// The modal is tests/track.test.mjs's; what belongs here is that Track +
	// opens it and that the field it declares is the field the bar then draws.
	test('the Track modal asks for the verb and writes it', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await page.locator('.bar button.track').click();
			const modal = page.locator('dialog.spec');
			await modal.waitFor();
			assert.equal(await modal.locator('.verb[aria-checked="true"]').count(), 0, 'a verb was chosen for them');

			await modal.locator('input.field').fill('sleep');
			await modal.locator('button[data-verb="start"]').click();
			assert.equal((await modal.locator('code.preview').innerText()).trim(), 'track sleep --verb start');
			await modal.locator('button.commit').click();
			await page.locator('.bar button[data-field="sleep"]').waitFor();
			assert.equal(JSON.parse((await app.run(['spec', 'show', 'sleep'])).stdout).verb, 'start');
		});
	});

	test('a press runs what its shape means, with the bar’s pills after it', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			for (const line of FIXTURE) await type(page, line);
			await page.locator('.bar [data-field="coffee"]').click();
			// Ghosted beside the prompt, not typed into it (da-jprg).
			await page.waitForFunction(
				() => document.querySelector('.prompt-bar .ghost')?.textContent === 'occur coffee'
			);
			await page.waitForFunction(() =>
				document.querySelector('.prompt-bar .doc .receipt')?.textContent.startsWith('CREATE ')
			);
			await app.idle();
			assert.equal((await app.tailRows()).rows.at(-1).field, 'coffee');

			await type(page, 'start work');
			await untilFieldStyle(page, 'work', 'backgroundColor', BLUE);
			await page.locator('.bar [data-field="work"]').click();
			await untilFieldStyle(page, 'work', 'backgroundColor', CLEAR);
			assert.equal((await app.echoes()).at(-1), 'end work');
			await app.idle();
			const ended = (await app.tailRows()).rows.at(-1);
			assert.deepEqual([ended.field, ended.state], ['work', false]);

			const seen = await app.spyOnRunner();
			await page.evaluate(() => window.datum.bar.setPills([{ id: 'when', text: '-t 09:00', kind: 'time' }]));
			await page.locator('.bar [data-field="coffee"]').click();
			await page.waitForFunction(() => window.__datumSeen.some((a) => a[0] === 'occur'));
			// `--show json` after the pills is the web's own (da-xfa4): every
			// command a reader runs is asked for its echo as data.
			assert.deepEqual((await seen()).find((a) => a[0] === 'occur'), [
				'occur',
				'coffee',
				'-t',
				'09:00',
				'--show',
				'json'
			]);
		});
	});

	// ⛔ THE ROUTE THAT REPLACES THE HOLD PANEL (da-e78x6). The panel's `-t`
	// and `dur` bubbles were one way to put a time and a length on a press;
	// removing it leaves the bar's own pills, which the time cursor mints
	// from a click or a drag on any drawn picture ($lib/cursor.ts) and which
	// a person can also type. The test above shows a TIME reaching a press;
	// this shows a LENGTH, and the two together, on the field pill's own
	// field — because "you can still set it, just elsewhere" is a claim
	// about a path that has to be walked, not asserted.
	test('a `dur=` pill, and a time with it, ride into the press the widget composes', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			for (const line of FIXTURE) await type(page, line);
			await untilFieldStyle(page, 'work', 'backgroundColor', CLEAR);

			const seen = await app.spyOnRunner();
			// What a drag in the nowview leaves on the bar (nowpoint.test.mjs):
			// an instant and the span that ENDS at it (da-zwct).
			await page.evaluate(() =>
				window.datum.bar.setPills([
					{ id: 'when', text: '-t 09:00', kind: 'time' },
					{ id: 'span', text: 'dur=30m', kind: 'dur' }
				])
			);
			// A duration leaves the fields a length can be spent on (da-qu16),
			// and `work` is a block, so it is still there to press.
			await page.locator('.bar [data-field="work"]').click();
			await page.waitForFunction(() => window.__datumSeen.some((a) => a[0] === 'start'));
			assert.deepEqual((await seen()).find((a) => a[0] === 'start'), [
				'start',
				'work',
				'-t',
				'09:00',
				'dur=30m',
				'--show',
				'json'
			]);

			// And it LANDED as that block: 09:00 is the end, the half hour
			// before it is the span (da-zwct).
			await app.idle();
			const stored = (await app.entries()).find((e) => e.field === 'work');
			assert.match(stored.occur_time.utc, /T09:00/, JSON.stringify(stored.occur_time));
			assert.equal(stored.dur, 'PT30M', JSON.stringify(stored.dur));
		});
	});

	test('a record fills the prompt with its line, focused, and runs nothing', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			for (const line of FIXTURE) await type(page, line);
			const echoes = (await app.echoes()).length;
			await page.locator('.bar [data-field="note"]').click();
			await page.waitForFunction(() => {
				const input = document.querySelector('.prompt-bar input');
				return input.value === 'add note ' && document.activeElement === input;
			});
			assert.equal((await app.echoes()).length, echoes);
		});
	});

	// da-py5w: the comment is prose — only the person can type it. A button
	// in the hold panel used to put `note coffee ` in the bar to save the
	// first two words; da-e78x6 removed the panel, and NOTHING REPLACED
	// THAT BUTTON — `note` is typed whole now, and deliberately stayed off
	// the commands widget (see `COMMANDS` in $lib/commands.ts: a `note `
	// prefill beside a field pill composes the wrong command once the
	// comment's words are typed after it). So this is the route that
	// remains, and it is the half that was always the point: `note` routes
	// the bare words that follow into the comment, no quotes and no `-C`.
	test('`note <field> <words>` typed in the bar puts the words in the comment', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			for (const line of FIXTURE) await type(page, line);
			const echoes = (await app.echoes()).length;

			// No field button is pressed at all: there is no press that gets
			// here any more, which is the claim as much as the comment is.
			assert.equal(await page.locator('.bar .panel').count(), 0);

			const input = page.locator('.prompt-bar input');
			await input.fill('note coffee too strong');
			await input.press('Enter');
			await page.waitForFunction((n) => document.querySelectorAll('.repl .echo').length > n, echoes);

			assert.equal((await app.echoes()).at(-1), 'note coffee too strong');
			await app.idle();
			const stored = (await app.entries()).find((e) => e.field === 'coffee');
			assert.deepEqual(stored.data, { comment: 'too strong' });
		});
	});
});
