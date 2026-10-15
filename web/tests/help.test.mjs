// The Help button (da-vjwid): press it, then a command, a widget or a pill,
// and that thing is explained instead of acted on; press it again for
// general help. The answer lands in the Help PLACE (da-z955f): a Help pane
// in view, else the place `openPlace` hosts — a transient tab beside this
// one on a phone, which back closes, a modal on a desktop (da-jozgv).
// `$lib/help.ts`, `layout/HelpWidget.svelte`.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, seedLines } from './support/app.mjs';
import { DESKTOP, PHONE, along, atRest, widgetPane } from './support/layout.mjs';
import { listed } from './support/rows.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const PAGE = JSON.stringify(
	along('column', widgetPane('repl', 'repl'), widgetPane('cmd', 'command'), widgetPane('bar', 'widgetbar'))
);

// On a phone, where a place is a tab; the desktop's modal is the last test.
const shared = sharedApp({ ...PHONE });
const desktop = sharedApp({ ...DESKTOP });

const onPage = (fn, page = PAGE) =>
	shared.solo(async (app) => {
		// A test that failed armed must not leave the next one armed.
		await app.page.keyboard.press('Escape');
		return fn(app);
	}, page);

const help = (page) => page.locator('.bar.commands button[data-command="help"]');

const rootTabs = (page) =>
	page.locator('.root-bar [role="tab"]').evaluateAll((all) => all.map((b) => b.textContent.replace('•', '').trim()));
const shown = (page) =>
	page.locator('.root-bar [role="tab"][aria-selected="true"]').evaluate((b) => b.textContent.replace('•', '').trim());

/** The Help widget, wherever it is. */
const place = (page) => page.locator('.host[data-widget="help"]');

/** What the Help widget shows: its title and its lines, once it shows `title`. */
async function read(page, title) {
	await until(() => place(page).locator('h3.asked').innerText().catch(() => ''), (t) => t.trim() === title);
	return (await place(page).locator('pre.help').innerText()).split('\n');
}

/**
 * Press Help (`button`), then `target`: the answer in the Help place, a transient tab
 * right of this one and in front. Back (the browser's, or a tap on the tab it
 * left) returns and closes it; then, nothing was run, echoed or output.
 */
async function explain(page, target, title, back = () => page.goBack(), button = help(page)) {
	const output = () => page.locator('.prompt-bar .roll').allInnerTexts();
	const [echoes, said, tabs, from] = [
		await page.locator('.repl .echo').count(),
		await output(),
		await rootTabs(page),
		await shown(page)
	];
	await button.click();
	await target.click();
	await until(() => shown(page), (t) => t === 'help');
	const at = tabs.indexOf(from) + 1;
	assert.deepEqual(await rootTabs(page), [...tabs.slice(0, at), 'help', ...tabs.slice(at)]);
	const style = await page.locator('.root-bar [role="tab"][aria-selected="true"]').evaluate((b) => getComputedStyle(b).fontStyle);
	assert.equal(style, 'italic', 'the place is not transient');
	// The lines it leads with, and (`text`) all it holds, its folds' too.
	const lines = Object.defineProperty(await read(page, title), 'text', { value: await place(page).textContent() });
	await atRest(page.locator('section.root > .content'));
	await back();
	await until(() => rootTabs(page), (t) => t.join() === tabs.join());
	assert.equal(await shown(page), from);
	// Not in the bar's output section, and not in the REPL.
	assert.deepEqual(await output(), said, 'the explanation went to the output');
	assert.equal(await page.locator('.repl .echo').count(), echoes, 'the explanation was recorded');
	return lines;
}

const label = async (page) => (await help(page).locator('.label:not(.hidden)').innerText()).trim();

