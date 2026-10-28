// The `❯` as a button (da-xw655): pressed, it turns a quarter, inverts, and
// slides the command widget up over the output section; it closes on the
// `❯` again, on Escape, on a run (which the output then shows) and on a
// choice that fills the line.
//
// vividn, 2026-10-27: «the ❯ become a button, so it has a circle around it.
// When it is clicked (or activated, explained below), it rotates 90 degress
// (with animation please), inverts colors, (so that the button is filled in
// and the chevron is empty), and the command widget slides up overtop the
// output section». And at 17:39, of closing it: «yes all of those … For
// commands that run, then it should close the command widget to display the
// command that just ran and it's output in the outputbar».

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp } from './support/app.mjs';
import { withBar } from './support/layout.mjs';

before(startApp);
after(stopApp);

const BAR = '.prompt-bar';
const CHEVRON = `${BAR} button.prompt`;
const OVER = `${BAR} #bar-commands`;

const shared = sharedApp({ viewport: { width: 1280, height: 800 }, prepare: withBar });
const onPage = async (fn) => {
	const app = await shared();
	await app.page.locator(`${BAR} input`).fill('');
	await app.home();
	await app.empty();
	if (await app.page.locator(OVER).count()) await app.page.locator(CHEVRON).click();
	await app.page.locator(OVER).waitFor({ state: 'detached' });
	return fn(app);
};

/** What the button says to assistive tech, and how it is drawn. */
const chevron = (page) =>
	page.locator(CHEVRON).evaluate((el) => {
		const style = getComputedStyle(el);
		const accent = getComputedStyle(document.documentElement).getPropertyValue('--accent').trim();
		const probe = document.createElement('i');
		probe.style.color = accent;
		document.body.append(probe);
		const accentRgb = getComputedStyle(probe).color;
		probe.remove();
		return {
			label: el.getAttribute('aria-label'),
			expanded: el.getAttribute('aria-expanded'),
			controls: el.getAttribute('aria-controls'),
			transform: style.transform,
			filled: style.backgroundColor === accentRgb,
			radius: style.borderRadius,
			size: el.getBoundingClientRect().height
		};
	});

