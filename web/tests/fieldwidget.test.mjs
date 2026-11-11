// The field widget (da-fu29p): the Track dialog and the spec widget as one.
// vividn, 2026-10-08, after a newcomer found the two confusing: «let's fully
// combine track/spec widgets into just 1, and make the appearance better
// when no fields exist … make it so that you can switch the field you're
// editing inside of it more easy and intuitive … It should also be clearer
// what "field" actually is and means.»
//
// So what is held here is what a newcomer meets: «manual setup» and the fields
// touched last, alone of the form, whose modal says what a field is, takes a
// name and lists the fields there are (da-4ta1k); a field's pill, whose ×
// starts over; its status line, NOT SET UP, TRACKED or IGNORED; the examples
// under the form, which premade.test.mjs holds (da-asgtm); and a save that
// leaves a field the bar can log.
// The form's own tabs are `track.test.mjs`'s, the place's coming and going
// `specplace.test.mjs`'s.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, withApp } from './support/app.mjs';
import { VERBS } from '../src/lib/track.ts';
import { DESKTOP, PHONE } from './support/layout.mjs';
import { FIELD_FORM, NAMING, PANEL, leaveField, declared, fieldNamed, nameField, openField, trackSomething, openPalette, SETUP, setupCommand, setupCommands } from './support/field.mjs';
import { defaultColor } from '../src/lib/fields.ts';
import { until } from './support/until.mjs';
import { shellJoin } from '../src/lib/shellSplit.ts';
import { reach as reachCommand } from './support/commands.mjs';

before(startApp);
after(stopApp);

/** The widget, wherever it is drawn: the place modal on a desktop, or a pane. */
const WIDGET = ':is(dialog.sheet[data-place="setup"], section.pane[data-title="setup"])';
/** The modal «track something» opens, and the fields in it. */
const PICKER = NAMING;
const chips = (page) => page.locator(`${PICKER} .chips button`);
const chipTexts = async (page) => (await chips(page).allInnerTexts()).map((t) => t.trim());
/** The status line: its word, and its button's, or none. */
const status = async (page) => {
	const line = page.locator(`${FIELD_FORM} [data-status]`);
	const button = line.locator('button');
	return [(await line.locator('.word').innerText()).trim(), (await button.count()) ? (await button.innerText()).trim() : null];
};
/** `#rrggbb` as the browser computes it. */
const rgb = (hex) => `rgb(${[1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)).join(', ')})`;
/** WCAG's contrast ratio of two computed `rgb(…)` colours. */
const contrast = (a, b) => {
	const lum = (c) =>
		c
			.match(/[\d.]+/g)
			.slice(0, 3)
			.map((v) => v / 255)
			.map((v) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4))
			.reduce((sum, v, i) => sum + v * [0.2126, 0.7152, 0.0722][i], 0);
	const [hi, lo] = [lum(a), lum(b)].sort((x, y) => y - x);
	return (hi + 0.05) / (lo + 0.05);
};
/** A pill's fill, and the ink of its name, × and box (da-ksx39): the bar's pill's rule, the field's colour solid. */
const pillLook = (pill) =>
	pill.evaluate((el) => ({
		fill: getComputedStyle(el).backgroundColor,
		inks: [...el.querySelectorAll('h2, .named, input, button')].map((c) => getComputedStyle(c).color)
	}));
/** The field the form is on: its name line's h2. */
const heading = async (page) => (await page.locator(`${FIELD_FORM} h2`).allInnerTexts()).join().trim();

const TABS = ['basics', 'data', 'aliases'];
/** Whether a text box has the focus: on a phone, the keyboard up. */
const typing = (page) => page.evaluate(() => !!document.activeElement?.matches('input, textarea, [contenteditable]'));

