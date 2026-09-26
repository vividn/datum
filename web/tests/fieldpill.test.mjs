// The FIELD PILL (da-8810n): the gesture that mints it, the colour it is
// painted, the argv it lands in, and the widgets that narrow to it.
//
// vividn, 2026-09-21: «Create a field pill for this field and let's filter
// widgets accordingly. So stats widget would filter to this field. tail
// widget too. command widget would filter to commands that take fields.»
//
// The grammar's own rules — which commands survive a field pill, and where
// its word goes in an argv — are `selection-pure.test.mjs`, in node. This
// file is what a reader does and sees, so the page is written down before
// the app loads: the fields widget to hold from, the commands widget to be
// narrowed, a stats widget, a tail pane, a day strip, and a REPL for the
// echoes.
//
// Budget (da-lt0k, da-buqa): node's `--test-timeout` is per TEST and a FILE
// is a test, so the runner's 300 s is this file's whole budget —
// `before(startApp)`, the Chromium it launches, and every test in it.
// Measured alone on the 24-core build box: 84 s for the nine.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { seedLines, sharedApp, startApp, stopApp, withApp as withAnyApp } from './support/app.mjs';
import { DESKTOP, along, draft, pane, widgetPane } from './support/layout.mjs';
import { listed } from './support/rows.mjs';

before(startApp);
after(stopApp);

/**
 * Every surface the pill touches, on one page.
 *
 * `command` and `widgetbar` because the pill is minted in one and filters
 * the other; `stats` and `tail` because those are the two widgets vividn
 * named; `daystrip` because a picture cannot filter and outlines instead;
 * `repl` because a press leaves its echo there.
 */
const PAGE = JSON.stringify(
	along(
		'column',
		along('row', widgetPane('strip', 'daystrip'), widgetPane('stats', 'stats')).root,
		along('row', widgetPane('tail', 'tail'), widgetPane('repl', 'repl')).root,
		widgetPane('cmd', 'command'),
		widgetPane('bar', 'widgetbar')
	)
);

const withApp = (fn, options = {}) =>
	withAnyApp(fn, {
		...DESKTOP,
		...options,
		prepare: async (context) => {
			await context.addInitScript(draft, PAGE);
		}
	});

// ONE APP for the tests that stay on this page at 1280x900 (da-6b3dj,
// sharedApp in support/app.mjs): `solo` empties the database and the pills,
// and opens `main` from PAGE as a draft, as `prepare` does at boot. What
// reloads, counts a load's writes, or is the finger's screen keeps its own.
const shared = sharedApp(DESKTOP);
const onPage = (fn) => shared.solo(fn, PAGE);

/**
 * Two fields with entries of their own, and colours the CLI resolves
 * rather than hashes — so «painted in the field's colour» is a claim about
 * a value this file wrote, not about md5.
 */
const SEED = [
	'spec set coffee --verb occur --color #ffa500',
	'spec set mood --verb switch --state happy --color #0000ff',
	'occur coffee',
	'switch mood happy'
];

const ORANGE = 'rgb(255, 165, 0)';
const BLUE = 'rgb(0, 0, 255)';

/**
 * Seed, and wait until the fields widget has drawn both buttons AND every
 * refresh the seed set off has landed (da-ij6iy). A late one reorders the
 * bar — `mood` goes ahead of `coffee` — so a gesture aimed at a button that
 * merely EXISTS can land on its neighbour, or on a node already replaced.
 */
async function ready(app) {
	await seedLines(app, SEED);
	// A second on, as a real clock would be: `stats` counts up to NOW and
	// not an entry AT it, and the clock is held (da-ydao).
	await app.clock.forward(1000);
	await fieldsBar(app.page).locator('[data-field="coffee"]').waitFor();
	await fieldsBar(app.page).locator('[data-field="mood"]').waitFor();
}

const fieldsBar = (page) => pane(page, 'bar').locator('.bar');
const chip = (page, field) => fieldsBar(page).locator(`[data-field="${field}"]`);
const fieldPill = (page) => page.locator('.prompt-bar .pill.field');

/**
 * The same page, with every write to the pills key counted (da-1py00).
 *
 * `Storage.prototype` is patched in an init script, so the count is re-made
 * on each load and holds only what that load did — which is the whole
 * point: the claim is about what a LOAD costs.
 */
