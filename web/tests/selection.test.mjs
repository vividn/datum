// Selecting entries, in a browser (da-r13j): a tap on a tail row or on a
// document loads its hid as the bar's `hid` pill, and the commands widget
// answers — only the commands whose grammar takes the pills are drawn. Ctrl-click toggles, shift-click takes a
// range, and a LOCKED pill makes a plain tap add, which is how a phone
// selects several. The rules alone are `selection-pure.test.mjs`.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { ON_COLLECT, sharedApp, startApp, stopApp, withApp as withAnyApp, seedLines } from './support/app.mjs';
import { DESKTOP, along, widgetPane } from './support/layout.mjs';
import { listed } from './support/rows.mjs';

before(startApp);
after(stopApp);

/** The CLI widget, tail and the commands widget, all on one desktop screen. */
const PAGE = JSON.stringify(
	along('column', widgetPane('repl', 'cli'), widgetPane('tail', 'tail'), widgetPane('cmd', 'command'))
);

// ONE APP for every desktop test (da-6b3dj, sharedApp in support/app.mjs),
// `main` opened from PAGE's draft by `solo`, which empties the database and
// the bar's pills between them; each reads the CLI widget from its end. The
// phone opens its own: a first visit, with `mobile`'s tree in front.
const shared = sharedApp(DESKTOP);
const solo = (fn) => shared.solo(fn, PAGE);

/** A phone on its first visit, `collect` in front: `mobile`'s tree (da-shzs, da-u7vz8). */
const PHONE = { viewport: { width: 390, height: 664 }, device: { isMobile: true, hasTouch: true }, home: ON_COLLECT };

/** Four entries of one field, after the row that tracks it. */
const SEED = ['track a --verb occur', 'occur a', 'occur a', 'occur a', 'occur a'];

/**
 * `seedLines(SEED)`, a second apart: the clock is held (da-ydao), and one
 * instant holds one `a`. `log` is the door each line goes through, the
 * seam's quiet one and the bar for the last unless a test says otherwise.
 */
async function seedApart(app, log) {
	for (const [i, line] of SEED.entries()) {
		if (log) await log(line);
		else if (i < SEED.length - 1) await app.runLine(line);
		else await seedLines(app, [line]);
		await app.clock.forward(1000);
	}
}

/** The last `n` tail rows: the four occurrences, newest last. */
const last = async (page, n) => {
	const count = await rows(page).count();
	return Array.from({ length: n }, (_, i) => rows(page).nth(count - n + i));
};
const rowCount = (page) => page.evaluate(() => document.querySelectorAll('section[data-title="tail"] tbody tr[data-hid]').length);

const rows = (page) => page.locator('section[data-title="tail"] tbody tr[data-hid]');
const hidPill = (page) => page.locator('.prompt-bar .pill.hid');
const sections = (page) => hidPill(page).locator('button.remove .label').allInnerTexts();
const button = (page, name) => page.locator(`main .bar.commands button[data-command="${name}"]`);

/** The command buttons drawn, in order. */
const drawn = async (page) => (await listed(page.locator('main .bar.commands'), 'button[data-command]')).map((b) => b.command);

/** The last `.cli .echo`'s count, before a press. */
const echoCount = (page) => page.evaluate(() => document.querySelectorAll('.cli .echo').length);

/**
 * Press a command the selection COMPLETES — it runs at the tap, with no
 * Enter (da-3lxol) — and wait for the run to settle.
 */
async function run(app, name) {
	const echoes = await echoCount(app.page);
	await button(app.page, name).click();
	await app.page.waitForFunction((n) => document.querySelectorAll('.cli .echo').length > n, echoes);
	await app.idle();
	assert.equal(await app.page.locator('.prompt-bar input').inputValue(), '', 'the tap put something on the line');
}

const ID_COMMANDS = ['undo', 'redo', 'help', 'delete', 'update', 'retime', 'get', 'history'];

