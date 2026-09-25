// What the app does when durable storage never answers.
//
// `db.worker.ts` has always had an in-memory fallback and the banner has
// always had a sentence for it, but both were reachable only from a
// `catch` — and a browser that will not give the worker a sync access
// handle does not always reject. Observed in headless chromium: the OPFS
// open simply never settled, nothing threw, and the page sat at
// "opening database…" forever with every pane on its `…` placeholder
// (da-mskb).
//
// So the test is not that the page loads. It is that the app gives up on
// durable storage, says out loud that entries are in memory, and still
// works — the honest state the code already knew how to display.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { inWorker, startApp, stopApp, withApp } from './support/app.mjs';
import { until } from './support/until.mjs';

/** A sync access handle that is never granted and never refused. */
const NEVER = `FileSystemFileHandle.prototype.createSyncAccessHandle = () => new Promise(() => {});`;

/**
 * A sync access handle refused until `until` (epoch ms, on the wall clock:
 * the worker's `Date` is held), then real.
 *
 * An absolute instant rather than a delay, because the prelude runs afresh
 * on every load of the page and a delay would restart the outage under a
 * reload — which is the read the test is making.
 */
const notBefore = (until) => `
	const real = FileSystemFileHandle.prototype.createSyncAccessHandle;
	FileSystemFileHandle.prototype.createSyncAccessHandle = function (...args) {
		if (performance.timeOrigin + performance.now() < ${until}) return Promise.reject(new DOMException('not yet', 'InvalidStateError'));
		return real.apply(this, args);
	};
`;

/**
 * A sync access handle whose `flush` stalls for `ms` once told to — over the
 * `datum-test-slow` channel, so the panes that run at load are not the
 * command that stalls — and says so on the same channel as it starts.
 *
 * `flush` is where SQLite's sync lands under the shim, which is the middle
 * of a write: the journal is on disk and the transaction is not committed.
 * `once` stalls one flush and then runs at full speed again.
 */
const slowFlush = (ms, { once = false } = {}) => `
	const channel = new BroadcastChannel('datum-test-slow');
	let slow = false;
	channel.onmessage = (e) => { if (e.data === 'slow') slow = true; };
	const flush = FileSystemSyncAccessHandle.prototype.flush;
	FileSystemSyncAccessHandle.prototype.flush = function (...args) {
		if (slow) {
			slow = !${once};
			channel.postMessage('flushing');
			const deadline = performance.now() + ${ms};
			while (performance.now() < deadline) {}
		}
		return flush.apply(this, args);
	};
`;

/** Tell the database worker to stall its next flush; resolve once it has. */
async function stallNextFlush(page) {
	await page.evaluate(() => {
		const channel = new BroadcastChannel('datum-test-slow');
		window.__flushing = new Promise((resolve) => {
			channel.onmessage = (e) => e.data === 'flushing' && resolve(true);
		});
		channel.postMessage('slow');
	});
	return () => page.evaluate(() => window.__flushing);
}

before(startApp);
after(stopApp);

// An app per test, not `sharedApp` (da-6b3dj): every test here prepares the
// worker's storage before the page exists, reloads, or opens a second tab
// of the same OPFS — what a shared app's `empty` cannot stand in for.

/**
 * The repl layout, for the tests the day page is only scenery for.
 *
 * Opening the day frame draws a fortnight of dayview bands before
 * `withApp` returns, and these tests keep two and three TABS — so the
 * frame is paid for over and over to reach a question that is never asked
 * of it (da-x6ea). It is the layout the forty-writes test below already
 * had to move to, for the reason written above it.
 *
 * This is LESS contention, not the same, and the comment here used to say
 * otherwise: on the repl layout a write in the other tab refreshes one
 * pane and the bar's two commands, not six panes. What these tests claim
 * is about the DATA — forty writes land forty rows, once each, by hid,
 * with `check` clean; the same field in the same minute is a duplicate and
 * never a loss — and that is true of any pile deep enough to make the tabs
 * take turns, which the twenty and forty commands each test fires without
 * awaiting still are. The one timing claim, the burst test's, is already
 * written as a ratio to the burst's own length rather than a wall clock,
 * so a shorter queue moves both of its numbers together.
 *
 * The tests that DO read the page — the banner at boot when storage never
 * answers, the tail that has to see the other tab's write — stay where
 * they are.
 */