describe('the field widget', () => {
	const shared = sharedApp(DESKTOP);

	// vividn, 2026-11-05 (da-4ta1k): «the widget is empty but for its top line, field:, and a track something button»;
	// 2026-11-09 (da-asgtm): «manual setup», the three fields touched last beside it, the examples under it.
	test('with no field: «manual setup» and the fields touched last alone; its modal pills the name and narrows the fields there are', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			// A second apart, as the page's clock is held: the fields touched last are by time.
			for (const [name, verb] of [['tea', 'occur'], ['team', 'occur'], ['nap', 'start']]) {
				await app.clock.forward(1000);
				await app.run(['track', name, '--verb', verb]);
			}
			await app.redraw();
			await openField(page);
			const line = page.locator(`${FIELD_FORM} .name-line`);
			await line.waitFor();
			assert.equal(await line.locator('.label').count(), 0);
			// Its buttons, and none of the rest: no shapes, tabs, status or buttons.
			const buttons = () => page.locator(`${FIELD_FORM} button`).evaluateAll((all) => all.map((b) => b.dataset.recent ?? b.className.split(' ')[0]));
			await until(buttons, (b) => b.join() === 'track-something,nap,team,tea');
			assert.equal(await line.locator('button.track-something').innerText(), 'manual setup');
			for (const gone of ['.verbs', '[role="tab"]', '[data-status]', '.buttons', '.body']) assert.equal(await page.locator(`${FIELD_FORM} ${gone}`).count(), 0, gone);
			// The examples are under the form, not in it.
			assert.equal(await page.locator(`${FIELD_FORM} [data-example]`).count(), 0);
			await page.locator(`${WIDGET} [data-example]`).first().waitFor();

			// The modal says what a field is, has the keys, and lists the fields there are, in their colors and glyphs.
			await trackSomething(page);
			assert.ok((await page.locator(`${PICKER} p.what`).innerText()).trim(), 'it does not say what a field is');
			assert.equal(await typing(page), true, 'the name box does not have the keys');
			assert.deepEqual([...(await chipTexts(page))].sort(), ['● tea', '● team', '▬ nap'].sort());
			const pill = page.locator(`${PICKER} .pill`);
			const border = () => pill.evaluate((el) => getComputedStyle(el).borderTopColor);
			// As it is typed, the name is a pill in its own color, and the list narrows to it, whatever the case.
			const box = page.locator(`${PICKER} input.field`);
			await box.fill('TE');
			assert.equal(await border(), rgb(defaultColor('TE')));
			const typed = await pillLook(pill);
			assert.equal(typed.fill, rgb(defaultColor('TE')), 'the pill is not filled solid');
			for (const ink of typed.inks) assert.ok(contrast(ink, typed.fill) >= 3, `${ink} on ${typed.fill}`);
			assert.deepEqual([...(await chipTexts(page))].sort(), ['● tea', '● team'].sort());
			await box.fill('walk');
			assert.equal(await border(), rgb(defaultColor('walk')));
			assert.equal(await chips(page).count(), 0);
			// A name that is a field goes to it.
			await box.fill('tea');
			await page.locator(`${PICKER} button.enter`).click();
			await until(() => heading(page), (t) => t === 'tea');
			assert.deepEqual(app.errors, []);
		});
	});

	test('a new name is a pill reading NOT SET UP, with cancel, save without tracking and start tracking; its × starts over', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			const { saved } = await openField(page);
			await nameField(page, 'tea');
			assert.equal(await fieldNamed(page), 'tea');
			const pill = page.locator(`${FIELD_FORM} .name-line .pill`);
			assert.equal(await pill.evaluate((el) => getComputedStyle(el).borderTopColor), rgb(defaultColor('tea')));
			// vividn, 2026-11-11: «the field pill … should be compeletely filled in rather than the semi transparent fill».
			const look = await pillLook(pill);
			assert.equal(look.fill, rgb(defaultColor('tea')), 'the pill is not filled solid');
			assert.equal(look.inks.length, 2, 'its name and ×');
			for (const ink of look.inks) assert.ok(contrast(ink, look.fill) >= 3, `${ink} on ${look.fill}`);
			assert.deepEqual(await status(page), ['NOT SET UP', null]);
			// The line goes above the shapes.
			const top = async (sel) => (await page.locator(`${FIELD_FORM} ${sel}`).first().boundingBox()).y;
			assert.ok((await top('.name-line')) < (await top('[data-status]')) && (await top('[data-status]')) < (await top('.verb')), 'not field, status, shapes');
			// Its buttons are commands, start tracking leading (da-r674m), and the form has no row of its own.
			assert.equal(await page.locator(`${FIELD_FORM} :is(.buttons, button.commit, button.untracked)`).count(), 0, "a button row");
			// None chosen, and every tab there before a save, none of them required; nothing saves without a shape.
			assert.equal(await page.locator(`${FIELD_FORM} .verb[aria-checked="true"]`).count(), 0);
			assert.deepEqual(await page.locator(`${FIELD_FORM} [role="tab"]`).evaluateAll((all) => all.map((t) => t.dataset.tab)), TABS);
			assert.deepEqual(await setupCommands(page), [['start tracking', false], ['save without tracking', false], ['cancel', true]]);

			// The × starts over: the line is «track something» again, asking nothing for a name alone.
			await pill.locator('button.clear').click();
			await page.locator(`${FIELD_FORM} button.track-something`).waitFor();
			assert.equal(await page.locator('dialog.confirm').count(), 0, 'a name alone asked');
			assert.equal(await fieldNamed(page), '');

			// Saved without tracking, the spec is there and the field ignored, in one `setup F --ignore`.
			await nameField(page, 'tea');
			await page.locator(`${FIELD_FORM} button[data-verb="occur"]`).click();
			await setupCommand(page, 'save without tracking');
			const result = await saved;
			assert.equal(declared(shellJoin(result.argv)).line, 'setup tea --from-json --ignore');
			await until(() => status(page), (s) => s[0] === 'IGNORED');
			assert.deepEqual(await status(page), ['IGNORED', 'start tracking']);
			assert.equal(JSON.parse((await app.run(['fields', '--json'])).stdout).find((f) => f.field === 'tea')?.ignored, true);
			assert.deepEqual(app.errors, []);
		});
	});

	test('start tracking makes it TRACKED, with its current state; stop tracking makes it IGNORED at once, start tracking beside it', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await openField(page);
			await nameField(page, 'mood');
			await page.locator(`${FIELD_FORM} button[data-verb="switch"]`).click();
			await page.locator(`${FIELD_FORM} .states + button.add`).click();
			await page.locator(`${FIELD_FORM} .state input.name`).fill('good');
			await setupCommand(page);
			await until(() => heading(page), (t) => t === 'mood');
			const current = page.locator(`${FIELD_FORM} [data-status] .now b`);
			await until(() => status(page), (s) => s[0] === 'TRACKED');
			assert.deepEqual(await status(page), ['TRACKED', 'stop tracking']);
			assert.equal((await current.innerText()).trim(), 'off');
			await app.clock.forward(1000);
			await app.run(['switch', 'mood', 'good']);
			await app.redraw();
			await until(() => current.innerText(), (t) => t.trim() === 'good');

			await page.locator(`${FIELD_FORM} input.description`).fill('kept');
			for (const [said, verb, now] of [
				['stop tracking', 'ignore', ['IGNORED', 'start tracking']],
				['start tracking', 'track', ['TRACKED', 'stop tracking']]
			]) {
				// The app's clock is held, and two rows of one field at one instant collide.
				await app.clock.forward(1000);
				const before = (await app.echoes()).length;
				await page.locator(`${FIELD_FORM} [data-status] button`, { hasText: said }).click();
				await until(() => status(page), (seen) => seen[0] === now[0]);
				assert.deepEqual(await status(page), now);
				assert.deepEqual((await app.echoes()).slice(before), [`${verb} mood`]);
			}
			// The form stayed, with what was typed in it.
			assert.equal(await heading(page), 'mood');
			assert.equal(await page.locator(`${FIELD_FORM} input.description`).inputValue(), 'kept');
			assert.deepEqual(app.errors, []);
		});
	});

	// vividn, 2026-10-19: «after selecting an example don't enter text edit mode and open the keyboard, that is
	// jarring. In fact, maybe don't enter that either after selecting a data shape» (da-zz9rn).
	test('on a phone: the name line, description first, no keyboard after a shape, and a field saved tracked', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const { saved } = await openField(page);
			await nameField(page, 'coffee');
			// No tracked / ignored control before it is saved, no command preview, no paragraph explaining the data.
			for (const gone of ['[data-status] button', 'button.info', '.preview', 'p.about']) {
				assert.equal(await page.locator(`${FIELD_FORM} ${gone}`).count(), 0, gone);
			}
			// The description is the first thing on Basics.
			const panel = page.locator(`${FIELD_FORM} [data-panel="basics"]`);
			const tops = await panel.evaluate((el) => [...el.children].map((c) => [c.getBoundingClientRect().top, c.querySelector('input.description') !== null]));
			assert.equal(tops.find(([top]) => top === Math.min(...tops.map(([t]) => t)))[1], true, 'the description is not first');

			// A shape does not put the focus in a text box (an example neither: premade.test.mjs).
			await page.locator(`${FIELD_FORM} button[data-verb="occur"]`).click();
			assert.equal(await typing(page), false, 'a shape opened the keyboard');

			// Saved, it is tracked: one `setup F --track`. (A phone's CLI widget is folded away: the save says what it ran.)
			await setupCommand(page);
			const result = await saved;
			assert.equal(result.code, 0, result.stderr);
			assert.equal(declared(shellJoin(result.argv)).line, 'setup coffee --from-json --track');
			assert.equal(JSON.parse((await app.run(['fields', '--json'])).stdout).find((f) => f.field === 'coffee')?.ignored, false);
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	test('on a phone, a field chosen in the modal is its pill over its shapes, and its × starts over', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.run(['track', 'tea', '--verb', 'occur']);
			await app.run(['track', 'nap', '--verb', 'start']);
			await app.redraw();
			await openField(page);
			await trackSomething(page);
			await page.locator(`${PICKER} [data-chip="nap"]`).click();
			await page.locator(PICKER).waitFor({ state: 'detached' });
			await until(() => heading(page), (t) => t === 'nap');
			// On a field, the line is `field: nap` over its shapes.
			const top = async (sel) => (await page.locator(`${FIELD_FORM} ${sel}`).first().boundingBox()).y;
			assert.ok((await top('.name-line')) < (await top('.verb')), 'a field\'s name is not over its shapes');
			assert.equal((await page.locator(`${FIELD_FORM} .name-line .label`).innerText()).trim(), 'field:');
			await page.locator(`${FIELD_FORM} .name-line .pill button.clear`).click();
			await page.locator(`${FIELD_FORM} button.track-something`).waitFor();
			assert.equal(await fieldNamed(page), '');
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	test('the modal lists every field in its own color and glyph, an ignored one too, and a tap goes to that field', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'coffee', '--verb', 'occur', '--color', '#884400']);
			await app.run(['track', 'sleep', '--verb', 'start']);
			await app.run(['track', 'mood', '--verb', 'switch', '--add-state', 'good']);
			await app.run(['track', 'person', '--verb', 'add']);
			await app.run(['track', 'run', '--verb', 'occur']);
			await app.clock.forward(1000);
			await app.run(['ignore', 'run']);
			await app.redraw();
			await openField(page, 'coffee');
			await page.locator(`${FIELD_FORM} h2`).waitFor();
			await trackSomething(page);
			assert.deepEqual([...(await chipTexts(page))].sort(), ['¢ person', '▬ sleep', '▨ mood', '● coffee', '● run'].sort());
			assert.equal(await page.locator(`${PICKER} [data-chip="run"]`).getAttribute('data-ignored'), 'true');
			assert.equal(
				await page.locator(`${PICKER} [data-chip="coffee"] span`).evaluate((el) => getComputedStyle(el).color),
				'rgb(136, 68, 0)'
			);

			await page.locator(`${PICKER} [data-chip="sleep"]`).click();
			await until(() => heading(page), (t) => t === 'sleep');
			assert.equal(await page.locator(`${FIELD_FORM} .verb[aria-checked="true"]`).getAttribute('data-verb'), 'start');

			// A spec's edits are saved by its ×, with nothing asked (da-hda4j).
			await page.locator(`${FIELD_FORM} input.description`).fill('nights');
			await trackSomething(page);
			assert.equal(await page.locator('dialog.confirm').count(), 0, 'leaving a spec asked');
			await until(async () => JSON.parse((await app.run(['setup', 'sleep', '--json'])).stdout).description, (d) => d === 'nights');
			await page.locator(`${PICKER} [data-chip="coffee"]`).click();
			await until(() => heading(page), (t) => t === 'coffee');

			// A new field is not made by leaving it: its × asks once it holds more than its name, and cancel keeps it.
			const asks = (answer) => page.locator('dialog.confirm').getByRole('button', { name: answer, exact: true }).click();
			await nameField(page, 'tea');
			await page.locator(`${FIELD_FORM} button[data-verb="occur"]`).click();
			await page.locator(`${FIELD_FORM} .name-line .pill button.clear`).click();
			await page.locator('dialog.confirm').waitFor();
			await asks('cancel');
			assert.equal(await fieldNamed(page), 'tea', 'cancel lost the edit');
			await page.locator(`${FIELD_FORM} .name-line .pill button.clear`).click();
			await asks('leave');
			await page.locator(`${FIELD_FORM} button.track-something`).waitFor();
			assert.match((await app.run(['setup', 'tea', '--json'])).stdout, /No spec defined/, 'leaving made the new field');
			assert.deepEqual(app.errors, []);
		});
	});

	// Every tab is there before a save (da-uipyt), so what is set on one is saved with the field.
	test('a new field\'s tabs are saved with it, in one `setup F --track` that one Undo takes back (da-2pbvd)', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			const { saved } = await openField(page);
			await nameField(page, 'mood');
			await page.locator(`${FIELD_FORM} button[data-verb="switch"]`).click();
			await page.locator(`${FIELD_FORM} .states + button.add`).click();
			await page.locator(`${FIELD_FORM} .state input.name`).fill('good');
			await page.locator(`${FIELD_FORM} .state button.more`).click();
			await page.locator(`${FIELD_FORM} .state input.meaning`).fill('a good day');
			await page.locator(`${FIELD_FORM} input.description`).fill('how it feels');
			const before = (await app.echoes()).length;
			await setupCommand(page);
			assert.equal((await saved).code, 0);
			const echoes = (await app.echoes()).slice(before);
			assert.equal(echoes.length, 1, echoes.join(' | '));
			assert.match(echoes[0], /^setup mood --from-json .* --track$/);
			const shown = JSON.parse((await app.run(['setup', 'mood', '--json'])).stdout);
			assert.equal(shown.description, 'how it feels');
			assert.deepEqual(shown.state_descriptions, { good: 'a good day' });
			assert.deepEqual(shown.allowed_states, ['good']);
			assert.equal((await app.allEntries()).filter((e) => e.field === 'mood').length, 1, 'no TRACK row');
			assert.equal((await app.run(['undo'])).code, 0);
			assert.match((await app.run(['setup', 'mood', '--json'])).stdout, /No spec defined/, 'one Undo left the spec');
			assert.deepEqual(await app.allEntries(), [], 'one Undo left a row');
		});
	});

	// A save takes a moment, and a reader may go to another field in it: they stay where they went.
	test('a field picked while a save runs is where the reader stays, the place still open', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'tea', '--verb', 'occur']);
			await app.run(['track', 'nap', '--verb', 'start']);
			await app.redraw();
			await openField(page, 'nap');
			await page.locator(`${FIELD_FORM} h2`).waitFor();
			await page.locator(`${FIELD_FORM} input.description`).fill('short');
			const before = (await app.echoes()).length;
			// Save and another field asked for in one turn of the page, so the save is in flight when it lands.
			await openPalette(page);
			await page.evaluate((save) => {
				document.querySelector(save).click();
				void window.datum.field('tea');
			}, SETUP());
			await until(async () => (await app.echoes()).length, (n) => n === before + 1);
			await app.idle();
			assert.equal((await page.locator(`${FIELD_FORM} h2`).allInnerTexts()).join().trim(), 'tea', 'the save took the place away, or the field back');
			assert.match((await app.echoes()).at(-1), /^setup nap /);
		});
	});

	// The key a line's leftover words go to is a spec's to change as well as
	// a new field's: a row's mode (da-zqnt3).
	test('the rest-of-line row of a saved field is read, renamed, made one string and removed, each in its one `setup F`', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'emotion', '--verb', 'occur', '-R', 'emotions']);
			const route = async () => {
				const { remainder, string_remainder } = JSON.parse((await app.run(['setup', 'emotion', '--json'])).stdout);
				return [remainder, string_remainder];
			};
			const row = page.locator(`${FIELD_FORM} ul.data li.key[data-mode^="R"], ${FIELD_FORM} ul.data li.key[data-mode="SR"]`);
			for (const [change, stored] of [
				[() => row.locator('input.name').fill('words'), ['words', undefined]],
				[() => row.locator('.mode').click(), ['words', true]],
				[() => row.locator('.remove').click(), [undefined, undefined]]
			]) {
				const { saved } = await openField(page, 'emotion');
				await page.locator(`${FIELD_FORM} [data-tab="data"]`).click();
				assert.equal(await row.locator('input.name').inputValue(), (await route())[0]);
				await change();
				await setupCommand(page);
				assert.equal((await saved).code, 0);
				assert.deepEqual(await route(), stored);
			}
		});
	});

	/** A widget of `kind` in the page's first pane, as the chooser puts one. */
	const put = async (page, kind) => {
		const tab = await page.evaluate(() => window.datum.layout.current().root.tabs[0].content.children[0].tabs[0].id);
		await page.evaluate(([tab, kind]) => window.datum.shell.widgetInto(tab, kind), [tab, kind]);
		return page.locator(`section.pane[data-title="${kind}"]`);
	};

	// da-i9upr, vividn, 2026-10-19: «move the placement to the basics tab and show what the data looks like
	// displayed … completely remove the states tab … for card, embed the data widget directly on the basics».
	// da-srrqd, vividn, 2026-10-22: the samples «should actually be inside the placement configuration tool».
	test('Basics draws each shape as it is logged in its lane on the band; a switch\'s states start at off; a card\'s keys are there', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			const { saved } = await openField(page);
			const at = (sel) => page.locator(`${FIELD_FORM} [data-panel="basics"] ${sel}`);
			const own = at('.placement .band g.field[data-field="thing"]');
			/** Wait for the band to draw the field's sample as `ok` says: its blocks' states, and how many dots. */
			const drawn = (what, ok) =>
				until(
					async () => ({
						blocks: await own.locator('rect.block').evaluateAll((all) => all.map((b) => b.dataset.state)),
						dots: await own.locator('.point-fg').count()
					}),
					ok
				).catch((e) => assert.fail(`never drawn: ${what}: ${e.message}`));
			const off = at('.states li.off input.offer');
			const struck = async () => [await off.isChecked(), await off.isDisabled()];
			await nameField(page, 'thing');
			// No shape yet, nothing to draw.
			assert.equal(await at('.placement').count(), 0);
			assert.deepEqual(await page.locator(`${FIELD_FORM} [role="tab"]`).evaluateAll((all) => all.map((t) => t.dataset.tab)), TABS);

			await page.locator(`${FIELD_FORM} button[data-verb="occur"]`).click();
			await drawn('the dots of an occurrence', ({ blocks, dots }) => dots === 8 && blocks.length === 0);
			assert.equal(await at('.placement .box').count(), 0, 'no box at rest (da-r6wwe)');

			await page.locator(`${FIELD_FORM} button[data-verb="start"]`).click();
			await drawn('on with off between', ({ blocks, dots }) => dots === 0 && blocks.length === 4 && blocks.every((s) => s === 'on'));

			// A switch starts with off alone, which cannot go until there is another state.
			await page.locator(`${FIELD_FORM} button[data-verb="switch"]`).click();
			assert.deepEqual(await struck(), [true, true]);
			await at('.states + button.add').click();
			await at('.state input.name').fill('calm');
			await drawn('calm with off between', ({ blocks }) => blocks.length === 4 && blocks.every((s) => s === 'calm'));
			assert.equal(await off.isDisabled(), false);
			await off.click();
			await drawn('calm with no off: one stretch', ({ blocks }) => blocks.length === 1 && blocks[0] === 'calm');
			// The Data tab's button opens it.
			await page.locator(`${FIELD_FORM} [data-tab="data"]`).click();
			assert.equal(await page.locator(PANEL).getAttribute('data-panel'), 'data');
			await page.locator(`${FIELD_FORM} [data-tab="basics"]`).click();

			// A card has no time: no placement, so no sample, and the Data tab's rows, typed, on Basics.
			await page.locator(`${FIELD_FORM} button[data-verb="add"]`).click();
			for (const gone of ['.placement', '[data-tab="data"]', '.states']) assert.equal(await page.locator(`${FIELD_FORM} ${gone}`).count(), 0, gone);
			await at('ul.data + button.add').click();
			assert.equal(await at('ul.data li.key select.type').count(), 1, 'not the Data tab\'s rows');

			// Saved as a switch, off disabled is `never_false`.
			await page.locator(`${FIELD_FORM} button[data-verb="switch"]`).click();
			await setupCommand(page);
			assert.equal((await saved).code, 0);
			const shown = JSON.parse((await app.run(['setup', 'thing', '--json'])).stdout);
			assert.deepEqual([shown.allowed_states, shown.never_false], [['calm'], true]);
			assert.deepEqual(app.errors, []);
		});
	});

	// da-ooflv, vividn, 2026-10-28: «let's bring the data and aliases tabs back on the setup widget, but no swipe action
	// between them». A tap on a tab or an arrow key in its bar changes it; a swipe never does (fieldswipe.test.mjs).
	test('basics, data and aliases are tabs, each shown by a tap or the arrow keys; a record has no data tab, its data on basics', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			const { saved } = await openField(page);
			await nameField(page, 'thing');
			await page.locator(`${FIELD_FORM} button[data-verb="switch"]`).click();
			const tab = (name) => page.locator(`${FIELD_FORM} [role="tab"][data-tab="${name}"]`);
			const shown = () => page.locator(PANEL).getAttribute('data-panel');
			/** What each tab holds, which no other tab shows. */
			const inside = { basics: 'input.description', data: 'ul.data + button.add', aliases: 'input.autoalias' };
			assert.equal(await shown(), 'basics');
			for (const name of ['data', 'aliases', 'basics']) {
				await tab(name).click();
				assert.equal(await shown(), name);
				assert.equal(await tab(name).getAttribute('aria-selected'), 'true');
				for (const [other, sel] of Object.entries(inside)) {
					assert.equal(await page.locator(`${FIELD_FORM} ${sel}`).count(), other === name ? 1 : 0, `on ${name}, ${other}'s ${sel}`);
				}
			}
			// The arrows walk the bar, the focus with them, and stop at either end.
			await tab('basics').focus();
			for (const [key, name] of [['ArrowRight', 'data'], ['ArrowRight', 'aliases'], ['ArrowRight', 'aliases'], ['ArrowLeft', 'data']]) {
				await page.keyboard.press(key);
				assert.equal(await shown(), name, key);
				assert.equal(await page.evaluate(() => document.activeElement?.dataset.tab), name, `${key}: the focus`);
			}
			// A record, opened on data: its rows are on basics, and there is no data tab (da-eh4dl).
			await tab('data').click();
			await page.locator(`${FIELD_FORM} button[data-verb="add"]`).click();
			assert.deepEqual(await page.locator(`${FIELD_FORM} [role="tab"]`).evaluateAll((all) => all.map((t) => t.dataset.tab)), ['basics', 'aliases']);
			assert.equal(await shown(), 'basics');
			assert.equal(await page.locator(`${PANEL} ul.data + button.add`).count(), 1, "a record's data is not on basics");
			await leaveField(page);
			assert.equal(await saved, null);
			assert.deepEqual(app.errors, []);
		});
	});

	// da-eh4dl, vividn, 2026-11-05: over the data and aliases buttons what each tab holds, five and «+n more», which a
	// press opens into the tab, not under `prefers-reduced-motion`; off basics, its kind and color; a record has no data tab.
	test('the data and aliases buttons say what their tabs hold and unfold into them; off basics, its kind and color; a record has no data tab', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			const key = { required: false, value_type: 'any', in_slug: false, default: null };
			const withKeys = async (field, names) => {
				const spec = JSON.parse((await app.run(['setup', field, '--json'])).stdout);
				const r = await app.run(['setup', field, '--from-json', JSON.stringify({ ...spec, subfields: Object.fromEntries(names.map((n) => [n, key])) })]);
				assert.equal(r.code, 0, r.stderr);
			};
			await app.run(['track', 'sleep', '--verb', 'start', '--color', '#3366cc']);
			await withKeys('sleep', ['a', 'b', 'c', 'd', 'e', 'f', 'g']);
			await app.run(['track', 'tea', '--verb', 'occur']);
			await app.run(['track', 'book', '--verb', 'add']);
			await withKeys('book', ['title']);
			const summary = (name) => page.locator(`${FIELD_FORM} [data-summary="${name}"]`);
			const said = async (name) => (await summary(name).innerText()).trim();
			const tab = (name) => page.locator(`${FIELD_FORM} [role="tab"][data-tab="${name}"]`);
			const tabs = () => page.locator(`${FIELD_FORM} [role="tab"]`).evaluateAll((all) => all.map((t) => t.dataset.tab));
			const open = async (field) => {
				await openField(page, field);
				await until(() => page.locator(`${FIELD_FORM} h2`).innerText(), (t) => t.trim() === field);
			};

			await open('sleep');
			assert.equal(await said('data'), 'a, b, c, d, e +2 more');
			assert.equal(await said('aliases'), 'sleep, sleepe');
			assert.equal(await said('basics'), '', 'the open tab says it in full');
			assert.equal(await page.getByText('want to record extra data').count(), 0);
			for (const name of ['data', 'aliases']) {
				await tab(name).click();
				assert.equal(await page.locator(PANEL).getAttribute('data-panel'), name);
				assert.equal(await said(name), '');
				assert.match(await said('basics'), /^▬\s*on\/off/);
				assert.equal(await summary('basics').locator('.mark').evaluate((m) => getComputedStyle(m).color), rgb('#3366cc'));
			}

			// The press unfolds the summary into the tab: the panel starts at the summary's box.
			const unfolding = () =>
				page.locator(PANEL).evaluate((p) => p.getAnimations().map((a) => a.effect.getKeyframes()[0]?.transform ?? 'none'));
			await tab('basics').click();
			await tab('data').click();
			const [from] = await unfolding();
			assert.match(from ?? '', /translate\(.*scale\(/, 'no unfolding');
			await page.emulateMedia({ reducedMotion: 'reduce' });
			await tab('basics').click();
			assert.deepEqual(await unfolding(), [], 'it moved under reduced motion');
			await page.emulateMedia({ reducedMotion: null });

			await leaveField(page);
			await open('tea');
			assert.equal(await said('data'), 'log extra data with each entry');

			// A record: basics and aliases, its keys on basics.
			await leaveField(page);
			await open('book');
			assert.deepEqual(await tabs(), ['basics', 'aliases']);
			assert.deepEqual(await page.locator(`${PANEL} ul.data input.name`).evaluateAll((all) => all.map((i) => i.value)), ['title']);
			await tab('aliases').click();
			assert.match(await said('basics'), /^¢\s*record/);
			await leaveField(page);
			assert.deepEqual(app.errors, []);
		});
	});

	// da-puzij, vividn, 2026-10-22: «the individual states for switch data don't need individual symbols, just color is
	// sufficient. also, just put a ... button to the left of x button … remove the up and down arrows, and put reorg handles
	// like there are on the data tab … have the "off" state at the top look the same as the rest of the states».
	for (const [name, size] of [['a desktop', DESKTOP], ['a phone', PHONE]])
		test(`on ${name}, a switch's states are handle · color · name · … · ✕, off and on are a checkbox · color · name, and a save keeps all of it`, async () => {
			await withApp(async (app) => {
				const { page } = app;
				await app.run(['track', 'mood', '--verb', 'switch', '--states', 'a,b,c', '--state-description', 'a=first']);
				const { saved } = await openField(page, 'mood');
				const rows = page.locator(`${FIELD_FORM} .states li.state`);
				const names = () => rows.locator('input.name').evaluateAll((all) => all.map((i) => i.value));
				const off = page.locator(`${FIELD_FORM} .states li.off`);
				const strike = off.locator('input.offer');
				// da-ksx39, vividn, 2026-11-11: «remove the "x" button on off, and instead have it be a checkbox to the left /
				// add a second row, also with checkbox for "on", color should be the field's».
				const onRow = page.locator(`${FIELD_FORM} .states li.on`);
				const onBox = onRow.locator('input.offer');
				/** The parts of a row, left to right, each its class and the middle of its box. */
				const parts = (row) =>
					row.evaluate((li) =>
						[...li.children].filter((c) => c.getBoundingClientRect().width).map((c) => {
							const r = c.getBoundingClientRect();
							return { part: c.className.split(' ')[0], x: r.left, y: r.top + r.height / 2 };
						})
					);

				// One line each, in order: no symbol in the row, no description under it.
				const row = await parts(rows.first());
				assert.deepEqual(row.map((p) => p.part), ['handle', 'swatch', 'name', 'remove', 'remove']);
				assert.ok(row.every((p) => p.x >= row[0].x && Math.abs(p.y - row[0].y) <= 2), JSON.stringify(row));
				assert.deepEqual(await rows.first().locator(':scope > button:not(.handle)').evaluateAll((all) => all.map((b) => b.textContent.trim())), ['', '…', '✕']);
				for (const gone of ['.tile', 'input.meaning']) assert.equal(await rows.locator(gone).count(), 0, gone);
				// Off, then on: a checkbox where the handle stands, the swatch under the states', and the word, which is
				// text and no box to type in («so that they don't look like editable text fields»); no … and no ✕.
				for (const [li, word] of [[off, 'off'], [onRow, 'on']]) {
					const line = await parts(li);
					assert.deepEqual(line.map((p) => p.part), ['offer', 'swatch', 'name'], word);
					const handle = await rows.first().locator('.handle').boundingBox();
					assert.ok(line[0].x >= handle.x && line[0].x < handle.x + handle.width, `${word}'s checkbox is not in the handle's column`);
					assert.equal(line[1].x, row[1].x, `${word}'s swatch`);
					assert.ok(line.every((p) => Math.abs(p.y - line[0].y) <= 2), `${word} is not one line: ${JSON.stringify(line)}`);
					assert.equal((await li.locator('.name').innerText()).trim(), word);
					assert.equal(await li.locator('input:not([type="checkbox"]), [contenteditable]').count(), 0, `${word} can be typed in`);
					assert.equal(await li.locator('button').count(), 0, `${word} has a button`);
				}
				assert.equal(await off.locator('.swatch').evaluate((s) => getComputedStyle(s).backgroundColor), 'rgba(0, 0, 0, 0)');
				assert.equal(await onRow.locator('.swatch').evaluate((s) => getComputedStyle(s).backgroundColor), rgb(defaultColor('mood')), 'on is not in the field\'s color');
				// Off offered, on not, as the spec says.
				assert.deepEqual([await strike.isChecked(), await onBox.isChecked()], [true, false]);

				// … opens the description and, under «advanced», the symbol; pressed again, it shuts.
				const more = page.locator(`${FIELD_FORM} [aria-label="the options of a"]`);
				await more.click();
				assert.equal(await more.getAttribute('aria-expanded'), 'true');
				assert.equal(await rows.first().locator('input.meaning').inputValue(), 'first');
				assert.equal((await rows.first().locator('.advanced').innerText()).trim().toLowerCase(), 'advanced');
				assert.equal(await rows.first().locator('.tile[aria-label^="the symbol of a:"]').count(), 1);
				await rows.first().locator('input.meaning').fill('the first');
				await more.click();
				assert.equal(await rows.locator('input.meaning').count(), 0);
				await rows.nth(1).locator('.swatch[aria-expanded]').click();
				const color = await page.locator(`${FIELD_FORM} .state .set .swatch`).nth(3).getAttribute('data-color');
				await page.locator(`${FIELD_FORM} .state .set .swatch`).nth(3).click();

				// A drag on c's handle to a's row puts c first; the arrow keys move it back down one, keeping the focus.
				const handle = (s) => page.locator(`${FIELD_FORM} [aria-label="move ${s}: drag, or the arrow keys"]`);
				// In view first: a phone's form is shorter with the bar's output open
				// (da-mwp4m), and a drag off the visible box lands on nothing.
				await handle('c').scrollIntoViewIfNeeded();
				const [from, to] = [await handle('c').boundingBox(), await rows.first().boundingBox()];
				await page.mouse.move(from.x + from.width / 2, from.y + from.height / 2);
				await page.mouse.down();
				await page.mouse.move(from.x + from.width / 2, to.y + 2, { steps: 5 });
				await page.mouse.up();
				assert.deepEqual(await names(), ['c', 'a', 'b']);
				await handle('c').focus();
				await page.keyboard.press('ArrowDown');
				assert.deepEqual(await names(), ['a', 'c', 'b']);
				assert.equal(await handle('c').evaluate((el) => el === document.activeElement), true, 'the moved handle lost the focus');

				// Off's checkbox unchecked strikes the whole line, and checked again brings it back.
				const across = () => off.evaluate((li) => getComputedStyle(li, '::after').borderTopStyle);
				assert.deepEqual([await strike.isChecked(), await across()], [true, 'none']);
				await strike.click();
				assert.deepEqual([await strike.isChecked(), await across()], [false, 'solid']);
				await strike.click();
				assert.equal(await strike.isChecked(), true);
				await strike.click();
				// On checked lists `true`, first.
				await onBox.click();
				assert.equal(await onBox.isChecked(), true);

				await setupCommand(page);
				assert.equal((await saved).code, 0);
				const shown = JSON.parse((await app.run(['setup', 'mood', '--json'])).stdout);
				assert.deepEqual(shown.allowed_states, ['true', 'a', 'c', 'b']);
				assert.deepEqual(shown.state_colors, { b: color });
				assert.deepEqual(shown.state_descriptions, { a: 'the first' });
				assert.equal(shown.never_false, true);
				// `true` is the CLI's on: logged, it stores `true` and reads «on».
				await app.run(['change', 'mood', 'true', '-y', '-t', '10:00']);
				const [entry] = JSON.parse((await app.run(['tail', 'mood', '--json'])).stdout).rows;
				assert.deepEqual([entry.state, entry.state_label], [true, 'on']);

				// Opened again, on is checked and in no state row; unchecked, the save lists no `true`.
				const { saved: again } = await openField(page, 'mood');
				assert.deepEqual([await strike.isChecked(), await onBox.isChecked()], [false, true]);
				assert.deepEqual(await names(), ['a', 'c', 'b']);
				await onBox.click();
				await setupCommand(page);
				assert.equal((await again).code, 0);
				assert.deepEqual(JSON.parse((await app.run(['setup', 'mood', '--json'])).stdout).allowed_states, ['a', 'c', 'b']);

				// It cannot take off away while `false` is a state, nor while there is no other; on is another.
				const { saved: left } = await openField(page, 'mood');
				await strike.click();
				await page.locator(`${FIELD_FORM} .states + button.add`).click();
				await rows.last().locator('input.name').fill('false');
				assert.deepEqual([await strike.isChecked(), await strike.isDisabled()], [true, true]);
				assert.match(await strike.getAttribute('title'), /\bfalse\b/, 'the title names the state in the way');
				await onBox.click();
				for (const s of ['false', 'a', 'c', 'b']) await page.locator(`${FIELD_FORM} [aria-label="remove ${s}"]`).click();
				assert.equal(await strike.isDisabled(), false, 'on is another state');
				await onBox.click();
				assert.equal(await strike.isDisabled(), true);
				assert.match(await strike.getAttribute('title'), /another state/, 'a disabled checkbox says why');
				await leaveField(page);
				assert.equal(await left, null);
				assert.deepEqual(app.errors, []);
			}, size);
		});

	// da-ua9gv: a state dragged past a taller one (its … open) swapped with it on every move, the taller row's bottom
	// edge below the pointer again after each swap. It passes a neighbor once the pointer reaches the neighbor's middle.
	for (const [name, size, finger] of [['a mouse on a desktop', DESKTOP, false], ['a finger on a phone', PHONE, true]])
		test(`with ${name}, a state dragged past a taller one moves once and lands under the pointer`, async () => {
			await withApp(async (app) => {
				const { page } = app;
				await app.run(['track', 'mood', '--verb', 'switch', '--states', 'a,b,c']);
				const { saved } = await openField(page, 'mood');
				const rows = page.locator(`${FIELD_FORM} .states li.state`);
				const names = () => rows.locator('input.name').evaluateAll((all) => all.map((i) => i.value).join(''));
				await page.locator(`${FIELD_FORM} [aria-label="the options of b"]`).click();
				const grip = await page.locator(`${FIELD_FORM} [aria-label="move a: drag, or the arrow keys"]`).boundingBox();
				const [a, b] = [await rows.nth(0).boundingBox(), await rows.nth(1).boundingBox()];
				assert.ok(b.height > 2 * a.height, `b's … makes it the taller: ${a.height} and ${b.height}`);

				// From a's handle down to near b's bottom, the order read after every move.
				const [x, y] = [grip.x + grip.width / 2, grip.y + grip.height / 2];
				const end = b.y + b.height - 4;
				const cdp = finger && (await page.context().newCDPSession(page));
				const touch = (type, at) => cdp.send('Input.dispatchTouchEvent', { type, touchPoints: at === undefined ? [] : [{ x, y: at }] });
				const seen = [await names()];
				if (finger) await touch('touchStart', y);
				else {
					await page.mouse.move(x, y);
					await page.mouse.down();
				}
				for (let i = 1; i <= 30; i++) {
					const at = y + ((end - y) * i) / 30;
					await (finger ? touch('touchMove', at) : page.mouse.move(x, at));
					seen.push(await names());
				}
				await (finger ? touch('touchEnd') : page.mouse.up());
				if (cdp) await cdp.detach();

				assert.deepEqual(seen.filter((s, i) => s !== seen[i - 1]), ['abc', 'bac'], seen.join(' '));
				const landed = await rows.nth(1).boundingBox();
				assert.ok(landed.y <= end && end <= landed.y + landed.height, `a's row ${JSON.stringify(landed)} is under ${end}`);
				await leaveField(page);
				assert.equal(await saved, null);
				assert.deepEqual(app.errors, []);
			}, size);
		});

	// `setup`, in the fields submenu (da-fg4w4): `track` writes its word on the line now, as any command does.
	test('`setup` in the commands widget opens it on a new field, and writes no line', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			const setup = await reachCommand((await put(page, 'command')).locator('.bar.commands'), 'setup');
			assert.equal(await setup.getAttribute('data-act'), 'open');
			await setup.click();
			await page.locator(`${FIELD_FORM} button.track-something`).waitFor();
			assert.equal(await page.locator('.prompt-bar input').first().inputValue(), '', 'the tap wrote a line as well');
			assert.equal(await fieldNamed(page), '');
		});
	});

	// In a pane it is where someone chose to work: a field pill minted
	// beside it — a hold on a field in the collect widget — brings that field.
	test('in a pane, a field pill minted beside it brings that field, and a save stays', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await app.submit('track tea --verb occur');
			const pane = await put(page, 'setup');
			await pane.locator(`${FIELD_FORM} button.track-something`).waitFor();

			await page.locator('.bar button[data-field="tea"]').click({ modifiers: ['Shift'] });
			await pane.locator(`${FIELD_FORM} h2`).waitFor();
			assert.equal((await pane.locator(`${FIELD_FORM} h2`).innerText()).trim(), 'tea');
			assert.equal(await page.locator('dialog[open]').count(), 0, 'a widget in view opened a place as well');

			// And a new field goes to the pane, not to a place of its own.
			await page.evaluate(() => void window.datum.field());
			await nameField(pane, 'nap');
			await pane.locator(`${FIELD_FORM} button[data-verb="start"]`).click();
			await setupCommand(page);
			await until(async () => (await pane.locator(`${FIELD_FORM} h2`).allInnerTexts()).join().trim(), (t) => t === 'nap');
			assert.equal(JSON.parse((await app.run(['setup', 'nap', '--json'])).stdout).verb, 'start');
			assert.equal(await page.locator('dialog[open]').count(), 0);
			assert.deepEqual(app.errors, []);
		});
	});

	// da-upig4, vividn, 2026-11-11: «if you dismiss the field pill in the input bar, it should also be dismissed in
	// the setup widget and vice versa»: the widget's field is the bar's field pill while it is on the page.
	test('in a pane, its field is the bar\'s field pill: either × clears both, and a field opened in it is pilled', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await app.submit('track tea --verb occur');
			await app.submit('track nap --verb start');
			const pane = await put(page, 'setup');
			const pill = page.locator('.prompt-bar .pill.field');
			const heading = pane.locator(`${FIELD_FORM} h2`);
			const home = pane.locator(`${FIELD_FORM} button.track-something`);

			// A field opened in it is the bar's pill. Not by openField, which waits for every form to go: a pane's
			// stays, on nap or on none, and that wait outlived the test once it had mounted (da-fcd43).
			await pane.locator(FIELD_FORM).waitFor();
			await page.evaluate(() => void window.datum.field('tea'));
			await until(async () => (await heading.allInnerTexts()).join().trim(), (t) => t === 'tea');
			await until(() => pill.locator('.label').allInnerTexts(), (t) => t.join().trim() === 'tea');
			// The bar's × clears the widget's field.
			await pill.locator('button[aria-label="remove tea"]').click();
			await home.waitFor();
			assert.equal(await heading.count(), 0, 'the widget kept tea');

			// The widget's × takes the bar's pill away.
			await page.locator('.bar button[data-field="nap"]').click({ modifiers: ['Shift'] });
			await until(async () => (await heading.allInnerTexts()).join().trim(), (t) => t === 'nap');
			assert.equal(await pill.count(), 1);
			await pane.locator(`${FIELD_FORM} .name-line .pill button.clear`).click();
			await home.waitFor();
			await until(() => pill.count(), (n) => n === 0);
			assert.deepEqual(app.errors, []);
		});
	});

	// vividn, 2026-10-31: «on the web, it should fail once warning that the other field name already exists and then give you the option to merge the other name into it» (da-icna7).
	test('«rename» onto a name that exists warns, offers the merge with its numbers, and only a yes merges', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await app.run(['occur', 'coffe', '-t', '2026-03-01T08:00:00Z']);
			await app.run(['occur', 'coffe', '-t', '2026-03-01T10:00:00Z', 'cups=1']);
			await app.run(['occur', 'coffee', '-t', '2026-03-01T10:00:00Z', 'cups=2']);
			const fields = async () => (await app.entries()).map((e) => e.field).sort();
			const offer = async () => {
				await page.locator(`${FIELD_FORM} button.rename`).click();
				const ask = page.locator('dialog.ask');
				await ask.locator('input').fill('coffee');
				await ask.locator('button[value]').click();
				const confirm = page.locator('dialog.confirm');
				await confirm.waitFor();
				return confirm;
			};
			await openField(page, 'coffe');
			await page.locator(`${FIELD_FORM} h2`).waitFor();
			const before = await fields();

			const declined = await offer();
			const said = await declined.locator('p').innerText();
			for (const fact of [/\bcoffee\b/, /\b2\b/, /data\.cups/]) assert.match(said, fact);
			await page.keyboard.press('Escape');
			await declined.waitFor({ state: 'detached' });
			assert.deepEqual(await fields(), before);
			assert.equal(await heading(page), 'coffe');

			await (await offer()).locator('button[value="merge"]').click();
			await until(() => heading(page), (t) => t === 'coffee');
			assert.deepEqual(await fields(), ['coffee', 'coffee']);
			assert.equal(JSON.parse((await app.run(['conflicts', 'list', '--show', 'json'])).stdout).length, 1);
			assert.deepEqual(app.errors, []);
		});
	});

	// vividn, 2026-10-19: «there should probably be an explicit rename button though» (da-qimwi).
	test('«rename» on a field asks the new name, shows the dry run, runs `rename`, and the panes show the new name', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'mood', '--verb', 'switch', '--add-state', 'good']);
			await app.run(['track', 'tea', '--verb', 'occur']);
			// Not at the TRACK row's instant, whose slug it would share.
			await app.clock.forward(60_000);
			await app.run(['switch', 'mood', 'good']);
			await app.clock.forward(60_000);
			await app.run(['switch', 'mood', 'bad']);
			await app.redraw();
			const tail = page.locator('section.pane[data-title="tail"] tbody');
			await until(() => tail.innerText(), (t) => t.includes('mood'));
			// No rename on a new field: nothing to rename yet.
			await openField(page);
			await nameField(page, 'thing');
			assert.equal(await page.locator(`${FIELD_FORM} button.rename`).count(), 0);
			await leaveField(page);
			await openField(page, 'mood');
			await page.locator(`${FIELD_FORM} h2`).waitFor();
			const button = page.locator(`${FIELD_FORM} button.rename`);
			assert.equal((await button.innerText()).trim(), 'rename');
			const [box, name] = [await button.boundingBox(), await page.locator(`${FIELD_FORM} h2`).boundingBox()];
			assert.ok(Math.abs(box.y + box.height / 2 - (name.y + name.height / 2)) < 4, 'rename is not beside the name');

			// A name that cannot be is refused by the dry run, and asked again saying why.
			const ask = page.locator('dialog.ask');
			await button.click();
			assert.equal(await ask.locator('input').inputValue(), 'mood');
			await ask.locator('input').fill('spec');
			await ask.locator('button[value]').click();
			await until(() => ask.locator('span').innerText(), (t) => t.includes('spec') && t.endsWith('rename mood to:') && t !== 'rename mood to:');
			assert.equal(await ask.locator('input').inputValue(), 'spec');
			await ask.locator('input').fill('feeling');
			await ask.locator('button[value]').click();
			// The dry run's counts — the two switches and the TRACK row — and nothing moved yet.
			const confirm = page.locator('dialog.confirm');
			await confirm.waitFor();
			const counted = await confirm.locator('p').innerText();
			for (const fact of [/\bmood\b/, /\bfeeling\b/, /\b3\b/]) assert.match(counted, fact);
			assert.equal((await app.entries()).filter((e) => e.field === 'mood').length, 2);
			await confirm.locator('button[value="rename"]').click();

			// The form is on the new name, and the panes show it with its entries.
			await until(() => heading(page), (t) => t === 'feeling');
			assert.deepEqual((await app.entries()).map((e) => e.field).sort(), ['feeling', 'feeling']);
			await until(() => tail.innerText(), (t) => t.includes('feeling') && !t.includes('mood'));
			// The bar's pill follows the rename (da-upig4), so collect is on the field's own view.
			await page.locator('[data-open-field="feeling"]').waitFor();
			assert.equal(await page.locator('.bar button[data-field="mood"], [data-open-field="mood"]').count(), 0);
			// One undo takes it all back.
			assert.equal((await app.run(['undo'])).code, 0);
			assert.deepEqual((await app.entries()).map((e) => e.field), ['mood', 'mood']);
			assert.deepEqual(app.errors, []);
		});
	});

	// jasper on #944: at 390 px the placeholder was cut beside «fields» (da-qimwi); the line is a pill and its buttons now (da-4ta1k).
	test('on a phone a field\'s pill and its rename stand on one line', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.run(['track', 'tea', '--verb', 'occur']);
			await app.redraw();
			await openField(page);
			await page.locator(`${FIELD_FORM} button.track-something`).waitFor();
			await page.screenshot({ path: process.env.DATUM_SHOT ? `${process.env.DATUM_SHOT}/new.png` : undefined });
			await leaveField(page);

			await openField(page, 'tea');
			await page.locator(`${FIELD_FORM} h2`).waitFor();
			const [pill, rename] = [
				await page.locator(`${FIELD_FORM} .name-line .pill`).boundingBox(),
				await page.locator(`${FIELD_FORM} button.rename`).boundingBox()
			];
			assert.ok(Math.abs(pill.y + pill.height / 2 - (rename.y + rename.height / 2)) < 4 && rename.x > pill.x, 'rename does not stand next to the pill');
			await page.screenshot({ path: process.env.DATUM_SHOT ? `${process.env.DATUM_SHOT}/field.png` : undefined });
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	// vividn, 2026-10-28: «don't freeze the shapes at the top of the setup widget allow them to scroll with everything
	// else / pressing enter on the field text box should close the keyboard, not advance to description / likewise,
	// pressing enter on the description box should close the keyboard, not save the field» (da-uz2td).
	test('on a phone the shapes scroll away with the form, over the tabs and Save, which stay', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.run(['track', 'mood', '--verb', 'switch']);
			const spec = JSON.parse((await app.run(['setup', 'mood', '--json'])).stdout);
			const r = await app.run(['setup', 'mood', '--from-json', JSON.stringify({ ...spec, allowed_states: 'abcdefghijkl'.split('') })]);
			assert.equal(r.code, 0, r.stderr);
			await openField(page, 'mood');
			await page.locator(`${FIELD_FORM} h2`).waitFor();
			const seen = await page.locator(`${FIELD_FORM} .verbs`).evaluate((verbs) => {
				let box = verbs.parentElement;
				while (box && !['auto', 'scroll'].includes(getComputedStyle(box).overflowY)) box = box.parentElement;
				if (!box) return null;
				const taller = box.scrollHeight - box.clientHeight;
				box.scrollTop = box.scrollHeight;
				const form = verbs.closest('form');
				return {
					taller,
					holds: ['[data-panel]', '.tabs'].map((sel) => box.contains(form.querySelector(sel))),
					shapes: verbs.getBoundingClientRect().bottom - box.getBoundingClientRect().top,
					tabs: form.querySelector('.tabs').getBoundingClientRect().bottom - innerHeight
				};
			});
			assert.ok(seen, 'nothing scrolls the shapes');
			assert.ok(seen.taller > 0, 'the form fits the screen: nothing to scroll');
			assert.deepEqual(seen.holds, [true, false], 'the shapes scroll with the panel, and the tabs do not');
			assert.ok(seen.shapes <= 0, `scrolled down, the shapes are still ${seen.shapes}px in view`);
			assert.ok(seen.tabs <= 0, `scrolled down, the tabs are ${seen.tabs}px under the screen`);
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	for (const box of ['input.description'])
		test(`on a phone Enter in ${box} puts the keyboard away, and neither moves on nor saves`, async () => {
			await withApp(async (app) => {
				const { page } = app;
				const { saved } = await openField(page);
				let answered = false;
				saved.then(() => (answered = true), () => {});
				await nameField(page, 'tea');
				const input = page.locator(`${FIELD_FORM} ${box}`);
				await input.click();
				await input.press('Enter');
				assert.equal(await typing(page), false, 'the focus is in a box still');
				assert.equal(await fieldNamed(page), 'tea', 'the form is not the new field');
				assert.equal(JSON.parse((await app.run(['fields', '--json'])).stdout).some((f) => f.field === 'tea'), false, 'Enter saved tea');
				assert.equal(answered, false, 'Enter closed the place');
				// A phone's key reads Done, and is Enter: «next» would move the focus without one.
				assert.equal(await input.getAttribute('enterkeyhint'), 'done');
				await leaveField(page);
				assert.equal(await saved, null);
				assert.deepEqual(app.errors, []);
			}, PHONE);
		});
});