describe('a tap on a tail row', () => {
	test('loads its hid as a pill; only the id commands are drawn; get runs on it', async () => {
		await solo(async (app) => {
			const { page } = app;
			await seedApart(app);
			const whole = await drawn(page);
			assert.ok(whole.includes('occur'), 'no pill: every command');

			const [row] = await last(page, 2);
			const hid = await row.getAttribute('data-hid');
			await row.click();
			await hidPill(page).waitFor();
			assert.deepEqual(await sections(page), [hid.slice(0, 5)]);
			assert.equal(await row.evaluate((tr) => tr.classList.contains('selected')), true);

			await page.waitForFunction(() => !document.querySelector('main .bar.commands [data-command="occur"]'));
			assert.deepEqual(await drawn(page), ID_COMMANDS);

			await run(app, 'get');
			assert.equal((await app.echoes()).at(-1), `get ${hid}`);
			assert.equal(await hidPill(page).count(), 0, 'a press spends an unlocked hid pill');

			// Tap, tap again: the second clears.
			await row.click();
			await hidPill(page).waitFor();
			await row.click();
			await hidPill(page).waitFor({ state: 'detached' });
			await page.locator('main .bar.commands [data-command="occur"]').waitFor();
			assert.deepEqual(await drawn(page), whole, 'the dismiss brings the whole list back');
			assert.deepEqual(app.errors, []);
		});
	});

	// da-3lxol, acceptance 4: the double outline and the tap read ONE
	// predicate, so the selection flips both together.
	test('delete fills with nothing selected; a selected row arms it and the tap deletes; update never arms', async () => {
		await solo(async (app) => {
			const { page } = app;
			await seedApart(app);
			const armed = (name) => button(page, name).evaluate((b) => b.dataset.act === 'run');
			const input = page.locator('.prompt-bar input');
			// The shared app empties the pills between tests, not the line.
			await input.fill('');

			const echoes = await echoCount(page);
			assert.equal(await armed('delete'), false);
			await button(page, 'delete').click();
			await page.waitForFunction(() => document.querySelector('.prompt-bar input').value === 'delete ');
			assert.equal(await echoCount(page), echoes, 'nothing selected: the tap ran something');

			const before = await rowCount(page);
			const [row] = await last(page, 1);
			const hid = await row.getAttribute('data-hid');
			await row.click();
			await hidPill(page).waitFor();
			await page.waitForFunction(() => document.querySelector('main .bar.commands [data-command="delete"]')?.dataset.act === 'run');
			assert.deepEqual([await armed('delete'), await armed('update'), await armed('get')], [true, false, true]);

			await run(app, 'delete');
			assert.equal((await app.echoes()).at(-1), `delete ${hid}`);
			await page.waitForFunction((n) => document.querySelectorAll('section[data-title="tail"] tbody tr[data-hid]').length === n, before - 1);
			await page.waitForFunction(() => document.querySelector('main .bar.commands [data-command="delete"]')?.dataset.act !== 'run');

			const [next] = await last(page, 1);
			await next.click();
			await hidPill(page).waitFor();
			const nextHid = await next.getAttribute('data-hid');
			const quiet = await echoCount(page);
			assert.equal(await armed('update'), false);
			await button(page, 'update').click();
			// The hid goes into its slot on the line, and the caret after it (da-8ru0m).
			await page.waitForFunction((h) => document.querySelector('.prompt-bar input').value === `update ${h} `, nextHid);
			assert.equal(await echoCount(page), quiet, 'update ran on a tap');
			assert.deepEqual(await input.evaluate((el) => [el.selectionStart, el.value.length, document.activeElement === el]), [
				`update ${nextHid} `.length,
				`update ${nextHid} `.length,
				true
			]);
			await hidPill(page).waitFor({ state: 'detached' });
			await input.fill('');
			assert.deepEqual(app.errors, []);
		});
	});

	test('which commands take an id is what the CLI’s own `help --json` says', async () => {
		await solo(async (app) => {
			const { page } = app;
			await seedApart(app);
			const help = await page.evaluate(() => window.datum.runQuiet(['help', '--json']));
			// da-5flt: nowview takes `-t`, so a time pill leaves it drawn.
			const nowview = JSON.parse(help.stdout).find((c) => c.name === 'nowview');
			assert.ok(nowview.flags.some((f) => f.short === 't'), JSON.stringify(nowview));
			const takesId = new Set(
				JSON.parse(help.stdout)
					.filter((c) => c.usage.some((u) => /^datum \S+ [<[](ID_OR_SLUG|SLUG|HID)\b/.test(u.trim())))
					.flatMap((c) => [c.name, ...c.aliases])
			);
			const whole = await drawn(page);
			await rows(page).first().click();
			await hidPill(page).waitFor();
			await page.waitForFunction(() => !document.querySelector('main .bar.commands [data-command="occur"]'));
			// Help takes no pill and explains every one, so no pill hides it (da-vjwid).
			assert.deepEqual(await drawn(page), whole.filter((name) => takesId.has(name) || name === 'help'));
		});
	});
});

describe('a line the grammar does not know', () => {
	// Review 1498: a line led by a word the grammar does not know, typed with
	// an entry selected, got the hid as its STATE. An alias is such a word.
	test('never gets the hid: the alias `a` logs a stateless entry, and the pill stays', async () => {
		await solo(async (app) => {
			const { page } = app;
			await seedApart(app);
			await page.evaluate(() => window.datum.runQuiet(['alias', 'set', 'a', 'occur a']));
			const [row] = await last(page, 1);
			const hid = await row.getAttribute('data-hid');
			await row.click();
			await hidPill(page).waitFor();

			await app.submit('a');
			await app.idle();
			assert.equal((await app.echoes()).at(-1), 'a');
			const listing = await page.evaluate(() => window.datum.runQuiet(['tail', '--json']));
			const newest = JSON.parse(listing.stdout).rows.at(-1);
			assert.notEqual(newest.hid, hid);
			assert.equal(newest.field, 'a');
			assert.equal(newest.state, null, 'the hid was not written as a state');
			assert.deepEqual(await sections(page), [hid.slice(0, 5)], 'the hid pill is still loaded');
			assert.deepEqual(app.errors, []);
		});
	});
});

describe('selecting several', () => {
	test('ctrl-click two rows: one pill, two sections; update gets them as ONE comma word; delete takes both; one Undo brings both back', async () => {
		await solo(async (app) => {
			const { page } = app;
			await seedApart(app);
			const [first, , third] = await last(page, 4);
			const hids = [await first.getAttribute('data-hid'), await third.getAttribute('data-hid')];
			const before = await rowCount(page);
			await first.click({ modifiers: ['ControlOrMeta'] });
			await third.click({ modifiers: ['ControlOrMeta'] });
			await page.waitForFunction(() => document.querySelectorAll('.prompt-bar .pill.hid button.remove').length === 2);
			assert.equal(await hidPill(page).count(), 1);
			assert.deepEqual(await sections(page), hids.map((h) => h.slice(0, 5)));
			// Every id command takes a comma list (da-9p3o), so two hids keep them all.
			assert.deepEqual(await drawn(page), ID_COMMANDS);

			// A locked pill survives the update, for the delete after it.
			await hidPill(page).locator('button.lock').click();
			await button(page, 'update').click();
			const input = page.locator('.prompt-bar input');
			// Locked, the pill is written into the line AND stays for the delete (da-8ru0m).
			await page.waitForFunction((h) => document.querySelector('.prompt-bar input').value === `update ${h} `, hids.join(','));
			assert.equal(await hidPill(page).count(), 1);
			await input.pressSequentially('note=x');
			await input.press('Enter');
			await app.idle();
			// `update A B` would take B as a state; the pill is one word.
			assert.equal((await app.echoes()).at(-1), `update ${hids.join(',')} note=x`);
			const updated = await page.evaluate(() => window.datum.runQuiet(['tail', '--json']));
			const notes = JSON.parse(updated.stdout).rows.filter((r) => r.data?.note === 'x').map((r) => r.hid);
			assert.deepEqual(notes.sort(), [...hids].sort(), 'both entries got the note, and nothing took a second hid as a state');
			await hidPill(page).locator('button.lock').click();

			await run(app, 'delete');
			assert.equal((await app.echoes()).at(-1), `delete ${hids.join(',')}`);
			await page.waitForFunction((n) => document.querySelectorAll('section[data-title="tail"] tbody tr[data-hid]').length === n, before - 2);
			assert.equal(await hidPill(page).count(), 0);

			// Measured: `undo` after a two-id delete restores BOTH — the
			// delete is one step on the `_LAST` pointer.
			await button(page, 'undo').click();
			await app.idle();
			await page.waitForFunction((n) => document.querySelectorAll('section[data-title="tail"] tbody tr[data-hid]').length === n, before);
			assert.deepEqual(app.errors, []);
		});
	});

	test('two selected: get is offered, and runs on both — one document each', async () => {
		await solo(async (app) => {
			const { page } = app;
			await seedApart(app);
			const [first, , third] = await last(page, 4);
			const hids = [await first.getAttribute('data-hid'), await third.getAttribute('data-hid')];
			await first.click({ modifiers: ['ControlOrMeta'] });
			await third.click({ modifiers: ['ControlOrMeta'] });
			await page.waitForFunction(() => document.querySelectorAll('.prompt-bar .pill.hid button.remove').length === 2);
			await button(page, 'get').waitFor();

			await run(app, 'get');
			assert.equal((await app.echoes()).at(-1), `get ${hids.join(',')}`);
			const docs = await page.locator('.cli .doc[data-hid]').evaluateAll((all) => all.slice(-2).map((d) => d.dataset.hid));
			assert.deepEqual(
				docs.map((d, i) => hids[i].startsWith(d) || d.startsWith(hids[i])),
				[true, true],
				`the last two documents are the two entries, in order: ${docs}`
			);
			assert.deepEqual(app.errors, []);
		});
	});

	test('shift-click takes the range of four, and it is drawn in tail and on the CLI widget’s document', async () => {
		await solo(async (app) => {
			const { page } = app;
			await seedApart(app);
			const four = await last(page, 4);
			await four[0].click();
			await four[3].click({ modifiers: ['Shift'] });
			await page.waitForFunction(() => document.querySelectorAll('.prompt-bar .pill.hid button.remove').length === 4);
			assert.equal(await page.evaluate(() => getSelection().isCollapsed), true, 'shift-click selected no text');
			// The CLI widget drew the last seed line's document, the newest row, in the range.
			const doc = page.locator('.cli .doc[data-hid]').last();
			assert.equal(await doc.evaluate((d) => d.classList.contains('selected')), true);

			// A tap on that document alone replaces the range with it, in both.
			await doc.locator('.info').click();
			await page.waitForFunction(() => document.querySelectorAll('.prompt-bar .pill.hid button.remove').length === 1);
			const selected = await rows(page).evaluateAll((all) => all.filter((r) => r.classList.contains('selected')).length);
			assert.equal(selected, 1);
			assert.equal(await four[3].evaluate((tr) => tr.classList.contains('selected')), true);
		});
	});

	test('the hid button keeps its own tap: it swaps to the full id and selects nothing', async () => {
		await solo(async (app) => {
			const { page } = app;
			await seedApart(app);
			await page.locator('.cli .doc .hid').last().click();
			await page.locator('.cli .doc .hid[aria-expanded="true"]').waitFor();
			assert.equal(await hidPill(page).count(), 0);
		});
	});
});

describe('a -t pill', () => {
	test('leaves the commands that take -t; its dismiss restores them all', async () => {
		await solo(async (app) => {
			const { page } = app;
			await page.evaluate(() => window.datum.bar.setPills([{ id: 't', kind: 'time', text: '-t 10:00' }]));
			await page.waitForFunction(() => !document.querySelector('main .bar.commands [data-command="now"]'));
			const names = await drawn(page);
			for (const n of ['occur', 'start', 'end', 'switch', 'add', 'retime']) assert.ok(names.includes(n), n);
			for (const n of ['now', 'get', 'delete', 'undo']) assert.ok(!names.includes(n), n);
			await page.locator('.prompt-bar .pill.time button.remove').click();
			await page.locator('main .bar.commands [data-command="now"]').waitFor();
		});
	});
});

describe('on a phone', () => {
	test('lock the pill, then each tap adds — three sections; a tap on one again takes it out', async () => {
		await withAnyApp(async (app) => {
			const { page } = app;
			const count = await phoneRows(app);
			await rows(page).nth(count - 1).tap();
			await hidPill(page).waitFor();
			await hidPill(page).locator('button.lock').tap();
			await page.locator('.prompt-bar .pill.hid.locked').waitFor();
			await rows(page).nth(count - 2).tap();
			await rows(page).nth(count - 3).tap();
			await page.waitForFunction(() => document.querySelectorAll('.prompt-bar .pill.hid button.remove').length === 3);
			await rows(page).nth(count - 2).tap();
			await page.waitForFunction(() => document.querySelectorAll('.prompt-bar .pill.hid button.remove').length === 2);
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	// da-x19yo, vividn: «If you long press on piece of data, it locks the
	// pill, letting you then do multi select».
	test('a long press selects the row and locks the pill; taps add and remove; unlocked, a tap selects one', async () => {
		await withAnyApp(async (app) => {
			const { page } = app;
			const count = await phoneRows(app);
			const marked = () => page.evaluate(() => document.querySelectorAll('section[data-title="tail"] tbody tr.selected').length);
			const held = await rows(page).nth(count - 1).getAttribute('data-hid');

			await press(page, rows(page).nth(count - 1), async () => {
				await page.locator('.prompt-bar .pill.hid.locked').waitFor();
			});
			// The lift's click is the hold's: it did not take the entry out again.
			await app.idle();
			assert.deepEqual(await sections(page), [held.slice(0, 5)]);
			assert.equal(await marked(), 1);

			await rows(page).nth(count - 2).tap();
			await rows(page).nth(count - 3).tap();
			await page.waitForFunction(() => document.querySelectorAll('.prompt-bar .pill.hid button.remove').length === 3);
			assert.equal(await marked(), 3);
			await rows(page).nth(count - 2).tap();
			await page.waitForFunction(() => document.querySelectorAll('.prompt-bar .pill.hid button.remove').length === 2);
			assert.equal(await marked(), 2);

			await hidPill(page).locator('button.lock').tap();
			await page.locator('.prompt-bar .pill.hid.locked').waitFor({ state: 'detached' });
			const one = await rows(page).nth(count - 2).getAttribute('data-hid');
			await rows(page).nth(count - 2).tap();
			await page.waitForFunction(() => document.querySelectorAll('.prompt-bar .pill.hid button.remove').length === 1);
			assert.deepEqual(await sections(page), [one.slice(0, 5)]);
			assert.equal(await marked(), 1);
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	test('a finger that traveled holds nothing: a scroll neither selects nor locks', async () => {
		await withAnyApp(async (app) => {
			const { page } = app;
			const count = await phoneRows(app);
			// The hold's timer is kept back and run once the finger has moved,
			// so the order is the test's and not a loaded runner's. The
			// control: the same press, still, does lock.
			for (const travel of [24, 0]) {
				await page.evaluate(() => {
					const real = window.setTimeout;
					window.heldBack = undefined;
					// The hold's wait alone: 450 ms is `HOLD_MS`.
					window.setTimeout = (fn, ms, ...a) =>
						window.event?.type === 'pointerdown' && ms === 450 ? ((window.heldBack = fn), 0) : real(fn, ms, ...a);
					for (const lift of ['pointerup', 'pointercancel'])
						addEventListener(lift, () => (window.setTimeout = real), { capture: true, once: true });
				});
				await press(page, rows(page).nth(count - 2), async (cdp, at) => {
					if (travel) await cdp.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: [{ x: at.x, y: at.y - travel }] });
					await page.evaluate(() => window.heldBack());
				});
				await app.idle();
				assert.equal(await page.locator('.prompt-bar .pill.hid.locked').count(), travel ? 0 : 1, `travel ${travel}`);
			}
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});
});

/** The phone seeded, the ghost gone: how many tail rows it shows, at least three. */
async function phoneRows(app) {
	const { page } = app;
	// No CLI widget on `mobile` for `seedLines` to wait on: through the loud door, then idle.
	await seedApart(app, (line) => page.evaluate((l) => window.datum.runLine(l), line));
	await app.idle();
	// The loud door leaves the last line's ghost up for a second of real
	// time (`GHOST_MS`), the pills standing after it, and they slide 84px
	// left when it goes: a lock tap whose click came after that hit where
	// the lock had been, and the pill stayed unlocked (da-hoysk).
	await page.locator('.prompt-bar .ghost').waitFor({ state: 'detached' });
	await rows(page).nth(0).waitFor();
	const count = await rows(page).count();
	assert.ok(count >= 3, `the mobile tail shows ${count} rows`);
	return count;
}

/** One finger on `row`'s middle, by CDP: down, `meanwhile`, up. */
async function press(page, row, meanwhile) {
	const box = await row.boundingBox();
	const at = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
	const cdp = await page.context().newCDPSession(page);
	await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [at] });
	await meanwhile(cdp, at);
	await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
	await cdp.detach();
}