const withSpyingApp = (fn) =>
	withAnyApp(fn, {
		...DESKTOP,
		prepare: async (context) => {
			await context.addInitScript(draft, PAGE);
			await context.addInitScript(() => {
				window.__pillWrites = [];
				const proto = Storage.prototype;
				const { setItem, removeItem } = proto;
				proto.setItem = function (key, value) {
					if (key === 'datum.bar.pills.v1') window.__pillWrites.push(['set', value]);
					return setItem.call(this, key, value);
				};
				proto.removeItem = function (key) {
					if (key === 'datum.bar.pills.v1') window.__pillWrites.push(['remove']);
					return removeItem.call(this, key);
				};
			});
		}
	});

/** What this load has written to the pills key, in order. */
const writes = (page) => page.evaluate(() => window.__pillWrites);

/** Hold a field: the gesture that mints the pill (right click is one of four). */
async function hold(page, field) {
	await chip(page, field).click({ button: 'right' });
}

/**
 * Hold a field down for `HOLD_MS` and let go — the long press itself, over
 * real milliseconds, because the widget's hold is a TIMER and a synthesised
 * event pair would be a test of the test.
 *
 * `finger` goes through CDP's `Input.dispatchTouchEvent`: Playwright's
 * touchscreen only TAPS, and `page.mouse` on a touch-capable context is
 * still a mouse — so a coarse arm driven by the mouse would be the fine arm
 * again under another name, and would prove nothing about a finger.
 */
async function longPress(page, field, { finger = false } = {}) {
	const at = chip(page, field);
	await at.scrollIntoViewIfNeeded();
	const box = await at.boundingBox();
	const [x, y] = [box.x + box.width / 2, box.y + box.height / 2];
	if (!finger) {
		await page.mouse.move(x, y);
		await page.mouse.down();
		await page.waitForTimeout(700);
		await page.mouse.up();
		return;
	}
	const cdp = await page.context().newCDPSession(page);
	try {
		await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x, y }] });
		await page.waitForTimeout(700);
		await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
	} finally {
		await cdp.detach();
	}
}

/** The commands the commands widget is drawing, in its order. */
const commands = async (page) => (await listed(pane(page, 'cmd').locator('.bar'), 'button[data-command]')).map((b) => b.command);

