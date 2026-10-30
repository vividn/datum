// The CLI widget as a terminal: a clock, a stamp per command, and a scrollback
// that is still there tomorrow. Its prompt is the bottom bar's (da-rfs9):
// the pane is the scrollback, and nothing in it takes typing.
//
// Four things that all look cosmetic and are not (da-9ozz):
//
//   A clock beside the prompt is the app saying what time it thinks it is
//   — the same 24-hour convention every rendered row is in, ticking, so a
//   backdated `-t` can be typed against something rather than guessed at.
//
//   A stamp per command is when you typed it, dim on purpose: the row's
//   own `occur_time` under its field is the fact that matters. Past
//   midnight the stamp grows a day, as `datum tail` frames one (da-hxjrk),
//   through the tail's own `dateLabel` past three days rather than a
//   second date formatter (the shape of da-6eou).
//
//   A frame around each command is what makes a screenful of output
//   legible as commands rather than as one wall.
//
//   And persistence is the difference between a terminal and a text box:
//   a reload used to empty both the scrollback and the recall history,
//   so anything you had learned from the last ten minutes was gone.
//
// The assertions read the screen, not the module: the stamp is asserted
// dimmer than the echo by its computed colour, and the restored scrollback
// by its text and its times rather than by a count.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { HELD_AT, startApp, stopApp, withApp } from './support/app.mjs';
import { dragBar } from './support/bar.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const HHMMSS = /^\d{2}:\d{2}:\d{2}$/;

/** `HH:MM:SS` as a second of the day. */
const seconds = (label) => {
	const [h, m, s] = label.split(':').map(Number);
	return h * 3600 + m * 60 + s;
};

/** How far `later` is after `earlier`, in seconds, across midnight. */
const apart = (earlier, later) => (seconds(later) - seconds(earlier) + 86400) % 86400;

/**
 * The live clock — the bottom bar's.
 *
 * Under the logo since da-xfa4, not on the prompt line: vividn asked for it
 * "underneath the logo … nice and bright", lined up with the input row.
 */
/**
 * The bar's CLOCK — `.section.input`'s, not an entry's.
 *
 * The bar holds two `.stamp`s since da-ckw3 made its output section row one
 * of the history (vividn, 2026-09-21): the clock on the input row, and the
 * newest entry's own time under it. They are each right in their own
 * component; a selector that reaches into the bar has to say which it means.
 */
const promptClock = (page) => page.locator('.prompt-bar .section.input .stamp').innerText();

/** Every scrollback stamp, oldest first. */
const stamps = (page) => page.locator('.cli .head .stamp').allInnerTexts();

/** A colour's relative brightness, enough to compare two greys by. */
const brightness = (css) => {
	const [r, g, b] = css.match(/\d+/g).map(Number);
	return 0.2126 * r + 0.7152 * g + 0.0722 * b;
};

describe("the CLI widget's prompt, the bottom bar's", () => {
	test('carries a clock, beside the logo and on the prompt’s line, that counts up', async () => {
		// The 12-hour reader again (da-6o81): a clock is exactly the thing
		// that goes locale-shaped when nobody pins the locale in the test.
		await withApp(
			async ({ page }) => {
				const first = await promptClock(page);
				assert.match(first, HHMMSS, 'the prompt clock is not datum 24-hour HH:MM:SS');

				// Left of the caret and past the brand, on the screen: da-ckw3
				// put the mark, the clock, the `❯` and the box on ONE row.
				const seen = await page.evaluate(() => {
					const box = (s) => document.querySelector(s).getBoundingClientRect();
					return {
						clockRight: box('.prompt-bar .section.input .stamp').right,
						clockLeft: box('.prompt-bar .section.input .stamp').left,
						caretLeft: box('.prompt-bar .line > .prompt').left,
						brandRight: box('.prompt-bar .brand').right
					};
				});
				assert.ok(seen.clockRight <= seen.caretLeft, 'the clock is not left of the caret');
				assert.ok(seen.clockLeft >= seen.brandRight - 1, 'the clock is not past the logo');

				await page.waitForFunction(
					(was) => document.querySelector('.prompt-bar .section.input .stamp')?.innerText !== was,
					first
				);
				const second = await promptClock(page);
				assert.match(second, HHMMSS);
				const moved = apart(first, second);
				assert.ok(moved >= 1 && moved <= 3, `the clock did not tick: ${first} then ${second}`);
			},
			// The wall clock: this is a clock counting up (da-ydao).
			{ locale: 'en-US', clock: null }
		);
	});
});

