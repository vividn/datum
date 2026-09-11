// What /now actually shows.
//
// The page used to be a dashboard of its own, filled out of the browser's
// SQL, and it filled every card's value slot: a point field read
// "coffee / point / 9h ago", the stored *verb* sitting where the reader had
// learned to look for a state (da-sgvh). Since the CLI became the only
// runner (da-g8o8) the answer is datum's own; and since the shell
// (da-0a5o) `/now` is the nowview widget as a root tab — `datum nowview`
// drawn at the tab's size — so this holds it to that: the picture carries
// each field's current state, read off the CLI's own `now`, and no shape
// ever stands in for one.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { stripAnsi } from './support/tail.mjs';

before(startApp);
after(stopApp);

describe('/now', () => {
	test('is the nowview tab, drawn from `datum now`, and no shape stands in for a state', async () => {
		await withApp(async (app) => {
			const { page, run } = app;
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
			assert.match(stripAnsi(said.stdout), /mood\s+good/);

			await app.goto('/now');
			assert.equal(await page.locator('.root-bar [role="tab"][aria-selected="true"]').innerText(), 'nowview');
			await page.locator('section.pane[data-title="nowview"] svg.svg').waitFor();
			const svg = await app.paneSvg('nowview');
			// The field the CLI says has a state is drawn with one past the
			// now-line (fit.test.mjs reads the same chunk), and the shapes are
			// nowhere in the picture's text.
			assert.match(svg, /<g class="field" data-field="mood">[\s\S]*?<rect class="now-chunk/);
			const texts = [...svg.matchAll(/<text[^>]*>([^<]*)<\/text>/g)].map((m) => m[1]);
			for (const shape of ['point', 'switch', 'block', 'block-switch', 'point-switch']) {
				assert.ok(!texts.some((t) => t.trim() === shape), `a shape word is drawn as a state: ${shape}`);
			}
		});
	});
});
