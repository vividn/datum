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
import { FIELD_FORM, leaveField, openField } from './support/field.mjs';

const PAGE = JSON.stringify(along('column', widgetPane('repl', 'cli'), widgetPane('bar', 'widgetbar')));
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
					// Off's ✕, pressed: off is struck (da-puzij).
					const box = page.locator(`${FIELD_FORM} .states li.off button.off-state`);
					assert.equal(await box.getAttribute('aria-pressed'), 'true');
					// Its sample on the placement's band (da-srrqd): its states in turn, and off a gap between them.
					await page.locator(`${FIELD_FORM} .placement .band svg`).waitFor();
					const drawn = () =>
						page.locator(`${FIELD_FORM} .placement .band g.field[data-field="env"] rect.block`).evaluateAll((all) => {
							const r = all.map((b) => ({ s: b.dataset.state, x: +b.getAttribute('x'), w: +b.getAttribute('width') })).sort((a, b) => a.x - b.x);
							return { states: [...new Set(r.map((b) => b.s))], gaps: r.filter((b, i) => i && b.x > r[i - 1].x + r[i - 1].w + 0.5).length };
						});
					assert.deepEqual(await drawn(), { states: ['home', 'office', 'cafe'], gaps: 0 }, 'off is drawn');
					// A field that lists `false` keeps its `end`, so the box cannot say otherwise.
					await page.locator(`${FIELD_FORM} .states + button.add`).click();
					await page.locator(`${FIELD_FORM} .state input.name`).last().fill('false');
					assert.equal(await box.isDisabled(), true, 'a listed `false` leaves off to untick');
					await page.locator(`${FIELD_FORM} [aria-label="remove false"]`).click();
					assert.equal(await box.isDisabled(), false);
					await box.click();
					assert.equal(await box.getAttribute('aria-pressed'), 'false');
					for (const end = Date.now() + 5000; (await drawn()).gaps === 0; await page.waitForTimeout(50))
						assert.ok(Date.now() < end, 'off is not drawn');
					await page.locator(`${FIELD_FORM} .commit`).click();
					await saved;
					assert.equal(JSON.parse((await app.run(['setup', 'env', '--json'])).stdout).never_false, undefined);
					// Save stays on the spec (da-hda4j): back to the bar.
					await leaveField(page);
					await app.idle();
					// On a phone the spec is a tab, which slides back when left:
					// tapped mid-glide, the bar is not where it stands.
					await page.waitForFunction(() => !document.querySelector('.moving'));
					assert.equal((await tap(page, 'env')).under, 'false', 'end did not come back');

					// Only a switch has a ring, so only a switch is offered it.
					const { saved: left } = await openField(page, 'alfa');
					await page.locator(`${FIELD_FORM} .placement .band svg`).waitFor();
					assert.equal(await page.locator(`${FIELD_FORM} .off-state`).count(), 0, 'an occur field offers off');
					await leaveField(page);
					await left;
				},
				{ viewport, clock: todayAt(15), prepare: (context) => context.addInitScript(draft, PAGE) }
			);
		});
});
