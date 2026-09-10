// What /now actually says.
//
// The page used to be a dashboard of its own, filled out of the browser's
// SQL, and it filled every card's value slot: a point field read
// "coffee / point / 9h ago", the stored *verb* sitting where the reader had
// learned to look for a state (da-sgvh). Since the CLI became the only
// runner (da-g8o8) the page is `datum now --bare` — the terminal's answer
// to the same question — so this holds it to that: the page shows what the
// CLI said, and no shape ever stands in for a state.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { stripAnsi } from './support/tail.mjs';

before(startApp);
after(stopApp);

/** Follow the header's own `now` link and read what the page shows. */
async function nowText(page) {
	await page.locator('header nav a[href="/now"]').click();
	const shown = page.locator('.now-view pre');
	await shown.first().waitFor();
	return (await shown.first().innerText()).trim();
}

describe('/now', () => {
	test('is `datum now`, and no shape stands in for a state', async () => {
		await withApp(async ({ page, run }) => {
			const commands = [
				['occur', 'coffee', '-t', '0700'],
				['switch', 'mood', 'good', '-t', '0930'],
				['start', 'work', '-t', '0800'],
				['stop', 'work', '-t', '1200']
			];
			for (const argv of commands) {
				const result = await run(argv);
				assert.equal(result.code, 0, `${argv.join(' ')} → ${result.stderr}`);
			}
			const said = await run(['now', '--bare']);
			assert.equal(said.code, 0, said.stderr);

			const shown = await nowText(page);
			assert.equal(shown, stripAnsi(said.stdout).trim(), 'the page is not what the CLI said');
			assert.match(shown, /mood\s+good/);
			// The bug, stated directly: no stored verb where a state goes.
			for (const shape of ['point', 'switch', 'block', 'block-switch', 'point-switch']) {
				assert.doesNotMatch(shown, new RegExp(`\\b${shape}\\b`), shown);
			}
		});
	});
});
