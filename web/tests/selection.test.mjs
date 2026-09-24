// Selecting entries, in a browser (da-r13j): a tap on a tail row or on a
// document loads its hid as the bar's `hid` pill, and the commands widget
// answers — only the commands whose grammar takes the pills are drawn. Ctrl-click toggles, shift-click takes a
// range, and a LOCKED pill makes a plain tap add, which is how a phone
// selects several. The rules alone are `selection-pure.test.mjs`.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, withApp as withAnyApp, seedLines } from './support/app.mjs';
import { DESKTOP, along, widgetPane } from './support/layout.mjs';
import { listed } from './support/rows.mjs';

before(startApp);
after(stopApp);

/** The REPL, tail and the commands widget, all on one desktop screen. */
const PAGE = JSON.stringify(
	along('column', widgetPane('repl', 'repl'), widgetPane('tail', 'tail'), widgetPane('cmd', 'command'))
);

// ONE APP for every desktop test (da-6b3dj, sharedApp in support/app.mjs),
// `main` opened from PAGE's draft by `solo`, which empties the database and
// the bar's pills between them; each reads the REPL from its end. The
// phone opens its own: a fresh device, which opens `mobile`.
const shared = sharedApp(DESKTOP);
const solo = (fn) => shared.solo(fn, PAGE);

/** A phone that has never opened the site: it opens `mobile` (da-shzs). */
const PHONE = { viewport: { width: 390, height: 664 }, device: { isMobile: true, hasTouch: true }, home: null };

/** Four entries of one field, after the row that tracks it. */
const SEED = ['track a --verb occur', 'occur a', 'occur a', 'occur a', 'occur a'];

/** The last `n` tail rows: the four occurrences, newest last. */
const last = async (page, n) => {
	const count = await rows(page).count();
	return Array.from({ length: n }, (_, i) => rows(page).nth(count - n + i));
};
const rowCount = (page) => page.evaluate(() => document.querySelectorAll('section[data-title="tail"] tbody tr[data-hid]').length);

const rows = (page) => page.locator('section[data-title="tail"] tbody tr[data-hid]');
const hidPill = (page) => page.locator('.prompt-bar .pill.hid');
const sections = (page) => hidPill(page).locator('button.remove .label').allInnerTexts();
const button = (page, name) => page.locator(`.bar.commands button[data-command="${name}"]`);

/** The command buttons drawn, in order. */
const drawn = async (page) => (await listed(page.locator('.bar.commands'), 'button[data-command]')).map((b) => b.command);

/** Press a filling command, then Enter in the bar, and wait for the run to settle. */
async function run(app, name) {
	await button(app.page, name).click();
	const input = app.page.locator('.prompt-bar input');
	await app.page.waitForFunction((n) => document.querySelector('.prompt-bar input').value === `${n} `, name);
	await input.press('Enter');
	await app.idle();
}

const ID_COMMANDS = ['undo', 'redo', 'retime', 'update', 'delete', 'history', 'get'];

describe('a tap on a tail row', () => {
	test('loads its hid as a pill; only the id commands are drawn; get runs on it', async () => {
		await solo(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);
			const whole = await drawn(page);
			assert.ok(whole.includes('occur'), 'no pill: every command');

			const [row] = await last(page, 2);
			const hid = await row.getAttribute('data-hid');
			await row.click();
			await hidPill(page).waitFor();
			assert.deepEqual(await sections(page), [hid.slice(0, 5)]);
			assert.equal(await row.evaluate((tr) => tr.classList.contains('selected')), true);

			await page.waitForFunction(() => !document.querySelector('.bar.commands [data-command="occur"]'));
			assert.deepEqual(await drawn(page), ID_COMMANDS);

			await run(app, 'get');
			assert.equal((await app.echoes()).at(-1), `get ${hid}`);
			assert.equal(await hidPill(page).count(), 0, 'a press spends an unlocked hid pill');

			// Tap, tap again: the second clears.
			await row.click();
			await hidPill(page).waitFor();
			await row.click();
			await hidPill(page).waitFor({ state: 'detached' });
			await page.locator('.bar.commands [data-command="occur"]').waitFor();
			assert.deepEqual(await drawn(page), whole, 'the dismiss brings the whole list back');
			assert.deepEqual(app.errors, []);
		});
	});

	test('which commands take an id is what the CLI’s own `help --json` says', async () => {
		await solo(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);
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
			await page.waitForFunction(() => !document.querySelector('.bar.commands [data-command="occur"]'));
			assert.deepEqual(await drawn(page), whole.filter((name) => takesId.has(name)));
		});
	});
});

