// The day strip's pictures outlive the strip (da-lt25u).
//
// vividn: «we should cache the dayview bands on the web (at least for the
// past month), right now if you have a dayview tab and swipe to and from
// it, every time you load it, it has to redraw everything which is
// jarring». A swipe to another layout unmounts the strip; the swipe back
// mounts a new one. So each case swipes away and back, and counts the
// `dayview` runs the return asks for: none when nothing changed, the days
// on screen after a write, and every day after a midnight.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp, seedLines, todayAt } from './support/app.mjs';
import { PHONE, swipeOver, atRest } from './support/layout.mjs';
import { band, colorOf, inView, labelFor, scrollToDay, stamps, viewsOf } from './support/daystrip.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const AT = todayAt(12);

/** This device's open layouts: `main`, with the strip, and `tail`, without one. */
const twoLayouts = (context, next = 'tail') =>
	context.addInitScript(
		(json) => localStorage.setItem('datum.layout.root.v1', json),
		JSON.stringify({ names: ['main', next], active: 0 })
	);

const shown = async (page) =>
	(await page.locator('.root-bar [role="tab"][aria-selected="true"]').innerText()).replace(/\s*•\s*$/, '');

/** Every band of the strip, once each has a picture: its SVG by days back. */
async function pictures({ page }) {
	await page.waitForFunction(() => {
		const bands = [...document.querySelectorAll('.band')];
		return bands.length > 1 && bands.every((b) => b.querySelector('svg.svg'));
	});
	return page.locator('.band').evaluateAll((bands) =>
		Object.fromEntries(bands.map((b) => [b.dataset.daysAgo, b.querySelector('svg.svg').outerHTML]))
	);
}

/** To the next layout and back to `main` by the root's swipe, with `between` done there. */
async function awayAndBack(app, between = async () => {}, to = 'tail') {
	const { page } = app;
	const root = page.locator('section.root > .content');
	await swipeOver(page, root, -150, 0);
	await until(() => shown(page), (name) => name === to);
	await atRest(root);
	if (to === 'tail') assert.equal(await page.locator('.band').count(), 0, 'the strip is still mounted on `tail`');
	await between();
	await swipeOver(page, root, 150, 0);
	await until(() => shown(page), (name) => name === 'main');
	await atRest(root);
}

/** A line typed into the bar on a layout with no REPL, whose scrollback `app.submit` waits on. */
async function barLine({ page }, line) {
	await page.evaluate(() => (window.__barLast = window.datum.bar.last()));
	await page.locator('.prompt-bar input').fill(line);
	await page.locator('.prompt-bar input').press('Enter');
	await page.waitForFunction(() => window.datum.bar.last() !== window.__barLast);
	assert.equal(await page.evaluate(() => window.datum.bar.last().result.code), 0, `\`${line}\` failed`);
}

/** Grow the stack to the first entry, `back` days ago, and draw every day of it. */
async function drawBackTo(app, back) {
	await app.page.waitForFunction(
		() => {
			const stack = document.querySelector('.stack');
			if (stack.querySelector('.first')) return true;
			stack.scrollTop = 0;
		},
		null,
		{ polling: 100 }
	);
	for (let day = back; day > 0; day--) {
		await scrollToDay(app, day);
		await app.page.locator(`.band[data-days-ago="${day}"] svg.svg`).waitFor();
	}
	await app.idle();
}

const open = (fn) => withApp(fn, { ...PHONE, clock: AT, prepare: twoLayouts });

