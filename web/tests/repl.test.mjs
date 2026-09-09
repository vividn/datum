// The REPL as a terminal: a clock, a stamp per command, and a scrollback
// that is still there tomorrow.
//
// Four things that all look cosmetic and are not (da-9ozz):
//
//   A clock beside the prompt is the app saying what time it thinks it is
//   — the same 24-hour convention every rendered row is in, ticking, so a
//   backdated `-t` can be typed against something rather than guessed at.
//
//   A stamp per command is when you typed it, dim on purpose: the row's
//   own `occur_time` two lines below is the fact that matters. Past
//   midnight the stamp grows a date, through the tail's own `dateLabel`
//   rather than a second date formatter (the shape of da-6eou).
//
//   A rule between commands is what makes a screenful of output legible
//   as commands rather than as one wall.
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
import { startApp, stopApp, withApp } from './support/app.mjs';

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

/** The live clock on the prompt line. */
const promptClock = (page) => page.locator('.repl .line .stamp').innerText();

/** Every scrollback stamp, oldest first. */
const stamps = (page) => page.locator('.repl .head .stamp').allInnerTexts();

/** A colour's relative brightness, enough to compare two greys by. */
const brightness = (css) => {
	const [r, g, b] = css.match(/\d+/g).map(Number);
	return 0.2126 * r + 0.7152 * g + 0.0722 * b;
};

describe('the REPL prompt', () => {
	test('carries a clock, left of the caret, that counts up', async () => {
		// The 12-hour reader again (da-6o81): a clock is exactly the thing
		// that goes locale-shaped when nobody pins the locale in the test.
		await withApp(
			async ({ page }) => {
				const first = await promptClock(page);
				assert.match(first, HHMMSS, 'the prompt clock is not datum 24-hour HH:MM:SS');

				// Left of the caret, in the DOM and on the screen.
				const [clockRight, caretLeft] = await page.evaluate(() => {
					const line = document.querySelector('.repl .line');
					return [
						line.querySelector('.stamp').getBoundingClientRect().right,
						line.querySelector('.prompt').getBoundingClientRect().left
					];
				});
				assert.ok(clockRight <= caretLeft, 'the clock is not left of the caret');

				await page.waitForTimeout(1100);
				const second = await promptClock(page);
				assert.match(second, HHMMSS);
				const moved = apart(first, second);
				assert.ok(moved >= 1 && moved <= 3, `the clock did not tick: ${first} then ${second}`);
			},
			{ locale: 'en-US' }
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
				const head = document.querySelector('.repl .head');
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

	test('shows a date as well when it was not run today', async () => {
		// The clock cannot be wound forward mid-session, so the day is moved
		// under the app instead: a scrollback written yesterday, restored
		// today. That is also the only way this is ever seen in real use.
		const yesterday = Date.now() - 26 * 3600 * 1000;
		await withApp(
			async ({ page }) => {
				const [stamp] = await stamps(page);
				const day = new Date(yesterday).toISOString().slice(0, 10);
				assert.equal(stamp, `${day} ${new Date(yesterday).toISOString().slice(11, 19)}`);
			},
			{
				prepare: (context) =>
					context.addInitScript(
						([key, ranAt]) => {
							localStorage.setItem(
								key,
								JSON.stringify({
									scrollback: [
										{
											line: 'tail',
											ranAt,
											result: { argv: ['tail'], code: 0, stdout: 'nothing yet', stderr: '' }
										}
									],
									history: ['tail']
								})
							);
						},
						['datum.repl.v1', yesterday]
					)
			}
		);
	});
});

describe('the space between commands', () => {
	test('is one dim rule, between them and not around them', async () => {
		await withApp(async ({ page, submit }) => {
			const rules = () => page.locator('.repl hr.rule').count();

			await submit('occur coffee');
			assert.equal(await rules(), 0, 'a lone command got a separator');

			await submit('occur tea');
			assert.equal(await rules(), 1, 'two commands did not get exactly one separator');

			await submit('tail');
			assert.equal(await rules(), 2);

			// Dim: the pane's own hairline, not a bright line across it.
			const colour = await page.evaluate(
				() => getComputedStyle(document.querySelector('.repl hr.rule')).borderTopColor
			);
			assert.ok(brightness(colour) < 60, `the rule between commands is not dim: ${colour}`);
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
			assert.equal(await page.locator('.repl hr.rule').count(), 1);

			// Restored, not re-run: `tail` printed one coffee before the
			// reload and would print it again — but a second execution would
			// also have appended a third command.
			assert.equal((await echoes()).length, 2);

			// The recall history came back too, which is the half of this a
			// scrollback alone would not give you.
			const input = page.locator('.repl input');
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
			async ({ page, submit, echoes, reload, errors }) => {
				// The session itself works — a blocked store must cost the
				// scrollback, not the terminal.
				await submit('occur coffee');
				assert.deepEqual(await echoes(), ['occur coffee']);
				assert.match((await stamps(page))[0], HHMMSS);

				await reload();
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
