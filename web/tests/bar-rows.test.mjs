// The commands bar's row cap beside a tall pane (da-hs6c, da-arms4): it
// draws as many rows as fit, up to three. The fields bar it stood beside
// had no cap; since da-w6437 it is collect's cluster, which has no rows at
// all (`fields-center.test.mjs`). The helpers are `./support/rows.mjs`.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp, todayAt } from './support/app.mjs';
import { along, draft, widgetPane } from './support/layout.mjs';
import { MAX_ROWS, atRows, axes } from './support/rows.mjs';

before(startApp);
after(stopApp);

describe('the commands bar stops at three rows', () => {
	const SIDE = JSON.stringify(
		along('column', widgetPane('repl', 'cli'), along('row', widgetPane('cmd', 'command'), widgetPane('bar', 'widgetbar')).root)
	);

	test('in a pane six of collect’s rows tall, the commands beside it draw three', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await page.locator('section[data-pane="bar"] .bar').waitFor();
				await atRows(page, 'bar', 6, Infinity);
				const cmd = page.locator('section[data-pane="cmd"] .bar');
				// A submenu: the base row is one row however tall its box (da-fg4w4).
				await cmd.locator('button[data-menu="write"]').click();
				await cmd.locator('button[data-back]').waitFor();
				const fits = await cmd.evaluate((el) => {
					const probe = el.closest('section').querySelector('.probe').offsetHeight;
					return Math.floor((el.clientHeight + 6.4) / (probe + 6.4));
				});
				assert.ok(fits > MAX_ROWS, `the commands box fits ${fits} rows, so its cap is what holds it`);
				assert.equal(await cmd.getAttribute('data-rows'), String(MAX_ROWS), 'the commands bar is capped');
				assert.equal((await axes(cmd)).rows, MAX_ROWS, 'the commands bar draws three rows');
			},
			{
				viewport: { width: 1280, height: 900 },
				clock: todayAt(15),
				prepare: (context) => context.addInitScript(draft, SIDE)
			}
		);
	});
});