describe('the gesture, and the pill it mints', () => {
	test('a hold mints the field pill, in the field’s own colour', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await ready(app);
			assert.equal(await fieldPill(page).count(), 0, 'a pill before the gesture');

			await hold(page, 'coffee');
			await fieldPill(page).waitFor();
			// The pill's text IS the argv (da-ui4i): the bare field name, and
			// the × is the mark beside it rather than part of what is sent.
			assert.equal(await fieldPill(page).locator('.label').innerText(), 'coffee');
			// vividn: "let's color them with the field color". The colour is
			// `fields --json`'s resolved one, which the seed above SET, so a
			// pill drawn in the accent instead says so here.
			assert.equal(
				await fieldPill(page).evaluate((el) => getComputedStyle(el).backgroundColor),
				ORANGE
			);
			// And the button says where the pill came from.
			assert.equal(await chip(page, 'coffee').getAttribute('data-pilled'), '');

			// ONE field at a time, as the time cursor holds one instant.
			await hold(page, 'mood');
			await page.waitForFunction(
				() => document.querySelector('.prompt-bar .pill.field')?.textContent.includes('mood')
			);
			assert.equal(await fieldPill(page).count(), 1, 'two field pills at once');
			assert.equal(
				await fieldPill(page).evaluate((el) => getComputedStyle(el).backgroundColor),
				BLUE
			);
			assert.equal(await chip(page, 'coffee').getAttribute('data-pilled'), null);
			assert.deepEqual(app.errors, []);
		});
	});

	// vividn named four gestures and the `-t` / `dur` / `note` panel had every
	// one of them, so da-8810n made the panel a step BEHIND the pill: hold
	// again and the bubbles opened. vividn then decided the other way — «mint
	// the pill, let's remove that -t/dur panel» — so the gesture has ONE
	// meaning at every count (da-e78x6). And then that it TOGGLES (da-05oit):
	// «long pressing or right clicking on a field again after a pill is
	// added should remove the pill/filter».
	test('shift+click mints it too, and the same gesture again takes it away', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await ready(app);

			await chip(page, 'coffee').click({ modifiers: ['Shift'] });
			await fieldPill(page).waitFor();
			// And nothing was logged: shift+click is the hold, not a press.
			assert.equal((await app.echoes()).at(-1), 'switch mood happy');

			await chip(page, 'coffee').click({ modifiers: ['Shift'] });
			// The gesture that used to open the panel: a beat to let one
			// appear, and then the claim that none did. Any floating surface,
			// not `.panel` alone — the bead removed the thing, so the assertion
			// must not be aimed at the one class name it went by.
			await page.waitForTimeout(200);
			assert.equal(
				await page.locator('.bar .panel, .bar .bubbles, .bar [aria-expanded="true"]').count(),
				0,
				'the hold opened something'
			);
			assert.equal(await fieldPill(page).count(), 0, 'the pill survived a second hold');
			assert.equal(await chip(page, 'coffee').getAttribute('data-pilled'), null);
			assert.equal((await app.echoes()).at(-1), 'switch mood happy', 'the second hold ran a command');
			assert.deepEqual(app.errors, []);
		});
	});

	// The long press is the other half of the same gesture, and the bead
	// names both pointers: «minting a pill by hold, on a coarse pointer and
	// a fine one». One body, run on each. 450 ms is `HOLD_MS`; 700 clears it
	// without racing it.
	for (const [pointer, options] of [
		['a fine pointer', {}],
		['a finger', { device: { isMobile: true, hasTouch: true }, viewport: { width: 390, height: 844 } }]
	]) {
		const finger = pointer === 'a finger';
		test(`a long press with ${pointer} mints, and a second takes it away`, async () => {
			// The finger is the one test on its screen: an app of its own.
			await (finger ? withApp : onPage)(async (app) => {
				const { page } = app;
				await ready(app);

				await longPress(page, 'coffee', { finger });
				// The mint itself, which is the positive arm: without it the
				// «nothing opened» below would be true of a gesture that never
				// landed on the chip at all.
				await fieldPill(page).waitFor();
				assert.equal(await fieldPill(page).locator('.label').innerText(), 'coffee', 'the press landed on another field');
				assert.equal(await chip(page, 'coffee').getAttribute('data-pilled'), '');

				await longPress(page, 'coffee', { finger });
				await page.waitForTimeout(200);
				assert.equal(
					await page.locator('.bar .panel, .bar .bubbles, .bar [aria-expanded="true"]').count(),
					0,
					'the second long press opened something'
				);
				assert.equal(await fieldPill(page).count(), 0, 'the second long press kept the pill');
				assert.equal((await app.echoes()).at(-1), 'switch mood happy', 'a long press logged');
				assert.deepEqual(app.errors, []);
			}, options);
		});
	}

	// The bead: «a field pill must be mintable and removable without a
	// pointer». The menu key raises `contextmenu`, which is the same door
	// the right click comes through, and the pill's × is a button in the
	// tab order.
	test('the keyboard mints it and the keyboard takes it away', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await ready(app);

			await chip(page, 'coffee').focus();
			await page.keyboard.press('ContextMenu');
			await fieldPill(page).waitFor();

			const remove = fieldPill(page).locator('button.remove');
			await remove.focus();
			await page.keyboard.press('Enter');
			await fieldPill(page).waitFor({ state: 'detached' });
			assert.deepEqual(app.errors, []);
		});
	});

	// «it behaves like every other pill»: unlocked, so a successful press
	// spends it and the filter is gone. Named here so it can be corrected
	// in a word if it turns out to be annoying in use.
	test('it is unlocked, so a successful command clears it', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await ready(app);
			await hold(page, 'coffee');
			await fieldPill(page).waitFor();

			await app.submit('tail');
			await fieldPill(page).waitFor({ state: 'detached' });
			assert.equal((await app.echoes()).at(-1), 'tail coffee');
			assert.deepEqual(app.errors, []);
		});
	});

	// THE SILENT ONE, at the seam rather than in the pure test: datum's line
	// is field-first, so `occur coffee` under a `mood` pill would log mood as
	// coffee's STATE. The pill is refused and stays loaded.
	test('a press that already names a field does not get the pill appended', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await ready(app);
			await hold(page, 'mood');
			await fieldPill(page).waitFor();

			const seen = await app.spyOnRunner();
			await chip(page, 'coffee').click();
			await page.waitForFunction(() => window.__datumSeen.some((a) => a[0] === 'occur'));
			assert.deepEqual((await seen()).find((a) => a[0] === 'occur'), [
				'occur',
				'coffee',
				'--show',
				'json'
			]);
			await app.idle();
			const logged = (await app.entries()).at(0);
			assert.deepEqual([logged.field, logged.state], ['coffee', null]);
			assert.equal(await fieldPill(page).count(), 1, 'a refused pill did not stay loaded');
			assert.deepEqual(app.errors, []);
		});
	});
});

