// A swipe up on the output section is snappy, and the commands move with it
// from the first frame (da-6lqa6). vividn, 2026-10-29: «It should swipe up
// and animate much faster, like really snappy. The animations of the entries
// should start immediately with the swipe too. even if you only pull up a
// little bit, the most recent entry should expand to fill the bottom and the
// preceding entries should go up and hide behind the app to be revealed as
// you pull up more. With a swipe this should happen all in 200-250 ms or so».
//
// What broke it: 0b9d872b9 (da-pnxug) took the handle away, whose tap went to
// the full screen in a 220ms settle, and left the swipe, whose glide lasted
// `3 * distance / speed` up to 960ms, the commands flowing to their lines only
// once it had arrived. And a6e3aa8d4 (da-3eczg) made the strip's pull to clear
// read every touch, so a swipe drifting left dragged the strip with it.
//
// Real touches through CDP, a frame apart (`support/pans.mjs`), on a phone;
// each frame read in the page by the boxes drawn. The probe that bisected it
// on a phone at CPU 4× is `support/measure-swipe.mjs`.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { withBar } from './support/layout.mjs';
import { liftFinger, touchDrag } from './support/pans.mjs';

before(startApp);
after(stopApp);

const PHONE = { viewport: { width: 390, height: 844 }, device: { isMobile: true, hasTouch: true }, prepare: withBar };

/** Four commands in the strip, the bar at the row, and where a finger lands on the strip. */
async function atTheRow(app) {
	for (const line of ['occur coffee', 'occur tea', 'switch mood happy', 'occur water']) await app.submit(line);
	await app.idle();
	const box = await app.page.locator('.prompt-bar .roll').boundingBox();
	return { x: Math.round(box.x + box.width / 2), y: Math.round(box.y + box.height / 2) };
}

/**
 * Each frame while `act` runs: how far the bar is open, the newest command's
 * box and the one's before it, wherever each is drawn, the log's box, and
 * when the page heard the finger lift.
 */
async function framesOf(page, act) {
	await page.evaluate(() => {
		const seen = (window.__swipe = { frames: [], lifted: null });
		const shell = document.querySelector('.shell');
		const px = (name) => parseFloat(getComputedStyle(shell).getPropertyValue(name)) || 0;
		seen.onUp = () => (seen.lifted = performance.now());
		addEventListener('pointerup', seen.onUp, { capture: true, passive: true });
		const box = (el) => {
			if (!el) return null;
			const { top, bottom, left, width } = el.getBoundingClientRect();
			return { top, bottom, left, width };
		};
		const frame = (t) => {
			if (window.__swipe !== seen) return;
			const ran = [...document.querySelectorAll('.log [data-ran], .prompt-bar .roll [data-ran]')].filter(
				(el) => el.getBoundingClientRect().height > 0
			);
			const by = (stamp) => ran.filter((el) => el.dataset.ran === stamp).at(-1);
			const stamps = [...new Set(ran.map((el) => el.dataset.ran))].sort();
			seen.frames.push({
				t,
				shown: px('--shown'),
				log: px('--log-height'),
				strip: !!document.querySelector('.prompt-bar .roll'),
				logBox: box(document.querySelector('.log')),
				newest: box(by(stamps.at(-1))),
				before: box(by(stamps.at(-2)))
			});
			requestAnimationFrame(frame);
		};
		requestAnimationFrame(frame);
	});
	await act();
	await page.waitForTimeout(600);
	return page.evaluate(() => {
		const seen = window.__swipe;
		removeEventListener('pointerup', seen.onUp, { capture: true });
		window.__swipe = null;
		return { frames: seen.frames, lifted: seen.lifted };
	});
}

