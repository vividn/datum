// The page before the CLI's first answer (da-bhudz).
//
// vividn, 2026-10-07, on live after a deploy: «it is still blank for me even
// after a minute of waiting … like none of the tabs are even there in header
// bar». Nothing of the shell drew before the CLI answered — and the first
// open after a deploy that bumps the schema upgrades the database inside
// whichever command opens it first, a minute on a big history.
//
// So the CLI is made slow here, not the database big: the worker holds every
// answer it gives for HOLD ms after the page's first request — or after an
// upgrade begins — and the page is timed from its navigation, frame by frame. What runs is the real CLI on
// real OPFS; only the answers wait.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { inWorker, startApp, stopApp, withApp } from './support/app.mjs';
import { OPFS_DIR } from '../src/lib/databases.ts';
import { LAYOUT_VERSION } from '../src/lib/layout/types.ts';

const HOLD = 3000;

/**
 * Every answer the worker gives held for HOLD ms from `from` — the page's
 * first request, or the moment a schema upgrade begins, which is a slow
 * upgrade as the page sees it. Its notices are not held.
 */
const held = (from) => `
	const post = self.postMessage.bind(self);
	let until = null;
	const queue = [];
	const start = () => (until ??= performance.now() + ${HOLD});
	${from === 'request' ? "self.addEventListener('message', start);" : ''}
	self.postMessage = (message) => {
		if (message && 'notice' in message) {
			if (message.notice === 'upgrading') start();
			return post(message);
		}
		if (!queue.length && !(performance.now() < until)) return post(message);
		if (!queue.length) setTimeout(() => queue.splice(0).forEach((m) => post(m)), until - performance.now());
		queue.push(message);
	};
`;

/** When things first showed, from the navigation: a frame-by-frame watch, kept on `window.__marks`. */
const watch = (context) =>
	context.addInitScript(() => {
		const marks = (window.__marks = { said: [] });
		const tick = () => {
			const now = performance.now();
			const seen = (key, selector) => marks[key] === undefined && document.querySelector(selector) && (marks[key] = now);
			seen('tab', '.root-bar [role="tab"]');
			seen('pane', 'section.pane');
			seen('prompt', '.prompt-bar input:not([disabled])');
			if (marks.pane !== undefined && marks.stale === undefined) marks.stale = document.querySelectorAll('[data-stale]').length;
			const line = document.querySelector('.waiting')?.textContent.replace(/\s+/g, ' ').trim();
			if (line) marks.said.push([Math.round(now), line]);
			if (marks.said.length && !line && marks.quiet === undefined) marks.quiet = now;
			if (now < 30000) requestAnimationFrame(tick);
		};
		tick();
	});

const marksOf = (page) => page.evaluate(() => window.__marks);

before(startApp);
after(stopApp);