describe('what narrows to it', () => {
	// The bead's trap: `fits` FAILS OPEN, so a `field` kind it had not been
	// taught would leave every command visible and the filter would do
	// nothing at all while looking as though it worked. The claim is the
	// HIDING, and that `spec` — the way to the spec editor — survives.
	test('the commands widget keeps the commands that name a field, and hides the rest', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await ready(app);
			const whole = await commands(page);
			for (const name of ['get', 'now', 'delete', 'grep']) assert.ok(whole.includes(name), name);

			await hold(page, 'coffee');
			await page.waitForFunction(
				() => !document.querySelector('[data-command="get"]')
			);
			const under = await commands(page);
			for (const name of ['occur', 'tail', 'stats', 'count', 'ids', 'spec'])
				assert.ok(under.includes(name), `${name} names a field and was hidden`);
			for (const name of ['get', 'now', 'delete', 'update', 'grep', 'dayview'])
				assert.ok(!under.includes(name), `${name} names no field and was not hidden`);
			// coffee is an OCCUR field (da-57j14): the other verbs are hidden.
			for (const name of ['start', 'end', 'switch', 'add'])
				assert.ok(!under.includes(name), `${name} does not fit an occur field and was not hidden`);
			// The order is the widget's usual one; the filter only removes.
			assert.deepEqual(under, whole.filter((name) => under.includes(name)));
			assert.deepEqual(app.errors, []);
		});
	});

	// da-57j14: the verbs are narrowed by what the FIELD takes — the same
	// rule the fields widget narrows by under a pending verb. mood is a
	// switch in a state, so it can be switched or ended, not occurred,
	// started or added; and taking the pill away brings the five back.
	test('a switch field leaves switch and end of the verbs, and every verb comes back without it', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await ready(app);
			const verbs = async () =>
				(await commands(page)).filter((name) => ['occur', 'start', 'end', 'switch', 'add'].includes(name));

			await hold(page, 'mood');
			await page.waitForFunction(() => !document.querySelector('[data-command="occur"]'));
			assert.deepEqual(await verbs(), ['end', 'switch']);

			await fieldPill(page).locator('button.remove').click();
			await page.waitForFunction(() => document.querySelector('[data-command="occur"]'));
			assert.deepEqual(await verbs(), ['occur', 'start', 'end', 'switch', 'add']);
			assert.deepEqual(app.errors, []);
		});
	});

	test('the stats widget narrows to that field, and widens back when the pill clears', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await ready(app);
			const stats = pane(page, 'stats');
			// Every field until something narrows it (da-ptuwr): a bare `stats`.
			await stats.locator('code.argv').waitFor();
			await page.waitForFunction(() => document.querySelector('.stats code.argv')?.textContent === 'stats');

			await hold(page, 'coffee');
			await page.waitForFunction(
				() => document.querySelector('.stats code.argv')?.textContent === 'stats coffee'
			);
			// And the body is that field's own, not the table of all of them.
			await page.waitForFunction(
				() => document.querySelector('.stats .body')?.textContent?.startsWith('coffee')
			);

			// A filter, not a write: it widens back to every field.
			await fieldPill(page).locator('button.remove').click();
			await page.waitForFunction(() => document.querySelector('.stats code.argv')?.textContent === 'stats');
			await page.waitForFunction(
				() => document.querySelector('.stats .body')?.textContent?.includes('all fields')
			);
			assert.deepEqual(app.errors, []);
		});
	});

	// da-05oit: the second hold is the filter's off switch, and a LOCKED
	// pill goes too — the reader asked for it by name. «Whole again» is read
	// off the widgets, not the pill: stats is bare `stats` and the tail
	// lists mood again.
	test('a second right click takes a locked pill away, and stats and the tail are whole again', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await ready(app);
			await hold(page, 'coffee');
			await page.waitForFunction(() => document.querySelector('.stats code.argv')?.textContent === 'stats coffee');
			await page.waitForFunction(() =>
				[...document.querySelectorAll('section[data-pane="tail"] tbody td.field')].every((td) => td.textContent.trim() === 'coffee')
			);
			await fieldPill(page).locator('button.lock').click();
			await page.waitForFunction(() => document.querySelector('.prompt-bar .pill.field .lock')?.ariaPressed === 'true');

			await hold(page, 'coffee');
			await fieldPill(page).waitFor({ state: 'detached' });
			assert.equal(await chip(page, 'coffee').getAttribute('data-pilled'), null);
			await page.waitForFunction(() => document.querySelector('.stats code.argv')?.textContent === 'stats');
			await page.waitForFunction(() =>
				[...document.querySelectorAll('section[data-pane="tail"] tbody td.field')].some((td) => td.textContent.trim() === 'mood')
			);
			assert.deepEqual(app.errors, []);
		});
	});

	test('the tail pane lists that field alone', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await ready(app);
			const rows = () =>
				pane(page, 'tail').evaluate((el) =>
					[...el.querySelectorAll('tbody tr')].map((tr) => tr.querySelector('td.field')?.textContent.trim())
				);
			await page.waitForFunction(() => document.querySelectorAll('section[data-pane="tail"] tbody tr').length > 1);
			assert.ok((await rows()).includes('mood'), 'the whole listing does not hold mood');

			await hold(page, 'coffee');
			await page.waitForFunction(
				() =>
					[...document.querySelectorAll('section[data-pane="tail"] tbody tr')].length > 0 &&
					[...document.querySelectorAll('section[data-pane="tail"] tbody td.field')].every(
						(td) => td.textContent.trim() === 'coffee'
					)
			);
			assert.deepEqual(app.errors, []);
		});
	});

	// A picture cannot filter, so it BRINGS OUT instead (vividn: "nothing too
	// intense, it must still be possible to see all the other data well").
	// The claim is both halves: the pilled field's marks carry the halo, and
	// nothing at all is done to the others.
	test('a picture outlines that field’s own marks and leaves every other field alone', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await ready(app);
			const groups = () =>
				pane(page, 'strip').evaluate((el) =>
					[...el.querySelectorAll('svg.svg .field[data-field]')].map((g) => ({
						field: g.dataset.field,
						pilled: g.hasAttribute('data-pilled'),
						filter: getComputedStyle(g).filter
					}))
				);
			await page.waitForFunction(
				() => document.querySelectorAll('section[data-pane="strip"] svg.svg .field[data-field]').length > 1
			);
			assert.deepEqual(
				[...new Set((await groups()).map((g) => g.filter))],
				['none'],
				'a group was already filtered before any pill'
			);

			await hold(page, 'coffee');
			await page.waitForFunction(
				() => document.querySelector('section[data-pane="strip"] .field[data-pilled]')
			);
			const marked = await groups();
			assert.ok(marked.length > 1, 'only one field is drawn — there is nothing to compare');
			for (const g of marked) {
				assert.equal(g.pilled, g.field === 'coffee', g.field);
				// The class is only wiring; the CLAIM is that the browser
				// really paints one differently and the rest not at all.
				assert.equal(g.filter === 'none', g.field !== 'coffee', `${g.field}: ${g.filter}`);
			}

			await fieldPill(page).locator('button.remove').click();
			await page.waitForFunction(
				() => !document.querySelector('section[data-pane="strip"] .field[data-pilled]')
			);
			assert.deepEqual(
				[...new Set((await groups()).map((g) => g.filter))],
				['none'],
				'the halo outlived the pill'
			);
			assert.deepEqual(app.errors, []);
		});
	});
});

