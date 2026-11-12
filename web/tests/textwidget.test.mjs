// The text widget (da-12308). vividn, 2026-11-11: «Let's make a "text"
// widget, it's normally static text, but during modify mode, you can enter
// in markdown text that gets formatted when not in modify mode».
//
// So out of modify mode its markdown is drawn and nothing is editable; in
// the mode a textarea holds the source, written into the layout, so ✓ keeps
// it over a reload and the built-in's Factory default brings back its text.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP, chooseRoot, leaveWidget, modifyWidget, saveOpenTab, titledPane } from './support/layout.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const SOURCE = '# Morning\n\nA *slow* start, **then** work.\n\n- coffee\n- walk\n\n[datum](https://datum.day)';

/** `/text` alone, and its pane's id. */
async function opened(app) {
	await app.goto('/text');
	const pane = titledPane(app.page, 'text');
	await pane.waitFor();
	return { pane, id: await pane.getAttribute('data-pane') };
}

/** What the text widget's rendered markdown holds, by element. */
const drawn = (pane) =>
	pane.locator('.text').evaluate((el) => ({
		h1: el.querySelector('h1')?.textContent,
		em: el.querySelector('em')?.textContent,
		strong: el.querySelector('strong')?.textContent,
		items: [...el.querySelectorAll('ul > li')].map((li) => li.textContent),
		link: [el.querySelector('a')?.textContent, el.querySelector('a')?.getAttribute('href')]
	}));

/** A 1×1 PNG. */
const PNG = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==', 'base64');

const WANT = { h1: 'Morning', em: 'slow', strong: 'then', items: ['coffee', 'walk'], link: ['datum', 'https://datum.day'] };

describe('the text widget', () => {
	test('empty it says what it is; its markdown, written in modify mode, is drawn after ✓, kept over a reload, and Factory default brings back the built-in', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const { pane, id } = await opened(app);
			await pane.locator('p.quiet').waitFor();
			assert.equal(await pane.locator('textarea').count(), 0, 'editable out of modify mode');

			await modifyWidget(page, id);
			await pane.locator('textarea').fill(SOURCE);
			await leaveWidget(page, id);
			assert.equal(await pane.locator('textarea').count(), 0, 'still editable after ✓');
			assert.deepEqual(await drawn(pane), WANT);

			await app.reload();
			await pane.locator('.text').waitFor();
			assert.deepEqual(await drawn(pane), WANT, 'a reload lost the text');

			// Saved, so the reset has a save to go back from.
			await saveOpenTab(page);
			await chooseRoot(page, 'Factory default');
			await pane.locator('p.quiet').waitFor();
			assert.equal(await pane.locator('.text').count(), 0, 'the reset kept the text');
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	test("an image from datum.day's own origin is drawn; any other is a button to keep it, and nothing asks its host (da-ipk3v, da-wgf4a)", async () => {
		await withApp(async (app) => {
			const { page } = app;
			const asked = [];
			page.on('request', (r) => {
				if (new URL(r.url()).hostname === 'example.com') asked.push(r.url());
			});
			const { pane, id } = await opened(app);
			await modifyWidget(page, id);
			await pane.locator('textarea').fill('![ours](/favicon.png)\n\n![theirs](https://example.com/watching.png)');
			await leaveWidget(page, id);
			const ours = pane.locator('.text img[alt="ours"]');
			await until(() => ours.evaluate((el) => el.complete && el.naturalWidth), (w) => w > 0);
			assert.deepEqual(await pane.locator('.text img').evaluateAll((els) => els.map((el) => el.getAttribute('src'))), ['/favicon.png'], 'the outside image was drawn as an image');
			const keep = pane.locator('.text button.keep');
			assert.deepEqual([await keep.textContent(), await keep.getAttribute('data-keep')], ['image from example.com, tap to keep', 'https://example.com/watching.png']);
			assert.deepEqual(asked, [], 'the page asked the outside host');
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	test('a tap keeps an outside image: fetched once, kept once by its hash, its every target written as the kept copy and drawn from the database after a reload with no request; one over the cap is refused, saying so (da-wgf4a)', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const asked = [];
			await page.route('https://example.com/**', (route) => {
				asked.push(route.request().url());
				if (route.request().url().endsWith('gone.png')) return route.fulfill({ status: 404, headers: { 'access-control-allow-origin': '*' }, body: '' });
				const body = route.request().url().endsWith('big.png') ? Buffer.concat([PNG, Buffer.alloc(1 << 20)]) : PNG;
				return route.fulfill({ status: 200, headers: { 'access-control-allow-origin': '*', 'content-type': 'image/png' }, body });
			});
			const { pane, id } = await opened(app);
			await modifyWidget(page, id);
			await pane.locator('textarea').fill('![a](https://example.com/dot.png) and ![b](https://example.com/dot.png)\n\n![big](https://example.com/big.png) ![gone](https://example.com/gone.png)');
			await leaveWidget(page, id);
			const keep = pane.locator('.text button.keep');
			assert.equal(await keep.count(), 4);
			assert.deepEqual(asked, [], 'the page asked the outside host before a tap');

			await keep.first().click();
			const drawnImages = () => pane.locator('.text img').evaluateAll((els) => els.map((el) => [el.getAttribute('src').slice(0, 22), el.complete && el.naturalWidth > 0]));
			const BOTH = [['data:image/png;base64,', true], ['data:image/png;base64,', true]];
			await until(drawnImages, (d) => JSON.stringify(d) === JSON.stringify(BOTH));
			assert.deepEqual(asked, ['https://example.com/dot.png'], 'not fetched once');
			const kept = async () => JSON.parse((await app.run(['--db', '_internal', 'tail', 'attachment', '--json'])).stdout).rows.length;
			assert.equal(await kept(), 1, 'the image used twice is not kept once');

			await pane.locator('.text button.keep').first().click();
			await until(() => pane.locator('p.refused').textContent(), (t) => /at most 1 MB/.test(t ?? ''));
			assert.equal(await kept(), 1, 'the image over the cap was kept');
			await pane.locator('.text button.keep').last().click();
			await until(() => pane.locator('p.refused').textContent(), (t) => /example\.com answered 404/.test(t ?? ''));
			assert.equal(await pane.locator('.text button.keep').count(), 2, 'a refused image lost its target');

			asked.length = 0;
			await app.reload();
			await until(drawnImages, (d) => JSON.stringify(d) === JSON.stringify(BOTH));
			assert.deepEqual(asked, [], 'a reload asked the outside host for a kept image');
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	test('↺ takes back what modify mode wrote', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const { pane, id } = await opened(app);
			await modifyWidget(page, id);
			await pane.locator('textarea').fill('kept');
			await leaveWidget(page, id);
			await until(() => pane.locator('.text').textContent(), (t) => t?.trim() === 'kept');

			await modifyWidget(page, id);
			assert.equal(await pane.locator('textarea').inputValue(), 'kept', 'modify mode lost the source');
			await pane.locator('textarea').fill('thrown away');
			await leaveWidget(page, id, 'reset');
			await until(() => pane.locator('.text').textContent(), (t) => t?.trim() === 'kept');
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});
});