describe('the Help button', () => {
	test('is second, after Undo, single-outlined; armed it asks "? on what" in the same width; Escape disarms', async () => {
		await onPage(async ({ page }) => {
			const names = (await listed(page.locator('.bar.commands'), 'button[data-command]')).map((b) => b.command);
			assert.deepEqual(names.slice(0, 3), ['undo', 'redo', 'help']);

			// Undo runs at once and wears the double outline; Help waits, and wears one.
			const undo = page.locator('.bar.commands button[data-command="undo"]');
			assert.equal(await undo.getAttribute('data-act'), 'run');
			assert.equal(await help(page).getAttribute('data-act'), 'arm');
			const style = (el) => el.evaluate((b) => getComputedStyle(b).borderTopStyle);
			assert.equal(await style(undo), 'double');
			assert.equal(await style(help(page)), 'solid');

			assert.equal(await label(page), 'Help');
			const before = (await help(page).boundingBox()).width;
			await help(page).click();
			assert.equal(await label(page), '? on what\n(tap: all)');
			assert.equal(await help(page).getAttribute('aria-pressed'), 'true');
			const armed = (await help(page).boundingBox()).width;
			assert.ok(Math.abs(armed - before) <= 3, `the width moved: ${before} -> ${armed}`);
			// And the armed words fit inside it: the box holds by not growing, not by spilling.
			const spill = await help(page).locator('.armed').evaluate((l) => {
				const words = document.createRange();
				words.selectNodeContents(l);
				return words.getBoundingClientRect().width - l.clientWidth;
			});
			assert.ok(spill <= 1, `the armed label spills out of the button by ${spill}px`);
			await page.keyboard.press('Escape');
			assert.equal(await label(page), 'Help');
		});
	});

	test('then a command: the CLI\'s own help for it in the Help place, and the command does not run', async () => {
		await onPage(async ({ page }) => {
			const occur = page.locator('.bar.commands button[data-command="occur"]');
			const lines = await explain(page, occur, 'datum help occur');
			assert.match(lines[0], /^datum occur — record a point-in-time event/);
			assert.equal(await page.locator('.prompt-bar input').inputValue(), '', 'occur filled the line');
			assert.equal(await label(page), 'Help', 'one-shot: it disarms');

			// The next tap acts as it always did.
			await occur.click();
			await page.waitForFunction(() => document.querySelector('.prompt-bar input').value === 'occur ');
		});
	});

	// da-803mk: a subcommand is explained by the CLI's help for it, which is
	// its own page and not db's (da-2bb7z).
	test('then a subcommand: `datum help db backup`', async () => {
		await onPage(async ({ page }) => {
			await page.locator('.bar.commands button[data-command="db"]').click();
			const backup = page.locator('.bar.commands button[data-command="db backup"]');
			const lines = await explain(page, backup, 'datum help db backup');
			assert.equal(lines[0], 'datum db backup');
			assert.ok(lines.text.includes('datum db backup <PATH | ->'), lines.text);
			assert.ok(!lines.text.includes('datum db purge'), lines.text);
			assert.equal(await page.locator('.prompt-bar input').inputValue(), '', 'backup filled the line');
		});
	});

	// da-st2xl: a held command's flag is explained by its line of the CLI's
	// flag table, and the tap appends nothing. A right click is the hold.
	test('then a flag of a held command: its help line, and the line is left alone', async () => {
		await onPage(async ({ page }) => {
			await page.locator('.bar.commands button[data-command="occur"]').click({ button: 'right' });
			const t = page.locator('.bar.commands button[data-flag="occur -t"]');
			await t.waitFor();
			const lines = await explain(page, t, 'datum occur -t');
			assert.deepEqual(lines, ['Time override (offset, HH:MM, 9:15am, a date, or both)']);
			assert.equal(await page.locator('.prompt-bar input').inputValue(), 'occur ', '-t was appended');
		});
	});

	test('then a widget: its kind\'s help, and a press inside it does nothing else', async () => {
		await onPage(async (app) => {
			const { page } = app;
			await seedLines(app, ['spec set coffee --verb occur']);
			const coffee = page.locator('section[data-pane="bar"] .bar button.widget', { hasText: 'coffee' }).first();
			const lines = await explain(page, coffee, 'fields');
			assert.match(lines[0], /^Every field you log/);
			assert.equal((await app.entries(5)).length, 0, 'the press logged coffee');
		});
	});

	// da-6i6dx: the setup widget's help speaks the form's words — its shapes
	// and its data, and a new field's save, one `spec set --track` and one Undo (da-2pbvd).
	test('then the setup widget: its help in the words its form uses', async () => {
		const withField = JSON.stringify(along('column', widgetPane('setup', 'setup'), widgetPane('cmd', 'command'), widgetPane('bar', 'widgetbar')));
		await onPage(async ({ page }) => {
			const lines = await explain(page, page.locator('.host[data-widget="setup"] p.what'), 'setup');
			assert.match(lines[0], /its shape, states, data, aliases and placement\.$/);
			assert.match(lines[1], /^Save makes a new field with one `spec set --track`, which one Undo takes back;/);
			assert.doesNotMatch(lines.join('\n'), /\bkind\b|\bkeys\b/);
		}, withField);
	});

	// da-iggnd: a control in a widget with a `data-help` of its own is explained by it, not by its widget's help.
	test('then the setup widget\'s tracked or ignored: what each means, in the person\'s voice', async () => {
		const withField = JSON.stringify(along('column', widgetPane('setup', 'setup'), widgetPane('cmd', 'command'), widgetPane('bar', 'widgetbar')));
		await onPage(async ({ page }) => {
			// The toggle, and the ⓘ beside it (canary-deepseek-v4-pro on #775).
			for (const target of ['[data-tracking="ignored"]', 'button.info']) {
				const lines = await explain(page, page.locator(`.host[data-widget="setup"] ${target}`), 'tracked or ignored');
				assert.deepEqual(lines, [
					"tracked: I'm paying attention to this: it's on my now view and in my fields.",
					"ignored: I've set this aside. Its entries stay, and logging it again brings it back."
				], target);
			}
			assert.equal(await page.locator('.host[data-widget="setup"] .bubble').count(), 0, 'the ⓘ opened its bubble under Help');
			assert.equal(
				await page.locator('.host[data-widget="setup"] [data-tracking="tracked"]').getAttribute('aria-checked'),
				'true',
				'the press under Help changed the toggle'
			);
		}, withField);
	});

	test('then a pill: what that kind of pill does, with its value; the pill stays', async () => {
		await onPage(async ({ page }) => {
			await page.evaluate(() => window.datum.bar.setPills([{ id: 't', kind: 'time', text: '-t 14:30' }]));
			const pill = page.locator('.prompt-bar [data-pill="t"]');
			const lines = await explain(page, pill, '-t 14:30');
			assert.match(lines[0], /^-t 14:30: the time cursor/);
			assert.equal(await pill.count(), 1, 'the tap removed the pill');
		});
	});

	test('then Help again: general help, `datum help`; a tap back on the tab it left closes it too', async () => {
		await onPage(async ({ page }) => {
			const from = await shown(page);
			const tap = () => page.locator('.root-bar [role="tab"]', { hasText: from }).click();
			const lines = await explain(page, help(page), 'datum help', tap);
			assert.match(lines[0], /^datum — personal data tracker/);
			assert.equal(await label(page), 'Help');
		});
	});

	// da-ftj39: the top bar's controls carry their own `data-help`, read
	// before the general help a tap on anything unknown gets.
	test('then a top bar control: its own help, not `datum help`', async () => {
		await onPage(async ({ page }) => {
			const bar = page.locator('.root-bar');
			for (const control of [
				bar.locator('[role="tab"][aria-selected="true"]'),
				bar.locator('button.add'),
				bar.locator('button.gear')
			]) {
				const title = await control.getAttribute('title');
				const lines = await explain(page, control, title);
				assert.doesNotMatch(lines[0], /^datum — personal data tracker/, title);
				assert.equal(lines.join('\n'), await control.getAttribute('data-help'));
			}
		});
	});

	// vividn, 2026-09-29: «Every modal is also a widget that can be put into a
	// layout». A Help pane in view takes the answer, and no tab opens.
	test('with a Help pane in the layout, the answer lands there', async () => {
		const withPane = JSON.stringify(
			along('column', widgetPane('cmd', 'command'), widgetPane('help', 'help'), widgetPane('bar', 'widgetbar'))
		);
		await onPage(async ({ page }) => {
			await page.locator('section.pane[data-title="help"]').waitFor();
			const tabs = await rootTabs(page);
			await help(page).click();
			await page.locator('.bar.commands button[data-command="occur"]').click();
			const lines = await read(page, 'datum help occur');
			assert.match(lines[0], /^datum occur — record a point-in-time event/);
			// The pane shows it at once; a tab `openPlace` opened would land a
			// layout load later, so the tabs are watched for a while.
			for (let i = 0; i < 10; i++, await page.waitForTimeout(100)) {
				assert.deepEqual(await rootTabs(page), tabs, 'a tab opened beside the pane');
			}
		}, withPane);
	});
	// canary-kimi-k3 on #355: a pane's tabs behind the front one stay mounted
	// (da-rjmg), hidden, so a Help pane in one is no Help pane in view.
	test('with a Help pane only in a tab behind, the answer opens the place', async () => {
		const pane = (id, kind) => ({ id, content: { id: `${id}-w`, kind, settings: {} } });
		const behind = JSON.stringify(
			along('column', widgetPane('cmd', 'command'), {
				id: 'x',
				tabs: [
					pane('x-repl', 'repl'),
					{
						id: 'x-inner',
						content: { id: 'x-split', direction: 'row', ratios: [0.5, 0.5], children: [widgetPane('h', 'help'), widgetPane('r', 'repl')] }
					}
				],
				active: 0,
				bar: { edge: 'top', align: 'start' }
			})
		);
		await onPage(async ({ page }) => {
			await page.locator('.tab-body.behind .host[data-widget="help"]').waitFor({ state: 'attached' });
			const occur = page.locator('.bar.commands button[data-command="occur"]');
			const lines = await explain(page, occur, 'datum help occur');
			assert.match(lines[0], /^datum occur — record a point-in-time event/);
		}, behind);
	});
	test('on a desktop, the answer opens the Help place in a modal, and Escape closes it', async () => {
		await desktop.solo(async ({ page }) => {
			const modal = page.locator('dialog.sheet[open][data-place="help"]');
			const tabs = await rootTabs(page);
			await help(page).click();
			await page.locator('.bar.commands button[data-command="occur"]').click();
			await until(() => modal.locator('h3.asked').innerText().catch(() => ''), (t) => t.trim() === 'datum help occur');
			assert.match(await modal.locator('pre.help').innerText(), /^datum occur — record a point-in-time event/);
			assert.deepEqual(await rootTabs(page), tabs, 'a tab opened');
			await page.keyboard.press('Escape');
			await modal.waitFor({ state: 'detached' });
		}, PAGE);
	});
});