describe('a command in the scrollback', () => {
	test('is stamped with the time it ran, dimly, beside the echo and not in it', async () => {
		await withApp(async ({ page, submit, echoes }) => {
			const before = await promptClock(page);
			await submit('occur coffee');
			const after = await promptClock(page);

			const [stamp] = await stamps(page);
			assert.match(stamp, HHMMSS);
			// Between the clock read before the command and the one read
			// after it: the stamp is the moment of the run, not of the render.
			assert.ok(
				apart(before, stamp) <= apart(before, after) + 1,
				`stamped ${stamp}, but the run was between ${before} and ${after}`
			);

			// The echo is still only the command. Every other test in this
			// suite reads it that way, and a stamp folded into it would have
			// changed all of them quietly.
			assert.deepEqual(await echoes(), ['occur coffee']);

			const { stampColour, echoColour } = await page.evaluate(() => {
				const head = document.querySelector('.cli .head');
				return {
					stampColour: getComputedStyle(head.querySelector('.stamp')).color,
					echoColour: getComputedStyle(head.querySelector('.echo')).color
				};
			});
			assert.ok(
				brightness(stampColour) < brightness(echoColour),
				`the stamp (${stampColour}) is not dimmer than the command (${echoColour})`
			);
		});
	});

	test('frames another day as `datum tail` does: -1d within three days, the date past them', async () => {
		// The clock cannot be wound forward mid-session, so the days are moved
		// under the app instead: a scrollback written earlier, restored today.
		// That is also the only way this is ever seen in real use. Entries
		// logged at the same instants are what `tail` prints them as (da-hxjrk).
		const runs = [5 * 24, 26].map((hours) => HELD_AT - hours * 3600 * 1000);
		await withApp(
			async (app) => {
				for (const at of runs) await app.submit(`occur coffee -t ${new Date(at).toISOString()}`);
				const tail = await until(
					() => app.paneTable('tail'),
					(t) => t.rows.filter((r) => r.includes('coffee')).length === runs.length
				);
				const printed = tail.rows.filter((r) => r.includes('coffee')).map((r) => r[tail.keys.indexOf('time')]);
				assert.match(printed[0], /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/);
				assert.match(printed[1], /^-1d \d{2}:\d{2}:\d{2}$/);
				const restored = (await stamps(app.page)).filter((stamp) => !HHMMSS.test(stamp));
				// Compared as sets: the scrollback's order is its own, `tail`'s by time.
				assert.deepEqual(restored.sort(), [...printed].sort());
			},
			{
				prepare: (context) =>
					context.addInitScript(
						([key, runs]) => {
							localStorage.setItem(
								key,
								JSON.stringify({
									scrollback: runs.map((ranAt) => ({
										line: 'tail',
										ranAt,
										result: { argv: ['tail'], code: 0, stdout: 'nothing yet', stderr: '' }
									})),
									history: ['tail']
								})
							);
						},
						['datum.repl.v1', runs]
					)
			}
		);
	});
});

describe('the space between commands', () => {
	// EACH COMMAND IS A FRAME (da-hxjrk), its time on the frame's border, so a
	// seam between two commands is their two frames' edges with air between
	// them, beside each other or one over the other, and nothing is ruled.
	const seams = (page) =>
		page.evaluate(() => {
			const cells = [...document.querySelectorAll('.cli .entry')];
			const top = (el) => Math.round(el.getBoundingClientRect().top);
			const frames = cells.map((el) => el.querySelector('.frame'));
			const first = cells.length ? top(cells[0]) : 0;
			const above = frames.filter((_, i) => top(cells[i]) === first);
			const below = cells.filter((el) => top(el) !== first);
			return {
				rows: new Set(cells.map(top)).size,
				framed: frames.filter((f) => parseFloat(getComputedStyle(f).borderTopWidth) >= 1).length,
				ruled: cells.filter((el) => getComputedStyle(el).borderTopWidth !== '0px').length,
				// The first row's lowest frame against the top of the next row's time.
				air: below.length
					? Math.min(...below.map((el) => el.querySelector('.stamp').getBoundingClientRect().top)) -
						Math.max(...above.map((f) => f.getBoundingClientRect().bottom))
					: null,
				frame: frames.length ? getComputedStyle(frames[0]).borderTopColor : null,
				echo: frames.length ? getComputedStyle(frames[0].querySelector('.echo')).color : null
			};
		});

	test('is each command’s own frame, dimmer than the command, with air between two rows', async () => {
		await withApp(async ({ page, submit }) => {
			await submit('occur coffee');
			await submit('occur tea');
			// A `tail` answers with a table, which takes a row of its own
			// however many columns there are — so there is certainly a seam
			// here, whatever the first two entries did.
			await submit('tail');
			const three = await seams(page);
			assert.ok(three.rows > 1, 'a table did not start a row of its own');
			assert.equal(three.framed, 3, 'a command is not framed');
			assert.equal(three.ruled, 0, 'a row is ruled as well as framed');
			assert.ok(three.air > 0, `the rows' frames touch: ${three.air}px`);
			assert.ok(
				brightness(three.frame) < brightness(three.echo),
				`the frame (${three.frame}) is not dimmer than the command (${three.echo})`
			);
		});
	});
});

