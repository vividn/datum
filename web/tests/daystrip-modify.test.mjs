// The dayview in modify mode (da-1tmd1), modifying layouts 5 of 5. vividn,
// 2026-11-10: «in dayview optinos could be to change the day height and the
// day label width, then we disable this functinality when not in modify
// mode», and «yes exactly» to drag handles that appear only there.
//
// So the day lines (where they cross the dates) and the spine are handles in
// modify mode alone, what they move is written into the layout (`modifies`,
// registry.ts), ✓ keeps it and ↺ takes it back; out of the mode a line is a
// line. The rest of the strip is still in the mode, so no hour is written.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp, todayAt } from './support/app.mjs';
import { finger } from './support/press.mjs';
import { DESKTOP, PHONE, leaveWidget, modifyWidget, pane, rootMenuButton } from './support/layout.mjs';
import { pillsOf } from './support/nowaxes.mjs';
import { until } from './support/until.mjs';
import { following, heard, modifyingStrip, settledAt, stripPane, toggleFreeze, valueOf, widthOf } from './support/daystrip.mjs';

before(startApp);
after(stopApp);

const BAND = '.band[data-days-ago="0"] svg.svg';
const TALL = { ...DESKTOP, viewport: { width: 1280, height: 1400 }, clock: todayAt(15) };

/** What the LAYOUT holds for the strip in pane `id`: the tree's settings, no session over them. */
const inTree = (page, id) =>
	page.evaluate((id) => {
		const find = (p) => {
			if (p.id === id) return p.tabs[p.active].content.settings;
			for (const t of p.tabs) if (!('kind' in t.content)) for (const c of t.content.children) { const got = find(c); if (got) return got; }
			return null;
		};
		return find(window.datum.layout.current().root);
	}, id);
/** What this session holds over the strip (the only widget a test here moves). */
const heldOver = (page) =>
	page.evaluate(() => Object.values(window.datum.layout.session()).find((s) => 'freezeToday' in s || 'dayHeight' in s || 'labelWidth' in s) ?? {});

/** A drag of `selector`'s middle by (dx, dy): the mouse, or with `touch` a finger. */
async function pull(page, selector, dx, dy, touch) {
	const box = await page.locator(selector).boundingBox();
	const from = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
	const to = { x: from.x + dx, y: from.y + dy };
	if (touch) return finger(page, from, to);
	await page.mouse.move(from.x, from.y);
	await page.mouse.down();
	await page.mouse.move(to.x, to.y, { steps: 8 });
	await page.mouse.up();
}

async function opened(app) {
	await app.page.locator(BAND).waitFor();
	await app.idle();
	return stripPane(app);
}