describe('the day strip across a swipe', () => {
	test('back from another layout, the strip shows what it drew and asks for none of it again', async () => {
		await open(async (app) => {
			await seedLines(app, ['occur coffee -d -1', 'occur coffee -d -3', 'switch mood happy -t 10']);
			const before = await pictures(app);
			await app.idle();
			const seen = await app.spyOnRunner();

			await awayAndBack(app);
			const after = await pictures(app);
			await app.idle();

			assert.deepEqual(
				(await seen()).filter((argv) => argv[0] === 'dayview'),
				[],
				'the return ran dayview again'
			);
			assert.deepEqual(after, before, 'the return drew other pictures than the strip had');
		});
	});

	test('a write while away re-draws the days on screen and only them', async () => {
		await open(async (app) => {
			await seedLines(app, ['occur coffee -d -1', 'occur coffee -d -3']);
			const before = await pictures(app);
			await app.idle();
			const held = Object.keys(before).length;

			let seen;
			await awayAndBack(app, async () => {
				await barLine(app, 'switch mood happy -t -5');
				await app.idle();
				seen = await app.spyOnRunner();
			});
			const happy = colorOf('happy');
			await app.page.waitForFunction(
				(c) => document.querySelector('.band[data-days-ago="0"] svg.svg')?.outerHTML.includes(c),
				happy
			);
			await app.idle();

			const asked = await viewsOf(seen);
			const visible = await inView(app);
			assert.ok(asked.includes(0), 'today was not re-drawn');
			assert.ok(
				asked.every((end) => end === 0 || visible.includes(end)),
				`a day off screen was re-drawn: asked ${asked}, on screen ${visible}`
			);
			assert.ok(asked.length < held, `every band was re-drawn: ${asked.length} of ${held}`);
			const unasked = Object.keys(before).filter((end) => !asked.includes(Number(end)));
			for (const end of unasked)
				assert.equal(await band(app, end), before[end], `day ${end} lost the picture it had`);
		});
	});

	test('a midnight while away re-draws every day', async () => {
		await open(async (app) => {
			await seedLines(app, ['occur coffee -d -1', 'occur coffee -d -3']);
			const held = Object.keys(await pictures(app)).length;
			await app.idle();

			let seen;
			await awayAndBack(app, async () => {
				seen = await app.spyOnRunner();
				await app.clock.forward(13 * 3600_000);
			});
			const now = app.clock.at();
			await until(
				() => stamps(app),
				(rows) => JSON.stringify(rows.at(-1)) === JSON.stringify(labelFor(now, 0))
			);
			await pictures(app);
			await app.idle();

			const asked = new Set(await viewsOf(seen));
			assert.equal(asked.size, held, `after a midnight ${asked.size} of ${held} days were re-drawn`);
		});
	});

	test('the strip holds a month of its days, and asks again for the days before it', async () => {
		await open(async (app) => {
			await seedLines(app, ['occur coffee -d -40']);
			await pictures(app);
			await drawBackTo(app, 40);

			let seen;
			await awayAndBack(app, async () => (seen = await app.spyOnRunner()));
			await drawBackTo(app, 40);

			assert.deepEqual(
				[...new Set(await viewsOf(seen))],
				Array.from({ length: 40 - 31 }, (_, i) => 32 + i),
				'not the days past a month, each asked again'
			);
		});
	});

	// Beside a layout whose root is a split, as `main`'s is, the strip is
	// laid out in the box it lands in, and no resize is reported on the
	// landing: what moved while it was away is read when it lands.
	test('a box that changed while the strip was away is the box it draws at', async () => {
		await withApp(
			async (app) => {
				await seedLines(app, ['occur coffee -d -1']);
				await pictures(app);
				await app.idle();
				await awayAndBack(app, () => app.page.setViewportSize({ width: 360, height: 700 }), 'collect');
				const today = app.page.locator('section[data-pane] .band[data-days-ago="0"]').first();
				await until(
					async () => [
						Math.floor((await today.boundingBox()).width),
						await today.locator('svg.svg').evaluate((svg) => svg.width.baseVal.value)
					],
					([box, drawn]) => box === drawn
				);
			},
			{ ...PHONE, clock: AT, prepare: (context) => twoLayouts(context, 'collect') }
		);
	});
});

// vividn: «can dayview remain cached between refreshes on datum.day. The
// data really shouldn't update much so we don't need to redraw from scratch
// everytime». A write checks the days on screen by one `dayview --digests`,
// and asks again only for today and the days whose picture changed.
describe('the day strip across a write (da-efxk5)', () => {
	const mark = (app, end) =>
		app.page.evaluate((end) => (document.querySelector(`.band[data-days-ago="${end}"] svg.svg`).__kept = true), end);
	const marked = (app, end) =>
		app.page.evaluate((end) => !!document.querySelector(`.band[data-days-ago="${end}"] svg.svg`)?.__kept, end);
	const painted = (app, end, color) =>
		app.page.waitForFunction(
			([end, color]) => document.querySelector(`.band[data-days-ago="${end}"] svg.svg`)?.outerHTML.includes(color),
			[end, color]
		);

	test('a write into today redraws today and keeps yesterday, its element and all', async () => {
		await open(async (app) => {
			await seedLines(app, ['occur coffee -d -1', 'occur coffee -d -3']);
			await pictures(app);
			await app.idle();
			assert.ok((await inView(app)).includes(1), 'yesterday is not on screen to be kept');
			await mark(app, 1);
			const seen = await app.spyOnRunner();

			await barLine(app, 'switch mood happy -t -5');
			await painted(app, 0, colorOf('happy'));
			await app.idle();

			assert.deepEqual(await viewsOf(seen), [0], 'a day besides today was drawn again');
			assert.ok((await seen()).some((argv) => argv.includes('--digests')), 'nothing checked the days on screen');
			assert.ok(await marked(app, 1), "yesterday's picture was replaced");
		});
	});

	test('a write backdated into yesterday redraws yesterday', async () => {
		await open(async (app) => {
			await seedLines(app, ['occur coffee -d -1', 'occur coffee -d -3']);
			await pictures(app);
			await app.idle();
			await mark(app, 1);
			const seen = await app.spyOnRunner();

			await barLine(app, 'switch mood happy -y -t 10');
			await painted(app, 1, colorOf('happy'));
			await app.idle();

			assert.deepEqual(await viewsOf(seen), [0, 1]);
			assert.ok(!(await marked(app, 1)), "yesterday kept the picture from before the write");
		});
	});
});
