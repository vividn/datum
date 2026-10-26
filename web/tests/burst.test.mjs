// A burst of changes to a pane runs one process at a time and paints only
// the newest (da-eoq3): the run already out, then the newest request — never
// the ones between. How many runs that is depends on how many came back
// DURING the burst, which is the box's speed and not the page's: under load
// none does and the burst costs two; on an idle box one returns mid-burst
// and a third is rightly asked for (da-w2oj). So the bound is counted
// against the returns the page really saw — and a paint against the return
// before it: one that came back with a newer click already made is never
// drawn (da-ehaa). And the worker's queue drops a keyed job that has not
// started when a newer one with the same key arrives; a keyless one never.
//
// Delta-blind on purpose: runs are counted at the seam, paints off the DOM,
// and the final picture is held to its own geometry.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { inWorker, startApp, stopApp, withApp } from './support/app.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const NOW = Date.UTC(2026, 8, 10, 15, 0, 0);
const TODAY = '.band[data-days-ago="0"] svg.svg';
const NOWVIEW = 'section.pane[data-title="nowview"]';

// Told, in the worker, of each `--version` request as it arrives. This
// listener runs before the app's, but in the same dispatch: by the time the
// page hears the count, the request is queued.
const ARRIVALS = `
	let versions = 0;
	const channel = new BroadcastChannel('datum-test-arrived');
	self.addEventListener('message', (e) => {
		if (e.isTrusted && e.data?.argv?.includes('--version')) channel.postMessage(++versions);
	});
`;

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
						// `clicks` counts the requests made: the pane asks for a
						// nowview on each, in the microtasks after it (`run` hands
						// the runner its argv synchronously). A run that comes back
						// with more clicks made than when it started came back
						// behind a newer request — STALE — and the paint that
						// follows a return is that return's.
						const nowview = { out: 0, most: 0, back: 0, bursting: true, clicks: 0, stale: 0, lastStale: false };
						const previous = window.datum.setRunner(async (argv, ...rest) => {
							const mine = argv[0] === 'nowview';
							const asked = nowview.clicks;
							if (mine) nowview.most = Math.max(nowview.most, ++nowview.out);
							const result = await previous(argv, ...rest);
							if (mine) nowview.out--;
							if (mine && nowview.bursting) nowview.back++;
							if (mine) {
								nowview.lastStale = nowview.clicks > asked;
								nowview.stale += nowview.lastStale;
							}
							if (!result.superseded) ran.push(argv);
							return result;
						});
						window.__ran = ran;
						window.__nowview = nowview;
						window.__paints = 0;
						window.__stalePaints = 0;
						// A PICTURE, not any `<svg>`: `CliOutput` classes the one it
						// draws `svg.svg`, and the pane holds other svgs that are
						// nobody's picture — a margin pill's lock glyph
						// (da-rskp), which mounting once would have counted as a
						// repaint of the band.
						new MutationObserver((records) => {
							for (const r of records)
								for (const n of r.addedNodes)
									if (n.nodeName.toLowerCase() === 'svg' && n.classList?.contains('svg')) {
										window.__paints++;
										window.__stalePaints += nowview.lastStale;
									}
						}).observe(document.querySelector(scope), { childList: true, subtree: true });
						window.__datumSeen.length = 0;

						const box = document.querySelector(band).getBoundingClientRect();
						// The day's GROUND at each x: a click on a mark is its entry
						// since da-xg1vi, and these are cursor changes.
						const ground = (x) => {
							for (let dy = 0; dy < box.height / 2; dy++)
								for (const y of [box.top + box.height * 0.6 + dy, box.top + box.height * 0.6 - dy]) {
									const at = document.elementFromPoint(x, y);
									if (at?.closest('svg.svg') && !at.closest('[data-id]')) return y;
								}
							throw new Error(`no ground at ${x}`);
						};
						let dimmed = false;
						for (let i = 0; i < 10; i++) {
							const x = box.left + box.width * (0.1 + i * 0.08);
							const y = ground(x);
							const init = { bubbles: true, clientX: x, clientY: y };
							const target = document.elementFromPoint(x, y);
							target.dispatchEvent(new PointerEvent('pointerdown', init));
							target.dispatchEvent(new PointerEvent('pointerup', init));
							target.dispatchEvent(new MouseEvent('click', init));
							nowview.clicks++;
							// A macrotask: the page reacts to each change on its own.
							await new Promise((r) => setTimeout(r, 0));
							dimmed ||= !!document.querySelector(`${scope} .picture.behind`);
						}
						nowview.bursting = false;
						return { dimmed, most: nowview.most, back: nowview.back };
					},
					[TODAY, NOWVIEW]
				);
				await app.idle();

				const { requested, ran, paints, stale, stalePaints, behind } = await page.evaluate((scope) => ({
					requested: window.__datumSeen.filter((a) => a[0] === 'nowview'),
					ran: window.__ran.filter((a) => a[0] === 'nowview'),
					paints: window.__paints,
					stale: window.__nowview.stale,
					stalePaints: window.__stalePaints,
					behind: document.querySelectorAll(`${scope} .behind`).length
				}), NOWVIEW);
				assert.ok(requested.length >= 2, `a burst of ${requested.length} requests is no burst`);
				assert.equal(burst.most, 1, `${burst.most} nowview runs out at once`);
				// Each return during the burst may start one more; past those,
				// the run still out when it ends and then the newest request.
				assert.ok(
					ran.length <= burst.back + 2,
					`${ran.length} nowview processes for one burst: ${JSON.stringify(ran)} (${burst.back} came back during it)`
				);
				// A return with nothing newer behind it is the newest picture
				// of its moment; one with a newer request waiting is never drawn.
				// Held per return, not as a count: on an idle box a count cannot
				// tell the one from the other (da-ehaa).
				assert.ok(stale >= 1, 'no run came back behind a newer click: the burst tested nothing');
				assert.equal(stalePaints, 0, `${stalePaints} of ${paints} paints followed a run that came back behind a newer request`);
				assert.ok(paints >= 1, 'nothing was painted');
				assert.deepEqual(ran.at(-1), requested.at(-1), 'the last run is not the last request');
				assert.ok(burst.dimmed, 'the old picture was not dimmed while the new one was on its way');
				assert.equal(behind, 0, 'the final picture is still dimmed');

				// The final picture is centred on the last cursor: its line sits
				// where its stretch holding that instant draws it — the now strip
				// cuts the picture in two (da-6csum), so not the first's middle.
				const last = requested.at(-1);
				const [mid, line] = await page.locator(`${NOWVIEW} svg.svg`).evaluate((svg, iso) => {
					const t = Date.parse(iso);
					const day = [...svg.querySelectorAll('[data-lo]')].find(
						(d) => Date.parse(d.getAttribute('data-from')) <= t && t <= Date.parse(d.getAttribute('data-to'))
					);
					const [from, to] = ['data-from', 'data-to'].map((n) => Date.parse(day.getAttribute(n)));
					const [lo, hi] = ['data-lo', 'data-hi'].map((n) => Number(day.getAttribute(n)));
					return [lo + ((t - from) / (to - from)) * (hi - lo), Number(svg.querySelector('line.cursor').getAttribute('x1'))];
				}, last[last.indexOf('-t') + 1]);
				assert.ok(Math.abs(mid - line) < 0.5, `the cursor at ${line}, the centre ${mid}`);
				assert.deepEqual(app.errors, []);
			},
			{ clock: NOW }
		);
	});

	test('a queued keyed job is dropped by a newer one with its key; a keyless job never is', async () => {
		await withApp(
			async ({ page }) => {
				const outcomes = await page.evaluate(async () => {
					const keys = [undefined, 'k', 'k', undefined, 'other', 'k', undefined];
					// Every `--version` the worker has been handed, the warm-ups' included.
					const seen = [];
					const channel = new BroadcastChannel('datum-test-arrived');
					channel.onmessage = (e) => seen.push(e.data);
					const heard = (n, ms) =>
						new Promise((resolve) => {
							const at = performance.now();
							const poll = () =>
								seen.includes(n) ? resolve(true) : performance.now() - at > ms ? resolve(false) : setTimeout(poll, 10);
							poll();
						});
					// A new channel hears nothing until the browser has connected it,
					// and a count sent before then is lost: all seven of them were,
					// 5 runs in 80 under load, the worker having counted every one
					// (da-oh6f1). Hearing a warm-up's own count proves it connected.
					let warm = 0;
					do await window.datum.runQuiet(['--version']);
					while (!(await heard(++warm, 1000)) && warm < 10);
					const runner = window.datum.setRunner(null);
					window.datum.setRunner(runner);
					// The queue is HELD, not hoped to be slow (da-nixla): under the
					// worker's own lock nothing behind the first job can start, and
					// the lock goes back only once the last request is in the worker.
					// Arriving is a message hop, not a process: a count not heard in
					// seconds will never come, and throwing gives the lock back.
					let jobs;
					await navigator.locks.request('datum-cli-files', async () => {
						jobs = keys.map((key) => runner(['--version'], undefined, undefined, key));
						if (await heard(warm + keys.length, 20000)) return;
						const locks = JSON.stringify(await navigator.locks.query());
						throw new Error(`arrivals ${JSON.stringify(seen)}, awaiting ${warm + keys.length}; locks ${locks}`);
					});
					channel.close();
					return (await Promise.all(jobs)).map((r) => (r.superseded ? 'dropped' : r.stdout ? 'ran' : r.stderr));
				});
				assert.deepEqual(outcomes, ['ran', 'dropped', 'dropped', 'ran', 'ran', 'ran', 'ran']);
			},
			{ prepare: inWorker(ARRIVALS) }
		);
	});
});