// ---- A LOCKED PILL SURVIVES A RELOAD (da-1py00) -------------------------
//
// vividn, 2026-09-22, on whether a stats selection should outlive a reload:
// «Maybe if they are locked? Yeah, let's try that.» The two are one thing —
// the stats widget's selection IS the field pill (da-ptuwr removed its own
// selector) — so this is where the question is answered.
//
// ⚑ The claim is the FILTER and not the pill. A restoration that produced a
// pill which reads `coffee`, wears the field's colour and narrows nothing
// would pass every assertion about the pill itself, so each test here reads
// the stats widget's argv AND its body back.
describe('what survives a reload', () => {
	test('a LOCKED field pill comes back, and narrows stats exactly as a fresh one does', async () => {
		await withApp(async (app) => { // reloads
			const { page } = app;
			await ready(app);
			await hold(page, 'coffee');
			await fieldPill(page).waitFor();
			await page.waitForFunction(
				() => document.querySelector('.stats code.argv')?.textContent === 'stats coffee'
			);

			await fieldPill(page).locator('button.lock').click();
			await page.waitForFunction(
				() => document.querySelector('.prompt-bar .pill.field .lock')?.ariaPressed === 'true'
			);

			await app.reload();

			await fieldPill(page).waitFor();
			assert.equal(await fieldPill(page).locator('.label').innerText(), 'coffee');
			// Still locked, so the next press does not spend it either.
			assert.equal(await fieldPill(page).locator('button.lock').getAttribute('aria-pressed'), 'true');
			// The field's own colour came back with it, not the kind's accent.
			assert.equal(
				await fieldPill(page).evaluate((el) => getComputedStyle(el).backgroundColor),
				ORANGE
			);
			// THE POINT: it filters. The argv the widget runs, and the body
			// that came back from it.
			await page.waitForFunction(
				() => document.querySelector('.stats code.argv')?.textContent === 'stats coffee'
			);
			await page.waitForFunction(
				() => document.querySelector('.stats .body')?.textContent?.startsWith('coffee')
			);
			// And the widget that minted it knows it is pilled again, so a
			// second hold has somewhere to be aimed.
			assert.equal(await chip(page, 'coffee').getAttribute('data-pilled'), '');
			assert.deepEqual(app.errors, []);
		});
	});

	test('an unlocked one does not, and stats is every field again', async () => {
		await withApp(async (app) => { // reloads
			const { page } = app;
			await ready(app);
			await hold(page, 'coffee');
			await fieldPill(page).waitFor();
			await page.waitForFunction(
				() => document.querySelector('.stats code.argv')?.textContent === 'stats coffee'
			);

			await app.reload();

			await page.waitForFunction(
				() => document.querySelector('.stats code.argv')?.textContent === 'stats'
			);
			assert.equal(await fieldPill(page).count(), 0, 'an unlocked pill came back');
			await page.waitForFunction(
				() => document.querySelector('.stats .body')?.textContent?.includes('all fields')
			);
			assert.equal(await chip(page, 'coffee').getAttribute('data-pilled'), null);
			assert.deepEqual(app.errors, []);
		});
	});

	// The comment on `written` in bar.ts CLAIMS that an ordinary load — one
	// where storage already holds what this same code wrote — costs no
	// storage write at all, the subscriber's first call included (canary
	// minimax-m3-preview on PR #86). A claim in a comment that nothing
	// checks is a claim that rots, so this counts the writes.
	test('an ordinary load does not write to storage, and still restores the pill', async () => {
		await withSpyingApp(async (app) => { // an init script of its own, and reloads
			const { page } = app;
			await ready(app);
			await hold(page, 'coffee');
			await fieldPill(page).waitFor();
			await fieldPill(page).locator('button.lock').click();
			await page.waitForFunction(
				() => document.querySelector('.prompt-bar .pill.field .lock')?.ariaPressed === 'true'
			);
			// The lock itself IS a change, so it writes: the counter works,
			// and what it wrote is the pill. Read back PARSED rather than as
			// the exact string (canary minimax-m3-preview on PR #86): a
			// refactor that reorders the `KeptPill` map in both places keeps
			// the behaviour and would fail a string compare. Nothing is lost
			// by it — what pins the round-trip is the count after the reload
			// below, which can only be zero if the string re-derived at load
			// is byte-identical to the one written here, in whatever order.
			const written = await writes(page);
			assert.equal(written.length, 1, 'the lock did not write exactly once');
			assert.equal(written[0][0], 'set');
			assert.deepEqual(JSON.parse(written[0][1]), [
				{ id: 'field', kind: 'field', text: 'coffee', color: '#ffa500' }
			]);

			await app.reload();

			// The pill came back and narrows, exactly as the test above says.
			await fieldPill(page).waitFor();
			await page.waitForFunction(
				() => document.querySelector('.stats code.argv')?.textContent === 'stats coffee'
			);
			// And this load wrote nothing: the counter is re-made by the init
			// script on every load, so it holds only what THIS load did.
			assert.deepEqual(await writes(page), [], 'the load rewrote what it had just read');
			assert.deepEqual(app.errors, []);
		});
	});

	// Locking is not the only way out: a locked pill taken away stays away,
	// which is the half a store that only ever WROTE would get wrong.
	test('a locked pill removed before the reload does not come back', async () => {
		await withApp(async (app) => { // reloads
			const { page } = app;
			await ready(app);
			await hold(page, 'coffee');
			await fieldPill(page).waitFor();
			await fieldPill(page).locator('button.lock').click();
			await page.waitForFunction(
				() => document.querySelector('.prompt-bar .pill.field .lock')?.ariaPressed === 'true'
			);
			await fieldPill(page).locator('button.remove').click();
			await fieldPill(page).waitFor({ state: 'detached' });

			await app.reload();

			await page.waitForFunction(
				() => document.querySelector('.stats code.argv')?.textContent === 'stats'
			);
			assert.equal(await fieldPill(page).count(), 0, 'a removed pill came back');
			assert.deepEqual(app.errors, []);
		});
	});
});