// A test that throws with a place open fails with what it threw, not with
// the save's «Target page closed» as its page is closed under it (da-w3y9q).
test('a test that throws with the place open fails as itself', async () => {
	await assert.rejects(
		withApp(async ({ page }) => {
			await openField(page, 'coffee');
			await page.locator(FIELD_FORM).waitFor();
			throw new Error('the real assertion');
		}),
		/the real assertion/
	);
});

// A spec saves when it is left, and its bottom is Revert · Save · Redo (da-hda4j), vividn, 2026-10-26: «if you
// navigate away from a spec, maybe we should autosave it? But we change the bottom to have [Revert] and [Save]
// buttons, clicking revert goes back to the unsaved settings, or if already at a save checkpoint undoes the spec to
// the previous save (maybe we also then need a [redo] button for undoing that revert??)», and how far: «whole saved
// history».
const described = async (app, field) => JSON.parse((await app.run(['setup', field, '--json'])).stdout).description;
/** Press the setup command its button was (da-r674m): `commit` is the primary, save or start tracking. */
const press = (page, name) => setupCommand(page, name === 'commit' ? undefined : name);
const description = (page) => page.locator(`${FIELD_FORM} input.description`);

/** The widget opened, then `field` picked in its own list of fields: reached by the person themselves (da-wz84y). */
async function reach(page, field) {
	await openField(page);
	await trackSomething(page);
	await page.locator(`${PICKER} [data-chip="${field}"]`).click();
	await until(() => heading(page), (t) => t === field);
	await description(page).waitFor();
}