const REPL = { query: 'repl' };

describe('when durable storage never answers', () => {
	test('the app opens in memory and says so', async () => {
		await withApp(
			async ({ run, entries, bannerText }) => {
				// Reaching here at all is the fix: `withApp` waits for the
				// REPL to enable and every pane to answer, which the app
				// does only once `initDb()` resolves. Before the deadline
				// that wait never ended.
				assert.match(await bannerText(), /held in memory only/);

				const logged = await run(['occur', 'coffee', '-t', '0700']);
				assert.equal(logged.code, 0, logged.stderr);
				assert.deepEqual(
					(await entries()).map((e) => e.field),
					['coffee']
				);
			},
			{ prepare: inWorker(NEVER) }
		);
	});
});

// The recovery the banner promises, driven rather than read. This is the
// test that would have caught both defects the first cut of the fallback
// shipped with a green suite: a retry that could not retry (sqlite-wasm
// caches the install's rejection) and a copy that could not copy (generated
// columns refuse INSERT). Neither shows unless the handle is really refused
// and then really granted.
describe('when durable storage arrives late', () => {
	test('the tab takes it over on its own, and keeps what was typed meanwhile', async () => {
		// Absolute, so the outage does not restart under the reload below.
		const heals = Date.now() + 5000;
		await withApp(
			async (app) => {
				assert.match(await app.bannerText(), /held in memory only/);
				for (const time of ['0700', '0800', '0900']) {
					const typed = await app.run(['occur', 'coffee', '-t', time]);
					assert.equal(typed.code, 0, typed.stderr);
				}
				// The handover is `db backup -` out of memory and `db restore -`
				// into the file: the same rows, by hid, not three lookalikes.
				const typed = (await app.entries()).map((e) => e.hid).sort();
				assert.equal(typed.length, 3);

				// Bounded: the worker retries with backoff, so a healthy build
				// heals within a few seconds. Never healing is the defect this
				// exists for.
				await app.page.waitForFunction(() => !document.querySelector('section.pane.alarm'));

				// Through a reload, which is the read only durable storage
				// answers: the entry typed while in memory was moved across.
				await app.reload();
				assert.deepEqual((await app.entries()).map((e) => e.hid).sort(), typed);
				assert.doesNotMatch(await app.bannerText(), /memory only/);
			},
			{ prepare: inWorker(notBefore(heals)) }
		);
	});
});

