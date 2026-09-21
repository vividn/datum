// The scroll rule's vocabulary (da-ltux, da-hs6c): sizing a widget's pane
// to a number of button-rows, and reading back how its bar is drawn.
// Shared by `commands.test.mjs` and `bar-rows.test.mjs`.

import assert from 'node:assert/strict';

const GAP = 6.4;

/**
 * Resize the page until the bar in pane `id` reports `want` rows.
 *
 * The bar says how many rows its box holds (`data-rows`), and the box is
 * a share of the page, so how much of a change in the page's height
 * reaches this pane is measured between two sizes rather than assumed.
 * The box aimed at is the middle of the band that holds `want` rows, so a
 * pixel of rounding cannot land on the edge of the next one.
 */
export async function atRows(page, id, want) {
	/** The bar's rows, the rows its box holds, one row's height, and the box. */
	const read = () =>
		page.evaluate(
			({ id, gap }) => {
				const pane = document.querySelector(`section[data-pane="${id}"]`);
				const box = pane.querySelector('.bar').clientHeight;
				const row = pane.querySelector('.probe button').offsetHeight;
				const el = pane.querySelector('.bar');
				const rows = Number(el.dataset.rows);
				// The TALLEST button, not the probe's: a bar's rows are as tall
				// as the tallest thing in them (`Track +` is taller than a
				// field's chip), so a box that holds `want` probe rows can
				// still be a few pixels short of the real ones — measured at
				// 280x275, box 61px against two 27px probes, every button 3px
				// above the top (da-ckw3, found when the bar got shorter and
				// the search landed on that height instead of beside it).
				const tall = Math.max(
					row,
					...[...el.querySelectorAll('button:not([tabindex=\"-1\"])')].map((b) => b.offsetHeight)
				);
				return {
					rows,
					row,
					box,
					tall,
					counted: rows === Math.max(1, Math.floor((box + gap) / (row + gap)))
				};
			},
			{ id, gap: GAP }
		);
	let last = null;
	for (let i = 0; i < 14; i += 1) {
		const { rows, row, box, tall } = await read();
		// The rows drawn AND the room for them at their real height.
		if (rows === want && box + 0.5 >= want * tall + (want - 1) * GAP) return;
		const { width, height } = page.viewportSize();
		const measured = last && last.height !== height ? (box - last.box) / (height - last.height) : 0;
		const slope = measured > 0.05 ? measured : 1 / 3;
		const target = (want + 0.5) * (row + GAP) - GAP;
		last = { height, box };
		const next = Math.round(height + (target - box) / slope);
		await page.setViewportSize({ width, height: Math.min(3000, Math.max(200, next)) });
		// Wait on the thing, not on frames — under the fake clock a rAF is a
		// timer: the box has moved and the bar has counted its rows in it.
		for (let t = 0; t < 40; t += 1) {
			const now = await read();
			if (now.box !== box && now.counted) break;
		}
	}
	assert.fail(`the bar in ${id} never reported ${want} rows`);
}

/**
 * How a bar scrolls, and how many rows its buttons are drawn on.
 *
 * The rows are counted by the middle of each button, since the bar centres
 * what it lays out and two buttons of different heights on one line share
 * only that. The probe is not one of them.
 */
export const axes = (locator) =>
	locator.evaluate((bar) => {
		const style = getComputedStyle(bar);
		const lines = new Set(
			[...bar.querySelectorAll('button:not([tabindex="-1"])')].map((b) => {
				const box = b.getBoundingClientRect();
				return Math.round(box.top + box.height / 2);
			})
		);
		return {
			x: style.overflowX,
			y: style.overflowY,
			display: style.display,
			rows: lines.size,
			/** Does the bar hold its buttons, or spill them out of the pane? */
			fits: bar.getBoundingClientRect().width <= bar.parentElement.getBoundingClientRect().width + 1
		};
	});

/**
 * Where each of the bar's `selector` buttons is drawn, in DOM order, and
 * whether it sits wholly inside the bar's box top to bottom — so a second
 * row is not a clipped half of one.
 */
export const boxes = (locator, selector) =>
	locator.evaluate(
		(bar, selector) => {
			const outer = bar.getBoundingClientRect();
			return [...bar.querySelectorAll(selector)].map((b) => {
				const box = b.getBoundingClientRect();
				return {
					name: b.dataset.command ?? b.dataset.field ?? b.dataset.state ?? b.textContent.trim(),
					left: Math.round(box.left),
					line: Math.round(box.top + box.height / 2),
					inside: box.top >= outer.top - 0.5 && box.bottom <= outer.bottom + 0.5
				};
			});
		},
		selector
	);

/** Scroll the bar to its right end: is its last `selector` button then wholly in it? */
export const lastReached = (locator, selector) =>
	locator.evaluate(
		(bar, selector) => {
			bar.scrollLeft = bar.scrollWidth;
			const outer = bar.getBoundingClientRect();
			const last = [...bar.querySelectorAll(selector)].at(-1).getBoundingClientRect();
			const reached = last.left >= outer.left - 0.5 && last.right <= outer.right + 0.5;
			bar.scrollLeft = 0;
			return reached;
		},
		selector
	);
