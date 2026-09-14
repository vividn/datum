// The dashboard extends from its edges (da-qk2k): a faint `…` in the
// page's margin on each side of the root's box, whose one item, Add
// widget, adds a pane spanning that whole side.
//
// Boxes and tree shapes, never "one more pane than before" alone: a
// delta-blind probe passes against a page that grew the wrong pane.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { fill } from './support/layout.mjs';

before(startApp);
after(stopApp);

const SCREENS = [
	{ width: 1280, height: 800 },
	{ width: 390, height: 844 }
];

const handle = (page, edge) => page.getByRole('button', { name: `${edge} edge of the dashboard` });

/** Choose Add widget from the `edge` handle's menu. */
async function extend(page, edge) {
	await handle(page, edge).click();
	await page.getByRole('menuitem', { name: 'Add widget', exact: true }).click();
	await page.locator('[role="menu"]').waitFor({ state: 'detached' });
}

/** The root split's box, and the box of the empty pane the edge just added. */
const boxes = (page) =>
	page.evaluate(() => {
		const rect = (el) => {
			const { left, right, top, bottom } = el.getBoundingClientRect();
			return { left, right, top, bottom };
		};
		const empty = document.querySelector('section.pane[data-title="empty"]');
		return {
			split: rect(document.querySelector('section.root > .content > .split')),
			fresh: empty ? rect(empty) : null
		};
	});

/** The active root tab's content, from the draft the page saved. */
const draft = (page) =>
	page.evaluate(() => {
		const layout = JSON.parse(localStorage.getItem('datum.layout.draft.v1.day'));
		return layout.root.tabs[layout.root.active].content;
	});

const near = (a, b) => Math.abs(a - b) <= 1;

describe('the edge handles', () => {
	for (const viewport of SCREENS) {
		const size = `${viewport.width}x${viewport.height}`;

		test(`are four, in the margin, over neither a pane nor the prompt bar, at ${size}`, async () => {
			await withApp(async ({ page }) => {
				const seen = await page.evaluate(() => {
					const rect = (el) => el.getBoundingClientRect();
					const overlaps = (a, b) =>
						a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom;
					const root = rect(document.querySelector('section.root'));
					const others = [...document.querySelectorAll('section[data-pane], .prompt-bar')].map(rect);
					return [...document.querySelectorAll('button.faint')].map((el) => {
						const b = rect(el);
						return {
							name: el.getAttribute('aria-label'),
							width: b.width,
							height: b.height,
							clear: others.every((o) => !overlaps(b, o)),
							// Centred on its side of the root's box.
							centred: /^(top|bottom)/.test(el.getAttribute('aria-label'))
								? Math.abs((b.left + b.right) / 2 - (root.left + root.right) / 2) <= 1
								: Math.abs((b.top + b.bottom) / 2 - (root.top + root.bottom) / 2) <= 1
						};
					});
				});
				assert.deepEqual(
					seen.map((h) => h.name).sort(),
					['bottom', 'left', 'right', 'top'].map((e) => `${e} edge of the dashboard`)
				);
				for (const h of seen) {
					// The margin is 12px deep, so the target is all of it by 32px along the side.
					assert.ok(Math.max(h.width, h.height) >= 24 && Math.min(h.width, h.height) >= 12, `${h.name} is ${h.width}x${h.height}`);
					assert.ok(h.clear, `${h.name} lies over a pane or the prompt bar`);
					assert.ok(h.centred, `${h.name} is not centred on its side`);
				}
			}, { viewport });
		});

		test(`bottom adds a pane across the whole bottom, which takes a widget and survives a reload, at ${size}`, async () => {
			await withApp(async (app) => {
				const { page } = app;
				await extend(page, 'bottom');
				const { split, fresh } = await boxes(page);
				assert.ok(fresh, 'no pane is empty');
				assert.ok(near(fresh.left, split.left) && near(fresh.right, split.right), 'the new pane does not span the width');
				assert.ok(near(fresh.bottom, split.bottom), 'the new pane is not at the bottom');
				const content = await draft(page);
				assert.equal(content.direction, 'column');
				assert.equal(content.ratios.at(-1), 0.2);
				const added = content.children.at(-1).id;

				await fill(page, 'stats');
				const stats = page.locator(`section[data-pane="${added}"] .stats`);
				await stats.waitFor();
				await app.reload();
				await stats.waitFor();
				const after = await draft(page);
				assert.equal(after.children.at(-1).id, added);
				assert.equal(after.children.at(-1).tabs[0].content.kind, 'stats');
			}, { viewport });
		});

		test(`left adds a pane down the whole left, the old content intact to its right, at ${size}`, async () => {
			await withApp(async ({ page }) => {
				const before = await page.evaluate(() =>
					[...document.querySelectorAll('section.pane')].map((el) => el.dataset.pane)
				);
				await extend(page, 'left');
				const { split, fresh } = await boxes(page);
				assert.ok(fresh, 'no pane is empty');
				assert.ok(near(fresh.left, split.left), 'the new pane is not at the left');
				assert.ok(near(fresh.top, split.top) && near(fresh.bottom, split.bottom), 'the new pane does not span the height');
				const content = await draft(page);
				assert.equal(content.direction, 'row');
				assert.deepEqual(content.ratios, [0.2, 0.8]);
				assert.equal(content.children[1].tabs[0].content.direction, 'column', 'the old column is not the holder');
				const right = await page.evaluate(
					(ids) => ids.map((id) => document.querySelector(`section[data-pane="${id}"]`)?.getBoundingClientRect().left ?? null),
					before
				);
				for (const [i, left] of right.entries()) {
					assert.ok(left !== null, `pane ${before[i]} is gone`);
					assert.ok(left >= fresh.right, `pane ${before[i]} is not to the right of the new one`);
				}
			}, { viewport });
		});
	}

	test('under the lock, none is in the DOM', async () => {
		await withApp(
			async ({ page }) => {
				await page.locator('section.root').waitFor();
				assert.equal(await page.locator('button.faint').count(), 0);
			},
			{ prepare: (context) => context.addInitScript(() => localStorage.setItem('datum.layout.locked.v1.day', '1')) }
		);
	});
});