describe('the scrollback across a reload', () => {
	test('comes back with its output, its times and its recall history', async () => {
		await withApp(async (app) => {
			const { page, submit, reload, echoes } = app;

			await submit('occur coffee');
			await submit('tail');
			const before = { echoes: await echoes(), stamps: await stamps(page) };
			const outputs = await app.outputs();

			await reload();

			assert.deepEqual(await echoes(), before.echoes, 'the scrollback did not come back');
			assert.deepEqual(await stamps(page), before.stamps, 'the times did not come back');
			assert.deepEqual(await app.outputs(), outputs, 'the output did not come back');
			// `tail`'s table takes a row of its own, so there is one seam
			// above it whatever the pane's width (da-5iyo).
			assert.equal(await page.locator('.cli .entry.ruled').count(), 1);

			// Restored, not re-run: `tail` printed one coffee before the
			// reload and would print it again — but a second execution would
			// also have appended a third command.
			assert.equal((await echoes()).length, 2);

			// The recall history came back too, which is the half of this a
			// scrollback alone would not give you.
			const input = page.locator('.prompt-bar input');
			await input.press('ArrowUp');
			assert.equal(await input.inputValue(), 'tail');

            // And it keeps growing from there.
			await submit('occur water');
			await reload();
			assert.deepEqual(await echoes(), [...before.echoes, 'occur water']);
			assert.equal((await stamps(page)).length, 3);
		});
	});

	test('is empty, and quiet, when the browser blocks the store', async () => {
		await withApp(
			async ({ page, submit, echoes, reload, errors, toCollect }) => {
				// Nothing remembers a visit, so each load is a first one, on
				// `welcome` (da-u7vz8): the CLI widget is a tab away.
				await toCollect();
				// The session itself works — a blocked store must cost the
				// scrollback, not the terminal.
				await submit('occur coffee');
				assert.deepEqual(await echoes(), ['occur coffee']);
				assert.match((await stamps(page))[0], HHMMSS);

				await reload();
				await toCollect();
				await page.locator('.cli').first().waitFor();
				assert.deepEqual(await echoes(), [], 'something was restored from a blocked store');
				assert.deepEqual(errors, [], `the blocked store threw at the page: ${errors[0]}`);
			},
			{
				prepare: (context) =>
					context.addInitScript(() => {
						Object.defineProperty(window, 'localStorage', {
							configurable: true,
							get() {
								throw new DOMException('blocked', 'SecurityError');
							}
						});
					})
			}
		);
	});
});

