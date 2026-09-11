// The input widget (da-0dhc): the REPL's prompt with no output pane.
//
// vividn asked for "just a box to enter in a command, and if expanded
// vertically the history of the commands with their timestamps (but no
// output shown)", and for that history to BE the REPL's — one store for
// both prompts. So the claims here are about what each prompt can see of
// the other: a command typed at /input is in the REPL's scrollback with its
// output, and one typed in the REPL is in the widget's history without.
//
// The assertions read the command text and its time, never a line count: a
// probe that only asks for "more lines" passes on the wrong line.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { inWorker, startApp, stopApp, withApp } from './support/app.mjs';

before(startApp);
after(stopApp);

const HHMMSS = /^\d{2}:\d{2}:\d{2}$/;

/** Type a line into the widget and submit it, as a user does. */
async function type(page, line) {
	const before = await page.locator('.input-widget .history li').count();
	const input = page.locator('.input-widget input');
	await input.fill(line);
	await input.press('Enter');
	await page.waitForFunction(
		(n) => document.querySelectorAll('.input-widget .history li').length > n,
		before
	);
}

/** The widget's history as the reader sees it, oldest first. */
const history = (page) =>
	page.locator('.input-widget .history li').evaluateAll((items) =>
		items.map((li) => ({
			stamp: li.querySelector('.stamp').textContent,
			line: li.querySelector('.echo').textContent.replace(/^\s*❯\s*/, '').trim()
		}))
	);

const lines = async (page) => (await history(page)).map((row) => row.line);

/** A store that refuses every read and write, as a locked-down browser's does. */
const blockedStorage = (context) =>
	context.addInitScript(() => {
		Object.defineProperty(window, 'localStorage', {
			configurable: true,
			get() {
				throw new DOMException('blocked', 'SecurityError');
			}
		});
	});

describe('a command typed at /input', () => {
	test('is logged, and listed with its time and without its output', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.goto('/input');
			await type(page, 'occur coffee');

			assert.deepEqual(
				(await app.entries()).map((e) => e.field),
				['coffee']
			);
			const [row] = await history(page);
			assert.equal(row.line, 'occur coffee');
			assert.match(row.stamp, HHMMSS);
			const shown = await page.locator('.input-widget').innerText();
			assert.match(shown, /occur coffee/);
			assert.doesNotMatch(shown, /CREATE/, 'the widget printed the command\'s output');

			// Not shown is not dropped: the output is in the REPL's
			// scrollback, which is what makes the CREATE above a real absence.
			// (`/day`, not `/`: `/` is whichever tab was last active — this
			// device's choice, da-0a5o — and that is the input tab now.)
			await app.goto('/day');
			assert.deepEqual(await app.echoes(), ['occur coffee']);
			assert.match((await app.outputs()).join('\n'), /CREATE/);
		});
	});
});

describe('the history the widget shares with the REPL', () => {
	test("goes both ways: Up recalls in either prompt what the other ran — the widget's and the bottom bar's", async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.goto('/input');
			await type(page, 'occur tea');

			await app.goto('/day');
			const bar = page.locator('.prompt-bar input');
			await bar.press('ArrowUp');
			assert.equal(await bar.inputValue(), 'occur tea');
			assert.deepEqual(await app.echoes(), ['occur tea']);
			assert.match(await page.locator('.repl .head .stamp').first().innerText(), HHMMSS);

			await app.submit('occur water');
			await app.goto('/input');
			assert.deepEqual(await lines(page), ['occur tea', 'occur water']);
			const widget = page.locator('.input-widget input');
			await widget.press('ArrowUp');
			assert.equal(await widget.inputValue(), 'occur water');
			await widget.press('ArrowUp');
			assert.equal(await widget.inputValue(), 'occur tea');
		});
	});
});

describe('the widget across a reload', () => {
	test('comes back with its commands and their times', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.goto('/input');
			await type(page, 'occur coffee');
			await type(page, 'tail');
			const before = await history(page);
			assert.deepEqual(
				before.map((row) => row.line),
				['occur coffee', 'tail']
			);

			await app.reload();
			assert.deepEqual(await history(page), before);
		});
	});

	test('is empty, and quiet, when the browser blocks the store', async () => {
		await withApp(
			async (app) => {
				const { page, errors } = app;
				const logged = [];
				page.on('console', (m) => m.type() === 'error' && logged.push(m.text()));
				await app.goto('/input');

				// The session works; a blocked store costs the history only.
				await type(page, 'occur coffee');
				assert.deepEqual(await lines(page), ['occur coffee']);

				await app.reload();
				assert.deepEqual(await lines(page), [], 'something was restored from a blocked store');
				assert.deepEqual(errors, [], `the blocked store threw at the page: ${errors[0]}`);
				assert.deepEqual(logged, [], `the blocked store logged an error: ${logged[0]}`);
			},
			{ prepare: blockedStorage }
		);
	});
});

describe('a command that fails', () => {
	test('says why in the prompt line for a few seconds, and is still in the history', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const failed = await app.run(['foo']);
			assert.notEqual(failed.code, 0, 'foo is supposed to be an unknown command');
			const why = failed.stderr
				.replace(/\x1b\[[0-9;]*m/g, '')
				.split('\n')
				.find((l) => l.trim());

			await app.goto('/input');
			await type(page, 'foo');
			const alert = page.locator('.input-widget .line [role="alert"]');
			assert.equal(await alert.innerText(), why.trim());
			assert.deepEqual(await lines(page), ['foo']);

			await alert.waitFor({ state: 'detached', timeout: 10000 });
		});
	});
});

describe('the widget at the height it is given', () => {
	test('one line tall is the prompt alone; 300px tall shows the history', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.goto('/input');
			await type(page, 'occur coffee');
			// Whether the command's row is inside the box the history clips
			// to. Not `isVisible`, which is blind to an overflow clip and
			// calls a row visible that no reader can see.
			const listed = () =>
				page.evaluate(() => {
					const list = document.querySelector('.input-widget .history').getBoundingClientRect();
					const row = [...document.querySelectorAll('.input-widget .history li')]
						.find((li) => li.textContent.includes('occur coffee'))
						.getBoundingClientRect();
					return row.top >= list.top && row.bottom <= list.bottom;
				});

			await page.evaluate(() => {
				const widget = document.querySelector('.input-widget');
				const border = widget.offsetHeight - widget.clientHeight;
				widget.style.height = `${widget.querySelector('.entry').offsetHeight + border}px`;
			});
			assert.equal(await listed(), false, 'a one-line widget showed its history');
			assert.ok(await page.locator('.input-widget input').isVisible());

			await page.evaluate(() => {
				document.querySelector('.input-widget').style.height = '300px';
			});
			assert.ok(await listed(), 'a 300px widget did not show its history');
		});
	});
});

describe('while the database is held in memory', () => {
	test("the widget's frame carries the alarm, as the REPL's does", async () => {
		// A sync access handle that never answers: the app gives up on
		// durable storage and opens in memory (see storage.test.mjs).
		const NEVER = `FileSystemFileHandle.prototype.createSyncAccessHandle = () => new Promise(() => {});`;
		await withApp(
			async (app) => {
				await app.goto('/input');
				await app.page.locator('.input-widget.alarm').waitFor();
			},
			{ prepare: inWorker(NEVER) }
		);
	});
});
