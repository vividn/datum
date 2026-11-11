// Every widget's tour (da-7dflk). vividn, 2026-11-05: «every widget should
// have a tour, where stuff is highlighted and invites you to actually
// interact with it … if there are other widgets open on the page, it can go
// into how they interact. at the end of the tour it can offer a read more
// button … In the widget menu for every widget there should be a "? help" and
// a documentation line».
//
// So, for each kind a person can open alone (the docs' list less the kit's
// widget, which needs a kit installed), its tour started from its menu's
// «? help» runs to its end: each step rings the element it names in that
// widget as drawn, a step that invites is passed only by doing what it asks
// there, doing it logs nothing, and the last is the widget whole with «read more», which opens its
// guide in the docs. On `main`, beside other widgets, a tour gains the steps
// on them, and alone it has none. `guides-pure.test.mjs` holds the kinds no
// test opens alone to a tour of the widget whole.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { seedLines, sharedApp, startApp, stopApp } from './support/app.mjs';
import { DESKTOP, menuLines } from './support/layout.mjs';
import { resolveLibAsSource } from './support/libhook.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

let KINDS, WHOLE, guided;
before(async () => {
	resolveLibAsSource();
	({ KINDS, WHOLE } = await import('../src/lib/layout/registry.ts'));
	({ guided } = await import('../src/lib/guides.ts'));
});

const shared = sharedApp(DESKTOP);
/** A moment, a change and an on/off field, the last logged through the bar, so the CLI widget holds an entry. */
const SEED = ['moment coffee -t 1400', 'switch mood happy -t 1410', 'start work -t 1420'];
/** The built-in layout that is a kind alone, where its name is not the kind's. */
const LAYOUT = { daystrip: 'dayview', widgetbar: 'fields' };

/** The menu button of the drawn widget of `kind`, not one in a root tab behind (clipped, not hidden): its pane's name. */
const nameOf = (page, kind) => page.locator(`.host[data-widget="${kind}"]:not(.tab-body.behind .host) > section button.title`).first();

/** The stop on screen: its sentence, what it invites, whether «read more» is there, and the box of `at` in the drawn widget of `kind` and whether the ring stands round it. */
const onScreen = (page, kind, at) =>
	page.evaluate(
		([kind, at]) => {
			const drawn = (e) => {
				const r = e.getBoundingClientRect();
				return r.width > 0 && r.height > 0;
			};
			const hosts = [...document.querySelectorAll(`.host[data-widget="${kind}"]`)].filter((h) => !h.closest('.tab-body.behind'));
			const el = hosts.flatMap((h) => [...h.querySelectorAll(at)]).find(drawn);
			const ring = document.querySelector('.tour .ring')?.getBoundingClientRect();
			const r = el?.getBoundingClientRect();
			const round = !!ring && !!r && [ring.left - r.left, ring.top - r.top, r.right - ring.right, r.bottom - ring.bottom].every((d) => Math.abs(d + 4) < 1);
			return {
				say: document.querySelector('.tour .say p')?.textContent ?? null,
				do: document.querySelector('.tour')?.getAttribute('data-do') ?? null,
				more: !!document.querySelector('.tour button.more'),
				round,
				at: r && { x: r.left + r.width / 2, y: r.top + r.height / 2 }
			};
		},
		[kind, at]
	);

/** Do what a stop invites, at the middle of its element. */
async function act(page, how, { x, y }) {
	if (how === 'tap') return page.mouse.click(x, y);
	await page.mouse.move(x, y);
	if (how === 'scroll') return page.mouse.wheel(0, -300);
	if (how === 'type') {
		await page.mouse.click(x, y);
		return page.keyboard.type('tail');
	}
	await page.mouse.down();
	// The hold itself, not a wait for the page: past HOLD_MS (#lib/hold.ts, 450 ms),
	// the press a hold stop's timer (#lib/tour.ts) must outlast before the lift clears it.
	if (how === 'hold') await page.waitForTimeout(600);
	else await page.mouse.move(x + 60, y, { steps: 6 });
	await page.mouse.up();
}

/** The kinds drawn on the page, by their hosts. */
const drawnKinds = (page) =>
	page.evaluate(() => [
		...new Set(
			[...document.querySelectorAll('.host[data-widget]')]
				.filter((h) => !h.closest('.tab-body.behind') && h.querySelector(':scope > section')?.getBoundingClientRect().width)
				.map((h) => h.dataset.widget)
		)
	]);

/**
 * `kind`'s tour from its menu's «? help», walked: each stop expected in turn
 * — its steps, a step `with` a kind where one is drawn, then the widget whole
 * — ringed round its element, passed by doing what it invites or by «next»;
 * the last one's «read more» opens its guide in the docs. Returns the kinds
 * whose steps it showed.
 */