describe('before the CLI answers', () => {
	test('the shell draws at once, and says what it is waiting for', async () => {
		await withApp(
			async (app) => {
				const marks = await marksOf(app.page);
				// Drawn while the worker still held every answer: the prompt
				// enables on the first one.
				assert.ok(marks.tab < marks.prompt, `tabs at ${marks.tab} ms, the first answer at ${marks.prompt}`);
				assert.ok(marks.pane < marks.prompt, `panes at ${marks.pane} ms, the first answer at ${marks.prompt}`);
				assert.ok(marks.prompt >= HOLD, `the first answer at ${marks.prompt} ms: the hold did not hold`);
				console.log(`# tabs +${Math.round(marks.tab)} ms, panes +${Math.round(marks.pane)} ms, first answer +${Math.round(marks.prompt)} ms`);
				// Said after a second, counting, and gone with the answer.
				const [at, line] = marks.said[0];
				assert.ok(at >= 1000 && at < marks.prompt, `said at ${at} ms`);
				assert.match(line, /^Opening your data… 1 s$/);
				// Counting, never ahead of the clock, and through the hold. No
				// upper bound: how long past the hold the answer takes is the
				// CLI's latency, which a loaded runner stretches («4 s» on CI,
				// da-xldfb). The count trails its clock by up to a 250 ms tick,
				// so at the hold's end it reads at least one second less.
				const counts = marks.said.map(([at, said]) => {
					const n = Number(said.match(/^Opening your data… (\d+) s$/)?.[1]);
					assert.ok(n <= at / 1000, `«${said}» at ${at} ms`);
					return n;
				});
				assert.ok(counts.every((n, i) => !i || n >= counts[i - 1]), counts.join(' '));
				assert.ok(counts.at(-1) >= HOLD / 1000 - 1, `the count stopped at ${counts.at(-1)} s`);
				// Through the hold, and gone after it: the prompt's own `help --json`
				// is sent only once `open` answers, so it may enable well after.
				assert.ok(marks.quiet !== undefined, 'the line outlived the wait');
				assert.ok(marks.quiet >= HOLD, `the line went at ${marks.quiet} ms, inside the hold`);
			},
			{ prepare: async (context) => (await inWorker(held('request'))(context), await watch(context)) }
		);
	});

	// vremya on #506: with the copy never drawn, every test above still
	// passed. A copy of the last visit, its first tab renamed, tells it from
	// the provisional tree.
	for (const [version, drawn] of [[LAYOUT_VERSION, true], [LAYOUT_VERSION + 1, false]]) {
		test(`a copy of the last visit at version ${version} is ${drawn ? '' : 'not '}what draws first`, async () => {
			await withApp(async (app) => {
				const key = await app.page.evaluate(() => Object.keys(localStorage).find((k) => k.startsWith('datum.layout.shell.v1.')));
				await app.page.evaluate(
					([key, version]) => {
						const copy = JSON.parse(localStorage.getItem(key));
						localStorage.setItem(key, JSON.stringify({ ...copy, version, root: { ...copy.root, tabs: copy.root.tabs.map((t, i) => (i ? t : { ...t, layout: 'from-the-copy' })) } }));
					},
					[key, version]
				);
				await inWorker(held('request'))(app.page.context());
				await watch(app.page.context());
				await app.page.context().addInitScript(() => {
					const tick = () => {
						if ([...document.querySelectorAll('.root-bar [role="tab"]')].some((t) => t.textContent.includes('from-the-copy')))
							return (window.__marks.copy = performance.now());
						if (performance.now() < 30000) requestAnimationFrame(tick);
					};
					tick();
				});
				await app.reload();
				const marks = await marksOf(app.page);
				assert.ok(marks.prompt >= HOLD, `the first answer at ${marks.prompt} ms: the hold did not hold`);
				if (drawn) assert.ok(marks.copy < marks.prompt, `the copy's tab at ${marks.copy} ms, the first answer at ${marks.prompt}`);
				else assert.equal(marks.copy, undefined, 'a copy of another version was drawn');
				assert.ok(marks.tab < marks.prompt, `tabs at ${marks.tab} ms, the first answer at ${marks.prompt}`);
				// The real pass writes a copy of its own, at this version.
				const copy = await app.page.evaluate((key) => JSON.parse(localStorage.getItem(key)), key);
				assert.equal(copy.version, LAYOUT_VERSION);
				assert.ok(!JSON.stringify(copy).includes('from-the-copy'), 'the old copy was kept');
			});
		});
	}

	test('a change made to the early tree outlives the boot, and is written down', async () => {
		await withApp(
			async (app) => {
				const early = await app.page.evaluate(() => window.__early);
				assert.ok(early < 3000, `changed at ${early} ms, after the boot`);
				const settings = await app.page.evaluate(() => {
					const find = (pane) => pane.tabs.flatMap((t) => (t.content.kind ? [t.content] : t.content.children.flatMap(find)));
					return find(window.datum.layout.current().root).find((w) => w.id === 'tail-widget').settings;
				});
				assert.equal(settings.early, true, 'the boot dropped the change');
				assert.match(await app.page.evaluate(() => localStorage.getItem('datum.layout.draft.v1.main') ?? ''), /"early":true/);
			},
			{
				prepare: async (context) => {
					await inWorker(held('request'))(context);
					// While every answer is held: a setting on the tail, through `apply`.
					await context.addInitScript(() => {
						const set = (pane) => ({
							...pane,
							tabs: pane.tabs.map((t) => ({
								...t,
								content: t.content.id === 'tail-widget'
									? { ...t.content, settings: { ...t.content.settings, early: true } }
									: t.content.children ? { ...t.content, children: t.content.children.map(set) } : t.content
							}))
						});
						const tick = () => {
							if (!window.datum?.layout || !document.querySelector('[data-title="tail"]')) return requestAnimationFrame(tick);
							window.datum.layout.apply((l) => ({ ...l, root: set(l.root) }));
							window.__early = performance.now();
						};
						tick();
					});
				}
			}
		);
	});

	test('a schema upgrade says so until it is over, then the panes are its answers', async () => {
		await withApp(
			async (app) => {
				await app.run(['occur', 'coffee']);
				await previousSchema(app.page);
				await app.reload();
				const marks = await marksOf(app.page);
				// Whatever was said before the upgrade began, the upgrade is what
				// was said from then until it was over.
				const lines = marks.said.map(([, line]) => line);
				const from = lines.findIndex((line) => line.startsWith('Upgrading'));
				assert.ok(from >= 0, `the upgrade was never named: ${lines.join(' | ')}`);
				assert.ok(
					lines.slice(from).every((line) => /^Upgrading your data to this version \(one time\), keeping a copy of it as it was… \d s$/.test(line)),
					lines.join(' | ')
				);
				assert.ok(marks.quiet !== undefined, 'the line stayed after the upgrade');
				const status = await app.run(['db', 'migrate', 'status']);
				assert.match(status.stdout, /Current schema version: v(\d+)\s+Binary target version: {2}v\1/);
				assert.equal((await app.run(['tail', '-n', '1'])).stdout.includes('coffee'), true);
			},
			{ prepare: async (context) => (await inWorker(held('upgrade'))(context), await watch(context)) }
		);
	});

	// da-hse93: the copy was a file in the process's memory, gone at its exit
	// while the CLI's note said it was saved.
	test('the upgrade keeps a copy of the data as it was, beside it, past a reload', async () => {
		await withApp(async (app) => {
			await app.run(['occur', 'coffee']);
			await previousSchema(app.page);
			await app.reload();
			await app.reload();
			await app.run(['tail', '-n', '1']);
			const names = await app.page.evaluate(async (dir) => {
				const files = await (await navigator.storage.getDirectory()).getDirectoryHandle(dir);
				const names = [];
				for await (const name of files.keys()) names.push(name);
				return names;
			}, OPFS_DIR);
			const kept = names.filter((name) => /^datum\.db\.pre-v54-to-v\d+-\d{8}T\d{6}Z\.bak$/.test(name));
			assert.equal(kept.length, 1, names.join(', '));
			const scratch = await mkdtemp(join(tmpdir(), 'datum-backup-'));
			try {
				const path = join(scratch, kept[0]);
				await writeFile(path, Uint8Array.from(await opfsBytes(app.page, kept[0])));
				const db = new DatabaseSync(path, { readOnly: true });
				assert.equal(db.prepare("SELECT value FROM _meta WHERE key = 'schema_version'").get().value, '54');
				assert.equal(db.prepare("SELECT COUNT(*) AS n FROM entries WHERE field = 'coffee'").get().n, 1);
				db.close();
			} finally {
				await rm(scratch, { recursive: true, force: true });
			}
		});
	});

	// da-nc8i1: a purge leaves nothing of the database, as deleting its file
	// would. Every file the worker keeps for it is read: the database, its
	// journal, and the copy an upgrade kept, which is the whole of it as it was.
	test('a purge leaves none of the data in OPFS: the file, its journal, the copy an upgrade kept', async () => {
		await withApp(async (app) => {
			// Long enough to fill more pages than the new schema takes back:
			// a page it reuses is zeroed whether or not the file is rewritten.
			const mark = 'zebra-marker-77';
			await app.run(['occur', 'coffee', `note=${mark.repeat(8000)}`]);
			await previousSchema(app.page);
			await app.reload();
			await app.run(['tail', '-n', '1']);
			const holding = () =>
				app.page.evaluate(
					([dir, mark]) =>
						navigator.locks.request('datum-cli-files', async () => {
							const files = await (await navigator.storage.getDirectory()).getDirectoryHandle(dir);
							const names = [];
							for await (const [name, file] of files.entries()) {
								const bytes = await (await file.getFile()).arrayBuffer();
								if (new TextDecoder('latin1').decode(bytes).includes(mark)) names.push(name);
							}
							return names.sort();
						}),
					[OPFS_DIR, mark]
				);
			const before = await holding();
			assert.ok(before.includes('datum.db') && before.some((name) => name.endsWith('.bak')), before.join(', '));
			const purged = await app.run(['db', 'purge', '--yes']);
			assert.equal(purged.code, 0, purged.stderr);
			assert.match(purged.stdout, /removed: \S*datum\.db\.pre-v54-to-v\d+-\d{8}T\d{6}Z\.bak/);
			assert.deepEqual(await holding(), []);
			assert.equal((await app.run(['count'])).stdout.trim(), '0');
		});
	});

	test('each pane shows its last output until its answer lands', async () => {
		await withApp(async (app) => {
			// Through the seam, which refreshes no pane: the reload draws it.
			await app.run(['occur', 'coffee']);
			await app.reload();
			// Kept when the page is left, which a reload is.
			await watch(app.page.context());
			await app.reload();
			const kept = await app.page.evaluate(() => JSON.parse(localStorage.getItem('datum.stale.v1') ?? '{}').copies ?? {});
			assert.ok(Object.values(kept).some((html) => html.includes('coffee')), 'no pane kept the coffee');
			const marks = await marksOf(app.page);
			assert.equal(marks.stale, Object.keys(kept).length, 'a copy for every pane that kept one, drawn with the panes');
			// ready() has waited every copy out: none outlives its answer.
			assert.equal(await app.page.locator('[data-stale]').count(), 0);
		});
	});

	// da-qhu1g: a REPL asks the CLI nothing, and through `/repl` the tree
	// mounts after the boot's commands have answered, so no queue was left to
	// run dry for it. Beside another tab, because a lone one leads the sync,
	// and its read of the chain took the copy down by accident.
	test('a pane that asks the CLI nothing loses its copy on a tab opened through /repl', async () => {
		await withApp(
			async (app) => {
				// Kept when a tab is left, which a close is.
				await (await app.newTab('repl')).page.close();
				await watch(app.page.context());
				const third = await app.newTab('repl');
				assert.equal((await marksOf(third.page)).stale, 1, 'the copy was not drawn');
				// ready() has waited it out.
				assert.equal(await third.page.locator('[data-stale]').count(), 0);
			},
			{ query: 'repl' }
		);
	});
});

