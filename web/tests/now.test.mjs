// What the /now cards actually say.
//
// The page filled every card's value slot: an entry with a state showed the
// state, and one without showed the *stored verb* instead — so a point field
// read "coffee / point / 9h ago", with "point" sitting between "good" and
// "false" in the same typographic position the reader has just learned means
// "the current value". "point" is a shape, not a state. Italics were the only
// hint, and a reader has no way to read italics as "placeholder" (da-sgvh).
//
// Nothing caught it because the suites compare what an argv STORES — the
// cli-parity fixtures are field/verb/state/dur/data — and nothing compares
// what a view RENDERS. So this test reads the cards off the screen. An
// assertion on stored state would pass while the page still said the wrong
// word.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';

before(startApp);
after(stopApp);

/**
 * Follow the header's own `now` link and read back every card.
 *
 * `value: null` is the card having no value line at all, which is different
 * from its having an empty one.
 */
async function nowCards(page) {
	await page.locator('header nav a[href="/now"]').click();
	await page.locator('.now-view .field-card').first().waitFor();
	return page.$$eval('.now-view .field-card', (cards) =>
		cards.map((card) => ({
			field: card.querySelector('.field-name').textContent.trim(),
			value: card.querySelector('.field-value')?.textContent.trim() ?? null,
			time: card.querySelector('.field-time').textContent.trim(),
			text: card.textContent.replace(/\s+/g, ' ').trim()
		}))
	);
}

describe('the /now cards', () => {
	test('a point field shows no value, and every value shown is a real one', async () => {
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

			const cards = await nowCards(page);
			const byField = Object.fromEntries(cards.map((c) => [c.field, c]));

			// The card stays — dropping the point from the list would be a
			// different change, and one this bead explicitly does not make:
			// whether /now mirrors `datum now` (which omits points and
			// stopped fields) or is its own dashboard is undecided. The fix
			// is confined to the value slot, and this asserts it stayed
			// there rather than silently answering the wider question.
			assert.ok(byField.coffee, `no coffee card among ${cards.map((c) => c.field)}`);

			// The bug, stated directly.
			assert.equal(byField.coffee.value, null, 'a point field filled the value slot');
			assert.doesNotMatch(byField.coffee.text, /point/);

			// And it still says everything true about itself: its name and
			// when it happened.
			assert.notEqual(byField.coffee.time, '');

			// The fields that do have a state still show it — the fix is
			// not "stop showing values".
			assert.equal(byField.mood.value, 'good');

			// Nothing anywhere renders a shape where a value goes. The
			// stored verbs are src/lib/types.ts's `StoredVerb`.
			const shapes = ['point', 'switch', 'block', 'block-switch', 'point-switch'];
			for (const card of cards) {
				if (card.value !== null) assert.ok(!shapes.includes(card.value), card.text);
			}
		});
	});
});
