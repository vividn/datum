// Tap EVERY button (da-y0ocz): the double outline ⇔ the tap wrote or ran
// something. Rule 1 of the design language — DOUBLE acts now; SINGLE arms,
// opens or fills; NONE is display — held not per widget but across the
// whole of a seeded page, the shipped layout at a desktop and a phone
// size. `act-guard.test.mjs` holds the source to one door (`use:act`);
// this holds what that door draws to what a tap then DOES.
//
// A file per screen (`act-taps-desktop`, `act-taps-phone`): node's budget
// is per FILE, and the two together took 201 s of its 300 under load.
//
// "Ran" is the bar's receipt (`bar.last()`, a fresh object for every
// command anything runs) moving; "wrote" is the database's change counter
// moving — SQLite's header, read without a CLI process. An
// IGNORED field that acts draws a double DASHED line (da-nmna0) — a dashed
// border with a dashed outline inside it — and counts as double here.
//
// Each tap is taken on the page as it was seeded: `solo` puts the layout,
// the line, the menus and the modals back, and empties the database only
// if the tap wrote, which is then seeded again. Twice over: the page idle,
// and with `occur ` on the line — which is what draws Send, the ×, and
// the fields under a pending verb.

import { before, after, describe, test } from 'node:test';
import assert from 'node:assert/strict';
import { changeCounter, sharedApp, startApp, stopApp } from './app.mjs';
import { toggleOption } from './layout.mjs';
import { FIXTURE, MOOD } from './fields.mjs';

/** Every shape of field, one of them ignored (`run`, a point). */
const SEED = [...FIXTURE, ...MOOD, 'occur run -t 0800', 'ignore run'];

/** Where the database's change counter stands: it moves on every write. */
const writes = (app) => changeCounter(app.page);

/**
 * Every button a reader can tap, keyed by where it is and what it says, and
 * whether it LOOKS as if it acts. `mark` also tags that key's button, so a
 * locator can click it.
 */
const survey = (page, { within, mark } = {}) =>
	page.evaluate(
		({ within, mark }) => {
			document.querySelector('[data-tap]')?.removeAttribute('data-tap');
			const seen = new Map();
			const out = [];
			for (const b of document.querySelectorAll(within ? `:is(${within}) button` : 'button')) {
				const box = b.getBoundingClientRect();
				const style = getComputedStyle(b);
				if (
					b.disabled ||
					b.tabIndex === -1 ||
					// The output section is the LAST command's result — whichever
					// ran, gone once a reseed runs quietly — so it is not the page.
					b.closest('dialog, [role="menu"], .prompt-bar .section.output') ||
					style.pointerEvents === 'none' ||
					!b.checkVisibility({ visibilityProperty: true }) ||
					box.width === 0 ||
					box.height === 0
				)
					continue;
				const where = b.closest('[data-title]')?.dataset.title ?? (b.closest('.prompt-bar') ? 'bar' : 'page');
				// Keyed by what the button IS, not by a label that tracks its state:
				// an entry's hid toggle names whichever entry ran last, and the
				// output caret says `open` or `close` as the section stands.
				const what = (b.dataset.field ?? b.dataset.command ?? b.getAttribute('aria-label') ?? b.textContent.trim())
					.replace(/^full id .*/, 'full id')
					.replace(/^(open|close) the output section$/, 'open/close the output section');
				const base = `${where}: ${what}`;
				const n = (seen.get(base) ?? 0) + 1;
				seen.set(base, n);
				const key = n === 1 ? base : `${base} #${n}`;
				const double =
					style.borderTopStyle === 'double' || (style.borderTopStyle === 'dashed' && style.outlineStyle === 'dashed');
				if (key === mark) b.dataset.tap = '';
				out.push({ key, double, kind: b.dataset.act ?? null });
			}
			return out;
		},
		{ within, mark }
	);

/** The layout world as the page shows it: what a tap may have moved. */
const world = (page) =>
	page.evaluate(() => JSON.stringify([window.datum.layout.current(), localStorage.getItem('datum.layout.root.v1')]));

/**
 * The page as seeded, with `line` typed on it. `home` closes what a tap
 * opened; `solo` is taken only when the tap moved the layout world, and the
 * database emptied and seeded again only when it wrote — each is seconds,
 * and most taps do neither.
 */
