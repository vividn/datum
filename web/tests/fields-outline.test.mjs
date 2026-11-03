// The collect widget's outlines (da-z8z40): a button a tap RUNS is drawn
// in the DOUBLE outline, and one that opens or fills is not — since
// da-w6437 a bare tap opens, so only a pending verb's tap runs — rule 1 of
// the design language, derived from `pressKind` in #lib/commands.ts, the
// same answer the press itself takes. The pure table is in
// `commands-pure.test.mjs`; here is what a reader sees, and what a tap on
// each button then does.
//
// A file of its own (da-lt0k): node's budget is per FILE, and the other
// `fields*.test.mjs` are already near a minute each under load.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp } from './support/app.mjs';
import { BLUE, FIXTURE, MOOD, type, untilFieldStyle } from './support/fields.mjs';

/** Every button under `selector` — the bar, or the states' window — by what it is, with its top border's style. */
const outlines = (page, selector = 'section.pane[data-title="collect"] .bar') =>
	page.locator(selector).evaluate((el) =>
		Object.fromEntries(
			[...el.querySelectorAll('button')].map((b) => [
				b.dataset.field ?? (b.dataset.state ? `=${b.dataset.state}` : b.dataset.back !== undefined ? 'back' : b.textContent.trim()),
				getComputedStyle(b).borderTopStyle
			])
		)
	);

/** How many entries the database holds, by the CLI's own count. */
const entries = async (app) =>
	JSON.parse((await app.run(['count', '--group-by', 'field', '--json'])).stdout).reduce((n, g) => n + g.count, 0);

/**
 * The pixels a button DRAWS, not its computed style (da-0lmb7): the double's
 * gap, a device pixel past the outer line at mid-height, and the fill, just
 * inside the border — as `[r, g, b]`, read from a screenshot of the button.
 */
const drawn = async (page, button) => {
	const box = await button.boundingBox();
	const png = await page.screenshot({ clip: box });
	return page.evaluate(async (b64) => {
		const img = await createImageBitmap(await (await fetch(`data:image/png;base64,${b64}`)).blob());
		const ctx = new OffscreenCanvas(img.width, img.height).getContext('2d');
		ctx.drawImage(img, 0, 0);
		const at = (x) => [...ctx.getImageData(x, img.height >> 1, 1, 1).data.slice(0, 3)];
		return { gap: at(1), fill: at(4) };
	}, png.toString('base64'));
};

/** How far apart two colours are, the largest of the three channels. */
const apart = (a, b) => Math.max(...a.map((v, i) => Math.abs(v - b[i])));

describe('the outline says what a tap does', () => {
	before(startApp);
	after(stopApp);
	const shared = sharedApp();

	// da-w6437: a bare tap opens a field in collect, so with no verb nothing
	// is double; under a pending verb a tap runs, and is.
	test('with no verb no field is double, and a state in its view is; under a verb a run is', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			for (const line of [...FIXTURE, ...MOOD]) await type(page, line);
			await page.locator('.bar [data-field="mood"]').waitFor();
			await app.idle();
			assert.deepEqual(await outlines(page), {
				coffee: 'solid',
				work: 'solid',
				untracked: 'solid',
				mood: 'solid',
				note: 'none'
			});

			// A state logs at once (vividn, 16:07): double.
			await page.locator('.bar [data-field="mood"]').click();
			await page.locator('.bar [data-state="good"]').waitFor();
			const states = await page.locator('.bar [data-state]').evaluateAll((bs) => bs.map((b) => getComputedStyle(b).borderTopStyle));
			assert.ok(states.length > 0 && states.every((s) => s === 'double'), `states: ${states}`);
			await page.locator('.bar [data-back]').click();
			await page.locator('.bar [data-field="mood"]').waitFor();

			// Under a pending verb the press is that verb: `switch` on a
			// many-stated field still asks which, `add` on a record still fills.
			const input = page.locator('.prompt-bar input');
			await input.fill('switch ');
			await page.waitForFunction(() => document.querySelectorAll('.bar [data-field]').length === 1);
			assert.equal((await outlines(page)).mood, 'solid');
			await input.fill('add ');
			await page.locator('.bar [data-field="note"]').waitFor();
			assert.equal((await outlines(page)).note, 'none');
			await input.fill('occur ');
			await page.waitForFunction(() => !document.querySelector('.bar [data-field="note"]'));
			assert.equal((await outlines(page)).coffee, 'double');
		});
	});

	// vividn: «The double outline is obscured when the button is filled in
	// with a color». A block that is on is filled in its own line's color;
	// under `end` it runs, and the double's gap must still show something
	// else than the fill.
	test('a double reads on a button filled in its own colour', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			for (const line of [...FIXTURE, ...MOOD, 'start work']) await type(page, line);
			await untilFieldStyle(page, 'work', 'backgroundColor', BLUE);
			await page.locator('.prompt-bar input').fill('end ');
			const work = page.locator('.bar [data-field="work"]');
			await page.waitForFunction(() => getComputedStyle(document.querySelector('.bar [data-field="work"]')).borderTopStyle === 'double');
			await app.idle();
			const block = await drawn(page, work);
			assert.ok(apart(block.gap, block.fill) > 64, `work: gap ${block.gap} on fill ${block.fill}`);
		});
	});

	// The behavior behind the look: tap every field button in turn with no
	// verb, and none writes an entry.
	test('with no verb a tap writes nothing: a field opens, a record fills', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			for (const line of [...FIXTURE, ...MOOD]) await type(page, line);
			await page.locator('.bar [data-field="mood"]').waitFor();
			await app.idle();

			for (const field of ['coffee', 'work', 'untracked', 'mood', 'note']) {
				const had = await entries(app);
				const echoed = (await app.echoes()).length;
				await page.locator(`.bar [data-field="${field}"]`).click();
				if (field === 'note') await page.waitForFunction(() => document.querySelector('.prompt-bar input').value.startsWith('add note'));
				else await page.locator('.bar [data-back]').click();
				await app.idle();
				assert.equal((await app.echoes()).length, echoed, `${field}: ran`);
				assert.equal(await entries(app), had, `${field}: wrote`);
				await page.locator('.prompt-bar input').fill('');
			}
		});
	});
});
