// The custom widget: a pane whose command the reader types (da-wb2n).
//
// A prototype of a pane that can run any datum command, so what is tested
// is what such a pane owes the reader whatever it runs: it follows the
// database like the other panes, a command that WRITES runs once and then
// only when asked, the command survives a reload (and a blocked store costs
// only that), a drawn view is asked for at the pane's size, and a command
// that fails says so without taking the pane or its neighbours with it.
//
// Content assertions throughout — which entry the pane shows, how many
// `tea` rows are stored, the width in the SVG — never "more than before".

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';

before(startApp);
after(stopApp);

/** The custom pane: the one with an edit affordance or an open editor. */
const PANE = 'section.pane:has(h2 button.edit, h2 input.command)';

/** Type `line` into the pane's editor, as a reader does. */
async function setCommand(page, line) {
	await page.locator(`${PANE} button.edit`).click();
	const input = page.locator(`${PANE} input.command`);
	await input.fill(line);
	await input.press('Enter');
	await page.locator(`${PANE} h2`, { hasText: line }).waitFor();
}

/** Wait until the pane's body matches `re`, and return its text. */
async function shows(page, re) {
	await page.waitForFunction(
		([pane, source]) => new RegExp(source).test(document.querySelector(`${pane} .body`)?.innerText ?? ''),
		[PANE, re.source]
	);
	return page.locator(`${PANE} .body`).innerText();
}

/** How many `field` rows are stored, by the CLI's own backup. */
const count = async ({ entries }, field) =>
	(await entries(500)).filter((e) => e.field === field).length;

