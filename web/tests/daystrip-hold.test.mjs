// A day height gesture holds one day still and pushes the others (da-cbqrq).
//
// vividn, 2026-10-30: «when changing the day height, days start flying around
// everywhere. fix the day whose height is being adjusted in place, so that the
// height change visibly changes its height and then dynamically push the days
// above and below it away».
//
// Read frame by frame, every `.band`'s box in a requestAnimationFrame from the
// press until the release's pictures have landed. Which point is held: a line
// holds the BOTTOM of the day under it, so the line stays under the pointer
// (vividn, 2026-10-30: «the line under your finger should move with your finger,
// fix the other side of the day in place»), and the divider where it cuts a day.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp } from './support/app.mjs';
import {
	settledRoom,
	say,
	settledAt,
	sizeFor,
	waitForHeights,
	frozen,
	toggleFreeze,
	pinch,
	PHONE
} from './support/daystrip.mjs';

before(startApp);
after(stopApp);

const shared = sharedApp();
const phone = sharedApp(PHONE);

const band = (back) => `.band[data-days-ago="${back}"]`;
const lineOver = (back) => `.stack .row:has(> ${band(back)}) > .day-line`;

/** Every band's top and bottom, and the day height, each frame `act` takes and until its pictures land. */
async function framesOf(app, height, act) {
	await app.page.evaluate(() => {
		const frames = (window.__frames = []);
		const read = () => {
			const days = {};
			for (const b of document.querySelectorAll('.band')) {
				const r = b.getBoundingClientRect();
				days[b.dataset.daysAgo] = { top: r.top, bottom: r.bottom, mid: (r.top + r.bottom) / 2 };
			}
			frames.push({ days, height: days[0].bottom - days[0].top });
			window.__reading = requestAnimationFrame(read);
		};
		read();
	});
	await act();
	await waitForHeights(app, height);
	await app.idle();
	const frames = await app.page.evaluate(() => (cancelAnimationFrame(window.__reading), window.__frames));
	assert.ok(new Set(frames.map((f) => f.height)).size >= 4, `the height took ${frames.length} frames and hardly moved`);
	assert.equal(frames.at(-1).height, height);
	return frames;
}

/** Drag a line by `dy`, a frame at a time. */
async function drag({ page }, selector, dy) {
	const box = await page.locator(selector).boundingBox();
	const [x, y] = [box.x + box.width / 2, box.y + box.height / 2];
	await page.mouse.move(x, y);
	await page.mouse.down();
	await page.mouse.move(x, y + dy, { steps: 10 });
	await page.mouse.up();
}

/** `read` of the frames, its range no more than a pixel. */
function still(frames, read, what) {
	const at = frames.map(read);
	const range = Math.max(...at) - Math.min(...at);
	assert.ok(range <= 1, `${what} moved ${range}px: ${at.join(' ')}`);
}

/**
 * Each day `back` is held from `from` by a distance that goes with the height:
 * `steps` pitches of it, frame on frame, so nothing jumps or runs back.
 */
function pushed(frames, from, back, steps, what) {
	for (const [i, f] of frames.entries()) {
		if (!i) continue;
		const gap = (x) => Math.abs(x.days[back].mid - x.days[from].mid);
		const moved = gap(f) - gap(frames[i - 1]);
		const grew = f.height - frames[i - 1].height;
		assert.ok(
			Math.abs(moved - steps * grew) <= 1.5,
			`${what}: frame ${i} grew the day ${grew}px and moved day ${back} ${moved}px from day ${from}`
		);
	}
}

/** A strip at `height`, today in the scroller or frozen, with the newest day at the bottom. */
async function strip(app, height, freeze) {
	await say(app, { dayHeight: height });
	await settledAt(app, height);
	await sizeFor(app, 5);
	if ((await frozen(app)) !== freeze) await toggleFreeze(app);
	await waitForHeights(app, height);
	await app.idle();
}

for (const [how, dy] of [
	['grows', -40],
	['shrinks', 20]
])
	test(`a day ${how} from its line, its far edge still and the days either side pushed`, async () => {
		await settledRoom(shared, `far edge ${how}`, async (app) => {
			await strip(app, 60, false);
			await app.page.locator('.stack').evaluate((el) => (el.scrollTop = el.scrollHeight - el.clientHeight - 150));
			await app.idle();
			const frames = await framesOf(app, 60 - dy, () => drag(app, lineOver(3), dy));
			still(frames, (f) => f.days[3].bottom, 'the far edge of the day under the line');
			still(frames, (f) => f.days[3].top + f.height, 'the line, against the pointer');
			still(frames, (f) => f.days[2].top, 'the top of the day below');
			pushed(frames, 3, 4, 1, 'the day above');
			pushed(frames, 3, 2, 1, 'the day below');
			assert.deepEqual(app.errors, []);
		});
	});