describe('a line the grammar does not know', () => {
	// Review 1498: a field-first line typed with an entry selected got the
	// hid as its STATE. A bare field is the CLI's typo guard, so the line is
	// the field-first form, which the grammar does not know either.
	test('never gets the hid: `a occur` logs a stateless entry, and the pill stays', async () => {
		await solo(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);
			const [row] = await last(page, 1);
			const hid = await row.getAttribute('data-hid');
			await row.click();
			await hidPill(page).waitFor();

			await app.submit('a occur');
			await app.idle();
			assert.equal((await app.echoes()).at(-1), 'a occur');
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
			await seedLines(app, SEED);
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
			await page.waitForFunction(() => document.querySelector('.prompt-bar input').value === 'update ');
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
			await seedLines(app, SEED);
			const [first, , third] = await last(page, 4);
			const hids = [await first.getAttribute('data-hid'), await third.getAttribute('data-hid')];
			await first.click({ modifiers: ['ControlOrMeta'] });
			await third.click({ modifiers: ['ControlOrMeta'] });
			await page.waitForFunction(() => document.querySelectorAll('.prompt-bar .pill.hid button.remove').length === 2);
			await button(page, 'get').waitFor();

			await run(app, 'get');
			assert.equal((await app.echoes()).at(-1), `get ${hids.join(',')}`);
			const docs = await page.locator('.repl .doc[data-hid]').evaluateAll((all) => all.slice(-2).map((d) => d.dataset.hid));
			assert.deepEqual(
				docs.map((d, i) => hids[i].startsWith(d) || d.startsWith(hids[i])),
				[true, true],
				`the last two documents are the two entries, in order: ${docs}`
			);
			assert.deepEqual(app.errors, []);
		});
	});

	test('shift-click takes the range of four, and it is drawn in tail and on the REPL’s document', async () => {
		await solo(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);
			const four = await last(page, 4);
			await four[0].click();
			await four[3].click({ modifiers: ['Shift'] });
			await page.waitForFunction(() => document.querySelectorAll('.prompt-bar .pill.hid button.remove').length === 4);
			assert.equal(await page.evaluate(() => getSelection().isCollapsed), true, 'shift-click selected no text');
			// The REPL drew the last seed line's document, the newest row, in the range.
			const doc = page.locator('.repl .doc[data-hid]').last();
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
			await seedLines(app, SEED);
			await page.locator('.repl .doc .hid').last().click();
			await page.locator('.repl .doc .hid[aria-expanded="true"]').waitFor();
			assert.equal(await hidPill(page).count(), 0);
		});
	});
});

describe('a -t pill', () => {
	test('leaves the commands that take -t; its dismiss restores them all', async () => {
		await solo(async (app) => {
			const { page } = app;
			await page.evaluate(() => window.datum.bar.setPills([{ id: 't', kind: 'time', text: '-t 10:00' }]));
			await page.waitForFunction(() => !document.querySelector('.bar.commands [data-command="now"]'));
			const names = await drawn(page);
			for (const n of ['occur', 'start', 'end', 'switch', 'add', 'retime']) assert.ok(names.includes(n), n);
			for (const n of ['now', 'get', 'delete', 'undo']) assert.ok(!names.includes(n), n);
			await page.locator('.prompt-bar .pill.time button.remove').click();
			await page.locator('.bar.commands [data-command="now"]').waitFor();
		});
	});
});

describe('on a phone', () => {
	test('lock the pill, then each tap adds — three sections; a tap on one again takes it out', async () => {
		await withAnyApp(async (app) => {
			const { page } = app;
			// No REPL on `mobile` for `seedLines` to wait on: through the loud door, then idle.
			for (const line of SEED) await page.evaluate((l) => window.datum.runLine(l), line);
			await app.idle();
			await rows(page).nth(0).waitFor();
			const count = await rows(page).count();
			assert.ok(count >= 3, `the mobile tail shows ${count} rows`);
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
});