async function reset(app, line) {
	const { page } = app;
	await app.home();
	// The ⓘ's card, which Escape closes (Beta.svelte), and a maximize, which
	// is view state no `solo` puts back (maximize.ts).
	if ((await page.locator('.beta-hint').count()) > 0) await page.keyboard.press('Escape');
	const big = page.locator('button.maximize[aria-label^="restore"]');
	if ((await big.count()) > 0) await big.first().click();
	// A bar a tap drilled into — a field's states, a command's subcommands
	// or flags — backed out of, as a reader would.
	for (let back; (back = page.locator('.bar [data-back]')), (await back.count()) > 0; ) await back.first().click();
	// Once seeded, the layout goes back without the database: whether the
	// tap wrote is the counter's to say, below.
	if (app.seeded === undefined || (await world(page)) !== app.world) {
		await app.solo(undefined, { keepDb: app.seeded !== undefined });
		app.world = await world(page);
	}
	if ((await writes(app)) !== app.seeded) {
		await app.empty();
		// Through the quiet door, and one redraw over all of it. Line by line
		// and not a `db restore -`: a backup carries no undo trail, and
		// `command: undo` then writes nothing.
		for (const line of SEED) assert.equal((await app.runLine(line)).code, 0, line);
		await app.redraw();
		app.seeded = await writes(app);
	}
	// The session's «show ignored fields», which `solo` keeps — unless the
	// tap before closed or changed the pane it was set on. Read once the
	// bar is drawn, or a bar still mounting reads as the option off.
	await page.locator('.bar [data-field="coffee"]').waitFor();
	if ((await page.locator('.bar [data-field="run"]').count()) === 0) {
		const id = await page.locator('section.pane[data-title="fields"]').first().getAttribute('data-pane');
		await toggleOption(page, id, 'show ignored fields');
		await page.locator('.bar [data-field="run"]').waitFor();
	}
	if (line) {
		await page.locator('.prompt-bar input').fill(line);
		await page.locator('.prompt-bar .send').waitFor();
	}
	await app.idle();
}

/** Tap `key` on a freshly reset page and say what the tap did. */
async function tap(app, key, line, within) {
	const { page } = app;
	await reset(app, line);
	const found = (await survey(page, { within, mark: key })).find((b) => b.key === key);
	assert.ok(found, `${key}: not on the page after a reset`);
	// A second on, so a tap that logs does not collide with the seed's own `now`.
	await app.clock.forward(1000);
	await page.evaluate(() => (window.__lastRun = window.datum.bar.last()));
	await page.locator('[data-tap]').click();
	// A press dispatches, and the run starts a frame or two behind it.
	await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
	await app.idle();
	const ran = await page.evaluate(() => window.datum.bar.last() !== window.__lastRun);
	// `reset` left the counter at `seeded`.
	const wrote = (await writes(app)) !== app.seeded;
	return { ...found, acted: ran || wrote, ran, wrote };
}

/** Every button, tapped; the ones whose look and whose deed disagree, by label. */
async function tapAll(app, line, within) {
	await reset(app, line);
	const keys = (await survey(app.page, { within })).map((b) => b.key);
	const wrong = [];
	const acted = [];
	for (const key of keys) {
		const t = await tap(app, key, line, within);
		if (t.acted) acted.push(key);
		if (t.double !== t.acted)
			wrong.push(`${key} (${t.kind ?? 'no kind'}): ${t.double ? 'double, and did nothing' : `single, and ${t.wrote ? 'wrote' : 'ran'}`}`);
	}
	return { keys, acted, wrong };
}

/**
 * Every button at one screen, tapped: `must` is what its survey has to see,
 * and see act — a survey that found nothing that acts would pass for the
 * wrong reason.
 */
export function everyButton(screen, options, must) {
	before(startApp);
	after(stopApp);
	describe(`every button at the ${screen} size`, () => {
		const shared = sharedApp(options);

		test('idle: a double outline is exactly a tap that wrote or ran', async () => {
			const { keys, acted, wrong } = await tapAll(await shared());
			assert.deepEqual(wrong, []);
			for (const key of must) {
				assert.ok(keys.includes(key), `${key} not surveyed; saw ${keys.join(', ')}`);
				assert.ok(acted.includes(key), `${key} did nothing; acted: ${acted.join(', ')}`);
			}
		});

		test('with `occur ` on the line: the bar and the widgets it changes', async () => {
			const within = '.prompt-bar, section.pane[data-title="fields"], section.pane[data-title="command"]';
			const { keys, acted, wrong } = await tapAll(await shared(), 'occur ', within);
			assert.deepEqual(wrong, []);
			for (const key of ['bar: run the line', 'fields: coffee']) {
				assert.ok(keys.includes(key), `${key} not surveyed; saw ${keys.join(', ')}`);
				assert.ok(acted.includes(key), `${key} did nothing; acted: ${acted.join(', ')}`);
			}
		});
	});
}