// vremya on #485: «the stale cache must never crowd out the reader's own
// layout state». A store the copies have filled to the last character still
// takes the reader's draft: the cache goes first.
describe('the copies from the last visit', () => {
	test('yield the room when the reader\'s own layout state needs it', async () => {
		await withApp(async (app) => {
			const kept = await app.page.evaluate(() => {
				let size = 0;
				for (let step = 1 << 23; step >= 1; step >>= 1) {
					try {
						localStorage.setItem('datum.stale.v1', JSON.stringify({ db: 'datum', copies: { x: 'x'.repeat(size + step) } }));
						size += step;
					} catch {
						// Past the quota: a smaller step.
					}
				}
				localStorage.setItem('datum.stale.v1', JSON.stringify({ db: 'datum', copies: { x: 'x'.repeat(size) } }));
				try {
					localStorage.setItem('datum.probe', 'xx');
					return 'not full';
				} catch {
					return size;
				}
			});
			assert.equal(typeof kept, 'number', 'the store was not filled');
			// A layout edit, which `settle` writes down as the draft.
			await app.page.evaluate(() => {
				const set = (pane) => ({
					...pane,
					tabs: pane.tabs.map((t) => ({
						...t,
						content: t.content.id === 'tail-widget'
							? { ...t.content, settings: { ...t.content.settings, mine: true } }
							: t.content.children ? { ...t.content, children: t.content.children.map(set) } : t.content
					}))
				});
				window.datum.layout.apply((l) => ({ ...l, root: set(l.root) }));
			});
			const draft = await app.page.evaluate(() => localStorage.getItem('datum.layout.draft.v1.main'));
			assert.match(draft ?? '', /"mine":true/, 'the draft was not kept');
			assert.equal(await app.page.evaluate(() => localStorage.getItem('datum.stale.v1')), null, 'the cache did not yield');
		});
	});
});