// Two tabs of datum.day, and the invariant local-first exists for: both
// write, both stay in sync, nothing typed in either is ever lost (da-eovo).
//
// Every command is its own process that takes the database files for its
// length and lets them go (da-g8o8), so there is no owner tab: both tabs
// run their own commands against the one file, taking turns.
//
// These have to be two pages in ONE browser context. Two contexts get
// separate storage, so the second tab would open OPFS happily and every
// test here would pass against a build that never learned to share.
describe('two tabs', () => {
	test('both are durable, and each sees the other write without a reload', async () => {
		await withApp(async (first) => {
			const second = await first.newTab();

			// The second tab is not a warning any more.
			assert.doesNotMatch(await second.bannerText(), /memory only/);
			assert.equal(
				await second.page.locator('section.pane.alarm').count(),
				0,
				'the second tab is durable, so its repl frame is not alarmed'
			);

			// Through the REPL, because that is the path a person takes and
			// the one that tells the other tabs anything happened.
			await first.submit('occur coffee -t 0700');
			await second.submit('occur tea -t 0800');

			// Both rows in both tails, oldest first, with nobody reloading.
			for (const tab of [first, second]) {
				const text = await until(
					() => tab.paneText('tail'),
					(t) => t.includes('coffee') && t.includes('tea')
				);
				assert.ok(text.indexOf('coffee') < text.indexOf('tea'), text);
			}
		});
	});

	test('both keep everything through a reload', async () => {
		await withApp(async (first) => {
			const second = await first.newTab();
			await first.submit('occur coffee -t 0700');
			await second.submit('occur tea -t 0800');

			// A reload hands the database on and takes it back: the reloading
			// tab's worker dies, so its lock frees and the other tab opens the
			// pool. Durability has to survive that, twice.
			await first.reload();
			await second.reload();
			for (const tab of [first, second]) {
				assert.deepEqual(
					(await tab.entries()).map((e) => e.field).sort(),
					['coffee', 'tea']
				);
			}
		}, REPL);
	});

	// On the repl layout: on the day page every write re-runs the other
	// tab's five panes, 200 processes the writes queue behind for these
	// forty, and the parallel suite stretched that pile past two minutes.
	test('forty writes from both tabs at once all land, once each, and check is clean', async () => {
		await withApp(async (first) => {
			const second = await first.newTab();
			const tabs = [first, second];
			// Fired without awaiting, so both tabs' processes contend for the
			// files at once — each tab's own queue and the turns between
			// them, not the test, are what keep them apart.
			const logged = await Promise.all(
				Array.from({ length: 40 }, (_, i) =>
					tabs[i % 2].run(['occur', 'ping', '-t', `07${String(i).padStart(2, '0')}`])
				)
			);
			logged.forEach((r, i) => assert.equal(r.code, 0, `${i}: ${r.stderr}`));

			// Counted by hid, not by "more than before": a delta-blind probe
			// cannot tell forty rows from twenty written twice.
			const third = await second.newTab();
			for (const tab of [first, second, third]) {
				const rows = await tab.entries(100);
				assert.equal(rows.length, 40);
				assert.equal(new Set(rows.map((e) => e.hid)).size, 40);
			}
			const check = await third.run(['check']);
			assert.equal(check.code, 0, check.stdout + check.stderr);
		}, REPL);
	});

	// Both tabs logging the same field in the same minute is the one case
	// where one connection can refuse a write, and it must refuse it the way
	// the CLI does: the slug is UNIQUE, so the second is a benign duplicate
	// (exit 11) of a row that is already there — never a row that went
	// missing. Two tabs must not make this any worse than one tab does.
	test('the same field in the same minute is a duplicate, not a loss', async () => {
		await withApp(async (first) => {
			const second = await first.newTab();
			const both = await Promise.all([
				first.run(['occur', 'coffee', '-t', '0700']),
				second.run(['occur', 'coffee', '-t', '0700'])
			]);
			assert.deepEqual(
				both.map((r) => r.code).sort(),
				[0, 11],
				both.map((r) => `${r.code}: ${r.stderr}`).join(' | ')
			);
			for (const tab of [first, second]) {
				assert.deepEqual(
					(await tab.entries()).map((e) => e.field),
					['coffee']
				);
			}
		}, REPL);
	});

	test('a tab closed mid-write leaves the database whole, and check says so', async () => {
		await withApp(
			async (first) => {
				assert.equal((await first.run(['occur', 'coffee', '-t', '0600'])).code, 0);

				// A long write: one `db restore -` of many rows, which is one
				// transaction. Made from the CLI's own dump of the row above,
				// renamed so every copy is a row of its own.
				const [header, row] = (await first.run(['db', 'backup', '-'])).stdout.split('\n');
				const template = JSON.parse(row);
				const copies = Array.from({ length: 500 }, (_, i) =>
					JSON.stringify({
						...template,
						hid: String(i).padStart(12, 'x'),
						slug: `ping:${i}`,
						field: 'ping',
						content_hash: i.toString(16).padStart(64, '0')
					})
				);
				const flushing = await stallNextFlush(first.page);
				// Not awaited: the page closes under it.
				void first.run(['db', 'restore', '-', '--merge'], [header, ...copies].join('\n')).catch(() => {});
				assert.equal(await flushing(), true);
				await first.page.close();

				// The journal that write left is still there; the next process
				// rolls it back before it reads anything.
				const next = await first.newTab();
				const check = await next.run(['check']);
				assert.equal(check.code, 0, check.stdout + check.stderr);
				assert.deepEqual(
					(await next.entries(1000)).map((e) => e.field),
					['coffee']
				);
				assert.equal((await next.run(['occur', 'tea', '-t', '0700'])).code, 0);
			},
			{ ...REPL, prepare: inWorker(slowFlush(60000)) }
		);
	});

	// The owner design this replaced sent a request again when 3 s went by
	// with no answer, and under argv that is a second row — the CLI mints
	// the hid, so nothing marks the two as one write. Per-command handles
	// resend nothing; a command slower than that must still land once.
	test('a command slower than three seconds lands once, not twice', async () => {
		await withApp(
			async (app) => {
				await stallNextFlush(app.page);
				const started = Date.now();
				const logged = await app.run(['occur', 'coffee', '-t', '0700']);
				assert.equal(logged.code, 0, logged.stderr);
				assert.ok(Date.now() - started > 3000, 'the stall did not happen, so this proves nothing');
				// No wait: a resend at 3 s went out before this answer did, and
				// the worker runs what it is sent in order, so the read below
				// comes after anything a resend would have written.
				assert.deepEqual(
					(await app.entries()).map((e) => e.field),
					['coffee']
				);
			},
			{ ...REPL, prepare: inWorker(slowFlush(3500, { once: true })) }
		);
	});
});