describe('the custom pane', () => {
	test('follows the database like the other panes', async () => {
		await withApp(async (app) => {
			await setCommand(app.page, 'tail -n 1');
			await app.submit('occur coffee');
			assert.match(await shows(app.page, /coffee/), /coffee/);

			await app.submit('occur tea');
			const text = await shows(app.page, /tea/);
			assert.doesNotMatch(text, /coffee/, `tail -n 1 still shows coffee:\n${text}`);
		});
	});

	test('runs a command that writes once, then only on ▶', async () => {
		await withApp(async (app) => {
			await setCommand(app.page, 'occur tea');
			await app.page.locator(`${PANE} button.play`).waitFor();
			assert.match(
				await app.page.locator(`${PANE} .body`).innerText(),
				/this command writes — it runs only when you press ▶/
			);
			assert.equal(await count(app, 'tea'), 1);

			// An epoch bump. By the time the tail table shows the coffee the
			// panes have all been told; any run the custom pane started on
			// it is ahead of the count in the worker's queue.
			await app.submit('occur coffee');
			await app.page.locator('section.pane td', { hasText: 'coffee' }).first().waitFor();
			assert.equal(await count(app, 'tea'), 1, 'the pane ran `occur tea` again on an epoch');

			await app.page.locator(`${PANE} button.play`).click();
			await app.page.waitForFunction(
				async () => (await window.datum.entries(500)).filter((e) => e.field === 'tea').length === 2
			);
			assert.equal(await count(app, 'tea'), 2);
		});
	});

	// The pane learns a command writes by running it (da-v36y). The command
	// survives a reload, so the knowledge must too: a mount that learns it
	// again writes once more. Counted in the database, since the pane's text
	// is redrawn either way, and over two reloads, since one cannot tell
	// "once more" from "once per mount".
	test('does not run a restored command that writes again on a reload', async () => {
		await withApp(async (app) => {
			assert.equal(await count(app, 'kettle'), 0);
			await setCommand(app.page, 'occur kettle');
			await app.page.locator(`${PANE} button.play`).waitFor();
			assert.equal(await count(app, 'kettle'), 1);

			for (const reload of [1, 2]) {
				await app.reload();
				await app.idle();
				await app.page.locator(`${PANE} button.play`).waitFor();
				assert.equal(await count(app, 'kettle'), 1, `reload ${reload} ran \`occur kettle\` again`);
			}

			// Escape still leaves the command be, and still unrun.
			await app.page.locator(`${PANE} button.edit`).click();
			await app.page.locator(`${PANE} input.command`).press('Escape');
			await app.page.locator(`${PANE} h2`, { hasText: 'occur kettle' }).waitFor();
			await app.idle();
			assert.equal(await count(app, 'kettle'), 1);
		});
	});

	test('keeps its command across a reload', async () => {
		await withApp(async (app) => {
			await setCommand(app.page, 'tail -n 1');
			await app.submit('occur coffee');
			await shows(app.page, /coffee/);

			await app.reload();
			await app.page.locator(`${PANE} h2`, { hasText: 'tail -n 1' }).waitFor();
			assert.match(await shows(app.page, /coffee/), /coffee/);
		});
	});

	// The kind was `command` until da-zqop renamed it `custom`; the pane's
	// own key was not renamed with it, because a line a reader saved under
	// the old build has to come back under the new one. The key is asserted
	// by name so that renaming it later is a deliberate act.
	test('keeps its line under the key that predates the name', async () => {
		await withApp(async (app) => {
			await setCommand(app.page, 'tail -n 1');
			assert.equal(
				await app.page.evaluate(() => localStorage.getItem('datum.command-pane.v1')),
				'tail -n 1'
			);

			// Written by a build that called the widget `command`, read by this one.
			await app.page.evaluate(() => localStorage.setItem('datum.command-pane.v1', 'tail -n 2'));
			await app.reload();
			await app.page.locator(`${PANE} h2`, { hasText: 'tail -n 2' }).waitFor();
		});
	});

	test('works for the visit when the store is blocked', async () => {
		await withApp(
			async (app) => {
				const logged = [];
				app.page.on('console', (m) => m.type() === 'error' && logged.push(m.text()));

				await setCommand(app.page, 'tail -n 1');
				await app.submit('occur coffee');
				await shows(app.page, /coffee/);

				await app.reload();
				await app.page.locator(`${PANE} h2`, { hasText: 'custom' }).waitFor();
				assert.match(await shows(app.page, /type a datum command/), /type a datum command/);
				assert.deepEqual(app.errors, [], `the blocked store threw at the page: ${app.errors[0]}`);
				assert.deepEqual(logged, [], `the blocked store logged an error: ${logged[0]}`);
			},
			{
				prepare: (context) =>
					context.addInitScript(() => {
						Object.defineProperty(window, 'localStorage', {
							configurable: true,
							get() {
								throw new DOMException('storage is blocked', 'SecurityError');
							}
						});
					})
			}
		);
	});

	test('draws dayview at its own width, and again when the window changes', async () => {
		await withApp(
			async (app) => {
				/** The SVG's authored width against the pane's content box. */
				const widths = () =>
					app.page.evaluate((pane) => {
						const body = document.querySelector(`${pane} .body`);
						const style = getComputedStyle(body);
						const src = body.querySelector('svg.svg')?.outerHTML ?? '';
						return {
							svg: Number(src.match(/<svg[^>]*\swidth="([\d.]+)"/)?.[1]),
							box: Math.floor(
								body.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight)
							)
						};
					}, PANE);
				const drawnAt = (box) =>
					app.page.waitForFunction(
						([pane, box]) =>
							(document.querySelector(`${pane} svg.svg`)?.outerHTML ?? '').includes(`width="${box}"`),
						[PANE, box]
					);

				await setCommand(app.page, 'dayview');
				await app.page.locator(`${PANE} svg.svg`).waitFor();
				let { box } = await widths();
				await drawnAt(box);
				const wide = await widths();
				assert.equal(wide.svg, wide.box, `dayview drew ${wide.svg}px into a ${wide.box}px pane`);
				assert.ok(wide.svg < 2000, 'the pane took datum’s 2000px default');

				await app.page.setViewportSize({ width: 800, height: 600 });
				await app.page.waitForFunction(
					([pane, was]) => {
						const body = document.querySelector(`${pane} .body`);
						return body.clientWidth < was;
					},
					[PANE, wide.box]
				);
				({ box } = await widths());
				await drawnAt(box);
				const narrow = await widths();
				assert.equal(narrow.svg, narrow.box, `dayview kept ${narrow.svg}px in a ${narrow.box}px pane`);
				assert.ok(narrow.svg < wide.svg);
			},
			{ viewport: { width: 1280, height: 900 } }
		);
	});

	test('shows a failing command’s own error and stays editable', async () => {
		await withApp(async (app) => {
			const said = await app.run(['foo']);
			assert.notEqual(said.code, 0, `datum foo succeeded: ${said.stdout}`);
			const first = said.stderr.replace(/\x1b\[[0-9;]*m/g, '').split('\n')[0];

			await setCommand(app.page, 'foo');
			await app.page.locator(`${PANE} pre.error`).waitFor();
			assert.ok(
				(await app.page.locator(`${PANE} pre.error`).innerText()).includes(first),
				`the pane did not show the CLI's own error: ${first}`
			);

			// The neighbours never saw it, and still follow the database.
			await app.submit('occur coffee');
			await app.page.locator('section.pane td', { hasText: 'coffee' }).first().waitFor();
			assert.equal(await app.page.locator('section.pane pre.error').count(), 1);
			await app.page
				.locator('section.pane', { has: app.page.locator('h2', { hasText: 'nowview' }) })
				.locator('svg.svg')
				.waitFor();

			// And the pane is still a pane: its command can be changed.
			await setCommand(app.page, 'tail -n 1');
			await app.page.locator(`${PANE} pre.error`).waitFor({ state: 'detached' });
			await shows(app.page, /coffee/);
		});
	});
});
