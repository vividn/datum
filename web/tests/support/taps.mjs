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
//
// And the one other mark (da-vteda): a trailing chevron `›` ⇔ the tap
// opened a modal — a `<dialog>`, a sheet, a confirm, or the browser's own
// (`prompt`, a file picker). The chevron is `[data-act='open']`'s alone
// (app.css), so a button that wears one says `open`, and one that opens a
// modal without saying so is named here.

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
const survey = (page, { within, mark, layer } = {}) =>
	page.evaluate(
		({ within, mark, layer }) => {
			document.querySelector('[data-tap]')?.removeAttribute('data-tap');
			const seen = new Map();
			const out = [];
			// A `layer` survey is of the dialog or menu `open` tagged, and of
			// nothing over or under it; a page survey is of no layer at all.
			const root = layer ? document.querySelector('[data-layer]') : null;
			for (const b of (root ?? document).querySelectorAll(within ? `:is(${within}) button` : 'button')) {
				const box = b.getBoundingClientRect();
				const style = getComputedStyle(b);
				if (
					b.disabled ||
					b.tabIndex === -1 ||
					// The output section is the LAST command's result — whichever
					// ran, gone once a reseed runs quietly — so it is not the page.
					b.closest('dialog, [role="dialog"], [role="menu"]') !== root ||
					b.closest('.prompt-bar .section.output') ||
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
				const chevron = getComputedStyle(b, '::after').content.includes('›');
				if (key === mark) b.dataset.tap = '';
				out.push({ key, double, chevron, kind: b.dataset.act ?? null });
			}
			return out;
		},
		{ within, mark, layer }
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
	// Help armed (`help.ts`) explains the next tap instead of acting it:
	// Escape disarms it.
	if ((await page.locator('.command.help.armed').count()) > 0) await page.keyboard.press('Escape');
	// A widget option a tap set holds for the session, which `solo` keeps:
	// forgotten, and the fields widget's own put back below.
	const session = () => page.evaluate(() => JSON.stringify(window.datum.layout.session()));
	if (app.session !== undefined && (await session()) !== app.session)
		await page.evaluate(() => window.datum.layout.forgetSession());
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
	app.session = await session();
	// What a layer's buttons need in `_internal` (`everyLayer`'s plan), put
	// back whenever something there moved — a layout's save as much as a
	// plan's delete.
	if (app.internal !== undefined && (await changeCounter(page, '_internal')) !== app.internal) {
		for (const argv of INTERNAL) await app.run(argv);
		app.internal = await changeCounter(page, '_internal');
	}
	if (line) {
		await page.locator('.prompt-bar input').fill(line);
		await page.locator('.prompt-bar .send').waitFor();
	}
	await app.idle();
}

/**
 * Watch for the modal a tap may open — a `<dialog>`, or the browser's own
 * (`prompt`, a file picker), which no survey of the page can see — and
 * return what says, once the tap has settled, whether one opened. The save
 * picker is the one Playwright does not report, so a stand-in takes its
 * place: it says it opened, and is cancelled, as a reader may.
 */
async function watchModals(page) {
	const front = () => page.evaluate(() => window.datum.layout.current().root.tabs.at(window.datum.layout.current().root.active)?.id);
	const was = await front();
	await page.evaluate(() => {
		document.querySelectorAll('dialog[open]').forEach((d) => (d.seen = true));
		window.savePicked = false;
		window.showSaveFilePicker = async () => {
			window.savePicked = true;
			throw new DOMException('cancelled', 'AbortError');
		};
	});
	let native = false;
	const dialog = (d) => ((native = true), void d.dismiss());
	const chooser = () => (native = true);
	page.on('dialog', dialog).on('filechooser', chooser);
	return async () => {
		page.off('dialog', dialog).off('filechooser', chooser);
		// A PLACE (`openPlace`, da-6du3o) opens as a modal or as a transient
		// tab by the screen (da-jozgv); either way the tap opened it.
		const place = await page.evaluate((was) => {
			const { root } = window.datum.layout.current();
			const tab = root.tabs[root.active];
			return tab?.id !== was && tab?.layout !== undefined && window.datum.shell.isPreview(tab.layout);
		}, was);
		return (
			native ||
			place ||
			(await page.evaluate(() => window.savePicked || [...document.querySelectorAll('dialog[open]')].some((d) => !d.seen)))
		);
	};
}

/**
 * Where a tap's chevron disagrees with its kind or with what it opened:
 * `null` where all three agree. The kind on its own too, so a `›` some
 * stray rule drew on a button that opens nothing is named as well.
 */
const chevronWrong = (t) =>
	t.chevron !== (t.kind === 'open')
		? `${t.chevron ? 'wears a ›' : 'wears no ›'} as ${t.kind ?? 'no kind'}`
		: t.chevron === t.modal
			? null
			: t.modal
				? `opened a modal, and wears no › (${t.kind ?? 'no kind'})`
				: `wears a › (${t.kind ?? 'no kind'}), and opened no modal`;

/** Tap `key` on a freshly reset page and say what the tap did. */
async function tap(app, key, line, within) {
	const { page } = app;
	await reset(app, line);
	const found = (await survey(page, { within, mark: key })).find((b) => b.key === key);
	assert.ok(found, `${key}: not on the page after a reset`);
	// A second on, so a tap that logs does not collide with the seed's own `now`.
	await app.clock.forward(1000);
	await page.evaluate(() => (window.__lastRun = window.datum.bar.last()));
	const opened = await watchModals(page);
	await page.locator('[data-tap]').click();
	// A press dispatches, and the run starts a frame or two behind it.
	await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
	await app.idle();
	const modal = await opened();
	const ran = await page.evaluate(() => window.datum.bar.last() !== window.__lastRun);
	// `reset` left the counter at `seeded`.
	const wrote = (await writes(app)) !== app.seeded;
	return { ...found, modal, acted: ran || wrote, ran, wrote };
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
		if (chevronWrong(t)) wrong.push(`${key}: ${chevronWrong(t)}`);
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

		test('idle: a double outline is exactly a tap that wrote or ran, a › one that opened a modal', async () => {
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

// ---- the layers: sheets, dialogs and menus (da-juku7) ----------------------
//
// The page survey skips every button inside a dialog or a menu, which is how
// «Delete plan» deleted at once from a button drawn as display (#241). So
// `everyLayer` taps every button on the page that is not drawn as acting,
// and follows each one that OPENS a layer — a `<dialog>`, a `role=dialog`
// card, a `role=menu` — surveying the buttons within it under the page's
// rule, and deeper where a button there opens another: three layers at
// most, which is the gear's menu, its Backups sheet and the confirm behind
// «Delete plan». "Wrote" counts `_internal` too, where plans and saved
// layouts live.
//
// A layer is surveyed once per kind: two openers whose layers hold the same
// buttons (every pane's `▾`, say) are one survey, named for the first.

/** A layer: a modal, a card that says it is a dialog, or a menu. */
const LAYERS = 'dialog[open], [role="dialog"], [role="menu"]';

/**
 * A backup plan, so the Backups sheet has one to delete — on `memory`, the
 * tab's own bucket, which needs no network (`backups.ts`). Put back by
 * `reset` whenever `_internal` moves. It backs up no database: one that it
 * did would be overdue, and the page's minute check (`backups.ts`) backed
 * it up whenever a walk ran past a minute — a `Restore` button, and its
 * confirm, turning up in the sheet halfway through.
 */
const PLAN = { name: 'taps', kind: 'memory', config: { bucket: 'taps' }, dbs: [], every: 3600, slots: null };
const INTERNAL = [
	['--db', '_internal', 'delete', 'backup_plan:taps'],
	['--db', '_internal', 'occur', 'backup_plan', '--id', '%name', '-k', 'name="taps"', '-k', `plan=${JSON.stringify(PLAN)}`]
];

/**
 * The buttons in a layer this walk does not tap, by path, each with why:
 * a deed neither counter can see, or one that needs what a walk has not got.
 * Each must be met, so the list cannot outlive its buttons.
 */
const SKIP = {
	'page: settings › page: Backups › backups: Back up now': "writes the plan's destination, the tab's memory bucket, which no counter reads",
	'page: settings › page: Backups › backups: Test connection': "needs the network, an S3 endpoint (s3.test.mjs stubs it)",
	'page: settings › page: Backups › backups: Add plan': 'acts on a filled form only, which backups.test.mjs fills',
	'page: settings › page: Reset default layouts › page: reset':
		'writes over a saved layout of a name datum ships, and the seeded page has saved none'
};

/**
 * The layers a walk opens but does not follow, by path, each with why: the
 * same buttons as a layer another share walks, which a single walk would
 * have surveyed once. Each must be met, as `SKIP`'s.
 */
const ELSEWHERE = {
	'page: manage the layout › page: Change Widget':
		"the widget chooser, whose buttons a pane's «add a tab» holds too (act-taps-dialogs-pane); on the root it changes a widget `solo` does not put back"
};

/**
 * Tap `key` — on the page, or in the layer tagged `data-layer` — and tag
 * the layer that tap opened, if it opened one: its buttons' keys, or null.
 */
async function press(app, key, inLayer) {
	const { page } = app;
	await page.evaluate((sel) => document.querySelectorAll(sel).forEach((l) => (l.dataset.under = '')), LAYERS);
	// A layer can fill in after an await `idle()` cannot see — the Backups
	// sheet walks OPFS for its databases before it asks for the plans that
	// draw «Delete plan» — so the button is looked for until it turns up or
	// five seconds have passed.
	let found;
	for (const end = Date.now() + 5000; ; await new Promise((r) => setTimeout(r, 100))) {
		found = (await survey(page, { mark: key, layer: inLayer })).find((b) => b.key === key);
		if (found || Date.now() > end) break;
	}
	assert.ok(found, `${key}: not there after a reset`);
	const modal = await watchModals(page);
	await page.locator('[data-tap]').click();
	await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
	await app.idle();
	found.modal = await modal();
	const opened = await page.evaluate((sel) => {
		const fresh = [...document.querySelectorAll(sel)].filter((l) => !('under' in l.dataset));
		if (fresh.length === 0) return false;
		document.querySelector('[data-layer]')?.removeAttribute('data-layer');
		fresh.at(-1).dataset.layer = '';
		return true;
	}, LAYERS);
	return { ...found, opened: opened ? (await survey(page, { layer: true })).map((b) => b.key) : null };
}

/** The seeded page, with the layers `path`'s taps open, open. */
async function openPath(app, path) {
	await reset(app);
	for (const [i, key] of path.entries())
		assert.ok((await press(app, key, i > 0)).opened, `${path.slice(0, i + 1).join(' › ')}: opened nothing`);
}

/** Tap `key` in the layer `path` opens, and say what the tap did. */
async function tapIn(app, path, key) {
	const { page } = app;
	await openPath(app, path);
	await app.clock.forward(1000);
	await page.evaluate(() => (window.__lastRun = window.datum.bar.last()));
	const before = [await writes(app), await changeCounter(page, '_internal')];
	const t = await press(app, key, path.length > 0);
	const ran = await page.evaluate(() => window.datum.bar.last() !== window.__lastRun);
	const wrote = (await writes(app)) !== before[0] || (await changeCounter(page, '_internal')) !== before[1];
	return { ...t, acted: ran || wrote, ran, wrote };
}

/**
 * The page's openers, in four shares that together are all of them, one
 * file each: node's budget is per FILE, and the whole walk took 147 s on an
 * idle box — more than 300 on CI's lane. What costs is the menus that
 * reshape the layout, since every split, tab or layout they make is a `solo`
 * to put back: the root bar's (`layout`) and a pane's (`pane`) are a share
 * each. A widget's own options menu (`menu`) is a share too, since the rest
 * alone reached the cap on a loaded lane (da-4j2hw), and every other layer
 * is `rest`.
 */
const ROOT_LAYOUT = /^page: (open a layout|manage the layout)$/;
const PANE_LAYOUT = /: (add a tab to|manage) the .* pane$/;
const PANE_MENU = /: menu of the .* pane$/;
export const SHARES = {
	layout: (key) => ROOT_LAYOUT.test(key),
	pane: (key) => PANE_LAYOUT.test(key),
	menu: (key) => PANE_MENU.test(key),
	rest: (key) => !ROOT_LAYOUT.test(key) && !PANE_LAYOUT.test(key) && !PANE_MENU.test(key)
};

/**
 * Every layer a tap on an opener in `share` opens from the page, and each
 * layer that opens from those: `{ layers: [{ path, keys }], wrong, skipped }`,
 * `wrong` naming each button in a layer whose outline and deed disagree.
 */
async function walk(app, share, depth = 3) {
	const layers = [];
	const wrong = [];
	const skipped = [];
	const seen = new Set();
	const visit = async (path) => {
		await openPath(app, path);
		const buttons = await survey(app.page, { layer: path.length > 0 });
		if (path.length > 0) layers.push({ path, keys: buttons.map((b) => b.key) });
		const next = [];
		for (const b of buttons) {
			// A double on the page acts, which the page's own survey holds.
			if (path.length === 0 && (b.double || !share(b.key))) continue;
			const named = [...path, b.key].join(' › ');
			if (SKIP[named]) {
				skipped.push(named);
				continue;
			}
			const t = await tapIn(app, path, b.key);
			if (path.length > 0 && t.double !== t.acted)
				wrong.push(
					`${named} (${t.kind ?? 'no kind'}): ${t.double ? 'double, and did nothing' : `single, and ${t.wrote ? 'wrote' : 'ran'}`}`
				);
			if (chevronWrong(t)) wrong.push(`${named}: ${chevronWrong(t)}`);
			const kind = t.opened?.join('\n');
			if (kind !== undefined && ELSEWHERE[named]) skipped.push(named);
			else if (kind !== undefined && !t.acted && path.length < depth && !seen.has(kind)) {
				seen.add(kind);
				next.push(b.key);
			}
		}
		for (const key of next) await visit([...path, key]);
	};
	await visit([]);
	return { layers, wrong, skipped };
}

/**
 * Every layer at one screen that the openers in `share` (`SHARES`) open,
 * and every button in it, tapped: `must` is the layer paths (joined by
 * ` › `) its walk has to reach — a walk that opened nothing would pass for
 * the wrong reason.
 */
export function everyLayer(screen, options, share, must) {
	before(startApp);
	after(stopApp);
	describe(`every button in a layer at the ${screen} size, ${share} openers`, () => {
		const shared = sharedApp(options);

		test('a double outline is exactly a tap that wrote or ran, a › one that opened a modal, in every sheet, dialog and menu', async () => {
			const app = await shared();
			app.internal = -1;
			const { layers, wrong, skipped } = await walk(app, SHARES[share]);
			for (const { path, keys } of layers) console.log(`# ${path.join(' › ')}: ${keys.length} buttons`);
			for (const path of skipped) console.log(`# skipped ${path}: ${SKIP[path] ?? ELSEWHERE[path]}`);
			assert.deepEqual(wrong, []);
			const mine = [...Object.keys(SKIP), ...Object.keys(ELSEWHERE)].filter((path) => SHARES[share](path.split(' › ')[0]));
			assert.deepEqual(mine.filter((path) => !skipped.includes(path)), [], 'skipped, and never met');
			const reached = layers.map((l) => l.path.join(' › '));
			for (const path of must) assert.ok(reached.includes(path), `${path} not reached; reached ${reached.join(' | ')}`);
		});
	});
}