// One acquisition of the files, shared by the commands of a burst (da-dvzx).
//
// Opening a sync access handle on each of the four files cost about as much
// as a warm command, and a burst paid it per command. Now the handles and
// the `datum-cli-files` lock are kept while the worker's queue still has
// jobs waiting — bounded twice, because the tab beside this one is waiting
// on that lock: they go back when the queue drains, and when a burst that
// never drains has held them past the cap.
//
// Counted, not timed: the open is the cost, so the tests count opens at the
// platform and read `ms.tries` back off the seam.

/** The cap in $lib/cli.ts — `BURST_HOLD_MS`, mirrored here. */
const HOLD_MS = 250;

/**
 * Count the worker's handle opens, and the acquisitions its replies report,
 * askable from the page over the `datum-test-opens` channel.
 *
 * Both in the worker, because the page cannot see every command: on a
 * loaded box one of its own (`now --json`, the layout store's `tail`) lands
 * inside the window a test is counting, and a count of opens set against
 * the test's commands alone reads that as a command that opened handles
 * without saying so (da-wtne). The worker sees every request, every open and
 * every reply, and answers with all three in one task.
 */
const countOpens = `
	const real = FileSystemFileHandle.prototype.createSyncAccessHandle;
	let opens = 0, acquired = 0, pending = 0;
	FileSystemFileHandle.prototype.createSyncAccessHandle = function (...args) {
		opens++;
		return real.apply(this, args);
	};
	// Trusted only: \`inWorker\` dispatches again what arrived before the module.
	self.addEventListener('message', (e) => e.isTrusted && pending++);
	const post = self.postMessage.bind(self);
	self.postMessage = (message, ...rest) => {
		if (message?.notice === undefined) {
			pending--;
			if (message?.cli?.ms?.tries > 0) acquired++;
		}
		return post(message, ...rest);
	};
	const channel = new BroadcastChannel('datum-test-opens');
	channel.onmessage = (e) => e.data === 'how many?' && channel.postMessage({ opens, acquired, pending });
`;