describe('a spec saves when it is left, and Revert and Redo walk its saved history', () => {
	const desktop = sharedApp(DESKTOP);
	/** Which of revert, save and redo can be pressed. */
	const enabled = async (page) => {
		const on = Object.fromEntries(await setupCommands(page));
		return { revert: on.revert, commit: on.save, redo: on.redo };
	};

	test('going to another field saves the edits, and so does closing the place', async () => {
		await desktop.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'tea', '--verb', 'occur']);
			await app.run(['track', 'nap', '--verb', 'start']);
			await app.redraw();
			await openField(page, 'tea');
			await page.locator(`${FIELD_FORM} h2`).waitFor();
			await description(page).fill('green');
			await trackSomething(page);
			await page.locator(`${PICKER} [data-chip="nap"]`).click();
			await until(() => heading(page), (t) => t === 'nap');
			await until(() => described(app, 'tea'), (d) => d === 'green');
			// The heading follows at once; the form is nap's once it is read.
			await until(() => description(page).inputValue(), (v) => v === '');

			await description(page).fill('short');
			await page.evaluate(() => window.datum.shell.leavePlace());
			await page.locator('dialog.sheet[data-place="setup"]').waitFor({ state: 'detached' });
			await until(() => described(app, 'nap'), (d) => d === 'short');
			assert.deepEqual(app.errors, []);
		});
	});

	test('Revert drops unsaved edits; at a save point it steps back through every version, the CLI\'s too, and Redo forward', async () => {
		await desktop.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'tea', '--verb', 'occur']);
			const spec = JSON.parse((await app.run(['setup', 'tea', '--json'])).stdout);
			for (const said of ['one', 'two']) await app.run(['setup', 'tea', '--from-json', JSON.stringify({ ...spec, description: said })]);
			// Reached by the person's own pick, so its Save stays (da-wz84y).
			await app.redraw();
			await reach(page, 'tea');
			assert.equal(await description(page).inputValue(), 'two');
			assert.deepEqual(await enabled(page), { revert: true, commit: false, redo: false }, 'at the newest save');

			// Unsaved edits: Revert drops them and writes nothing.
			const before = (await app.echoes()).length;
			await description(page).fill('three');
			assert.deepEqual(await enabled(page), { revert: true, commit: true, redo: false }, 'with edits');
			await press(page, 'revert');
			await until(() => description(page).inputValue(), (v) => v === 'two');
			assert.equal((await app.echoes()).length, before, 'dropping the edits ran something');
			assert.equal(await described(app, 'tea'), 'two');

			// At a save point: back a version a press, to the first, as `revert SPEC --to-version N`.
			const { hid } = JSON.parse((await app.run(['history', 'spec:tea', '--json'])).stdout)[0];
			for (const [shown, version] of [['one', 2], ['', 1]]) {
				await press(page, 'revert');
				await until(() => description(page).inputValue(), (v) => v === shown);
				assert.equal((await app.echoes()).at(-1), `revert ${hid} --to-version ${version}`);
				assert.equal(await described(app, 'tea'), shown || null);
			}
			assert.deepEqual(await enabled(page), { revert: false, commit: false, redo: true }, 'at the first version');

			// Redo walks forward again, up to where the walk began.
			for (const [shown, version] of [['one', 2], ['two', 3]]) {
				await press(page, 'redo');
				await until(() => description(page).inputValue(), (v) => v === shown);
				assert.equal((await app.echoes()).at(-1), `revert ${hid} --to-version ${version}`);
				assert.equal(await described(app, 'tea'), shown);
			}
			assert.deepEqual(await enabled(page), { revert: true, commit: false, redo: false }, 'back where the walk began');

			// A save is a new save point: the walk ahead of it is gone.
			await press(page, 'revert');
			await until(() => description(page).inputValue(), (v) => v === 'one');
			await description(page).fill('four');
			await press(page, 'commit');
			await until(() => described(app, 'tea'), (d) => d === 'four');
			await until(() => enabled(page), (e) => e.revert && !e.commit && !e.redo);
			assert.equal(await description(page).inputValue(), 'four');
			assert.deepEqual(app.errors, []);
		});
	});
});

