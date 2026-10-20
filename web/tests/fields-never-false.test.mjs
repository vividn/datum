// What stands under the tap when a switch field opens (da-cwieh). vividn:
// «for a switch field, the button that should be immediately under the
// cursor when it opens is "end", unles the false state is somehow removed
// from the field … For example environment is never false. So if false is
// disabled, then don't put any button right under that position, but
// rather a gap. "< Back" should always be immediately to the left of this».
// Where the states stand around it is `fields-cluster.test.mjs`.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp, seedLines, todayAt } from './support/app.mjs';
import { along, draft, widgetPane } from './support/layout.mjs';
import { atRows } from './support/rows.mjs';
import { FIELD_FORM, cancelField, openField } from './support/field.mjs';

const PAGE = JSON.stringify(along('column', widgetPane('repl', 'repl'), widgetPane('bar', 'widgetbar')));
const BAR = 'section[data-pane="bar"] .bar';
const GAP = 6;
/** Fields around the two alphabetically, so each stands mid-row and can be scrolled to the middle. */
const OTHERS = 'alfa bravo charlie delta echo foxtrot golf hotel india juliet kilo lima november oscar papa'.split(' ');

/**
 * Scroll the bar until `field` is at its middle, press it there, and read what is under the press, `< Back`
 * and the centre's box (`end`, or the gap); then Back.
 */
async function tap(page, field) {
	await page.locator(`${BAR} [data-field="${field}"]`).waitFor();
	const at = await page.locator(BAR).evaluate((bar, field) => {
		const box = bar.getBoundingClientRect();
		const button = () => bar.querySelector(`[data-field="${field}"]`);
		const m = button().getBoundingClientRect();
		bar.scrollLeft += m.left + m.width / 2 - (box.left + box.width / 2);
		const r = button().getBoundingClientRect();
		button().click();
		return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
	}, field);
	await page.locator(`${BAR} [data-add-state]`).waitFor();
	const seen = await page.locator(BAR).evaluate((bar, at) => {
		const box = (el) => {
			const r = el.getBoundingClientRect();
			return { left: r.left, right: r.right, y: r.top + r.height / 2, x: r.left + r.width / 2 };
		};
		const under = document.elementFromPoint(at.x, at.y)?.closest('button');
		return {
			under: under ? (under.dataset.state ?? under.textContent.trim()) : null,
			centre: box(bar.querySelector('.ring [data-role="end"]')),
			back: box(bar.querySelector('.ring [data-back]'))
		};
	}, at);
	await page.locator(`${BAR} [data-back]`).dispatchEvent('click');
	return { at, ...seen };
}

describe('a switch field opens with `end` under the tap, a never-false one with a gap', () => {
	before(startApp);
	after(stopApp);

	for (const viewport of [
		{ width: 1280, height: 800 },
		{ width: 390, height: 640 }
	])
		test(`${viewport.width}px`, async () => {
			await withApp(
				async (app) => {
					const { page } = app;
					// Neither lists `false`: `end` is the default all the same.
					await seedLines(app, [
						...OTHERS.map((f) => `setup ${f} --verb occur`),
						'setup mood --verb switch --states good,meh,bad',
						'setup env --verb switch --states home,office,cafe --never-false'
					]);
					await atRows(page, 'bar', 3);

					const mood = await tap(page, 'mood');
					assert.equal(mood.under, 'false', `under the tap on mood: ${mood.under}`);
					assert.ok(Math.abs(mood.centre.x - mood.at.x) <= 1, 'end is not on the tap');

					const env = await tap(page, 'env');
					assert.equal(env.under, null, `under the tap on env: ${env.under}`);
					assert.ok(Math.abs(env.centre.x - env.at.x) <= 1, 'the gap is not on the tap');
					// `< Back` immediately left of the gap, on its line.
					assert.ok(Math.abs(env.back.right + GAP - env.centre.left) <= 1, `< Back ends at ${env.back.right}, the gap starts at ${env.centre.left}`);
					assert.ok(Math.abs(env.back.y - env.centre.y) <= 1, '< Back is off the gap’s line');
					assert.ok(Math.abs(env.centre.right - env.centre.left - (mood.centre.right - mood.centre.left)) <= 1, 'the gap is not end’s width');

					// Basics says so, off first among its states and not one of them, and turns it back on (da-i9upr).
					const { saved } = await openField(page, 'env');
					const box = page.locator(`${FIELD_FORM} .states li.off input.off-state`);
					assert.equal(await box.isChecked(), false);
					assert.deepEqual(await page.locator(`${FIELD_FORM} .sample .block`).evaluateAll((all) => [...new Set(all.map((b) => b.dataset.state))]), ['home', 'office', 'cafe'], 'off is drawn');
					// A field that lists `false` keeps its `end`, so the box cannot say otherwise.
					await page.locator(`${FIELD_FORM} .states + button.add`).click();
					await page.locator(`${FIELD_FORM} .state input.name`).last().fill('false');
					assert.equal(await box.isDisabled(), true, 'a listed `false` leaves off to untick');
					await page.locator(`${FIELD_FORM} [aria-label="remove false"]`).click();
					assert.equal(await box.isDisabled(), false);
					await box.check();
					assert.equal(await page.locator(`${FIELD_FORM} .sample .block[data-state="off"]`).count() > 0, true, 'off is not drawn');
					await page.locator(`${FIELD_FORM} .commit`).click();
					await saved;
					assert.equal(JSON.parse((await app.run(['setup', 'env', '--json'])).stdout).never_false, undefined);
					await app.idle();
					// On a phone the spec is a tab, which slides back after Done:
					// tapped mid-glide, the bar is not where it stands.
					await page.waitForFunction(() => !document.querySelector('.moving'));
					assert.equal((await tap(page, 'env')).under, 'false', 'end did not come back');

					// Only a switch has a ring, so only a switch is offered it.
					const { saved: left } = await openField(page, 'alfa');
					await page.locator(`${FIELD_FORM} .sample`).waitFor();
					assert.equal(await page.locator(`${FIELD_FORM} input.off-state`).count(), 0, 'an occur field offers off');
					await cancelField(page);
					await left;
				},
				{ viewport, clock: todayAt(15), prepare: (context) => context.addInitScript(draft, PAGE) }
			);
		});
});
