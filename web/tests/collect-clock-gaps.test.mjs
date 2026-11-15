// Collect's clock line stands as far from the now section as from the fields
// (da-tpcw9). vividn, 2026-11-15: «the margin on the clock line should be
// symmetric between now section and fields section.»

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { ON_COLLECT, startApp, stopApp, todayAt, withApp } from './support/app.mjs';
import { DESKTOP, PHONE, pickOption } from './support/layout.mjs';

before(startApp);
after(stopApp);

const WIDGET = 'section.pane[data-title="collect"]';
const WHEN = `${WIDGET} .bar [data-when]`;
const CLOCK = `${WHEN} [data-when-clock]`;
const SHOTS = process.env.DATUM_SHOTS;

const SEED = ['setup mood --verb switch --states good,bad,meh', 'setup coffee --verb occur', 'occur coffee -t 09:00', 'occur tea -t 12:00', 'switch mood bad -t 13:00'];

/** Past the glide: nothing in the widget is moving, the section or the rows. */
const still = (page) => page.waitForFunction((w) => document.querySelector(w).getAnimations({ subtree: true }).length === 0, WIDGET);

/**
 * The two gaps toward the rows (mirrored for `side: right`), each clear of the
 * line's own pixel: from the strip's picture as drawn (its marks, cut to its
 * box) to the near line, and from the far line (closed, the one line) to the
 * fields section, its nearest field at rest or the active section's dotted
 * outline round it, whichever stands nearer.
 */
const gaps = (page, side) =>
	page.locator(WHEN).evaluate((when, side) => {
		const flip = (x) => (side === 'right' ? -x : x);
		const pane = when.closest('section.pane');
		const svg = pane.querySelector('[data-strip] svg.now-strip');
		const s = svg.getBoundingClientRect();
		const marks = [...svg.querySelectorAll('g rect, g path')].map((e) => e.getBoundingClientRect()).filter((r) => r.width > 0 && r.height > 0);
		const ink = side === 'right' ? Math.max(s.left, Math.min(...marks.map((r) => r.left))) : Math.min(s.right, Math.max(...marks.map((r) => r.right)));
		const r = when.getBoundingClientRect();
		const closed = when.classList.contains('closed');
		// Each line is a 1px border: its edge toward the picture, and toward the rows.
		const [nearLine, farLine] = side === 'right' ? [r.right, closed ? r.right - 1 : r.left] : [r.left, closed ? r.left + 1 : r.right];
		const outline = pane.querySelector('.room svg.dotted path');
		const marked = [...pane.querySelectorAll('.room [data-field]'), ...(outline ? [outline] : [])].map((b) => b.getBoundingClientRect());
		const fields = Math.min(...marked.map((b) => flip(side === 'right' ? b.right : b.left)));
		return { now: flip(nearLine) - flip(ink), fields: fields - flip(farLine), outline: !!outline, marks: marks.length, closed };
	}, side);

describe('collect: the clock line', () => {
	for (const [device, side, options] of [
		['phone', 'left', PHONE],
		['phone', 'right', PHONE],
		['desktop', 'left', DESKTOP]
	])
		test(`${device}, side: ${side}: the line stands as far from the now section as from the fields, closed and open`, async () => {
			await withApp(
				async (app) => {
					const { page } = app;
					for (const line of SEED) assert.equal((await app.run(line.split(' '))).code, 0, line);
					await app.reload();
					await page.locator(`${WIDGET} .bar .collect .room .spot.shelf`).first().waitFor();
					if (side === 'right') await pickOption(page, 'bar', 'side', 'right');
					await page.locator(`${CLOCK}[aria-expanded="false"]`).waitFor();
					await app.idle();
					for (const open of [false, true]) {
						if (open) await page.locator(CLOCK).click();
						await still(page);
						const m = await gaps(page, side);
						const at = JSON.stringify({ device, side, ...m });
						console.log(`gaps ${at}`);
						if (SHOTS) await page.locator(WIDGET).screenshot({ path: `${SHOTS}/gaps-${device}-${side}-${open ? 'open' : 'closed'}.png` });
						assert.equal(m.closed, !open, `the section is not ${open ? 'open' : 'closed'}: ${at}`);
						assert.ok(m.marks > 0, `nothing drawn on the strip: ${at}`);
						assert.ok(Math.abs(m.now - m.fields) <= 1, `the gaps differ: ${at}`);
					}
				},
				{ ...options, home: ON_COLLECT, clock: todayAt(15) }
			);
		});
});