test('a short pull: from the first frames the newest grows to the whole width at the bottom, and the one before it rises', async () => {
	await withApp(async (app) => {
		const { page } = app;
		const at = await atTheRow(app);
		const cdp = await page.context().newCDPSession(page);
		const { frames } = await framesOf(page, () => touchDrag(cdp, at, { x: 0, y: -10 }, 6, { lift: false }));
		await liftFinger(cdp);

		const rest = frames[0];
		assert.ok(rest.strip && rest.shown > 0 && rest.log === 0, `not at the row before the pull: ${JSON.stringify(rest)}`);
		const moved = frames.findIndex((f) => f.shown !== rest.shown || f.log !== rest.log);
		assert.ok(moved > 0, 'the bar never moved under the finger');
		const first = frames.slice(moved, moved + 3);
		const end = frames.at(-1);

		for (const f of first) {
			assert.equal(f.strip, false, `the strip is still drawn ${Math.round(f.t - frames[moved].t)}ms into the pull`);
			assert.ok(
				f.newest.width >= 0.9 * f.logBox.width,
				`the newest is ${Math.round(f.newest.width)}px wide of the log's ${Math.round(f.logBox.width)}, ${Math.round(f.t - frames[moved].t)}ms into the pull`
			);
		}
		// It rises from where it stood in the strip, never under it, and is on its
		// way up within 120ms of the first move: by time and not by frames, since a
		// loaded runner draws few of them.
		const early = frames.slice(moved).filter((f, i, all) => i === 0 || all[i - 1].t - frames[moved].t < 120);
		const feet = early.map((f) => `${Math.round(f.t - frames[moved].t)}ms:${Math.round(f.before.bottom)}`).join(' ');
		assert.ok(
			frames.slice(moved).every((f) => f.before.bottom <= rest.before.bottom + 1),
			`the one before the newest sank under its place in the strip (${Math.round(rest.before.bottom)}): ${feet}`
		);
		assert.ok(
			early.some((f) => f.before.bottom < rest.before.bottom - 1),
			`the one before the newest did not rise within 120ms: its foot at ${feet}, ${Math.round(rest.before.bottom)} in the strip`
		);
		// And held, they stand one over the other, the newest last, on the log's foot.
		assert.ok(end.before.bottom <= end.newest.top + 1, `held, the one before is not over the newest: ${JSON.stringify(end)}`);
		assert.ok(end.newest.bottom <= end.logBox.bottom + 0.5 && end.newest.bottom > end.logBox.bottom - 60, `held, the newest is not at the log's foot: ${JSON.stringify(end)}`);
		assert.deepEqual(app.errors, []);
	}, PHONE);
});

test('a fling: the bar is at the full screen within 250ms of the lift', async () => {
	await withApp(async (app) => {
		const { page } = app;
		const at = await atTheRow(app);
		const cdp = await page.context().newCDPSession(page);
		const { frames, lifted } = await framesOf(page, () => touchDrag(cdp, at, { x: 0, y: -33 }, 6));

		assert.ok(lifted, 'the page heard no lift');
		const end = frames.at(-1);
		assert.ok(end.shown === 0 && end.log > 600, `the fling did not end full: ${JSON.stringify(end)}`);
		// When the glide is SCHEDULED to arrive, and not the frame a loaded runner
		// drew it in, which came 261ms after the lift once under the full suite
		// (da-rubmm): the 250ms is the app's motion to keep, not the machine's.
		const ends = Number(await page.locator('.shell').getAttribute('data-glide-ends'));
		assert.ok(ends >= lifted, `no glide was scheduled by the lift: it ends at ${ends}, the lift at ${lifted}`);
		assert.ok(ends - lifted <= 250, `the bar is scheduled to come to rest ${Math.round(ends - lifted)}ms after the lift`);
		assert.deepEqual(app.errors, []);
	}, PHONE);
});

test('a swipe down drifting left moves the bar, and never pulls the strip off the ×', async () => {
	await withApp(async (app) => {
		const { page } = app;
		const at = await atTheRow(app);
		const shown = () => page.evaluate(() => parseFloat(getComputedStyle(document.querySelector('.shell')).getPropertyValue('--shown')));
		const row = await shown();
		const cdp = await page.context().newCDPSession(page);
		// Every frame of the drag: the strip's sideways offset, and the ×'s opacity.
		await page.evaluate(() => {
			const seen = (window.__drift = { slid: 0, opacity: 0 });
			const frame = () => {
				if (window.__drift !== seen) return;
				const roll = document.querySelector('.prompt-bar .roll');
				const clear = document.querySelector('.prompt-bar .clear-strip');
				if (roll) seen.slid = Math.max(seen.slid, Math.abs(new DOMMatrix(getComputedStyle(roll).transform).m41));
				if (clear) seen.opacity = Math.max(seen.opacity, Number(getComputedStyle(clear).opacity));
				requestAnimationFrame(frame);
			};
			requestAnimationFrame(frame);
		});
		await touchDrag(cdp, at, { x: -4, y: 6 }, 10, { lift: false });
		await page.waitForTimeout(100);
		const held = await shown();
		const seen = await page.evaluate(() => {
			const { slid, opacity } = window.__drift;
			window.__drift = null;
			return { slid, opacity };
		});
		await liftFinger(cdp);
		assert.ok(held < row - 20, `the swipe down did not move the bar: ${held}px of ${row}`);
		assert.equal(seen.slid, 0, `the strip slid ${seen.slid}px sideways under a swipe down`);
		assert.equal(seen.opacity, 0, 'the × came out under a swipe down');
		assert.deepEqual(app.errors, []);
	}, PHONE);
});