describe('the ❯ opens the commands over the output', () => {
	test('a press turns it, fills it and slides the commands over the output; a second takes them away', async () => {
		await onPage(async ({ page, errors }) => {
			const shut = await chevron(page);
			assert.equal(shut.label, 'Commands');
			assert.equal(shut.expanded, 'false');
			assert.equal(shut.filled, false, 'the shut ❯ is filled');
			assert.equal(shut.transform, 'none');
			assert.equal(shut.radius, '50%');
			assert.ok(shut.size >= 24, `the ❯ is ${shut.size}px`);

			await page.locator(CHEVRON).click();
			await page.locator(`${OVER} .bar.commands button[data-command="occur"]`).waitFor();
			await page.locator(OVER).evaluate((el) => Promise.all(el.getAnimations({ subtree: true }).map((a) => a.finished)));
			await page.locator(CHEVRON).evaluate((el) => Promise.all(el.getAnimations().map((a) => a.finished)));
			const open = await chevron(page);
			assert.equal(open.expanded, 'true');
			assert.equal(open.controls, 'bar-commands');
			assert.equal(open.filled, true, 'the open ❯ is not filled');
			// A quarter turn is matrix(cos 90°, sin 90°, -sin 90°, cos 90°, 0, 0).
			const [a, b, c, d] = open.transform.match(/-?[\d.e-]+/g).map(Number);
			assert.deepEqual([a, b, c, d].map((n) => Math.round(n)), [0, 1, -1, 0], open.transform);

			// Over the output section exactly: its box, on the input's top edge.
			const boxes = await page.evaluate(() => {
				const at = (s) => document.querySelector(`.prompt-bar ${s}`).getBoundingClientRect();
				const over = at('#bar-commands');
				const output = at('.section.output');
				const input = at('.section.input');
				return { over: [over.top, over.bottom, over.width], output: [output.top, output.bottom, output.width], input: input.top };
			});
			assert.ok(Math.abs(boxes.over[1] - boxes.input) < 1, `the commands end at ${boxes.over[1]}, the input starts at ${boxes.input}`);
			assert.ok(Math.abs(boxes.over[0] - boxes.output[0]) < 1.5, `the commands start at ${boxes.over[0]}, the output at ${boxes.output[0]}`);
			assert.ok(Math.abs(boxes.over[2] - boxes.output[2]) < 1, `the commands are ${boxes.over[2]}px wide, the output ${boxes.output[2]}px`);

			await page.locator(CHEVRON).click();
			await page.locator(OVER).waitFor({ state: 'detached' });
			const again = await chevron(page);
			assert.equal(again.expanded, 'false');
			assert.notEqual(await page.evaluate(() => document.activeElement?.tagName), 'INPUT', 'the ❯ took the caret');
			assert.deepEqual(errors, []);
		});
	});

	test('the keyboard: Enter and Space press it, Escape closes it after a subcommand list is backed out of', async () => {
		await onPage(async ({ page, errors }) => {
			await page.locator(CHEVRON).focus();
			await page.keyboard.press('Enter');
			await page.locator(OVER).waitFor();
			await page.keyboard.press('Space');
			await page.locator(OVER).waitFor({ state: 'detached' });
			await page.keyboard.press('Space');
			await page.locator(OVER).waitFor();

			// `db` opens its subcommands: the first Escape backs out of them
			// and leaves the commands up, the second takes them away.
			await page.locator(`${OVER} button[data-command="db"]`).click();
			await page.locator(`${OVER} button[data-back]`).waitFor();
			await page.keyboard.press('Escape');
			await page.locator(`${OVER} button[data-back]`).waitFor({ state: 'detached' });
			await page.waitForTimeout(50);
			assert.equal(await page.locator(OVER).count(), 1, 'one Escape backed out and closed too');
			await page.keyboard.press('Escape');
			await page.locator(OVER).waitFor({ state: 'detached' });
			assert.equal((await chevron(page)).expanded, 'false');
			assert.deepEqual(errors, []);
		});
	});

	test('a run closes them and the output shows it', async () => {
		await onPage(async ({ page, errors }) => {
			await page.locator(CHEVRON).click();
			await page.locator(OVER).waitFor();
			await page.locator(`${BAR} input`).fill('occur coffee');
			await page.locator(`${BAR} input`).press('Enter');
			await page.locator(OVER).waitFor({ state: 'detached' });
			await page.locator(`${BAR} .section.output .group:last-child`, { hasText: 'coffee' }).waitFor();
			assert.equal((await chevron(page)).expanded, 'false');
			assert.deepEqual(errors, []);
		});
	});

	test('choosing a command fills the line and closes them', async () => {
		await onPage(async ({ page, errors }) => {
			await page.locator(CHEVRON).click();
			await page.locator(`${OVER} button[data-command="occur"]`).click();
			await page.locator(OVER).waitFor({ state: 'detached' });
			assert.equal(await page.locator(`${BAR} input`).inputValue(), 'occur ');
			assert.deepEqual(errors, []);
		});
	});

	// A hold fills the bare command too, but to show its flags: it leaves the
	// commands up, and the flags tapped there compose on the line (canary on
	// PR #1281).
	test('holding a command keeps them up, its flags composing on the line', async () => {
		await onPage(async ({ page, errors }) => {
			await page.locator(CHEVRON).click();
			// Where it comes to rest: a press on a button still sliding under it is no hold.
			await page.locator(`${OVER} button[data-command="occur"]`).waitFor();
			await page.locator(OVER).evaluate((el) => Promise.all(el.getAnimations({ subtree: true }).map((a) => a.finished)));
			const box = await page.locator(`${OVER} button[data-command="occur"]`).boundingBox();
			await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
			await page.mouse.down();
			await page.locator(`${OVER} button[data-back]`).waitFor();
			await page.mouse.up();
			await page.locator(`${OVER} button[data-flag="occur -q"]`).click();
			await page.waitForFunction(() => document.querySelector('.prompt-bar input').value === 'occur -q ');
			await page.locator(`${OVER} button[data-flag="occur -t"]`).click();
			await page.waitForFunction(() => document.querySelector('.prompt-bar input').value === 'occur -q -t ');
			assert.equal(await page.locator(OVER).count(), 1, 'the flags closed the commands');
			assert.deepEqual(errors, []);
		});
	});

	test('with no motion asked for, it turns and slides without animating', async () => {
		await onPage(async ({ page, errors }) => {
			await page.emulateMedia({ reducedMotion: 'reduce' });
			try {
				await page.locator(CHEVRON).click();
				await page.locator(OVER).waitFor();
				const moving = await page.evaluate(() => ({
					turn: getComputedStyle(document.querySelector('.prompt-bar button.prompt')).transitionDuration,
					slides: document.querySelector('.prompt-bar #bar-commands').getAnimations().length
				}));
				assert.equal(moving.turn, '0s');
				assert.equal(moving.slides, 0);
				await page.locator(CHEVRON).click();
				await page.locator(OVER).waitFor({ state: 'detached' });
			} finally {
				await page.emulateMedia({ reducedMotion: null });
			}
			assert.deepEqual(errors, []);
		});
	});
});
