// An integrity finding's label stands beside its line (da-2nxxv). vividn,
// 2026-11-06: «the web dayview and nowview show the integrity error field label
// text not next to the corresponding red dotted line. Instead, it seems to
// appear at the beginning of the day?» The CLI draws it 2 units off the line
// along time; the page lays the picture out at its own size (`layMarks`), and
// read the line's x1 already scaled, so scaled the label's place twice.
//
// And on which side (da-9sjt5), vividn, 2026-11-07: «let's move them to the
// left side of the error line, and also line them up horizontally with the
// center of the band or where the field moment dot would be displayed on the
// axis». So it ends before its line in time, its middle on its field's row.
// Read off the screen, in the day strip at two heights and in the nowview both
// ways round.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, seedLines, todayAt } from './support/app.mjs';
import { settledRoom, say, settledAt, PHONE } from './support/daystrip.mjs';
import { bothWays, withAxes, NOWVIEW } from './support/nowaxes.mjs';

before(startApp);
after(stopApp);

/**
 * Along time, where the finding's line is and where its label starts and
 * ends; across, the label's middle and the middle of its field's row (a
 * block of it); and where the picture `picture` (a selector under `root`)
 * starts along time. In px on the screen. Run in the page.
 */
function placed(root, [picture, y]) {
	const g = root.querySelector('.integrity-error');
	if (!g) return null;
	const row = root.querySelector(`.block[data-field="${g.getAttribute('data-field')}"]`);
	const [line, label, start, lane] = [g.querySelector('line'), g.querySelector('.error-label'), root.querySelector(picture), row].map((e) => e?.getBoundingClientRect());
	return y
		? { line: line.top + line.height / 2, from: label.top, to: label.bottom, start: start.top, mid: label.left + label.width / 2, row: lane && lane.left + lane.width / 2 }
		: { line: line.left + line.width / 2, from: label.left, to: label.right, start: start.left, mid: label.top + label.height / 2, row: lane && lane.top + lane.height / 2 };
}

/** The label ends just before its line, and its middle is on its field's row. */
function beside(p, what) {
	assert.ok(p, `${what}: no finding drawn`);
	assert.ok(p.line - p.start > 50, `${what}: the line is ${p.line - p.start}px into the picture, too near its start to tell`);
	assert.ok(p.to <= p.line + 1 && p.to > p.line - 8, `${what}: the label ends at ${p.to}, its line at ${p.line}`);
	assert.ok(p.from < p.to - 4, `${what}: the label runs ${p.from} to ${p.to}`);
	assert.ok(p.row !== undefined, `${what}: no row of its field drawn`);
	assert.ok(Math.abs(p.mid - p.row) <= 1.5, `${what}: the label's middle is at ${p.mid}, its field's row at ${p.row}`);
}

const strip = sharedApp({ ...PHONE, clock: todayAt(16) });

for (const dayHeight of [60, 120]) {
	test(`in the day strip ${dayHeight} px high, the label stands beside its line`, async () => {
		await settledRoom(strip, `label-${dayHeight}`, async (app) => {
			await say(app, { dayHeight });
			await settledAt(app, dayHeight);
			// Started twice: `check integrity` draws a finding at 14:00.
			await seedLines(app, ['start rd -t 13:00', 'start rd -t 14:00']);
			const band = app.page.locator('.band[data-days-ago="0"]');
			await band.locator('.integrity-error').waitFor({ state: 'attached' });
			beside(await band.evaluate(placed, ['svg', false]), `the day strip at ${dayHeight}`);
			assert.deepEqual(app.errors, []);
		});
	});
}

bothWays((vertical) => {
	test('in the nowview, the label stands beside its line', async () => {
		await withAxes(vertical, async (app) => {
			// Started twice: a finding at 14:45, inside the window.
			for (const t of ['14:40', '14:45']) assert.equal((await app.run(['start', 'rd', '-t', t])).code, 0);
			await app.redraw();
			const view = app.page.locator(NOWVIEW);
			await view.locator('svg.svg .integrity-error').first().waitFor({ state: 'attached' });
			beside(await view.evaluate(placed, ['.picture', vertical]), `the nowview, ${vertical ? 'down' : 'across'}`);
			assert.deepEqual(app.errors, []);
		});
	});
});