// da-efruz: a command's or a topic's page leads with its intro, and its
// usage forms and flag tables fold into <details>, closed until opened.
// vividn, 2026-10-15: «better intros without getting so bogged down by
// options … the command line options can be in expandable menus for the
// curious or the keyboarders out there».
describe('a help page', () => {
	const WITH_PANE = JSON.stringify(
		along('column', widgetPane('cmd', 'command'), widgetPane('help', 'help'), widgetPane('bar', 'widgetbar'))
	);
	/** The pages `datum help` answers to that are not commands (`TOPICS` in datum-parse's help). */
	const TOPICS = ['logging', 'time', 'debug', 'color', 'env'];
	/** The pages whose intros were written as guides for a newcomer. */
	const GUIDES = ['logging', 'time', 'spec', 'alias', 'sync', 'db', 'stats', 'track', 'occur', 'start', 'stop', 'switch', 'tail', 'undo'];

	/** Open `datum help <name>` in the Help pane, as a «Learn more» link does. */
	async function open(page, name) {
		await page.evaluate((n) => window.datum.help(n), name);
		await until(() => place(page).locator('h3.asked').innerText().catch(() => ''), (t) => t.trim() === `datum help ${name}`);
	}

	test('every page shows its header and intro, with its usage and options folded away', async () => {
		await onPage(async ({ page }) => {
			await place(page).waitFor();
			const grammar = JSON.parse((await page.evaluate(() => window.datum.runQuiet(['help', '--json']))).stdout);
			const names = [...grammar.flatMap((c) => [c.name, ...c.subcommands.map((s) => `${c.name} ${s.name}`)]), ...TOPICS];
			assert.ok(names.length > 60, `only ${names.length} pages`);
			for (const name of names) {
				await open(page, name);
				const lead = (await place(page).locator('pre.help').innerText()).split('\n');
				assert.match(lead[0], name.includes(' ') ? /^datum / : / — /, name);
				if (GUIDES.includes(name)) assert.ok(lead.length >= 4, `datum help ${name} has no intro:\n${lead.join('\n')}`);
				assert.equal(await place(page).locator('details[open]').count(), 0, `${name}: a fold is open`);
				// One fold for each of the page's usage and flag sections: every one of them.
				const json = JSON.parse((await page.evaluate((n) => window.datum.runQuiet(['help', ...n.split(' '), '--json']), name)).stdout);
				const reference = json.sections.filter((s) => s.reference).map((s) => s.heading);
				const folds = await place(page).locator('details').evaluateAll((all) => all.map((d) => d.dataset.section));
				assert.deepEqual(folds, reference, name);
				// What the reader sees: no usage heading, and no flag row.
				const seen = await place(page).innerText();
				assert.doesNotMatch(seen, /^(USAGE|OPTIONS|OUTPUT|ARGS):$/m, name);
				assert.doesNotMatch(seen, /--show <MODE>|-y, --yesterday/, name);
			}
		}, WITH_PANE);
	});

	test('occur: the intro, then Usage, Options and Output folded; an opened fold stays open on the next page', async () => {
		await onPage(async ({ page }) => {
			await place(page).waitFor();
			await open(page, 'occur');
			const lead = await place(page).locator('pre.help').innerText();
			assert.match(lead, /^datum occur — record a point-in-time event\n\nOccur marks a moment/);
			assert.deepEqual(await place(page).locator('details summary').allInnerTexts(), ['Usage', 'Options', 'Output']);
			const options = place(page).locator('details[data-section="OPTIONS"]');
			await options.locator('summary').click();
			assert.match(await options.locator('pre').innerText(), /common logging options: `datum help logging`/);
			await open(page, 'start');
			assert.equal(await options.getAttribute('open'), '', 'the opened fold closed on the next page');
			assert.equal(await place(page).locator('details[data-section="USAGE"]').getAttribute('open'), null);
			// A reload keeps it open; folded again, it stays folded.
			await page.reload();
			await place(page).waitFor();
			await open(page, 'stop');
			assert.equal(await options.getAttribute('open'), '');
			await options.locator('summary').click();
			await open(page, 'switch');
			assert.equal(await options.getAttribute('open'), null);
		}, WITH_PANE);
	});
});