/**
 * The handles the worker has opened since it started, and the acquisitions
 * its commands reported — asked again until no command is in flight, so
 * every open counted belongs to an acquisition counted.
 */
async function counts(page) {
	for (;;) {
		const { opens, acquired, pending } = await page.evaluate(
			() =>
				new Promise((resolve) => {
					const channel = new BroadcastChannel('datum-test-opens');
					channel.onmessage = (e) => {
						if (typeof e.data?.opens === 'number') {
							channel.close();
							resolve(e.data);
						}
					};
					channel.postMessage('how many?');
				})
		);
		if (pending === 0) return { opens, acquired };
		await page.waitForTimeout(10);
	}
}

/** What passed between two `counts`. */
const since = (a, b) => ({ opens: b.opens - a.opens, acquired: b.acquired - a.acquired });

/**
 * Every `argvs` fired at once, so the queue behind the first is never empty.
 * Returns each outcome — with what it paid to acquire — and the span of all,
 * on the wall clock: the page's `Date` is held (da-ydao).
 */
const fireAtOnce = (page, argvs) =>
	page.evaluate(async (argvs) => {
		const started = performance.timeOrigin + performance.now();
		const runs = await Promise.all(
			argvs.map((argv) =>
				window.datum.runQuiet(argv).then((r) => ({
					code: r.code,
					stderr: r.stderr,
					tries: r.ms.tries,
					acquire: r.ms.acquire,
					at: performance.timeOrigin + performance.now()
				}))
			)
		);
		return { runs, spanMs: (performance.timeOrigin + performance.now()) - started, startedAt: started };
	}, argvs);

/** Ten writes, each its own minute; ten `--version`s, which touch no rows. */
const writes = Array.from({ length: 10 }, (_, i) => ['occur', 'ping', '-t', `07${String(i).padStart(2, '0')}`]);
const versions = Array.from({ length: 10 }, () => ['--version']);

