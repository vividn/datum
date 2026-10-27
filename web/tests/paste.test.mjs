// A paste of several lines into the prompt runs each line (da-jxfdw).
//
// vividn, 2026-10-21: an agent that cannot run commands helps its person by
// writing a setup for them to paste into datum.day's prompt — and the prompt
// is a one-line <input>, which joined the block into one line that failed.
// So a block runs line by line, as if each were typed and entered; a line
// that fails stops it and stays in the box, the lines after it held behind.
//
// What was written is asked of the CLI (`tail --json`), not read off the page.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp } from './support/app.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const shared = sharedApp({ viewport: { width: 1280, height: 800 }, query: 'cli' });

const BOX = '.prompt-bar input';

/** Paste `text` into the bar's empty box as a clipboard would. */
async function paste(page, text) {
	await page.locator(BOX).fill('');
	await page.locator(BOX).evaluate((box, text) => {
		const data = new DataTransfer();
		data.setData('text/plain', text);
		box.dispatchEvent(new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }));
	}, text);
}

/**
 * The recall history, newest first, once its newest line is `last`: what ran,
 * as typed and in order. (Not `tail`'s order: the held clock gives every
 * entry here one instant.)
 */
async function walk(page, last, n) {
	const box = page.locator(BOX);
	await until(async () => {
		await box.press('ArrowUp');
		const seen = await box.inputValue();
		await box.press('ArrowDown');
		return seen;
	}, (v) => v === last);
	const walked = [];
	for (let i = 0; i < n; i++) {
		await box.press('ArrowUp');
		walked.push(await box.inputValue());
	}
	return walked;
}

/** The fields of the newest `n` entries. */
const fields = async (app, n = 20) =>
	JSON.parse((await app.run(['tail', '--json', '-n', String(n)])).stdout).rows.map((r) => r.field);

describe('pasting several lines into the prompt', () => {
	test('runs each line in order, skipping blank and # lines', async () => {
		const app = await shared();
		// `coffee` is a word only once the setup has made it the field's alias,
		// so the second line runs only if the first ran before it.
		await paste(app.page, '# coffee, for the morning\nsetup coffee --verb occur\n\ncoffee\ntail -n 1\n');
		await until(() => fields(app), (f) => f.includes('coffee'));
		const spec = JSON.parse((await app.run(['tail', 'spec', '--json', '-n', '20'])).stdout).rows;
		assert.ok(spec.some((r) => r.slug === 'spec:coffee'), 'the setup wrote its spec');
		// Every line ran, the last one too, in order.
		assert.deepEqual(await walk(app.page, 'tail -n 1', 3), ['tail -n 1', 'coffee', 'setup coffee --verb occur']);
	});

	test('stops at a failing line and leaves it in the box, the rest held behind it', async () => {
		const app = await shared();
		await paste(app.page, 'occur tea\noccur --nonsense\noccur juice');
		await until(() => app.page.locator(BOX).inputValue(), (v) => v === 'occur --nonsense');
		const seen = await fields(app);
		assert.ok(seen.includes('tea'), `the first line ran: ${seen}`);
		assert.ok(!seen.includes('juice'), `the third line did not: ${seen}`);
		assert.equal(await app.page.locator('.prompt-bar .held').innerText(), '+1');

		// Fixed, Enter goes on from there: the line in the box, then the one held.
		await app.page.locator(BOX).fill('occur mango');
		await app.page.locator(BOX).press('Enter');
		await until(() => fields(app), (f) => f.includes('juice') && f.includes('mango'));
		assert.equal(await app.page.locator('.prompt-bar .held').count(), 0);
		assert.deepEqual(await walk(app.page, 'occur juice', 2), ['occur juice', 'occur mango']);
	});
});
