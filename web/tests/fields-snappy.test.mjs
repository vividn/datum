// A write's new state reaches the fields bar WITH the write's own output
// (da-70g2u). vividn: «starting a field appears immediately in the output
// section, but then takes several seconds to change the color of the
// button». On five years of data the button waited ~14 s for `now --json`,
// which queued behind every pane the write set off.
//
// The panes are made slow here rather than the database big: a runner over
// the seam's own (`setRunner`) runs one command at a time, as the worker
// does, and holds every pane's command for PANE_MS. The bar's colour is
// read every animation frame, beside whether the write's output is there.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP } from './support/layout.mjs';
import { FIXTURE, seed } from './support/fields.mjs';

before(startApp);
after(stopApp);

const PANE_MS = 600;

/** One command at a time; `fields`, `now` and the writes at their own speed, the rest PANE_MS slower. */
const slowPanes = (page, quick) =>
	page.evaluate(
		([ms, quick]) => {
			window.__datumLog = [];
			let chain = Promise.resolve();
			const previous = window.datum.setRunner((argv, ...rest) => {
				// Counted while it waits its turn too, or `idle()` sees a quiet page behind a queue.
				window.__datumInflight += 1;
				const turn = chain.then(async () => {
					const entry = { argv: argv.join(' '), start: performance.now() };
					window.__datumLog.push(entry);
					const result = await previous(argv, ...rest);
					if (!quick.includes(argv[0])) await new Promise((r) => setTimeout(r, ms));
					entry.end = performance.now();
					return result;
				});
				chain = turn.catch(() => {});
				return turn.finally(() => (window.__datumInflight -= 1));
			});
			window.__datumUnslow = () => window.datum.setRunner(previous);
		},
		[PANE_MS, quick]
	);

/**
 * Submit `line` and watch every frame until the page is quiet: the frame
 * the line's output first shows, and each `--fill` the field's button has
 * on the way. Also the order commands ran in after it.
 */
async function watch(app, field, line) {
	const { page } = app;
	await page.evaluate((field) => {
		const echoes = document.querySelectorAll('.repl .echo').length;
		const seen = (window.__datumSeen = { frames: [], output: null });
		let frame = 0;
		const tick = () => {
			frame += 1;
			const fill = document.querySelector(`.bar button.widget[data-field="${field}"]`)?.style.getPropertyValue('--fill').trim();
			if (seen.output === null && document.querySelectorAll('.repl .echo').length > echoes) seen.output = frame;
			if (seen.frames.at(-1)?.fill !== fill) seen.frames.push({ frame, fill, at: performance.now() });
			if (!seen.done) requestAnimationFrame(tick);
		};
		requestAnimationFrame(tick);
	}, field);
	await app.submit(line);
	await app.idle();
	return page.evaluate(() => {
		window.__datumSeen.done = true;
		return { ...window.__datumSeen, log: window.__datumLog };
	});
}

test('a switch shows on its button with its output, and the bar reads ahead of the panes', async () => {
	await withApp(async (app) => {
		await seed(app, [...FIXTURE, 'switch mood good -t -60'], 'mood');
		await app.idle();
		await slowPanes(app.page, ['fields', 'now', 'switch']);

		const seen = await watch(app, 'mood', 'switch mood bad');
		const [before, ...after] = seen.frames;
		assert.equal(before.fill, '#008000', 'mood was good');
		assert.equal(after[0]?.fill, '#ff0000', `the first change is to bad: ${JSON.stringify(seen.frames)}`);
		assert.equal(after.length, 1, `the reconcile agreed and drew nothing else: ${JSON.stringify(seen.frames)}`);
		assert.ok(after[0].frame - seen.output <= 1, `button at frame ${after[0].frame}, output at ${seen.output}`);

		const ran = seen.log.map((e) => e.argv);
		const panes = seen.log.filter((e) => !/^(fields|now|switch) /.test(e.argv));
		assert.ok(panes.length > 0, `the panes refreshed: ${ran.join(' | ')}`);
		assert.ok(after[0].at < panes[0].end, 'the button changed before the first pane answered');
		const now = ran.indexOf('now --json');
		assert.ok(now >= 0 && ran.slice(0, now).every((argv) => /^(fields|switch) /.test(argv)), `now --json ran ahead of the panes: ${ran.join(' | ')}`);
	}, DESKTOP);
});

test('a failed write leaves the bar alone, and the reconcile wins over a wrong guess', async () => {
	await withApp(async (app) => {
		await seed(app, [...FIXTURE, 'switch mood good -t -60', 'start work -t -120', 'end work -t -90'], 'work');
		await app.idle();
		// `now` slowed with the panes, so the guess stands for frames before the reconcile lands.
		await slowPanes(app.page, ['fields', 'switch', 'start']);

		const failed = await watch(app, 'mood', 'switch mood bad -t nonsense');
		assert.deepEqual(failed.frames.map((f) => f.fill), ['#008000'], 'a write that failed changed the bar');

		// Four minutes that END now: a closed block, which `now` does not call on.
		const closed = await watch(app, 'work', 'start work 4');
		assert.deepEqual(closed.frames.map((f) => f.fill), ['transparent', '#0000ff', 'transparent'], JSON.stringify(closed.frames));
	}, DESKTOP);
});