// The same economy for a burst that arrives from OUTSIDE the tab (da-s3pt).
// A write next door refreshes the whole frame — the epoch every pane hangs
// off, plus the widget bar's own two commands — so twenty entries logged in
// the other tab bought twenty of those, and every read the reader asked for
// queued behind the pile. A sync peer handing over a night of changesets
// would do the same. Held here by counting what the tab was TOLD against
// what it then ran, so the bound is against the burst that really reached
// it rather than a number written down.
//
// The reader's first refresh is held until the last write has landed. A
// coalesced burst is then always two refreshes, and one each is twelve:
// unheld, the count measured how long a refresh took against the gap
// between writes, and once commands got fast (da-o3sns) a refresh no longer
// outlasted the gap, and an economical burst counted 7-8 of 12.
describe('a burst of writes in another tab', () => {
	test('twelve writes next door cost the frame a couple of refreshes, not one each', async () => {
		await withApp(async (first) => {
			const second = await first.newTab();
			const seen = await second.spyOnRunner();
			// The channel the write is announced on ($lib/changes.ts). Counted
			// from a second handle in this tab, which hears every message the
			// page's own listener hears and does nothing else with it.
			await second.page.evaluate(() => {
				window.__told = 0;
				new BroadcastChannel('datum-db-changes').addEventListener(
					'message',
					() => (window.__told += 1)
				);
			});
			await second.idle();
			// The widget bar's `fields` opens every refresh: the first waits
			// for the burst to be over, so the rest of it arrives mid-refresh.
			await second.page.evaluate(() => {
				const over = new Promise((resolve) => (window.__burstOver = resolve));
				const previous = window.datum.setRunner(async (argv, ...rest) => {
					if (argv[0] === 'fields') await over;
					return previous(argv, ...rest);
				});
			});

			// Fired without awaiting each: the writes land while the second
			// tab is still refreshing from the first of them, which is the
			// burst this exists for.
			const logged = await Promise.all(
				Array.from({ length: 12 }, (_, i) =>
					first.run(['occur', `ping${i}`, '-t', `07${String(i).padStart(2, '0')}`])
				)
			);
			logged.forEach((r, i) => assert.equal(r.code, 0, `${i}: ${r.stderr}`));
			await second.page.evaluate(() => window.__burstOver());

			// The LAST write is on the second tab's screen: a coalesced burst
			// still ends in a refresh, and that refresh is after the last of
			// the writes it stood in for.
			await until(
				() => second.paneText('tail'),
				(text) => text.includes('ping11')
			);
			await second.idle();

			const told = await second.page.evaluate(() => window.__told);
			// `fields --json` is the widget bar's, run exactly once per
			// refresh ($lib/layout/engine.ts) — and this tab wrote nothing
			// itself, so each one of them is one external refresh.
			const refreshes = (await seen()).filter((argv) => argv[0] === 'fields').length;
			assert.ok(told >= 10, `a burst of ${told} announcements is no burst`);
			// Two, held as above; half the writes is the bound, as it was
			// before the hold (da-ef2tx), and one each is twice it.
			assert.ok(
				refreshes <= told / 2,
				`${refreshes} refreshes for ${told} writes next door — one each is the bug`
			);
			assert.ok(refreshes >= 1, 'the other tab\'s writes refreshed nothing at all');
			assert.deepEqual(second.errors, []);
		});
	});
});