// The bottom row by how the person came (da-wz84y), vividn, 2026-10-26: «maybe it should depend on how you got there?
// Like if you navigate yourself to the sync widget, stay on the field, probably flash some sort of field saved
// underneath the save button or something», and «I guess there could be a "cancel" button, that always goes back to
// where you came from. And then revert always just reverts to the last state without going back. If you navigate
// there on your own, maybe no cancel button. I guess to follow our convention if hitting save or cancel takes you
// somewhere else it should have a little chevron on it, right?»
// Its buttons are commands now (da-r674m): the row is the commands', each `[name, enabled]`, and what a save did is
// the output section's.
describe('the commands by how the person came', () => {
	const desktop = sharedApp(DESKTOP);
	const row = setupCommands;
	const place = (page) => page.locator('dialog.sheet[data-place="setup"]');
	const OWN = [['save', false], ['revert', false], ['redo', false], ['cancel', false]];
	// With a save behind it, revert steps back to it.
	const SAVED = [['save', false], ['revert', true], ['redo', false], ['cancel', false]];
	const BROUGHT = [['save', false], ['revert', false], ['redo', false], ['cancel', true]];

	test('reached by its own list: save · revert · redo, cancel grayed; save stays; revert stays', async () => {
		await desktop.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'tea', '--verb', 'occur']);
			await app.run(['track', 'nap', '--verb', 'start']);
			await app.redraw();
			await reach(page, 'tea');
			assert.deepEqual(await row(page), OWN);

			await description(page).fill('green');
			await press(page, 'commit');
			await until(() => described(app, 'tea'), (d) => d === 'green');
			assert.match((await app.echoes()).at(-1), /^setup tea --from-json /);
			assert.equal(await place(page).isVisible(), true, 'Save left the place');
			assert.equal(await heading(page), 'tea');

			// At the save point Revert steps back a version, and stays.
			await press(page, 'revert');
			await until(() => description(page).inputValue(), (v) => v === '');
			assert.equal(await place(page).isVisible(), true, 'Revert left the place');

			assert.deepEqual(app.errors, []);
		});
	});

	test('brought from another pane: cancel too; revert stays, cancel drops the edits and goes back, save saves and goes back', async () => {
		await desktop.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'tea', '--verb', 'occur']);
			await app.redraw();
			await openField(page, 'tea', undefined, true);
			await description(page).waitFor();
			assert.deepEqual(await row(page), BROUGHT);

			await description(page).fill('green');
			await press(page, 'revert');
			await until(() => description(page).inputValue(), (v) => v === '');
			assert.equal(await place(page).isVisible(), true, 'Revert left the place');

			const before = (await app.echoes()).length;
			await description(page).fill('green');
			await press(page, 'cancel');
			await place(page).waitFor({ state: 'detached' });
			await app.idle();
			assert.equal((await app.echoes()).length, before, 'Cancel ran something');
			assert.equal(await described(app, 'tea'), null);

			await openField(page, 'tea', undefined, true);
			await description(page).fill('green');
			await press(page, 'commit');
			await place(page).waitFor({ state: 'detached' });
			assert.equal(await described(app, 'tea'), 'green');

			// A pick in its own list is reaching it: the row is the person's own again.
			await openField(page, '', undefined, true);
			await trackSomething(page);
			await page.locator(`${PICKER} [data-chip="tea"]`).click();
			await until(() => row(page), (r) => JSON.stringify(r) === JSON.stringify(SAVED));
			assert.deepEqual(app.errors, []);
		});
	});

	test('a new field keeps start tracking · save without tracking · cancel; brought, its cancel goes back', async () => {
		await desktop.solo(async (app) => {
			const { page } = app;
			await openField(page, undefined, undefined, true);
			await nameField(page, 'tea');
			assert.deepEqual(await row(page), [['start tracking', false], ['save without tracking', false], ['cancel', true]]);
			await press(page, 'cancel');
			await place(page).waitFor({ state: 'detached' });
			assert.deepEqual(app.errors, []);
		});
	});

	test('on a phone, brought from the main tab, Save and Cancel go back to it', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const front = () => page.locator('.root-bar [role="tab"][aria-selected="true"]').evaluate((b) => b.textContent.replace('•', '').trim());
			await app.run(['track', 'tea', '--verb', 'occur']);
			await app.redraw();
			const home = await front();
			for (const leave of ['cancel', 'commit']) {
				await openField(page, 'tea', undefined, true);
				await description(page).waitFor();
				await until(front, (t) => t === 'setup');
				assert.deepEqual(await row(page), BROUGHT);
				await description(page).fill(leave);
				await press(page, leave);
				await until(front, (t) => t === home);
			}
			assert.equal(await described(app, 'tea'), 'commit');
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});
});