/**
 * The page's database moved back a schema version, under the worker's own
 * lock, so the next command that opens it upgrades it: v55's own down
 * migration (`down_55_to_54_ops_held`) and its version stamp.
 */
async function previousSchema(page) {
	const bytes = await opfsBytes(page, 'datum.db');
	const scratch = await mkdtemp(join(tmpdir(), 'datum-schema-'));
	try {
		const path = join(scratch, 'datum.db');
		await writeFile(path, Uint8Array.from(bytes));
		const db = new DatabaseSync(path);
		db.exec(`
			DROP INDEX IF EXISTS ops_held;
			UPDATE _meta SET value = '54' WHERE key = 'schema_version';
		`);
		db.close();
		const older = [...(await readFile(path))];
		await page.evaluate(
			async ([dir, older]) =>
				navigator.locks.request('datum-cli-files', async () => {
					const files = await (await navigator.storage.getDirectory()).getDirectoryHandle(dir);
					const writable = await (await files.getFileHandle('datum.db')).createWritable();
					await writable.write(Uint8Array.from(older));
					await writable.close();
				}),
			[OPFS_DIR, older]
		);
	} finally {
		await rm(scratch, { recursive: true, force: true });
	}
}

/** A file of the worker's in OPFS, read under its lock. */
function opfsBytes(page, name) {
	return page.evaluate(
		([dir, name]) =>
			navigator.locks.request('datum-cli-files', async () => {
				const files = await (await navigator.storage.getDirectory()).getDirectoryHandle(dir);
				const file = await (await files.getFileHandle(name)).getFile();
				return [...new Uint8Array(await file.arrayBuffer())];
			}),
		[OPFS_DIR, name]
	);
}
