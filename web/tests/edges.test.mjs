// The dashboard extends from its edges (da-qk2k). It used to be a faint
// `…` in the page's margin on each side, whose one item, Add widget,
// added a pane spanning that whole side. The handles were not
// discoverable (vividn: "We can remove the the 4 ... things on the edges of
// the screen"), so da-wjob took them away and the act moved into the root
// menu, where it is `Split` with the same four arrows every other menu
// row has — the root pane being the shell, splitting beside it is a new
// pane down one whole side. (It wore `Break out` until da-a642: a tab
// breaks out of its STRIP, and the root bar is the shell, not a strip.) The
// BEHAVIOUR is what this file tests, so it stayed and only its entry
// point moved.
//
// Boxes and tree shapes, never "one more pane than before" alone: a
// delta-blind probe passes against a page that grew the wrong pane.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, withApp } from './support/app.mjs';
import {
	fill,
	lockButton,
	manageRoot,
	menuLines,
	rootManageButton,
	rootMenuButton
} from './support/layout.mjs';

before(startApp);
after(stopApp);

const SCREENS = [
	{ width: 1280, height: 800 },
	{ width: 390, height: 844 }
];

// ONE APP for the tests at the desktop screen that stay on the page
// (da-6b3dj, sharedApp in support/app.mjs): `solo` puts the split `day`
// back. A reload, the phone's screen and a device that stored a lock open
// their own.
const desk = sharedApp({ viewport: SCREENS[0] });

/** Which arrow of `Split` puts a pane down each side. */
const WHERE = { top: 'above', bottom: 'below', left: 'to the left', right: 'to the right' };

/** Split toward `edge` from the root menu. */
const extend = (page, edge) => manageRoot(page, `split: a new pane ${WHERE[edge]}`);

/** The root split's box, and the box of the empty pane the split just added. */
const boxes = (page) =>
	page.evaluate(() => {
		const rect = (el) => {
			const { left, right, top, bottom } = el.getBoundingClientRect();
			return { left, right, top, bottom };
		};
		const empty = document.querySelector('section.pane[data-title="empty"]');
		return {
			split: rect(document.querySelector('section.root > .content > .tab-body > .split')),
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

describe('extending the dashboard from its edges', () => {
	test('the four margin handles are gone; the root menu carries the act instead', async () => {
		await desk.solo(async ({ page }) => {
			await page.locator('section.root').waitFor();
			assert.equal(await page.locator('button.faint').count(), 0, 'a margin handle is still drawn');
			assert.equal(
				await page.getByRole('button', { name: /edge of the dashboard/ }).count(),
				0,
				'an edge handle is still drawn'
			);
			// The act is under the `▾` beside the root's `+` (da-bzhq).
			await rootManageButton(page).click();
			const lines = await menuLines(page);
			assert.ok(lines.includes('Split ← ↓ ↑ →'), 'the root ▾ has no Split row');
			assert.ok(!lines.includes('Break out ← ↓ ↑ →'), 'the root ▾ still offers Break out');
			await page.keyboard.press('Escape');
		});
	});

	for (const viewport of SCREENS) {
		const size = `${viewport.width}x${viewport.height}`;

		test(`↓ adds a pane across the whole bottom, which takes a widget and survives a reload, at ${size}`, async () => {
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

		test(`← adds a pane down the whole left, the old content intact to its right, at ${size}`, async () => {
			const open = viewport === SCREENS[0] ? desk.solo : (fn) => withApp(fn, { viewport });
			await open(async ({ page }) => {
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
			});
		});
	}

	test('under the lock there is no ▾ at all, and the root menu is the acts alone', async () => {
		await withApp(
			async ({ page }) => {
				await page.locator('section.root').waitFor();
				// Nothing to split, so the `▾` that would hold Split is not
				// drawn (da-bzhq); the way out is the lock beside the gear.
				assert.equal(await rootManageButton(page).count(), 0, 'a locked layout still offers the ▾');
				assert.equal(await lockButton(page).getAttribute('aria-pressed'), 'true');
				await rootMenuButton(page).click();
				assert.deepEqual(await menuLines(page), [
					'Save layout',
					'Restore layout',
					// `day` ships with datum, so it can be reset (da-luwe).
					'Reset layout',
					'Save layout as…'
				]);
				await page.keyboard.press('Escape');
			},
			{ prepare: (context) => context.addInitScript(() => localStorage.setItem('datum.layout.locked.v1.day', '1')) }
		);
	});
});
