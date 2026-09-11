// The commands the REPL offers a new user must be commands datum accepts.
//
// The prompt's placeholder was `occur coffee 2 cups`, which errors — datum
// wants `cups=2`, and says so: "Extra data with no matching key". It was the
// only worked example on the screen and the first thing a new user types, so
// the app's opening move was a parse error (da-e5u7).
//
// Nothing caught it because nothing ran it. A test comparing the placeholder
// to a literal would have passed every day the bug existed, so this one reads
// the hints off the rendered page and RUNS them, each against its own empty
// database — the state a first-time reader is in. The hint and the parser can
// then only drift apart by failing here, whichever of the two moves.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';

before(startApp);
after(stopApp);

/**
 * Every command the freshly opened page offers, as the reader sees it.
 *
 * The placeholder is the bottom bar's prompt's own hint; the `code` spans
 * are the REPL's line, which the pane shows only while the scrollback is
 * empty. Both
 * are read from the page rather than from the source, so a hint that moves
 * or gains a sibling is still covered.
 */
let offered;
async function hints() {
	if (!offered) {
		await withApp(async ({ page }) => {
			offered = {
				placeholder: await page.locator('.prompt-bar input').getAttribute('placeholder'),
				examples: await page.locator('.repl .quiet code').allInnerTexts()
			};
		});
	}
	return offered;
}

/** Type the hint verbatim into a first-time reader's empty database. */
async function typedFresh(line) {
	let result;
	await withApp(async ({ runLine }) => {
		result = await runLine(line);
	});
	return result;
}

describe('the commands the REPL offers a new user', () => {
	test('the placeholder runs', async () => {
		const { placeholder } = await hints();
		assert.ok(placeholder, 'the enabled prompt shows no placeholder to check');

		const result = await typedFresh(placeholder);
		assert.equal(result.code, 0, `\`${placeholder}\` — ${result.stderr}`);
	});

	test('the worked examples run', async () => {
		const { examples } = await hints();
		assert.ok(examples.length > 0, 'the empty REPL offers no worked examples to check');

		for (const example of examples) {
			const result = await typedFresh(example);
			assert.equal(result.code, 0, `\`${example}\` — ${result.stderr}`);
		}
	});
});