// THE REPL IS THE CLI NOW (da-mwp4m). vividn, 2026-10-27: «let's rename the
// REPL to be CLI, I think that is a more commonly understood acronym». A
// widget kind's id is a saved-layout key, so the rename must lose no one's
// layout: `repl` in a stored tree opens as the CLI (`RETIRED_KINDS`,
// layout/shell.ts), and `/repl` is `/cli` (`ALIASES`, layout/defaults.ts).
describe('the CLI, which was the REPL', () => {
	/** A `main` draft stored before the rename: the old kind, beside a tail. */
	const OLD = JSON.stringify({
		version: 1,
		name: 'main',
		root: {
			id: 'root',
			active: 0,
			bar: { edge: 'top', align: 'start' },
			tabs: [
				{
					id: 'root-tab',
					content: {
						id: 'split',
						direction: 'column',
						ratios: [0.5, 0.5],
						children: ['repl', 'tail'].map((kind) => ({
							id: `old-${kind}`,
							tabs: [{ id: `old-${kind}-tab`, content: { id: `old-${kind}-widget`, kind, settings: {} } }],
							active: 0,
							bar: { edge: 'top', align: 'start' }
						}))
					}
				}
			]
		}
	});

	/** Every widget kind in the root tab in front. */
	const kinds = (page) =>
		page.evaluate(() => {
			const all = [];
			const walk = (content) => {
				if (content.kind) all.push(content.kind);
				for (const child of content.children ?? []) for (const tab of child.tabs) walk(tab.content);
			};
			const { root } = window.datum.layout.current();
			walk(root.tabs[root.active].content);
			return all;
		});

	test('a stored layout naming `repl` opens it as the CLI widget, and a command lands in it', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await page.locator('section[data-pane="old-repl"][data-title="cli"]').waitFor();
				assert.deepEqual(await kinds(page), ['cli', 'tail']);
				await app.submit('occur coffee');
				assert.deepEqual(await app.echoes(), ['occur coffee']);
				// And the bar's output is drawn: the CLI is not alone here.
				assert.equal(await page.locator('.prompt-bar .section.output').count(), 1);
				assert.deepEqual(app.errors, []);
			},
			{ prepare: (context) => context.addInitScript((json) => localStorage.setItem('datum.layout.draft.v1.main', json), OLD) }
		);
	});

	// vividn's answer, 2026-10-27: «for /repl or /cli (both can work) let's
	// have this be an exception where the output bar is then completely gone
	// on this full screen widget, since it basically becomes the output
	// section».
	for (const path of ['cli', 'repl'])
		test(`/${path} opens the CLI alone, and the bar draws no output section there`, async () => {
			await withApp(async (app) => {
				const { page } = app;
				const output = page.locator('.prompt-bar .section.output');
				await output.waitFor();
				await app.goto(`/${path}`);
				await page.locator('.root-bar [role="tab"][aria-selected="true"]', { hasText: 'cli' }).waitFor();
				assert.deepEqual(await kinds(page), ['cli']);
				await app.submit('occur coffee');
				assert.equal(await output.count(), 0, 'the CLI page drew the output section');
				// The input row is all of the bar, on the foot of the page.
				const [bar, input] = await Promise.all(
					['.prompt-bar', '.prompt-bar .section.input'].map((s) => page.locator(s).boundingBox())
				);
				assert.ok(Math.abs(bar.y + bar.height - (input.y + input.height)) <= 1 && bar.height - input.height <= 1.5);

				// No marks either: the widget says what a command said here, and
				// an ERROR would have no output to open.
				await app.submit('occur');
				assert.equal(await page.locator('.prompt-bar .reopen').count(), 0);

				// A drag on the bar here moves nothing, nor does PageUp, and neither
				// writes a fold for the pages that have an output (canary-qwen on #1256).
				const fold = () => page.evaluate(() => localStorage.getItem('datum.bar.output.v1'));
				const kept = await fold();
				await dragBar(page, 300);
				await dragBar(page, -900);
				await page.locator('.prompt-bar input').focus();
				await page.keyboard.press('PageUp');
				assert.equal(await fold(), kept, 'a drag or PageUp on the CLI page wrote the fold');
				assert.equal(await page.locator('.log section.pane').count(), 0, 'a drag on the CLI page mounted the log');

				// Another tab is drawn with it again, as it was: open, and no log.
				await page.evaluate(() => window.datum.shell.activate(0));
				await output.waitFor();
				assert.equal(
					await page.evaluate(() => getComputedStyle(document.querySelector('.shell')).getPropertyValue('--log-height').trim()),
					'0px',
					'the drag on the CLI page left the log open'
				);
				assert.deepEqual(app.errors, []);
			});
		});
});

describe('the scrollback is its database’s own (da-kb18k)', () => {
	// vividn, 2026-10-28: «the CLI should be per database, right now it seems
	// to show everything in every database». And a purge of one, the
	// tutorial's reset, took every database's with it.
	test('a database shows its own commands, and a purge of another leaves them', async () => {
		await withApp(
			async (app) => {
				// The store every database shared before is the default's.
				assert.deepEqual(await app.echoes(), ['note from before']);
				await app.submit('occur coffee');

				await app.goto('/?db=other');
				assert.deepEqual(await app.echoes(), []);
				await app.submit('occur tea');

				await app.goto('/');
				assert.deepEqual(await app.echoes(), ['note from before', 'occur coffee']);
				await app.goto('/?db=other');
				assert.deepEqual(await app.echoes(), ['occur tea']);

				// A tab on `other` hears its purge from a tab on `datum`; the
				// `datum` tab keeps its own.
				const peer = await app.newTab('');
				await peer.submit('db purge --db other --yes');
				await app.page.waitForFunction(() => document.querySelectorAll('.cli .echo').length === 0);
				assert.deepEqual(await peer.echoes(), ['note from before', 'occur coffee', 'db purge --db other --yes']);
				assert.equal(await app.page.evaluate(() => localStorage.getItem('datum.repl.v1:other')), null);
				assert.match(await app.page.evaluate(() => localStorage.getItem('datum.repl.v1')), /occur coffee/);
				assert.deepEqual(app.errors, []);
			},
			{
				prepare: (context) =>
					context.addInitScript(() => {
						if (localStorage.getItem('datum.repl.v1')) return;
						const result = { argv: ['note', 'from', 'before'], code: 0, stdout: '', stderr: '' };
						localStorage.setItem(
							'datum.repl.v1',
							JSON.stringify({ scrollback: [{ line: 'note from before', ranAt: 0, result }], history: ['note from before'] })
						);
					})
			}
		);
	});
});