describe('the files, held across a burst', () => {
	// `--version` for the counting, because sharing is only possible between
	// commands that fit inside the cap and this suite runs a browser per file
	// in parallel: a burst of ten writes on a loaded machine can spend the cap
	// on every one of them, which is the cap doing its job and would read here
	// as a regression. What the burst does to the rows is asserted below it,
	// where the command has to be a write.
	test('back-to-back commands share one acquisition, and give it back when the queue drains', async () => {
		await withApp(
			async (app) => {
				await app.idle();
				// What one acquisition costs in opens, measured rather than
				// written down: one handle per file SQLite may touch.
				let before = await counts(app.page);
				assert.equal((await app.run(['--version'])).code, 0);
				const measured = since(before, await counts(app.page));
				const perAcquire = measured.opens / measured.acquired;
				assert.ok(
					Number.isInteger(perAcquire) && perAcquire > 0,
					`${measured.opens} opens over ${measured.acquired} acquisitions — the count is not reaching the worker`
				);

				before = await counts(app.page);
				const burst = await fireAtOnce(app.page, versions);
				burst.runs.forEach((r, i) => assert.equal(r.code, 0, `${i}: ${r.stderr}`));

				// Each acquisition is one command's: the one that opened the
				// handles reports the tries, the ones after it opened nothing.
				const spent = since(before, await counts(app.page));
				assert.equal(
					spent.opens,
					spent.acquired * perAcquire,
					'the handles opened do not match the commands that say they opened them'
				);
				const acquisitions = burst.runs.filter((r) => r.tries > 0).length;
				assert.ok(acquisitions < 10, `ten commands, ${acquisitions} acquisitions — nothing was shared`);
				// One per cap window the burst spanned, plus the one it opened with.
				const allowed = Math.ceil(burst.spanMs / HOLD_MS) + 1;
				assert.ok(
					acquisitions <= allowed,
					`${acquisitions} acquisitions over ${burst.spanMs}ms, where ${allowed} is the cap's worth`
				);

				// Nothing was written, so no pane is refreshing behind this: the
				// queue drained, the files went back, and the next command has to
				// open them again.
				before = await counts(app.page);
				assert.equal((await app.run(['tail'])).code, 0);
				const reopened = since(before, await counts(app.page));
				assert.ok(reopened.acquired > 0, 'the files were still held across an idle page');
				assert.equal(reopened.opens, reopened.acquired * perAcquire);

				// And a burst of writes on shared handles keeps the database
				// whole: each created a journal and deleted it again, so a
				// directory not rebuilt per process would show up here.
				const wrote = await fireAtOnce(app.page, writes);
				wrote.runs.forEach((r, i) => assert.equal(r.code, 0, `${i}: ${r.stderr}`));
				const check = await app.run(['check']);
				assert.equal(check.code, 0, check.stdout + check.stderr);
				const rows = await app.entries(100);
				assert.equal(rows.length, 10);
				assert.equal(new Set(rows.map((e) => e.hid)).size, 10);
				assert.deepEqual(app.errors, []);
			},
			{ ...REPL, prepare: inWorker(countOpens) }
		);
	});

	test('a burst in one tab lets the other in without waiting for the whole of it', async () => {
		await withApp(async (first) => {
			const second = await first.newTab();
			await first.idle();
			await second.idle();

			// Not awaited: the other tab has to ask while this one is holding
			// the files, and this one's queue is never empty while it runs.
			await first.page.evaluate(() => {
				window.__started = performance.timeOrigin + performance.now();
				window.__done = [];
				window.__burst = Promise.all(
					Array.from({ length: 20 }, (_, i) =>
						window.datum
							.runQuiet(['occur', 'a', '-t', `07${String(i).padStart(2, '0')}`])
							.then((r) => {
								const done = { code: r.code, stderr: r.stderr, at: performance.timeOrigin + performance.now() };
								window.__done.push(done);
								return done;
							})
					)
				);
			});
			await first.page.waitForFunction(() => window.__done.length >= 3);

			// `ms.acquire` is what it waited for the files themselves — not the
			// pane refreshes of its own that the burst's writes put in front of
			// it, which is what a wall clock here would mostly be measuring.
			const other = await second.page.evaluate(async () => {
				const r = await window.datum.runQuiet(['occur', 'b', '-t', '0900']);
				return { code: r.code, stderr: r.stderr, acquire: r.ms.acquire, at: performance.timeOrigin + performance.now() };
			});
			const burst = await first.page.evaluate(() => window.__burst);
			const span = (await first.page.evaluate(() => (performance.timeOrigin + performance.now()) - window.__started)) || 1;
			burst.forEach((r, i) => assert.equal(r.code, 0, `${i}: ${r.stderr}`));
			assert.equal(other.code, 0, other.stderr);

			// It was let in mid-burst: writes of the burst landed after it.
			assert.ok(
				burst.filter((r) => r.at > other.at).length > 0,
				`the other tab's command landed only after all twenty (it waited ${other.acquire}ms for the files)`
			);
			// And it waited for a turn, not for the burst — measured against the
			// burst's own length, so a loaded machine slows both together.
			assert.ok(
				other.acquire * 4 < span,
				`${Math.round(other.acquire)}ms waiting for the files, of a ${span}ms burst of twenty`
			);

			const rows = await second.entries(100);
			assert.equal(rows.length, 21);
			assert.equal(new Set(rows.map((e) => e.hid)).size, 21);
			const check = await second.run(['check']);
			assert.equal(check.code, 0, check.stdout + check.stderr);
		}, REPL);
	});
});

// Asking the browser to keep it (da-frg2). Eviction is per origin, and an
// origin's storage is best-effort until the browser grants persistence — so
// the app asks, once, after the first entry lands durably, and the banner
// says what the browser answered.
//
// Chromium only: it is the one browser the harness launches, so these say
// nothing about Firefox's prompt.