// da-hibn3: the (?) in the root bar, where the padlock was, is the Help
// button on every page, one with no commands widget too. vividn, 2026-10-14:
// «even when the command widget is not visible».
describe('the root bar (?)', () => {
	const asks = (page) => page.locator('.root-bar button.help[data-command="help"]');
	const bare = JSON.stringify(along('column', widgetPane('welcome', 'welcome'), widgetPane('bar', 'widgetbar')));

	test("with no commands widget: pressed while armed, Escape disarms; then a pane's title explains its widget, and twice the general help", async () => {
		await onPage(async ({ page }) => {
			assert.equal(await page.locator('.bar.commands').count(), 0, 'a commands widget is on the page');
			const q = asks(page);
			assert.equal(await q.getAttribute('aria-pressed'), 'false');
			await q.click();
			assert.equal(await q.getAttribute('aria-pressed'), 'true');
			await page.keyboard.press('Escape');
			assert.equal(await q.getAttribute('aria-pressed'), 'false');
			const title = page.locator('section.pane[data-title="welcome"] .titlebox');
			const lines = await explain(page, title, 'welcome', undefined, q);
			assert.match(lines[0], /^What datum is/);
			const general = await explain(page, q, 'datum help', undefined, q);
			assert.match(general[0], /^datum — personal data tracker/);
			assert.equal(await q.getAttribute('aria-pressed'), 'false');
		}, bare);
	});
});