// da-ybai9. vividn, 2026-09-23: «long pressing a field adds a pill, yes, and
// then tapping the spec command should open up the spec modal immediately,
// no need for sub commands or pressing enter. It should just work.» Two
// gestures with a settle between them: the long press MOUNTS the pill, and
// the tap is only driven once the pill is drawn.
describe('`spec` under the pill', () => {
	const MODAL = 'dialog.spec';
	const specButton = (page) => pane(page, 'cmd').locator('[data-command="spec"]');
	const line = (page) => page.locator('.prompt-bar input').first();

	test('long press, tap `spec`: the modal is open on that field, and the pill is spent', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await ready(app);
			await longPress(page, 'mood');
			await fieldPill(page).waitFor();

			await specButton(page).click();
			await page.locator(`${MODAL}[aria-label="the spec of mood"]`).waitFor();
			assert.equal(await page.locator(`${MODAL} h2`).innerText(), 'mood');
			assert.equal(await page.locator(`${MODAL} [data-verb="switch"]`).getAttribute('aria-checked'), 'true');
			assert.equal(await fieldPill(page).count(), 0, 'the pill outlived the tap');
			assert.equal(await line(page).inputValue(), '', 'the tap wrote a line as well');

			// And what it saves is the one `spec set`, with no pill after it.
			await page.locator(`${MODAL} button[type="submit"]`).click();
			await page.locator(MODAL).waitFor({ state: 'detached' });
			await app.idle();
			assert.match((await app.echoes()).at(-1), /^spec set mood --from-json /);
			assert.deepEqual(app.errors, []);
		});
	});

	test('a field with no spec opens on the verb question, its name already in', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await ready(app);
			// Logging declares a field, so its spec is taken away after.
			await app.submit('occur tea');
			await app.submit('spec remove tea');
			assert.match((await app.run(['spec', 'show', 'tea'])).stdout, /No spec defined/, 'tea has a spec to edit');
			await chip(page, 'tea').waitFor();
			await longPress(page, 'tea');
			await fieldPill(page).waitFor();

			await specButton(page).click();
			await page.locator(`${MODAL}[aria-label="track a field"]`).waitFor();
			assert.equal(await page.locator(`${MODAL} input.field`).inputValue(), 'tea');
			assert.equal(await page.locator(`${MODAL} [role="radio"][aria-checked="true"]`).count(), 0);
			assert.equal(await fieldPill(page).count(), 0, 'the pill outlived the tap');
			assert.deepEqual(app.errors, []);
		});
	});

	// With no pill there is no subject, so `spec` opens its subcommands
	// (da-803mk), a tap on one writes it on the line, and nothing opens.
	test('with no pill, `spec` opens its subcommands and `show` writes `spec show `', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await ready(app);
			await specButton(page).click();
			await pane(page, 'cmd').locator('[data-command="spec show"]').click();
			await page.waitForFunction(() => document.querySelector('.prompt-bar input')?.value === 'spec show ');
			assert.equal(await page.locator(MODAL).count(), 0);
			assert.deepEqual(app.errors, []);
		});
	});
});