test('a newest day against the divider keeps it, and its line follows the pointer', async () => {
	await settledRoom(shared, 'newest', async (app) => {
		await strip(app, 60, false);
		const frames = await framesOf(app, 100, () => drag(app, lineOver(0), -40));
		still(frames, (f) => f.days[0].bottom, "today's bottom");
		pushed(frames, 0, 1, 1, 'yesterday');
		assert.equal(await app.page.locator('button.jagged').count(), 0, 'the newest day left the divider');
	});
});

test('a frozen today stays frozen: yesterday keeps to its top, the past pushed up', async () => {
	await settledRoom(shared, 'frozen', async (app) => {
		await strip(app, 60, true);
		const frames = await framesOf(app, 100, () => drag(app, '.today-line', -40));
		still(frames, (f) => f.days[0].bottom, "the frozen today's bottom");
		still(frames, (f) => f.days[0].top - f.days[1].bottom, 'the seam under yesterday');
		pushed(frames, 1, 2, 1, 'the day before');
	});
});

test('scrolled up, the divider holds the day it cuts, wherever it cuts it', async () => {
	await settledRoom(shared, 'scrolled', async (app) => {
		await strip(app, 60, false);
		await app.page.locator('.stack').evaluate((el) => (el.scrollTop = el.scrollHeight - el.clientHeight - 170));
		await app.idle();
		const edge = await app.page.locator('.stack').evaluate((el) => el.getBoundingClientRect().bottom);
		const cut = await app.page.evaluate((edge) => {
			const r = [...document.querySelectorAll('.band')].find((b) => b.getBoundingClientRect().bottom >= edge);
			const box = r.getBoundingClientRect();
			return { back: r.dataset.daysAgo, down: (edge - box.top) / box.height };
		}, edge);
		const frames = await framesOf(app, 100, () => drag(app, '.today-line', -40));
		still(frames, (f) => f.days[cut.back].top + cut.down * (f.days[cut.back].bottom - f.days[cut.back].top), 'the point at the divider');
	});
});

// The ends of the scroll: nothing scrolls past them, so days shrinking there
// keep the oldest's TOP on the start, or the newest's BOTTOM on the end, and
// the far edge held gives way to that, the days still drawn up evenly.
test('at the top of the scroll the days shrink from the start of it', async () => {
	await settledRoom(shared, 'top', async (app) => {
		await strip(app, 60, false);
		await app.page.locator('.stack').evaluate((el) => (el.scrollTop = 0));
		await app.idle();
		const oldest = await app.page.evaluate(() => Math.max(...[...document.querySelectorAll('.band')].map((b) => Number(b.dataset.daysAgo))));
		const frames = await framesOf(app, 40, () => drag(app, lineOver(oldest - 1), 20));
		still(frames, (f) => f.days[oldest].top, 'the top of the oldest day');
		pushed(frames, oldest - 1, oldest - 2, 1, 'the day below the line');
	});
});

test('at the bottom of the scroll the days shrink from the end of it, and the line goes faster than the pointer', async () => {
	await settledRoom(shared, 'bottom', async (app) => {
		await strip(app, 60, false);
		const frames = await framesOf(app, 40, () => drag(app, lineOver(1), 20));
		still(frames, (f) => f.days[0].bottom, "today's bottom");
		pushed(frames, 1, 2, 1, 'the day above the line');
		// The pointer went 20px: the day under the line and today each gave up 20.
		const line = frames.at(-1).days[1].top - frames[0].days[1].top;
		assert.ok(Math.abs(line - 40) <= 1, `the line went ${line}px`);
		assert.equal(await app.page.locator('button.jagged').count(), 0, 'the newest day left the divider');
	});
});

test('a line over yesterday on a frozen today holds yesterday, the seam rising over it', async () => {
	await settledRoom(shared, 'frozen line', async (app) => {
		await strip(app, 60, true);
		const frames = await framesOf(app, 100, () => drag(app, lineOver(1), -40));
		still(frames, (f) => f.days[1].bottom, "yesterday's far edge");
		still(frames, (f) => f.days[0].bottom, "the frozen today's bottom");
		pushed(frames, 1, 2, 1, 'the day above the line');
	});
});

test('a pinch on a frozen today holds it, the past pushed up from its top', async () => {
	await settledRoom(phone, 'pinch', async (app) => {
		await strip(app, 60, true);
		const box = await app.page.locator(band(0)).boundingBox();
		const frames = await framesOf(app, 120, () => pinch(app.page, box, 25, 50, null, 'y'));
		still(frames, (f) => f.days[0].bottom, "the frozen today's bottom");
		still(frames, (f) => f.days[0].top - f.days[1].bottom, 'the seam under yesterday');
	});
});

test('the keyboard holds what the divider does', async () => {
	await settledRoom(shared, 'keys', async (app) => {
		await strip(app, 60, false);
		await app.page.locator('.today-line').focus();
		const frames = await framesOf(app, 80, async () => {
			for (let i = 0; i < 4; i++) {
				await app.page.keyboard.press('ArrowUp');
				await app.page.evaluate(() => new Promise(requestAnimationFrame));
			}
		});
		still(frames, (f) => f.days[0].bottom, "today's bottom");
	});
});