/**
 * Count the page's calls to `navigator.storage.persist()`, keeping the
 * browser's answer. `answer: 'throw'` makes it throw instead; `answer: true`
 * grants, which headless Chromium itself does not (it answers false).
 */
const spyOnPersist =
	({ answer } = {}) =>
	(context) =>
		context.addInitScript((answer) => {
			const real = StorageManager.prototype.persist;
			window.__persistCalls = 0;
			StorageManager.prototype.persist = function () {
				window.__persistCalls++;
				if (answer === 'throw') throw new DOMException('refused by the test', 'NotAllowedError');
				return (window.__persistAnswer = answer === true ? Promise.resolve(true) : real.call(this));
			};
		}, answer);

const persistCalls = (app) => app.page.evaluate(() => window.__persistCalls);

const DURABLE =
	'datum.day is early software. Your entries are stored in this browser only: ' +
	'clearing site data deletes them, there is no sync yet, and';
const KEPT = `${DURABLE} the browser has promised to keep them.`;
const MAY_CLEAR = `${DURABLE} the browser may clear them if space runs short — install the app to keep them.`;

describe('asking the browser to keep it', () => {
	test('nothing is asked before the first write', async () => {
		await withApp(
			async (app) => {
				assert.equal(await app.page.evaluate(() => navigator.storage.persisted()), false);
				assert.equal(await persistCalls(app), 0);
				assert.equal(await app.bannerText(), MAY_CLEAR);
			},
			{ ...REPL, prepare: spyOnPersist() }
		);
	});

	test('the first durable write asks once, and the banner says what was granted', async () => {
		await withApp(
			async (app) => {
				for (const [i, time] of ['0700', '0701', '0702'].entries()) {
					const logged = await app.run(['occur', 'coffee', '-t', time]);
					assert.equal(logged.code, 0, logged.stderr);
					assert.equal(await persistCalls(app), 1, `after write ${i + 1}`);
				}
				// Headless Chromium may answer either way, so the claim is the
				// correspondence: the sentence is the one for what it answered.
				const granted = await app.page.evaluate(async () => {
					await window.__persistAnswer;
					return navigator.storage.persisted();
				});
				await until(app.bannerText, (t) => t === (granted ? KEPT : MAY_CLEAR));
			},
			{ ...REPL, prepare: spyOnPersist() }
		);
	});

	test('a granted request is what the banner promises', async () => {
		await withApp(
			async (app) => {
				assert.equal(await app.bannerText(), MAY_CLEAR);
				const logged = await app.run(['occur', 'coffee', '-t', '0700']);
				assert.equal(logged.code, 0, logged.stderr);
				await until(app.bannerText, (t) => t === KEPT);
			},
			{ ...REPL, prepare: spyOnPersist({ answer: true }) }
		);
	});

	test('nothing is asked while entries are held in memory', async () => {
		await withApp(
			async (app) => {
				const logged = await app.run(['occur', 'coffee', '-t', '0700']);
				assert.equal(logged.code, 0, logged.stderr);
				assert.equal(await persistCalls(app), 0);
				assert.match(await app.bannerText(), /held in memory only/);
			},
			{
				...REPL,
				prepare: async (context) => {
					await inWorker(NEVER)(context);
					await spyOnPersist()(context);
				}
			}
		);
	});

	test('a persist that throws does not fail the write', async () => {
		await withApp(
			async (app) => {
				const logged = await app.run(['occur', 'coffee', '-t', '0700']);
				assert.equal(logged.code, 0, logged.stderr);
				assert.deepEqual(
					(await app.entries()).map((e) => e.field),
					['coffee']
				);
				assert.equal(await persistCalls(app), 1);
				assert.equal(await app.bannerText(), MAY_CLEAR);
				assert.deepEqual(app.errors, []);
			},
			{ ...REPL, prepare: spyOnPersist({ answer: 'throw' }) }
		);
	});
});
