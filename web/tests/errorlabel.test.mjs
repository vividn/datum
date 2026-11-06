// An integrity finding's label stands beside its line (da-2nxxv). vividn,
// 2026-11-06: «the web dayview and nowview show the integrity error field label
// text not next to the corresponding red dotted line. Instead, it seems to
// appear at the beginning of the day?» The CLI draws it 2 units after the line
// along time; the page lays the picture out at its own size (`layMarks`), and
// read the line's x1 already scaled, so scaled the label's place twice.
// Read off the screen, in the day strip and in the nowview both ways round.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, seedLines, todayAt } from './support/app.mjs';
import { settledRoom, say, settledAt, PHONE } from './support/daystrip.mjs';
import { bothWays, withAxes, NOWVIEW } from './support/nowaxes.mjs';

before(startApp);
after(stopApp);

/**
 * Along time, where the finding's line is, where its label starts and ends,
 * and where the picture `picture` (a selector under `root`) starts, in px on
 * the screen. Run in the page.
 */
function placed(root, [picture, y]) {
	const g = root.querySelector('.integrity-error');
	if (!g) return null;
	const [line, label, start] = [g.querySelector('line'), g.querySelector('.error-label'), root.querySelector(picture)].map((e) => e.getBoundingClientRect());
	return y
		? { line: line.top + line.height / 2, from: label.top, to: label.bottom, start: start.top, y }
		: { line: line.left + line.width / 2, from: label.left, to: label.right, start: start.left, y };
}

/** Horizontally the label starts just after its line; down the band it ends just before it. */
function beside(p, what) {
	assert.ok(p, `${what}: no finding drawn`);
	assert.ok(p.line - p.start > 50, `${what}: the line is ${p.line - p.start}px into the picture, too near its start to tell`);
	if (p.y) assert.ok(p.to <= p.line + 3 && p.to > p.line - 8, `${what}: the label ends at ${p.to}, its line at ${p.line}`);
	else assert.ok(p.from >= p.line && p.from < p.line + 6, `${what}: the label starts at ${p.from}, its line at ${p.line}`);
}

const strip = sharedApp({ ...PHONE, clock: todayAt(16) });

test('in the day strip, the label stands beside its line', async () => {
	await settledRoom(strip, 'label', async (app) => {
		await say(app, { dayHeight: 60 });
		await settledAt(app, 60);
		// Started twice: `check integrity` draws a finding at 14:00.
		await seedLines(app, ['start rd -t 13:00', 'start rd -t 14:00']);
		const band = app.page.locator('.band[data-days-ago="0"]');
		await band.locator('.integrity-error').waitFor({ state: 'attached' });
		beside(await band.evaluate(placed, ['svg', false]), 'the day strip');
		assert.deepEqual(app.errors, []);
	});
});

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
