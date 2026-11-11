// A label inside a bar is cut twice (da-cepjy, da-ug7rv). datum cuts it to
// the bar's room at a second a unit, where any text fits, and carries that
// room and the whole text (`data-room`, `data-text`, `dayview_svg.rs`); the
// page lays the layer out at its own scale along time and the dot's across
// (`layMarks`), and cuts it again to the room the bar now has (`cutToRoom`,
// `dayLayers.ts`). Read off the screen, in the day strip: at a day height
// the dot's scale follows (120) and one past its cap (480, drawn as 240,
// da-4pak5), and again with the window zoomed in on the short bar.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, seedLines, todayAt } from './support/app.mjs';
import { settledRoom, say, settledAt, PHONE } from './support/daystrip.mjs';

before(startApp);
after(stopApp);

const WHOLE = 'riverside';
const BAND = '.band[data-days-ago="0"]';

/** Each of walk's bars on screen, left to right: its box, and its label's box, text and whole text. */
const bars = (band) =>
	band.evaluate((root) => {
		const box = (el) => {
			const { left, right, top, bottom } = el.getBoundingClientRect();
			return { left, right, top, bottom };
		};
		const labels = [...root.querySelectorAll('text.point-label.inside-start')];
		return [...root.querySelectorAll('rect.block[data-field="walk"]')]
			.map((rect) => {
				const bar = box(rect);
				// A label stands from its bar's start, in from it.
				const label = labels.find((l) => Math.abs(Number(l.getAttribute('x')) - Number(rect.getAttribute('x'))) < 0.5 * Number(l.getAttribute('font-size')));
				return { bar, label: label && { ...box(label), text: label.textContent, whole: label.getAttribute('data-text') } };
			})
			.sort((a, b) => a.bar.left - b.bar.left);
	});

/** The label is a cut of its whole text, ending in `…`, and its ink is inside its bar. */
function cut(label, bar, what) {
	assert.ok(label, `${what}: no label drawn`);
	assert.equal(label.whole, WHOLE, `${what}: the whole text carried`);
	assert.ok(label.text.endsWith('…') && label.text.length < WHOLE.length, `${what}: «${label.text}» is not cut`);
	assert.ok(WHOLE.startsWith(label.text.slice(0, -1)), `${what}: «${label.text}» is not a cut of «${WHOLE}»`);
	assert.ok(label.left >= bar.left - 0.5 && label.right <= bar.right + 0.5, `${what}: the label runs ${label.left}–${label.right}, its bar ${bar.left}–${bar.right}`);
}

const strip = sharedApp({ ...PHONE, clock: todayAt(16) });

for (const dayHeight of [120, 480]) {
	test(`in the day strip ${dayHeight} px high, a label inside a bar is cut to the bar's room, and grows back as the window zooms in`, async () => {
		await settledRoom(strip, `barlabel-${dayHeight}`, async (app) => {
			await say(app, { dayHeight });
			await settledAt(app, dayHeight);
			const setup = { field: 'walk', verb: 'start', labels: [{ show: 'why', at: 'inside-start' }] };
			assert.equal((await app.run(['setup', 'walk', '--from-json', JSON.stringify(setup)])).code, 0);
			await seedLines(app, [`start walk -t 00:10 why=${WHOLE}`, 'stop walk -t 11:40', `start walk -t 11:50 why=${WHOLE}`, 'stop walk -t 15:50']);
			const band = app.page.locator(BAND);
			await app.page.waitForFunction((sel) => document.querySelectorAll(`${sel} text.point-label.inside-start`).length === 2, BAND);

			const [long, short] = await bars(band);
			assert.equal(long.label?.text, WHOLE, `the long bar's label at ${dayHeight}: ${JSON.stringify(long)}`);
			assert.ok(long.label.right <= long.bar.right + 0.5, `the long bar's label runs past it: ${JSON.stringify(long)}`);
			cut(short.label, short.bar, `the short bar at ${dayHeight}`);

			// Zoomed in on the short bar: it is four times as long, its label as big.
			await say(app, { hourFrom: 11, hourTo: 17 });
			await app.page.waitForFunction(
				([sel, was]) => {
					const label = [...document.querySelectorAll(`${sel} text.point-label.inside-start`)].at(-1);
					return label && label.textContent !== was;
				},
				[BAND, short.label.text]
			);
			const zoomed = (await bars(band)).at(-1);
			assert.ok(zoomed.bar.right - zoomed.bar.left > 3 * (short.bar.right - short.bar.left), `the window zoomed: the bar ${JSON.stringify(zoomed.bar)}, was ${JSON.stringify(short.bar)}`);
			assert.ok(zoomed.label.text.length > short.label.text.length, `zoomed in, «${zoomed.label.text}» against «${short.label.text}»`);
			assert.ok(zoomed.label.text === WHOLE || WHOLE.startsWith(zoomed.label.text.slice(0, -1)), `zoomed in, «${zoomed.label.text}»`);
			assert.ok(zoomed.label.left >= zoomed.bar.left - 0.5 && zoomed.label.right <= zoomed.bar.right + 0.5, `zoomed in, the label runs past its bar: ${JSON.stringify(zoomed)}`);
			assert.deepEqual(app.errors, []);
		});
	});
}
