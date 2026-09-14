// A burst of changes to a pane costs at most two processes and paints only
// the last (da-eoq3): the run already out, then the newest request — never
// the ones between. And the worker's queue drops a keyed job that has not
// started when a newer one with the same key arrives; a keyless one never.
//
// Delta-blind on purpose: runs are counted at the seam, paints off the DOM,
// and the final picture is held to its own geometry.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';

before(startApp);
after(stopApp);

const NOW = Date.UTC(2026, 8, 10, 15, 0, 0);
const TODAY = '.band[data-days-ago="0"] svg.svg';
const NOWVIEW = 'section.pane[data-title="nowview"]';

describe('a burst of changes', () => {
	test('10 quick cursor changes start at most 2 nowview runs and paint only the last', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				for (const argv of [
					['occur', 'coffee', '-t', '09:00'],
					['start', 'work', '-t', '10:00']
				]) {
					assert.equal((await app.run(argv)).code, 0);
				}
				await app.reload();
				await page.locator(`${NOWVIEW} svg.svg`).waitFor();
				await app.spyOnRunner();
				await app.idle();

				const burst = await page.evaluate(
					async ([band, scope]) => {
						const ran = [];
						const previous = window.datum.setRunner(async (argv, ...rest) => {
							const result = await previous(argv, ...rest);
							if (!result.superseded) ran.push(argv);
							return result;
						});
						window.__ran = ran;
						window.__paints = 0;
						new MutationObserver((records) => {
							for (const r of records)
								for (const n of r.addedNodes) if (n.nodeName.toLowerCase() === 'svg') window.__paints++;
						}).observe(document.querySelector(scope), { childList: true, subtree: true });
						window.__datumSeen.length = 0;

						const box = document.querySelector(band).getBoundingClientRect();
						const y = box.top + box.height * 0.6;
						let dimmed = false;
						for (let i = 0; i < 10; i++) {
							const x = box.left + box.width * (0.1 + i * 0.08);
							const init = { bubbles: true, clientX: x, clientY: y };
							const target = document.elementFromPoint(x, y);
							target.dispatchEvent(new PointerEvent('pointerdown', init));
							target.dispatchEvent(new MouseEvent('click', init));
							// A macrotask: the page reacts to each change on its own.
							await new Promise((r) => setTimeout(r, 0));
							dimmed ||= !!document.querySelector(`${scope} .picture.behind`);
						}
						return { dimmed };
					},
					[TODAY, NOWVIEW]
				);
				await app.idle();

				const { requested, ran, paints, behind } = await page.evaluate((scope) => ({
					requested: window.__datumSeen.filter((a) => a[0] === 'nowview'),
					ran: window.__ran.filter((a) => a[0] === 'nowview'),
					paints: window.__paints,
					behind: document.querySelectorAll(`${scope} .behind`).length
				}), NOWVIEW);
				assert.ok(requested.length >= 2, `a burst of ${requested.length} requests is no burst`);
				assert.ok(ran.length <= 2, `${ran.length} nowview processes for one burst: ${JSON.stringify(ran)}`);
				assert.equal(paints, 1, 'an intermediate picture was painted');
				assert.deepEqual(ran.at(-1), requested.at(-1), 'the last run is not the last request');
				assert.ok(burst.dimmed, 'the old picture was not dimmed while the new one was on its way');
				assert.equal(behind, 0, 'the final picture is still dimmed');

				// The final picture is centred on the last cursor: its line sits mid-axis.
				const [mid, line] = await page.locator(`${NOWVIEW} svg.svg`).evaluate((svg) => {
					const day = svg.querySelector('[data-lo]');
					return [
						(Number(day.getAttribute('data-lo')) + Number(day.getAttribute('data-hi'))) / 2,
						Number(svg.querySelector('line.cursor').getAttribute('x1'))
					];
				});
				assert.ok(Math.abs(mid - line) < 0.5, `the cursor at ${line}, the centre ${mid}`);
				assert.deepEqual(app.errors, []);
			},
			{ clock: NOW }
		);
	});

	test('a queued keyed job is dropped by a newer one with its key; a keyless job never is', async () => {
		await withApp(async ({ page }) => {
			const outcomes = await page.evaluate(async () => {
				await window.datum.runQuiet(['--version']);
				const runner = window.datum.setRunner(null);
				window.datum.setRunner(runner);
				const go = (key) => runner(['--version'], undefined, undefined, key);
				// The first holds the queue while the rest arrive behind it.
				const jobs = [go(), go('k'), go('k'), go(), go('other'), go('k'), go()];
				return (await Promise.all(jobs)).map((r) => (r.superseded ? 'dropped' : r.stdout ? 'ran' : r.stderr));
			});
			assert.deepEqual(outcomes, ['ran', 'dropped', 'dropped', 'ran', 'ran', 'ran', 'ran']);
		});
	});
});