for (const [name, device, touch] of [
	['desktop', { ...DESKTOP, clock: todayAt(15) }, false],
	['phone', { ...PHONE, clock: todayAt(15) }, true]
]) {
	describe(`${name}: the dayview's sizes`, () => {
		test('out of modify mode neither line moves anything, by drag or key, and neither is a handle', async () => {
			await withApp(async (app) => {
				const { page } = app;
				const id = await opened(app);
				const [tall, wide] = [await valueOf(app), await widthOf(app)];
				await pull(page, '.today-line', 0, -30, touch);
				await pull(page, '.strip .spine', 30, 0, touch);
				await page.locator('.today-line').focus();
				await page.keyboard.press('ArrowUp');
				await page.locator('.strip .spine').focus();
				await page.keyboard.press('ArrowRight');
				await app.idle();
				assert.deepEqual([await valueOf(app), await widthOf(app)], [tall, wide], 'a line moved a size out of modify mode');
				assert.deepEqual(await heldOver(page), {}, 'a line wrote into the session');
				assert.equal((await inTree(page, id)).dayHeight, undefined, 'a line wrote into the layout');
				for (const line of ['.today-line', '.strip .spine'])
					assert.equal(await page.locator(line).getAttribute('aria-disabled'), 'true', `${line} is a live handle`);
			}, device);
		});

		test('in modify mode the day line and the spine move their sizes into the layout; ✓ keeps them over a reload, ↺ takes the next back', async () => {
			await withApp(async (app) => {
				const { page } = app;
				const id = await opened(app);
				const [tall, wide] = [await valueOf(app), await widthOf(app)];
				await modifyingStrip(app, async () => {
					assert.equal(await page.locator('.today-line').getAttribute('aria-disabled'), 'false');
					await pull(page, '.today-line', 0, -20, touch);
					await until(() => valueOf(app), (v) => v === tall + 20);
					await pull(page, '.strip .spine', 25, 0, touch);
					await until(() => widthOf(app), (w) => w === wide + 25);
					const tree = await inTree(page, id);
					assert.deepEqual([tree.dayHeight, tree.labelWidth], [tall + 20, wide + 25], 'the layout lacks the sizes');
					assert.deepEqual(await heldOver(page), {}, 'a size went to the session');
				});
				await app.reload();
				await page.locator(BAND).waitFor();
				await settledAt(app, tall + 20);
				assert.equal(await widthOf(app), wide + 25, 'a reload lost the label width ✓ kept');

				await modifyWidget(page, id);
				await pull(page, '.today-line', 0, 15, touch);
				await until(() => valueOf(app), (v) => v === tall + 5);
				await page.locator('.strip .spine').focus();
				await page.keyboard.press('ArrowLeft');
				await until(() => widthOf(app), (w) => w === wide + 20);
				await leaveWidget(page, id, 'reset');
				await settledAt(app, tall + 20);
				assert.equal(await widthOf(app), wide + 25, '↺ did not take the label width back');
				assert.deepEqual([(await inTree(page, id)).dayHeight, (await inTree(page, id)).labelWidth], [tall + 20, wide + 25]);
				assert.deepEqual(app.errors, []);
			}, device);
		});
	});
}

