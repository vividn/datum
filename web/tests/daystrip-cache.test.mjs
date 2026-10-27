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

/**
 * This device's open layouts: `main`, with the strip, and `tail`, without one.
 * One kept mounted at the root, so the swipe away unmounts the strip: a
 * layout once opened stays mounted (da-tajsm), and the cache is for the
 * strip that comes back anew, past what the root keeps.
 */
const twoLayouts = (context, next = 'tail') =>
	context.addInitScript(
		([json]) => {
			localStorage.setItem('datum.layout.root.v1', json);
			window.__datumKeptLayouts = 1;
		},
		[JSON.stringify({ names: ['main', next], active: 0 })]
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

/** The kept rows: each one's band argv, date and SVG, after `edit` has had its way with the store. */
const keptRows = (app, edit = '') =>
	app.page.evaluate(
		(edit) =>
			new Promise((resolve, reject) => {
				const open = indexedDB.open('datum-held-days');
				open.onerror = () => reject(open.error);
				open.onsuccess = () => {
					const tx = open.result.transaction('days', 'readwrite');
					const store = tx.objectStore('days');
					if (edit === 'clear') store.clear();
					const all = store.getAll();
					all.onsuccess = () => {
						if (edit === 'tamper')
							for (const row of all.result) store.put({ ...row, svg: row.svg.replace('</svg>', '<!-- an older datum --></svg>') });
						// Past both bounds: forty dates of 2020 at the size drawn, and four sizes used before any.
						if (edit === 'pad') {
							const { db, size, svg } = all.result[0];
							for (let day = 1; day <= 40; day++)
								store.put({ db, size, date: new Date(Date.UTC(2020, 0, day)).toISOString().slice(0, 10), svg, at: 0 });
							for (const width of [1, 2, 3, 4]) store.put({ db, size: size.replace(/--width \d+/, `--width ${width}`), date: '2020-01-01', svg, at: 0 });
						}
					};
					tx.oncomplete = () => {
						open.result.close();
						resolve(all.result.map(({ size, date, svg }) => ({ size, date, svg })));
					};
				};
			}),
		edit
	);

/** Every day of `before` but today, kept ($lib/heldDays.ts). */
const allKept = (app, before) =>
	until(() => keptRows(app), (rows) => rows.length === Object.keys(before).length - 1);

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

	// A kept picture is of its date, not of its place in the stack
	// (da-crq76): after a midnight each is drawn one day further back, and
	// only today and yesterday, the today it never kept, are drawn again.
	test('a midnight while away draws today and yesterday, and moves every kept day back one', async () => {
		await open(async (app) => {
			await seedLines(app, ['occur coffee -d -1', 'occur coffee -d -3']);
			const before = await pictures(app);
			await app.idle();
			await allKept(app, before);

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
			assert.deepEqual([...new Set(await viewsOf(seen))], [0, 1], 'not today and yesterday drawn again');

			// The check goes out a frame after the scroll, and under the held
			// clock that frame is Playwright's: wait for it, not for idle.
			await scrollToDay(app, 2);
			await until(seen, (argvs) => argvs.some((argv) => argv.includes('--digests')));
			await app.idle();
			assert.ok(!(await viewsOf(seen)).includes(2), 'the day before yesterday was drawn again');
			assert.equal(await band(app, 2), before[1], 'the day before yesterday is not the picture of its date');
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

// vividn, 2026-10-26: «will the website have easy access to the cache for
// quick rendering without having to go through the cli?» — «yes». Each
// finished day's picture is kept in IndexedDB ($lib/heldDays.ts), so a reload
// draws the earlier days from there and asks the CLI only for today and one
// `dayview --digests` check. Every dayview argv the page sends its database
// worker is recorded from the boot on, a reload's included.
describe('the day strip across a reload (da-crq76)', () => {
	const recordBoot = (context) =>
		context.addInitScript(() => {
			window.__booted = [];
			const post = Worker.prototype.postMessage;
			Worker.prototype.postMessage = function (message, ...rest) {
				if (message?.argv?.[0] === 'dayview') window.__booted.push(message.argv);
				return post.call(this, message, ...rest);
			};
		});
	const booted = (app) => () => app.page.evaluate(() => window.__booted);

	/**
	 * The page after a reload asked for it holds the store's read behind a
	 * busy transaction until `__keptHeld` is cleared: a read slower than its
	 * patience, as a big store's on a slow phone is.
	 */
	const holdRead = (context) =>
		context.addInitScript(() => {
			if (!sessionStorage.getItem('datum-test-hold-kept')) return;
			sessionStorage.removeItem('datum-test-hold-kept');
			window.__keptHeld = true;
			const open = indexedDB.open('datum-held-days');
			open.onsuccess = () => {
				const store = open.result.transaction('days', 'readwrite').objectStore('days');
				const spin = () => window.__keptHeld && (store.count().onsuccess = spin);
				spin();
			};
		});

	/** Draw the strip, and wait for every day but today to be kept. */
	async function drawnAndKept(app) {
		await seedLines(app, ['occur coffee -d -1', 'occur coffee -d -3', 'switch mood happy -y -t 10']);
		const before = await pictures(app);
		await app.idle();
		await allKept(app, before);
		return before;
	}

	const opened = (fn) =>
		withApp(fn, { ...PHONE, clock: AT, prepare: (context) => Promise.all([twoLayouts(context), recordBoot(context), holdRead(context)]) });

	test('a reload draws the earlier days from the store, asking only for today and the check', async () => {
		await opened(async (app) => {
			const before = await drawnAndKept(app);

			await app.reload();
			const after = await pictures(app);
			await app.idle();

			assert.deepEqual(await viewsOf(booted(app)), [0], 'the reload asked the CLI for an earlier day');
			assert.ok((await booted(app)()).some((argv) => argv.includes('--digests')), 'nothing checked the kept days');
			for (const end of Object.keys(before).filter((end) => end !== '0'))
				assert.equal(after[end], before[end], `day ${end} is not the picture it was`);
		});
	});

	// What a write on another tab, a sync or a newer datum leaves: a kept
	// picture that is not the one the CLI draws now.
	test('a kept picture the CLI no longer draws is redrawn after the check, and kept again', async () => {
		await opened(async (app) => {
			const before = await drawnAndKept(app);
			await keptRows(app, 'tamper');

			await app.reload();
			await pictures(app);
			const visible = (await inView(app)).filter((end) => end !== 0);
			await app.page.waitForFunction(
				(ends) => ends.every((end) => !document.querySelector(`.band[data-days-ago="${end}"]`)?.innerHTML.includes('an older datum')),
				visible
			);
			await app.idle();

			assert.deepEqual(await viewsOf(booted(app)), [0, ...visible], 'not today and the days on screen, redrawn');
			for (const end of visible) assert.equal(await band(app, end), before[end], `day ${end} is not drawn as before`);
			const rows = await keptRows(app);
			assert.equal(
				rows.filter(({ svg }) => svg.includes('an older datum')).length,
				rows.length - visible.length,
				'the redrawn days were not kept again, or the rest were'
			);
		});
	});

	// vremya, review of #1225: a read that missed its patience left the rows
	// of earlier sessions out of eviction for good.
	test('a read that misses its patience still lands, and the store is trimmed to its bounds', async () => {
		await opened(async (app) => {
			await drawnAndKept(app);
			await keptRows(app, 'pad');
			await app.page.evaluate(() => sessionStorage.setItem('datum-test-hold-kept', '1'));

			await app.reload();
			await pictures(app);
			await app.idle();
			assert.ok(
				(await viewsOf(booted(app))).some((end) => end > 0),
				'the read did not miss: no earlier day was drawn through the CLI'
			);
			await app.page.evaluate(() => (window.__keptHeld = false));

			const bounded = (rows) => {
				const sizes = Map.groupBy(rows, (row) => row.size);
				return sizes.size <= 3 && [...sizes.values()].every((days) => days.length <= 31);
			};
			const rows = await until(async () => (await keptRows(app)).map(({ size, date }) => ({ size: size.slice(0, 24), date })), bounded);
			assert.ok(rows.length > 0, 'nothing kept at all');
		});
	});

	test('a cleared store draws every day through the CLI, as before', async () => {
		await opened(async (app) => {
			const before = await drawnAndKept(app);
			await keptRows(app, 'clear');

			await app.reload();
			const after = await pictures(app);
			await app.idle();

			assert.deepEqual(
				[...new Set(await viewsOf(booted(app)))],
				Object.keys(before).map(Number).sort((a, b) => a - b),
				'not every day drawn again'
			);
			// A day off screen keeps a picture from before the last write
			// until it is scrolled to, kept and drawn again alike: so the days
			// are compared by the run, not to `before`.
			assert.deepEqual(Object.keys(after), Object.keys(before));
		});
	});
});
