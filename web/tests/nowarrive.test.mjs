// A dot logged now arrives (da-7257d). vividn: «when logged at now, a MOMENT dot
// should appear in the middle of the now section (of course at its correct y
// height) of the nowview and then slide to the left for maybe 500ms, after which
// it locks onto its place on the history section. The dots that appear on the
// very right of the history should not be cropped off by the right edge but
// rather be drawn on top of it.»
//
// The clock is held, so the flow stands still and a slide is read frame by frame
// off the animation itself, which runs on the page's timeline and not that clock.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, todayAt, withApp } from './support/app.mjs';
import { DESK } from './support/desk.mjs';

before(startApp);
after(stopApp);

const NOWVIEW = 'section.pane[data-title="nowview"]';
const AT = todayAt(15);

/** The suite's desk with its nowview at a 15-minute window. */
function desk(orientation = 'horizontal') {
	const tree = structuredClone(DESK);
	const visit = (node) => {
		if (node?.kind === 'nowview') node.settings = { history: '15m', orientation };
		for (const child of [...(node?.tabs ?? []), ...(node?.children ?? [])]) visit(child.content ?? child);
	};
	visit(tree.root);
	return tree;
}

const dot = (field) => `${NOWVIEW} svg.svg circle.point-bg[data-field="${field}"]`;

/**
 * Log `line` as a person does, on the bar, and wait for `field`'s dot and the picture it came in.
 * Each slide it starts stands still from its first frame: it runs in real time, so a loaded box
 * ended it before `frame`, which pauses it, could read it (da-tsb63).
 */
async function log(app, line, field) {
	await app.page.evaluate(() => {
		const animate = Element.prototype.animate;
		Element.prototype.animate = function (...args) {
			const run = animate.apply(this, args);
			if (this.matches('circle.point-bg, circle.point-bg + *')) run.pause();
			return run;
		};
	});
	const input = app.page.locator('.prompt-bar input');
	await input.fill(line);
	await input.press('Enter');
	await app.page.locator(dot(field)).last().waitFor({ state: 'attached' });
}

/** Where the dot's symbol is at `at` ms into its slide, or at rest with none; the strip's middle and the now-line's place. */
const frame = (app, field, at) =>
	app.page.locator(dot(field)).last().evaluate((mark, at) => {
		const center = (el) => {
			const r = el.getBoundingClientRect();
			return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
		};
		const symbol = mark.nextElementSibling;
		for (const el of [mark, symbol])
			for (const a of el.getAnimations()) {
				a.pause();
				a.currentTime = at;
			}
		const svg = mark.ownerSVGElement;
		return { ...center(symbol), stripY: center(svg.querySelector('.now-strip')).y, strip: center(svg.querySelector('.now-strip')).x, line: center(svg.querySelector('.current-time-line')).x, sliding: symbol.getAnimations().length };
	}, at);