describe('the dayview in modify mode', () => {
	// Collect's handle taught it (da-rw31h): Chrome turns a drag let go at speed
	// into a fling unless its touch moves are consumed, and a tap within ~150 ms
	// only cancels the fling. The drag's touches carry their own times, so every
	// box drags that fast.
	test('phone: ✓ tapped as a fast drag of the day line lets go still keeps the day height', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const id = await opened(app);
			const tall = await valueOf(app);
			await modifyWidget(page, id);
			const line = await page.locator('.today-line').boundingBox();
			const [x, y] = [line.x + line.width / 2, line.y + line.height / 2];
			const cdp = await page.context().newCDPSession(page);
			const t0 = Date.now() / 1000 - 0.2;
			for (const [i, type] of ['touchStart', 'touchMove', 'touchMove', 'touchMove', 'touchMove', 'touchMove', 'touchEnd'].entries())
				await cdp.send('Input.dispatchTouchEvent', { type, touchPoints: type === 'touchEnd' ? [] : [{ id: 4, x, y: y - 6 * Math.min(i, 5) }], timestamp: t0 + 0.03 * i });
			await cdp.detach();
			await pane(page, id).locator('> .titlebar .way-out').last().tap({ force: true });
			await page.locator('.frame[data-layout-mode]').waitFor({ state: 'detached' });
			assert.equal((await inTree(page, id)).dayHeight, tall + 30, '✓ did not keep the day height');
		}, { ...PHONE, clock: todayAt(15) });
	});

	// vremya, 2026-11-10: the session lays its settings over the layout's, so a
	// day height an older normal-mode drag left there would mask one modify
	// mode writes into the layout. The mode's change drops it, and ↺ holds it again.
	test('a day height the session holds is dropped by a line in modify mode and held again by ↺; ✓ and the tab’s Save write the layout entry', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await opened(app);
			// A key of the strip's in the session, then a day height planted in it, as a drag before this bead left one.
			await toggleFreeze(app);
			await until(() => heldOver(page), (s) => 'freezeToday' in s);
			await page.evaluate(() => {
				const all = JSON.parse(localStorage.getItem('datum.layout.session.v1'));
				for (const s of Object.values(all)) if ('freezeToday' in s) s.dayHeight = 90;
				localStorage.setItem('datum.layout.session.v1', JSON.stringify(all));
			});
			await app.reload();
			const id = await opened(app);
			await settledAt(app, 90);

			await modifyingStrip(app, async () => {
				await pull(page, '.today-line', 0, -20, false);
				await until(() => valueOf(app), (v) => v === 110);
				assert.equal((await heldOver(page)).dayHeight, undefined, 'the session still holds the day height');
				assert.equal((await inTree(page, id)).dayHeight, 110);
			}, 'reset');
			await settledAt(app, 90);
			assert.equal((await heldOver(page)).dayHeight, 90, '↺ did not hold the day height again');
			assert.ok('freezeToday' in (await heldOver(page)), '↺ lost the freeze');

			await modifyingStrip(app, async () => {
				await pull(page, '.today-line', 0, -30, false);
				await until(() => valueOf(app), (v) => v === 120);
			});
			await rootMenuButton(page).click();
			await page.getByRole('menuitem', { name: 'Save', exact: true }).click();
			const saved = await until(
				() =>
					page.evaluate(async () => {
						const mine = (await window.datum.layouts.listLayouts()).filter((l) => l.source === 'user');
						return Promise.all(mine.map((l) => window.datum.layouts.loadLayout(l.name).then((t) => JSON.stringify(t))));
					}),
				(all) => all.length > 0
			);
			assert.ok(saved.some((l) => l.includes('"dayHeight":120')), `the layout entry lacks the day height: ${saved}`);
			assert.deepEqual(app.errors, []);
		}, { ...DESKTOP, clock: todayAt(15) });
	});

	// vremya's walk of #1937: the strip is not made inert by its tab in the
	// mode, so it keeps itself still, or a pan would write the hours, which
	// pass (`passing`), into the layout.
	test('the strip is still but for its lines: a ctrl+wheel, a shift+wheel and a tap move no hour and select nothing', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const id = await opened(app);
			await modifyingStrip(app, async () => {
				const band = await page.locator('.band[data-days-ago="0"]').boundingBox();
				await page.mouse.move(band.x + band.width / 2, band.y + band.height / 2);
				await page.keyboard.down('Control');
				await page.mouse.wheel(0, -300);
				await page.keyboard.up('Control');
				await page.keyboard.down('Shift');
				await page.mouse.wheel(0, 200);
				await page.keyboard.up('Shift');
				await page.mouse.click(band.x + band.width / 2, band.y + band.height / 2);
				await app.idle();
				const tree = await inTree(page, id);
				assert.deepEqual([tree.hourFrom, tree.hourTo], [0, 24], 'the hours moved in the layout');
				assert.deepEqual([(await heard(app, 'hourFrom')), (await heard(app, 'hourTo'))], [0, 24], 'the hours moved');
				assert.equal(await page.locator('.strip.zoomed').count(), 0, 'the strip zoomed');
				assert.deepEqual(await pillsOf(page), [], 'a tap selected something');
			});
		}, { ...DESKTOP, clock: todayAt(15) });
	});

	// vremya's walk: following is an absence, and through modify mode the
	// layout ends without the key, so the day follows its box again.
	test("a line dragged back onto the box's height in modify mode leaves no day height in the layout, and the day follows its box over a reload", async () => {
		await withApp(async (app) => {
			const { page } = app;
			const id = await opened(app);
			const free = await following(app);
			await modifyingStrip(app, async () => {
				await pull(page, '.today-line', 0, -30, false);
				await until(() => valueOf(app), (v) => v === free + 30);
				assert.equal((await inTree(page, id)).dayHeight, free + 30);
				await pull(page, '.today-line', 0, 30, false);
				await until(() => valueOf(app), (v) => v === free);
				assert.ok(!('dayHeight' in (await inTree(page, id))), 'the layout holds a day height');
			});
			await app.reload();
			await page.locator(BAND).waitFor();
			await settledAt(app, free);
			assert.ok(!('dayHeight' in (await inTree(page, id))), 'a reload holds a day height');
			assert.equal(await heard(app), undefined);
			assert.deepEqual(app.errors, []);
		}, TALL);
	});
});