async function walk(page, kind) {
	const beside = (await drawnKinds(page)).filter((k) => k !== kind);
	const steps = [
		...KINDS[kind].tour.filter((s) => !s.with || beside.includes(s.with)),
		{ at: WHOLE, say: null, more: true }
	];
	await nameOf(page, kind).click();
	await page.getByRole('menuitem', { name: '? help', exact: true }).click();
	for (const [i, step] of steps.entries()) {
		const where = `${kind}'s step ${i} (${step.at}${step.with ? ` on ${step.with}` : ''})`;
		const s = await until(
			() => onScreen(page, step.with ?? kind, step.at),
			(s) => s.round && (step.say === null ? s.more : s.say === step.say)
		).catch((e) => assert.fail(`${where}: ${e.message}`));
		assert.equal(s.do, step.do ?? null, `${where} invites ${s.do}`);
		assert.equal(s.more, !!step.more, `${where}: read more`);
		if (step.do) await act(page, step.do, s.at);
		else if (!step.more) await page.locator('.tour button.next').click();
	}
	await page.locator('.tour button.more').click();
	await page.locator('.tour').waitFor({ state: 'detached' });
	await page.locator(`article.doc[data-doc="widgets/${kind}.md"]`).waitFor();
	return steps.filter((s) => s.with).map((s) => s.with);
}

/** Close the docs, where they opened over the page: Escape, which a field a hold left open on collect takes first. */
async function closeDocs(page) {
	const docs = page.locator('dialog[open][data-place="docs"]');
	for (let i = 0; i < 3 && (await docs.count()); i++) {
		await page.keyboard.press('Escape');
		await docs.waitFor({ state: 'detached', timeout: 2000 }).catch(() => {});
	}
	assert.equal(await docs.count(), 0, 'the docs stayed open');
}

describe('a widget\'s tour', () => {
	test('every kind a person opens alone has a tour', () => {
		const alone = guided().filter((kind) => kind !== 'kit');
		assert.ok(alone.length > 15 && alone.includes('widgetbar') && alone.includes('daystrip'), `${alone}`);
		for (const kind of alone) assert.ok(KINDS[kind].tour.length > 0, `${kind} has no tour`);
	});

	test('each runs to its end from its menu, ringing what it names and passed by doing what it invites; alone, no step on another', async () => {
		const alone = guided().filter((kind) => kind !== 'kit');
		await shared.solo(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);
			const logged = (await app.allEntries()).length;
			for (const kind of alone) {
				await page.evaluate((name) => window.datum.shell.open(name), LAYOUT[kind] ?? kind);
				await nameOf(page, kind).waitFor();
				// Drawn from its answer: a step on a part its data draws is in the tour once that has landed.
				await app.idle();
				const toured = await walk(page, kind).catch((e) => assert.fail(`${kind}: ${e.message}`));
				assert.deepEqual(toured, [], `${kind} alone toured another widget`);
				await closeDocs(page);
				await page.keyboard.press('Escape');
			}
			// Doing what every step invites logs nothing: a tour asks for no entry that did not happen.
			await app.idle();
			assert.equal((await app.allEntries()).length, logged, 'a tour logged an entry');
			assert.deepEqual(app.errors, []);
		});
	});

	test('beside other widgets, a tour goes into them: collect into the nowview and the dayview, the tail into collect and the dayview', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);
			assert.deepEqual(await walk(page, 'widgetbar'), ['nowview', 'daystrip']);
			await closeDocs(page);
			assert.deepEqual(await walk(page, 'tail'), ['widgetbar', 'daystrip']);
			await closeDocs(page);
			assert.deepEqual(app.errors, []);
		});
	});

	test('a menu has «? help» and «Documentation» after the options, a widget declaring none too; Documentation opens its guide; leave ends a tour', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await page.evaluate(() => window.datum.shell.open('aliases'));
			await nameOf(page, 'aliases').click();
			assert.deepEqual((await menuLines(page)).slice(0, 3), ['Modify', '? help', 'Documentation']);
			await page.getByRole('menuitem', { name: 'Documentation', exact: true }).click();
			await page.locator('article.doc[data-doc="widgets/aliases.md"]').waitFor();
			// «← back» goes to the contents.
			await page.locator('nav.docs-nav button.back').click();
			await page.locator('article.doc[data-doc="README.md"]').waitFor();
			await closeDocs(page);

			await page.evaluate(() => window.datum.shell.open('tail'));
			await nameOf(page, 'tail').click();
			assert.deepEqual((await menuLines(page)).slice(0, 5), ['Modify', 'Reset widget', 'type', '? help', 'Documentation']);
			await page.getByRole('menuitem', { name: '? help', exact: true }).click();
			await page.locator('.tour .say').waitFor();
			await page.locator('.tour button.leave').click();
			await page.locator('.tour').waitFor({ state: 'detached' });
			assert.deepEqual(app.errors, []);
		});
	});
});