describe("the setup widget in the database's colors", () => {
	// vividn, 2026-10-28 (da-c0laf): «many of the components (for example the
	// buttons to select field shape) seem to take their background color from
	// the deault db rather than the current db, so the color clashes».
	/** The hue of an `rgb(...)` string, in degrees. */
	const hue = (rgb) => {
		const [r, g, b] = rgb.match(/\d+/g).map(Number);
		const max = Math.max(r, g, b);
		const c = max - Math.min(r, g, b);
		const h = max === r ? (g - b) / c : max === g ? (b - r) / c + 2 : (r - g) / c + 4;
		return (h * 60 + 360) % 360;
	};
	/** How far apart two hues are, around the wheel. */
	const apart = (a, b) => Math.min(Math.abs(a - b), 360 - Math.abs(a - b));
	/** The page's ground and each shape button's, in `query`'s database. */
	async function painted(query) {
		let seen;
		await withApp(async ({ page }) => {
			await openField(page);
			await nameField(page, 'thing');
			await page.locator(`${FIELD_FORM} .verb`).first().waitFor();
			seen = {
				page: await page.evaluate(() => getComputedStyle(document.body).backgroundColor),
				shapes: await page.locator(`${FIELD_FORM} .verb`).evaluateAll((all) => all.map((b) => getComputedStyle(b).backgroundColor))
			};
		}, { ...DESKTOP, query });
		return seen;
	}

	test("the shape buttons' ground takes the page's hue, in each database", async () => {
		const datum = await painted('');
		const work = await painted('?db=work');
		assert.notEqual(work.page, datum.page, 'the two databases are one colour');
		for (const db of [datum, work]) {
			assert.equal(db.shapes.length, VERBS.length);
			// An 8-bit channel at this lightness holds a hue to a few degrees.
			for (const shape of db.shapes) assert.ok(apart(hue(shape), hue(db.page)) < 8, `a shape is ${shape} on a page of ${db.page}`);
		}
		assert.notEqual(work.shapes[0], datum.shapes[0], "the shapes are the default database's in both");
	});
});