describe('a dot logged now arrives', () => {
	test('it starts in the middle of the now section and slides left to its place in about 500 ms, at its own height', async () => {
		await withApp(
			async (app) => {
				await log(app, 'moment coffee', 'coffee');
				const [start, mid, late] = [await frame(app, 'coffee', 0), await frame(app, 'coffee', 250), await frame(app, 'coffee', 499)];
				assert.equal(start.sliding, 1, 'the dot is not sliding');
				assert.ok(Math.abs(start.x - start.strip) < 1, `it starts at ${start.x}, the section's middle is ${start.strip}`);
				assert.ok(late.x < mid.x && mid.x < start.x, `it does not move left: ${start.x}, ${mid.x}, ${late.x}`);
				assert.ok(Math.abs(mid.y - start.y) < 0.01 && Math.abs(late.y - start.y) < 0.01, `its height moved: ${start.y}, ${mid.y}, ${late.y}`);
				// It lands where it stands once the animation is gone, with no jump.
				await app.page.locator(dot('coffee')).last().evaluate((mark) => {
					for (const el of [mark, mark.nextElementSibling]) for (const a of el.getAnimations()) a.finish();
				});
				const rest = await frame(app, 'coffee', 0);
				assert.equal(rest.sliding, 0, 'it is still sliding at the end of its slide');
				assert.ok(Math.abs(late.x - rest.x) < 1, `a jump at the handover: ${late.x} to ${rest.x}`);
				assert.ok(rest.x < rest.line, `it rests at ${rest.x}, in the now section past the line at ${rest.line}`);
				assert.deepEqual(app.errors, []);
			},
			{ clock: AT, holdTimers: true, desk: desk() }
		);
	});

	test('time running down, it slides up the page, in its own lane', async () => {
		await withApp(
			async (app) => {
				await log(app, 'moment coffee', 'coffee');
				const [start, late] = [await frame(app, 'coffee', 0), await frame(app, 'coffee', 499)];
				assert.ok(Math.abs(start.y - start.stripY) < 1, `it starts at ${start.y}, the section's middle is ${start.stripY}`);
				assert.ok(late.y < start.y, `it does not move up: ${start.y}, ${late.y}`);
				assert.ok(Math.abs(late.x - start.x) < 0.01, `its lane moved: ${start.x}, ${late.x}`);
				assert.deepEqual(app.errors, []);
			},
			{ clock: AT, holdTimers: true, desk: desk('vertical') }
		);
	});

	test('each of several slides at its own height', async () => {
		await withApp(
			async (app) => {
				await log(app, 'moment tea , moment water', 'water');
				const [tea, water] = [await frame(app, 'tea', 100), await frame(app, 'water', 100)];
				assert.equal(tea.sliding + water.sliding, 2, 'not both slide');
				assert.notEqual(tea.y, water.y);
				assert.deepEqual(app.errors, []);
			},
			{ clock: AT, holdTimers: true, desk: desk() }
		);
	});

	test('a picture landing meanwhile takes the slide up where it was', async () => {
		await withApp(
			async (app) => {
				await log(app, 'moment coffee', 'coffee');
				await log(app, 'moment tea', 'tea');
				// A box slow enough to end the slide first has none left to read: run by hand, 250 to 380 ms in.
				const slide = await app.page.locator(dot('coffee')).last().evaluate((mark) => mark.getAnimations().map((a) => a.currentTime));
				assert.ok(slide.every((t) => t > 0), `the first dot's slide began again: ${slide}`);
				assert.deepEqual(app.errors, []);
			},
			{ clock: AT, holdTimers: true, desk: desk() }
		);
	});

	test('a dot with a time in the past just appears', async () => {
		await withApp(
			async (app) => {
				await log(app, 'moment coffee -t -5', 'coffee');
				assert.equal((await frame(app, 'coffee', 0)).sliding, 0, 'a dot five minutes back slid');
				assert.deepEqual(app.errors, []);
			},
			{ clock: AT, holdTimers: true, desk: desk() }
		);
	});

	test('with less motion asked for it just appears', async () => {
		await withApp(
			async (app) => {
				await app.page.emulateMedia({ reducedMotion: 'reduce' });
				await log(app, 'moment coffee', 'coffee');
				assert.equal((await frame(app, 'coffee', 0)).sliding, 0, 'it slid');
				assert.deepEqual(app.errors, []);
			},
			{ clock: AT, holdTimers: true, desk: desk() }
		);
	});

	test('an undo during the slide takes it away', async () => {
		await withApp(
			async (app) => {
				await log(app, 'moment coffee', 'coffee');
				assert.equal((await frame(app, 'coffee', 100)).sliding, 1);
				const input = app.page.locator('.prompt-bar input');
				await input.fill('undo');
				await input.press('Enter');
				await app.page.locator(dot('coffee')).waitFor({ state: 'detached' });
				assert.deepEqual(app.errors, []);
			},
			{ clock: AT, holdTimers: true, desk: desk() }
		);
	});
});

describe('a dot at the newest instant of the history', () => {
	test('is whole, over the edge: a press past the now-line finds it', async () => {
		await withApp(
			async (app) => {
				await log(app, 'moment coffee', 'coffee');
				const seen = await app.page.locator(dot('coffee')).evaluate((mark) => {
					for (const el of [mark, mark.nextElementSibling]) for (const a of el.getAnimations()) a.finish();
					const r = mark.getBoundingClientRect();
					const svg = mark.ownerSVGElement;
					const now = new DOMPoint(Number(svg.querySelector('.current-time-line').dataset.now), 0).matrixTransform(svg.getScreenCTM()).x;
					const hit = document.elementFromPoint(r.right - 1, r.top + r.height / 2);
					return { center: r.left + r.width / 2, right: r.right, now, hit: hit === mark || `${hit?.tagName}.${hit?.getAttribute('class')}`, clipped: !!mark.closest('[clip-path]') };
				});
				assert.ok(Math.abs(seen.center - seen.now) < 1, `the dot is at ${seen.center}, now at ${seen.now}`);
				assert.ok(seen.right > seen.now + 2, `the dot's right edge ${seen.right} is not past now at ${seen.now}, so nothing was checked`);
				assert.equal(seen.clipped, false, 'the dot is under a clip');
				assert.equal(seen.hit, true, 'the dot is not what is drawn at its right edge');
				assert.deepEqual(app.errors, []);
			},
			{ clock: AT, holdTimers: true, desk: desk() }
		);
	});

	test('one still to come waits behind the clip at the now-line', async () => {
		await withApp(
			async (app) => {
				await log(app, 'moment coffee -t +5', 'coffee');
				const clip = await app.page.locator(dot('coffee')).evaluate((mark) => mark.closest('[clip-path^="url(#flow-past"]') !== null);
				assert.equal(clip, true, 'a dot five minutes ahead stands over the now section');
				assert.deepEqual(app.errors, []);
			},
			{ clock: AT, holdTimers: true, desk: desk() }
		);
	});
});