// da-tu30m. vividn: «They pill a field, which then gives them a scrollable
// tail of that field, then they might tap a particular row in the tail to
// select that hid … So we need to somehow maintain the field filter, but
// have active in the input bar only the hid». The field is CONTEXT then:
// it narrows the panes and is in no command. The rules are
// `selection-pure.test.mjs`; this is the reader's flow through them.
describe('a field pill under a selected hid is context', () => {
	const PROMPT = '.prompt-bar form.line > .prompt';
	const BOX = '.prompt-bar input';
	const hidPill = (page) => page.locator('.prompt-bar .pill.hid');
	/** The tail lists coffee and nothing else. */
	const onlyCoffee = () => {
		const cells = [...document.querySelectorAll('section[data-pane="tail"] tbody td.field')];
		return cells.length > 0 && cells.every((td) => td.textContent.trim() === 'coffee');
	};

	/** Pill coffee, and select a row of the tail it narrowed: the row's hid. */
	async function pillThenSelect(app) {
		const { page } = app;
		await ready(app);
		await hold(page, 'coffee');
		await page.waitForFunction(onlyCoffee);
		const row = pane(page, 'tail').locator('tbody tr[data-hid]').first();
		const hid = await row.getAttribute('data-hid');
		await row.click();
		await hidPill(page).waitFor();
		return hid;
	}

	test('the tail stays narrowed, the field stands between the ❯ and the box, and the hid’s commands come back', async () => {
		await onPage(async (app) => {
			const { page } = app;
			const hid = await pillThenSelect(app);

			await fieldPill(page).and(page.locator('.context')).waitFor();
			const [field, prompt, box] = await Promise.all(
				[fieldPill(page), page.locator(PROMPT), page.locator(BOX)].map((l) => l.boundingBox())
			);
			assert.ok(field.x >= prompt.x + prompt.width, 'the context is not right of the ❯');
			assert.ok(field.x + field.width <= box.x, 'the context is not left of the box');
			assert.ok((await hidPill(page).boundingBox()).x >= box.x, 'the hid is not in the line');
			assert.ok(await page.evaluate(onlyCoffee), 'the tail widened');
			// Outlined, not filled: the reader tells the two roles apart.
			assert.equal(await fieldPill(page).evaluate((el) => getComputedStyle(el).backgroundColor), 'rgba(0, 0, 0, 0)');

			await page.waitForFunction(() => document.querySelector('[data-command="get"]'));
			const under = await commands(page);
			for (const name of ['get', 'delete', 'retime', 'history'])
				assert.ok(under.includes(name), `${name} is hidden under a field and an hid`);

			// A press carries no field: the echo is the hid's command alone.
			const echoes = await page.evaluate(() => document.querySelectorAll('.repl .echo').length);
			await pane(page, 'cmd').locator('button[data-command="get"]').click();
			await page.waitForFunction((n) => document.querySelectorAll('.repl .echo').length > n, echoes);
			await app.idle();
			assert.equal((await app.echoes()).at(-1), `get ${hid}`);
			// The hid was spent; the field was not, and is an argument again.
			await hidPill(page).waitFor({ state: 'detached' });
			assert.equal(await fieldPill(page).count(), 1, 'the press spent the context');
			assert.equal(await fieldPill(page).and(page.locator('.context')).count(), 0);
			assert.deepEqual(app.errors, []);
		});
	});

	// canary kimi-k3 on PR #180: a context pill draws no lock button, so
	// its ring is the only sign a locked field will outlive a press.
	test('a locked field keeps showing its lock while it is context', async () => {
		await onPage(async (app) => {
			const { page } = app;
			const ring = () => fieldPill(page).evaluate((el) => getComputedStyle(el).boxShadow);
			await pillThenSelect(app);
			await fieldPill(page).and(page.locator('.context')).waitFor();
			assert.match(await ring(), / 0px 0px 0px 1px inset$/, 'unlocked context');

			await page.evaluate(() =>
				window.datum.bar.setPills(window.datum.bar.pills().map((p) => (p.kind === 'field' ? { ...p, locked: true } : p)))
			);
			await fieldPill(page).and(page.locator('.context.locked')).waitFor();
			assert.match(await ring(), / 0px 0px 0px 2px inset$/, 'a locked context pill looks unlocked');
			assert.deepEqual(app.errors, []);
		});
	});

	// da-05oit: «Only that field's pill goes. Other pills, and the typed
	// line, stay.»
	test('a second hold takes the field pill alone: the hid and the typed line stay', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await pillThenSelect(app);
			await page.locator(BOX).fill('hello');

			await hold(page, 'coffee');
			await fieldPill(page).waitFor({ state: 'detached' });
			assert.equal(await hidPill(page).count(), 1, 'the hid pill went with the field');
			assert.equal(await page.locator(BOX).inputValue(), 'hello', 'the typed line went with the field');
			assert.deepEqual(app.errors, []);
		});
	});

	test('Escape drops the hid, and the field is an argument again; then the field', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await pillThenSelect(app);
			await page.locator(BOX).focus();

			await page.keyboard.press('Escape');
			await hidPill(page).waitFor({ state: 'detached' });
			assert.equal(await fieldPill(page).count(), 1, 'the first Escape took the field too');
			await page.waitForFunction(() => !document.querySelector('.prompt-bar .pill.field.context'));
			const [field, box] = await Promise.all([fieldPill(page), page.locator(BOX)].map((l) => l.boundingBox()));
			assert.ok(field.x >= box.x, 'the argument field is still before the box');

			await page.keyboard.press('Escape');
			await fieldPill(page).waitFor({ state: 'detached' });
			assert.deepEqual(app.errors, []);
		});
	});
});
