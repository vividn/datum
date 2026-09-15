// What a press on a fields-widget chip does (da-c7u8): the argv it runs,
// or the line it leaves in the prompt for a person to finish.
//
// The six: `fields` (the pure logic, and the widget as it is drawn),
// `fields-press` (what a press runs), `fields-fit` (at a screen's size,
// and its ink), `fields-box` (the pane's own box), `fields-states` (a
// state field's states) and `fields-bubbles` (the long-press panel's
// bubbles against that list). What more than one of them needs is
// `./support/fields.mjs`; a helper one file uses stays in that file.
//
// Budget (da-lt0k): node's `--test-timeout` is per TEST and a FILE is a
// test, so 120 s is this file's whole budget — `before(startApp)`, the
// Chromium it launches, and every test in it. One file held all of this,
// reached 32 tests, and `test timed out after 120000ms` took main's Web
// job red on four of five pushes; a cancelled file hides every other
// file's failures behind it. Measured node 22, the six at concurrency 3 on
// the 24-core build box under a load average of 95: fields 47s, press 60s,
// fit 63s, states 53s, bubbles 35s, box 36s — the one file was 215s
// serial. A group that costs a minute wants a file of its own, not another
// `describe` in one of these.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { BLUE, CLEAR, FIXTURE, type, untilFieldStyle } from './support/fields.mjs';

describe('a press', () => {
	before(startApp);
	after(stopApp);

	// da-emze: the bug, as vividn found it. A colour needs a spec to live in,
	// and the spec used to be minted with `occur` — so colouring a
	// start/stop field turned it into a one-tap occur button, because
	// da-u8n7 honours a spec's verb. A colour is not a verb: the button
	// stays a block, and an EXPLICIT verb still changes it.
	test('colouring a start/stop field leaves it a block; naming a verb is what changes it', async () => {
		// The shape is on the chip as its class, so the wait is for the bar
		// to have caught up with the command — not for a beat to have gone by.
		const untilShape = (page, kind) =>
			page.waitForFunction(
				(k) => document.querySelector('.bar [data-field="sleep"]')?.classList.contains(k),
				kind
			);
		await withApp(async (app) => {
			const { page } = app;
			for (const line of ['start sleep -t -60', 'end sleep -t -1', 'track sleep --color #0000ff']) await type(page, line);
			await untilFieldStyle(page, 'sleep', 'borderTopColor', BLUE);
			assert.equal(JSON.parse((await app.run(['spec', 'show', 'sleep'])).stdout).default_verb, null);

			await untilShape(page, 'state');
			await app.pressWidget('sleep');
			assert.equal((await app.echoes()).at(-1), 'start sleep', 'a colour made it a point');

			await type(page, 'end sleep');
			await type(page, 'spec set sleep --default-verb occur');
			await untilShape(page, 'point');
			await app.pressWidget('sleep');
			assert.equal((await app.echoes()).at(-1), 'occur sleep', 'an explicit verb was not honoured');
		});
	});

	test('the Track + form declares a field with no verb when the log is to decide', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await page.locator('.bar button.track').click();
			const form = page.locator('form.add');
			assert.equal(await form.locator('select').inputValue(), 'log', 'a verb was chosen for the user');

			await form.locator('input').first().fill('sleep');
			assert.equal((await form.locator('code.preview').innerText()).trim(), 'spec set sleep');
			await form.locator('button[type="submit"]').click();
			await page.locator('.bar button[data-field="sleep"]').waitFor();
			assert.equal(JSON.parse((await app.run(['spec', 'show', 'sleep'])).stdout).default_verb, null);
		});
	});

	test('a press runs what its shape means, with the bar’s pills after it', async () => {
		await withApp(async (app) => {
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

	test('a record fills the prompt with its line, focused, and runs nothing', async () => {
		await withApp(async (app) => {
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

	// da-py5w: the comment is prose — only the person can type it — so the
	// panel's `note` fills the bar and runs nothing, exactly as a record's
	// button does. What makes it worth a press is the CLI end of the same
	// bead: `note` routes the bare words that follow into the comment, so
	// what the person types after the prefill needs no quotes and no `-C`.
	test('the panel’s `note` prefills `note <field> ` and the words typed after it are the comment', async () => {
		await withApp(async (app) => {
			const { page } = app;
			for (const line of FIXTURE) await type(page, line);

			await page.locator('.bar [data-field="coffee"]').click({ button: 'right' });
			const note = page.locator('.bar .panel button.note');
			await note.waitFor();

			const echoes = (await app.echoes()).length;
			await note.click();
			await page.waitForFunction(() => {
				const input = document.querySelector('.prompt-bar input');
				return (
					input.value === 'note coffee ' &&
					document.activeElement === input &&
					input.selectionStart === input.value.length
				);
			});
			assert.equal((await app.echoes()).length, echoes, 'the press ran a command');
			await page.locator('.bar .panel').waitFor({ state: 'detached' });

			// Typed on, as a person does — no quotes around the words.
			const input = page.locator('.prompt-bar input');
			await input.press('End');
			await input.type('too strong');
			await input.press('Enter');
			await page.waitForFunction((n) => document.querySelectorAll('.repl .echo').length > n, echoes);

			assert.equal((await app.echoes()).at(-1), 'note coffee too strong');
			await app.idle();
			const stored = (await app.entries()).find((e) => e.field === 'coffee');
			assert.deepEqual(stored.data, { comment: 'too strong' });
		});
	});
});
