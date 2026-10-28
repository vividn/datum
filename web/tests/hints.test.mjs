// The commands the CLI widget offers a new user must be commands datum accepts.
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
//
// THE PLACEHOLDER ITSELF IS GONE (da-xfa4). vividn: "Let's get rid of the
// always present placeholder `occur coffee cups=2`, that doesn't add to the
// experience for new users and is annoying for non-new users." So the rule it
// was under is asserted the other way round now — the open prompt offers no
// example at all — and the CLI widget's worked examples, which a reader reads once
// and not on every line, are the whole of what this file runs.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp } from './support/app.mjs';
import { ONE_PANE } from './support/layout.mjs';

before(startApp);
after(stopApp);

// ONE APP for the file (da-6b3dj, sharedApp in support/app.mjs): `solo`
// empties the database before each hint is typed, and a hint is typed
// through the quiet door, so the CLI widget's scrollback stays the empty one the
// examples are read from.
const shared = sharedApp();
// `main` as a draft of ONE CLI widget: nothing here reads the day frame,
// and drawing it costs what a boot did (da-m1m9a).
const onSeam = (fn) => shared.solo(fn, ONE_PANE);

/**
 * Every command the freshly opened page offers, as the reader sees it.
 *
 * The `code` spans are the CLI widget's line, which the pane shows only while the
 * scrollback is empty. Read from the page rather than from the source, so a
 * hint that moves or gains a sibling is still covered. The placeholder is
 * read too — to assert that there is none.
 */
let offered;
async function hints() {
	if (!offered) {
		await onSeam(async ({ page }) => {
			offered = {
				placeholder: await page.locator('.prompt-bar input').getAttribute('placeholder'),
				examples: await page.locator('.cli .quiet code').allInnerTexts()
			};
		});
	}
	return offered;
}

/** Type the hint verbatim into a first-time reader's empty database. */
async function typedFresh(line) {
	let result;
	await onSeam(async ({ runLine }) => {
		result = await runLine(line);
	});
	return result;
}

describe('the commands the CLI widget offers a new user', () => {
	test('the open prompt offers no placeholder at all', async () => {
		const { placeholder } = await hints();
		// An example in the box every time you look at it is an example for
		// nobody: a new reader has the CLI widget's, and everyone else has typed it
		// a hundred times (da-xfa4, superseding da-e5u7's choice of text).
		assert.equal(placeholder, '', `the open prompt still hints \`${placeholder}\``);
	});

	test('the worked examples run', async () => {
		const { examples } = await hints();
		assert.ok(examples.length > 0, 'the empty CLI widget offers no worked examples to check');

		for (const example of examples) {
			const result = await typedFresh(example);
			assert.equal(result.code, 0, `\`${example}\` — ${result.stderr}`);
		}
	});
});
