// A log's five effects on a large database, timed from the tap (da-2w0md).
//
// vividn, 2026-11-02: «button should change colors if state is changed; entry
// should appear in the output section; nowview should update; dayview should
// update; the entry should be broadcast to sync peers. ideally this all happens
// in <200 ms, even when a lot of data is loaded in the db». And: «quickly open
// the app, tap to log something and then exit the app again very quickly».
//
// What it does:
//  1. Builds a database of `--entries` rows (default 290,000, his size) natively,
//     about forty a day back from today in Europe/Berlin: points, blocks, string
//     switches and start/stop fields, with their specs. Copy A restores it and
//     invites; copy B joins A's chain through a local relay, so B holds every op
//     as a member does. Cached under target/measure-log/, keyed by the entry
//     count, the datum version and the day, since the join takes minutes.
//  2. Writes B into a PERSISTENT profile's OPFS before the app boots (incognito
//     OPFS costs ~100x, see measure-wasi.mjs), and gives the page B's client id,
//     so the page is B: the same endpoint key, a member of A's chain. A runs
//     `sync run` as the peer.
//  3. Taps a start/stop field's button `--presses` times and records, from the
//     tap: the button turning on or off, the output section, nowview, the tail,
//     the day strip (desktop) — the first and last DOM change of each — and the
//     moment A's file holds the entry (polled every 10 ms). Every CLI process the
//     page asks for is timed at the worker's door, the sync session's included,
//     with how long it queued and how long it ran.
//  4. `--leave`: taps, then after D ms closes the tab or freezes it (CDP
//     `Page.setWebLifecycleState`), and says whether A ever got the entry.
//
// Usage:  pnpm run build && cargo build --release -p datum-cli
//         node scripts/measure-log.mjs [--entries N] [--presses N] [--layout phone|desktop|dayview]
//                                     [--throttle R] [--leave] [--json out]
// DATUM_BIN names the native datum (default target/release/datum).

import { execFile, execFileSync, spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { closeSync, cpSync, createWriteStream, openSync, readSync, statSync, existsSync, mkdirSync, mkdtempSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:net';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';
import { preview } from 'vite';
import { median } from './median.mjs';

const web = fileURLToPath(new URL('..', import.meta.url));
const repo = join(web, '..');
const arg = (name, fallback) => {
	const i = process.argv.indexOf(`--${name}`);
	return i < 0 ? fallback : process.argv[i + 1];
};
const ENTRIES = Number(arg('entries', 290_000));
const PRESSES = Number(arg('presses', 6));
const LAYOUT = arg('layout', 'phone');
const THROTTLE = Number(arg('throttle', 1));
const LEAVE = process.argv.includes('--leave');
const DURING = process.argv.includes('--during');
const JSON_OUT = arg('json', null);
const BIN = process.env.DATUM_BIN ?? join(repo, 'target', 'release', 'datum');
const ZONE = 'Europe/Berlin';
const FIELD = 'walk';
const round = (x) => (x == null ? null : Math.round(x));
const log = (...a) => console.log(...a);

// ---- the native copies ------------------------------------------------------

const RELAY = {
	url: 'https://github.com/n0-computer/iroh/releases/download/v1.3.0/iroh-relay-v1.3.0-x86_64-unknown-linux-musl.tar.gz',
	sha256: '677f4c62342a6ba8044459b5fd4302f2b1dcb8402542072e3a4ade5039bc0b9e',
	dir: 'iroh-relay-v1.3.0'
};

/** The relay the sync tests pin (tests/support/sync.mjs), fetched the same way. */
async function relayBinary() {
	const home = join(repo, 'target', 'toolchains', RELAY.dir);
	const bin = join(home, 'iroh-relay');
	if (existsSync(bin)) return bin;
	mkdirSync(home, { recursive: true });
	const tarball = Buffer.from(await (await fetch(RELAY.url)).arrayBuffer());
	const actual = createHash('sha256').update(tarball).digest('hex');
	if (actual !== RELAY.sha256) throw new Error(`${RELAY.url}: sha256 ${actual}, pinned ${RELAY.sha256}`);
	const tmp = mkdtempSync(join(home, '.extract-'));
	execFileSync('tar', ['-xzf', '-', '-C', tmp], { input: tarball });
	renameSync(join(tmp, 'iroh-relay'), bin);
	rmSync(tmp, { recursive: true, force: true });
	return bin;
}

const freePort = () =>
	new Promise((resolve, reject) => {
		const s = createServer().listen(0, '127.0.0.1', () => {
			const { port } = s.address();
			s.close(() => resolve(port));
		});
		s.on('error', reject);
	});

/**
 * Started processes, each killed by its own pid on the way out. Under Linux's
 * `setpriv --pdeathsig KILL` where there is one, so a killed run leaves none
 * behind; elsewhere (macOS) the command itself, as tests/support/sync.mjs's `tie`.
 */
const children = new Set();
const tied = (() => {
	try {
		execFileSync('setpriv', ['--pdeathsig', 'KILL', '--', 'true'], { stdio: 'ignore' });
		return true;
	} catch {
		return false;
	}
})();
function start(bin, argv, env) {
	const [cmd, args] = tied ? ['setpriv', ['--pdeathsig', 'KILL', '--', bin, ...argv]] : [bin, argv];
	const child = spawn(cmd, args, { env, stdio: ['ignore', 'pipe', 'pipe'] });
	const out = { stdout: '', stderr: '', child };
	child.stdout.on('data', (d) => (out.stdout += d));
	child.stderr.on('data', (d) => (out.stderr += d));
	children.add(child);
	child.on('exit', () => children.delete(child));
	return out;
}
const stopAll = () => children.forEach((c) => c.kill());

async function startRelay(port) {
	const dir = mkdtempSync(join(repo, 'target', 'measure-log-relay-'));
	writeFileSync(join(dir, 'relay.toml'), `http_bind_addr = "127.0.0.1:${port}"\nenable_metrics = false\n`);
	start(await relayBinary(), ['--dev', '--config-path', join(dir, 'relay.toml')], process.env);
	const url = `http://127.0.0.1:${port}`;
	const up = () => fetch(url, { signal: AbortSignal.timeout(1000) }).then(() => true, () => false);
	for (let i = 0; i < 100; i++, await sleep(100)) if (await up()) return url;
	// The port is the fixture's: B holds A's address on this relay, so another port is another chain.
	throw new Error(`no relay on port ${port} — taken since the fixture was built? Remove target/measure-log/ to build it again`);
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** `datum` in `home`, on its `datum.db`. */
const envOf = (home, relay) => ({
	...process.env,
	HOME: home,
	DATUM_HOME: home,
	DATUM_DB: join(home, 'datum.db'),
	DATUM_SYNC_RELAY: relay,
	DATUM_SYNC_NO_DAEMON: '1',
	TZ: ZONE,
	NO_COLOR: '1'
});
const datum = (home, relay, argv) =>
	new Promise((resolve, reject) =>
		execFile(BIN, argv, { env: envOf(home, relay), encoding: 'utf8', maxBuffer: 1 << 30 }, (e, stdout, stderr) =>
			e ? reject(new Error(`datum ${argv.join(' ')}: ${stderr}`)) : resolve(stdout)
		)
	);

const POINTS = ['coffee', 'water', 'tea', 'snack', 'vitamins', 'pushups'];
const BLOCKS = ['read', 'meditate', 'shower', 'cook'];
const STATES = { mood: ['happy', 'calm', 'tired', 'low'], project: ['datum', 'house', 'admin'], location: ['home', 'office', 'cafe'], energy: ['high', 'mid', 'low'] };
const SPANS = ['sleep', 'work', 'walk', 'exercise', 'commute'];

/** Minutes east of UTC that Berlin's clock reads at `ms`. */
const offsetAt = (ms) => {
	const name = new Intl.DateTimeFormat('en', { timeZone: ZONE, timeZoneName: 'longOffset' })
		.formatToParts(new Date(ms))
		.find((p) => p.type === 'timeZoneName').value;
	const m = /GMT([+-])(\d\d):(\d\d)/.exec(name);
	return m ? (m[1] === '-' ? -1 : 1) * (Number(m[2]) * 60 + Number(m[3])) : 0;
};

/**
 * A day of a life, about forty rows: sleep's ends, work twice, a walk, the
 * moods and the rest. `local(h, m)` is that clock time today in Berlin as UTC ms.
 */
function* dayOf(day, local, rand) {
	const at = (h, m) => local(h, m + Math.floor(rand() * 20));
	yield [local(-1, 0) + Math.floor(rand() * 60) * 60e3, 'sleep', 'switch', true];
	yield [at(6, 50), 'sleep', 'switch', false];
	for (const [h, f] of [[7, 'coffee'], [11, 'coffee'], [15, 'coffee'], [8, 'vitamins'], [17, 'pushups'], [10, 'snack'], [16, 'snack'], [20, 'tea']])
		yield [at(h, 25), f, 'point'];
	for (let h = 8; h < 22; h += 2) yield [at(h, 5), 'water', 'point'];
	for (const [h, f] of [[7, 'shower'], [13, 'meditate'], [19, 'cook'], [22, 'read']]) yield [at(h, 30), f, 'block'];
	for (const [f, hours] of [['mood', [8, 13, 18]], ['energy', [9, 16]], ['project', [9, 14, 17]], ['location', [8, 18]]])
		for (const h of hours) yield [at(h, 40), f, 'switch', STATES[f][Math.floor(rand() * STATES[f].length)]];
	for (const [f, from, to] of [['commute', 8, 8], ['work', 9, 12], ['work', 13, 17], ['commute', 17, 17], ['walk', 12, 12], ['exercise', 18, 18]]) {
		if (f === 'exercise' && day % 2) continue;
		yield [at(from, 0), f, 'switch', true];
		yield [at(to, 45), f, 'switch', false];
	}
}

/** The backup's lines: the specs `specs` holds, then the generated rows, oldest first, none after now. */
function generate(header, specs, count) {
	let seed = 42;
	const rand = () => ((seed = (seed * 1103515245 + 12345) & 0x7fffffff) / 0x7fffffff);
	const perDay = [...dayOf(0, () => 0, rand)].length;
	const days = Math.ceil(count / perDay) + 1;
	const now = Date.now();
	const today = new Date(new Date().toLocaleDateString('en-CA', { timeZone: ZONE }) + 'T00:00:00Z').getTime();
	const rows = [];
	for (let d = days - 1; d >= 0; d--) {
		const midnight = today - d * 864e5;
		const local = (h, m) => midnight + (h * 60 + m) * 60e3 - offsetAt(midnight + 12 * 3600e3) * 60e3;
		for (const row of dayOf(d, local, rand)) if (row[0] < now - 60e3) rows.push(row);
	}
	rows.sort((a, b) => a[0] - b[0]);
	const kept = rows.slice(-count);
	const seen = new Set();
	const lines = [JSON.stringify({ ...header, source_db: 'measure-log', entry_count: kept.length + specs.length }), ...specs];
	kept.forEach(([ms, field, kind, state], i) => {
		const iso = new Date(ms).toISOString();
		const slug = `${field}:${iso}`;
		if (seen.has(slug)) return;
		seen.add(slug);
		const hid = 'm' + i.toString(36).padStart(11, '0');
		const shape =
			kind === 'point'
				? { verb: 'point-switch', state: null, dur: 'MOMENT', following_state: 'TRACKED' }
				: kind === 'block'
					? { verb: 'block-switch', state: null, dur: 'PT30M', following_state: 'TRACKED' }
					: { verb: 'switch', state, dur: null, following_state: null };
		const t = new Date(ms + 1000).toISOString();
		lines.push(
			JSON.stringify({
				hid,
				slug,
				content_hash: createHash('sha256').update(hid).digest('hex'),
				field,
				...shape,
				data: null,
				at: { utc: iso.replace('.000Z', 'Z'), offset_minutes: offsetAt(ms), timezone: ZONE },
				create_time: t,
				modify_time: t,
				client_id: '01M6MEASURELOG0000000000AA',
				id_structure: '%at%',
				raw_input: `generated ${i}`,
				parser_version: 2
			})
		);
	});
	return lines;
}

/** Copy A (the peer) and copy B (the page's), cached: `{ a, b, relayPort, clientId }`. */
async function fixture() {
	const version = execFileSync(BIN, ['--version'], { encoding: 'utf8' }).trim().replace(/\W+/g, '-');
	// And the day it was built: the rows end at that day's now, and the tail and
	// the nowview draw the latest hours, which a fixture from last week has none of.
	const day = new Date().toLocaleDateString('en-CA', { timeZone: ZONE });
	const dir = join(repo, 'target', 'measure-log', `${ENTRIES}-${version}-${day}`);
	const metaFile = join(dir, 'meta.json');
	if (existsSync(metaFile)) return JSON.parse(readFileSync(metaFile, 'utf8'));
	rmSync(dir, { recursive: true, force: true });
	const [s, a, b] = ['specs', 'a', 'b'].map((n) => join(dir, n));
	[s, a, b].forEach((d) => mkdirSync(d, { recursive: true }));
	const relayPort = await freePort();
	const relay = await startRelay(relayPort);
	log(`building ${ENTRIES} entries in ${dir}`);
	// The specs as the CLI writes them: each field logged once in a scratch db.
	for (const f of POINTS) await datum(s, relay, ['moment', f, '-y']);
	for (const f of BLOCKS) await datum(s, relay, ['moment', f, 'dur=30m', '-y', '-t', '1200']);
	for (const [f, states] of Object.entries(STATES)) await datum(s, relay, ['switch', f, states[0], '-y']);
	for (const f of SPANS) await datum(s, relay, ['start', f, '-y']);
	const [headerLine, ...rest] = (await datum(s, relay, ['db', 'backup', '-'])).trim().split('\n');
	const specs = rest.filter((l) => JSON.parse(l).field === 'spec');
	const file = join(dir, 'backup.jsonl');
	const out = createWriteStream(file);
	for (const line of generate(JSON.parse(headerLine), specs, ENTRIES)) out.write(line + '\n');
	await new Promise((r) => out.end(r));
	let t = Date.now();
	await datum(a, relay, ['db', 'restore', file]);
	log(`  restored into A in ${Math.round((Date.now() - t) / 1000)} s`);
	t = Date.now();
	const invite = start(BIN, ['sync', 'chain', 'invite'], envOf(a, relay));
	let ticket;
	while (!(ticket = /datum sync chain join '([^']+)'/.exec(invite.stdout)?.[1])) {
		if (invite.child.exitCode !== null) throw new Error(`invite: ${invite.stderr}`);
		await sleep(200);
	}
	await datum(b, relay, ['sync', 'chain', 'join', ticket, '--label', 'datum.day']);
	invite.child.kill();
	log(`  B joined A's chain in ${Math.round((Date.now() - t) / 1000)} s`);
	stopAll();
	// The page's SQLite has no WAL (WASI): B leaves it, folded into the file.
	const db = new DatabaseSync(join(b, 'datum.db'));
	db.exec('PRAGMA wal_checkpoint(TRUNCATE); PRAGMA journal_mode=DELETE;');
	const { client_id: clientId } = db.prepare('SELECT client_id FROM sync_local').get();
	db.close();
	// A's WAL folded in too, so each run copies one file.
	const peer = new DatabaseSync(join(a, 'datum.db'));
	peer.exec('PRAGMA wal_checkpoint(TRUNCATE);');
	peer.close();
	cpSync(join(a, 'datum.db'), join(dir, 'a.db'));
	const meta = { a, pristine: join(dir, 'a.db'), b: join(b, 'datum.db'), relayPort, clientId, entries: ENTRIES };
	writeFileSync(metaFile, JSON.stringify(meta));
	return meta;
}

// ---- the page -----------------------------------------------------------------

/**
 * Installed before the app's own modules: every message to a worker that runs a
 * command (`op: 'cli'` to the database worker, `op: 'dial'` to the sync worker)
 * is stamped as it is sent and as its answer comes back.
 */
function recorder() {
	const rec = (window.__log = { runs: [], dials: [], paints: [] });
	const pending = new WeakMap();
	// Heard in the constructor, so before the app's own handler: its answer
	// is stamped before the microtasks it sets off draw it.
	const Base = window.Worker;
	window.Worker = class extends Base {
		constructor(...args) {
			super(...args);
			const mine = new Map();
			pending.set(this, mine);
			this.addEventListener('message', ({ data }) => {
				const run = data && 'ok' in data ? mine.get(data.id) : undefined;
				if (!run) return;
				mine.delete(data.id);
				run.settled = performance.now();
				// The frame its answer is drawn in, as far as a frame can say.
				requestAnimationFrame(() => (run.frame = performance.now()));
				run.ms = data.cli?.ms;
				run.changed = data.cli?.changed?.length > 0;
				run.stdout = data.cli?.stdout?.slice(0, 4000);
			});
		}
	};
	const post = Base.prototype.postMessage;
	Base.prototype.postMessage = function (m, ...rest) {
		if (m && (m.op === 'cli' || m.op === 'dial') && m.id !== undefined && pending.has(this)) {
			const run = { argv: m.argv ?? [m.op], env: m.env, stdin: m.stdin, pane: !!m.pane, session: !!m.session, sent: performance.now() };
			(m.op === 'dial' ? rec.dials : rec.runs).push(run);
			pending.get(this).set(m.id, run);
		}
		return post.call(this, m, ...rest);
	};
}

/** Every change to the regions a log should change, stamped, and the button's on/off. */
function watchRegions(field) {
	const rec = window.__log;
	const region = (node) => {
		const el = node.nodeType === 1 ? node : node.parentElement;
		if (!el) return null;
		if (el.closest(`button.widget[data-field="${field}"]`)) return 'button';
		if (el.closest('.prompt-bar .section.output')) return 'output';
		for (const name of ['nowview', 'tail', 'dayview']) if (el.closest(`section.pane[data-title="${name}"]`)) return name;
		return null;
	};
	const button = () => document.querySelector(`button.widget[data-field="${field}"]`);
	rec.on = button()?.classList.contains('on');
	new MutationObserver((records) => {
		const at = performance.now();
		const names = new Set(records.map((r) => region(r.target)).filter(Boolean));
		for (const name of names) rec.paints.push({ name, at });
		const on = button()?.classList.contains('on');
		if (on !== rec.on) rec.paints.push({ name: 'button-flipped', at }), (rec.on = on);
	}).observe(document.body, { subtree: true, childList: true, attributes: true, characterData: true });
}

/** The phone's `collect` tab, its `dayview` tab, or the desktop's `main`. */
const SHAPES = {
	phone: { viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true, deviceScaleFactor: 3, home: { names: ['welcome', 'setup', 'collect', 'dayview'], active: 2 } },
	dayview: { viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true, deviceScaleFactor: 3, home: { names: ['welcome', 'setup', 'collect', 'dayview'], active: 3 } },
	desktop: { viewport: { width: 1280, height: 800 }, home: { names: ['main'], active: 0 } }
};

const meta = await fixture();
// A afresh each run: the page is B as the fixture left it, so the ops it signs
// reuse the sequence numbers of the last run's, which a used A holds already.
// At its own path, since a copy of the file elsewhere is another device (`sync_local.path`).
const peerHome = meta.a;
for (const end of ['-wal', '-shm']) rmSync(join(peerHome, 'datum.db' + end), { force: true });
cpSync(meta.pristine, join(peerHome, 'datum.db'));
const relay = await startRelay(meta.relayPort);
start(BIN, ['sync', 'run'], envOf(peerHome, relay));
const held = new DatabaseSync(join(peerHome, 'datum.db'), { readOnly: true });
// The echo names the entry by its shortest unique prefix: one of this field's.
const holds = held.prepare(`SELECT 1 FROM entries WHERE field = ? AND hid LIKE ? || '%'`);
/** When A's file first holds `hid`, in this process's `Date.now()`, or null by `until`. */
async function arrival(hid, until) {
	while (Date.now() < until) {
		if (holds.get(FIELD, hid)) return Date.now();
		await sleep(10);
	}
	return null;
}

const server = await preview({ root: web, logLevel: 'warn', preview: { host: '127.0.0.1', port: 0 } });
const base = server.resolvedUrls.local[0].replace(/\/$/, '');
const shape = SHAPES[LAYOUT];
const profile = mkdtempSync(join(repo, 'target', 'measure-log-profile-'));
const context = await chromium.launchPersistentContext(profile, {
	timezoneId: ZONE,
	viewport: shape.viewport,
	isMobile: shape.isMobile,
	hasTouch: shape.hasTouch,
	deviceScaleFactor: shape.deviceScaleFactor
});
await context.addInitScript(
	({ clientId, relay, home }) => {
		try {
			localStorage.setItem('datum-client-id', clientId);
			localStorage.setItem('datum.syncRelay', relay);
			localStorage.setItem('datum.tour', 'seen');
			if (!localStorage.getItem('datum.layout.root.v1')) localStorage.setItem('datum.layout.root.v1', JSON.stringify(home));
		} catch {}
	},
	{ clientId: meta.clientId, relay, home: shape.home }
);
await context.addInitScript(recorder);
await context.route(`${base}/__blank`, (r) => r.fulfill({ contentType: 'text/html', body: '<!doctype html><title>seed</title>' }));
// B's file is over a gigabyte, past what one route can fulfil: in chunks.
const CHUNK = 64 << 20;
const seedSize = statSync(meta.b).size;
await context.route(`${base}/__seed/*`, (r) => {
	const at = Number(r.request().url().split('/').pop()) * CHUNK;
	const body = Buffer.alloc(Math.min(CHUNK, seedSize - at));
	const fd = openSync(meta.b, 'r');
	readSync(fd, body, 0, body.length, at);
	closeSync(fd);
	return r.fulfill({ body, contentType: 'application/octet-stream' });
});

const report = { entries: meta.entries, layout: LAYOUT, throttle: THROTTLE, load: (await import('node:os')).loadavg()[0], presses: [], leave: [] };

/** A tab on the app, booted and quiet; `seed` first writes B's file into OPFS. */
async function openTab(seed) {
	const page = await context.newPage();
	if (seed) {
		await page.goto(`${base}/__blank`);
		const t = Date.now();
		await page.evaluate(async ([chunk, size]) => {
			const root = await navigator.storage.getDirectory();
			const dir = await root.getDirectoryHandle('datum-cli', { create: true });
			const file = await dir.getFileHandle('datum.db', { create: true });
			const out = await file.createWritable();
			for (let i = 0; i * chunk < size; i++) await out.write(await (await fetch(`/__seed/${i}`)).arrayBuffer());
			await out.close();
		}, [CHUNK, seedSize]);
		log(`seeded OPFS in ${Math.round((Date.now() - t) / 1000)} s`);
	}
	// The page's main thread only: CDP leaves dedicated workers, where the CLI runs, at full speed.
	if (THROTTLE > 1) await (await context.newCDPSession(page)).send('Emulation.setCPUThrottlingRate', { rate: THROTTLE });
	const t = Date.now();
	await page.goto(base + '/');
	await page.locator(`button.widget[data-field="${FIELD}"]`).waitFor({ timeout: 600_000 });
	await page.waitForFunction(() => window.datum?.sync?.states?.().datum?.endpoint, null, { timeout: 600_000 });
	await quiet(page, 3000);
	report.bootMs ??= Date.now() - t;
	report.panes ??= await page.evaluate(() => [...document.querySelectorAll('section.pane')].map((p) => p.dataset.title));
	await page.evaluate(watchRegions, FIELD);
	return page;
}

/**
 * Until every command has answered and none has been asked for `ms`. Not the
 * screen: the bar's clock changes it every second.
 */
const quiet = (page, ms) =>
	page.waitForFunction(
		(ms) => {
			const { runs, dials } = window.__log;
			if (![...runs, ...dials].every((r) => r.settled)) return false;
			const last = Math.max(0, ...runs.map((r) => r.settled), ...dials.map((r) => r.settled));
			return performance.now() - last > ms;
		},
		ms,
		{ timeout: 600_000, polling: 100 }
	);

/** Tap the field's button; resolve the tap's instant in the page's clock and in `Date.now()`. */
const tap = (page) =>
	page.evaluate((field) => {
		const rec = window.__log;
		rec.runs.length = rec.dials.length = rec.paints.length = 0;
		const button = document.querySelector(`button.widget[data-field="${field}"]`);
		rec.t0 = performance.now();
		const wall = Date.now();
		button.click();
		return { t0: rec.t0, wall };
	}, FIELD);

/** The tap's hid, from the log's own echo, once it has answered. */
const hidOf = (page) =>
	page
		.waitForFunction(() => {
			const write = window.__log.runs.find((r) => r.changed && !r.session);
			return write && /"hid":\s*"([0-9a-z]+)"/.exec(write.stdout ?? '')?.[1];
		}, null, { timeout: 120_000, polling: 5 })
		.then((h) => h.jsonValue());

let page = await openTab(true);
log(`panes: ${report.panes.join(', ')}`);
log(`booted on ${meta.entries} entries in ${Math.round(report.bootMs / 1000)} s (${LAYOUT}, CPU ${THROTTLE}x, load ${report.load.toFixed(1)})`);

for (let i = 0; i < PRESSES; i++) {
	const { wall } = await tap(page);
	const hid = await hidOf(page);
	const reached = await arrival(hid, Date.now() + 60_000);
	await quiet(page, 1500);
	const got = await page.evaluate(() => JSON.parse(JSON.stringify(window.__log)));
	report.presses.push(shapeOf(got, reached === null ? null : reached - wall));
}

/** A command's word, past `--db <name>` and flags. */
function wordOf(argv) {
	return argv.find((w, i) => !w.startsWith('-') && argv[i - 1] !== '--db');
}

function shapeOf({ t0, runs, dials, paints }, peerMs) {
	const firsts = {};
	const lasts = {};
	for (const p of paints) {
		firsts[p.name] ??= round(p.at - t0);
		lasts[p.name] = round(p.at - t0);
	}
	const cmd = (r) => ({
		argv: r.argv.filter((w, i) => !['--db', '--as'].includes(w) && !['--db', '--as'].includes(r.argv[i - 1])).join(' ').slice(0, 60),
		kind: r.session ? 'session' : r.pane ? 'pane' : 'first',
		sent: round(r.sent - t0),
		settled: round(r.settled - t0),
		ran: r.ms ? round(r.ms.acquire + r.ms.instantiate + r.ms.run + r.ms.flush) : null,
		run: r.ms ? round(r.ms.run) : null,
		// The request whole, for measure-persist.mjs to replay (da-zv6hn).
		request: { argv: r.argv, env: r.env, stdin: r.stdin, pane: r.pane, session: r.session }
	});
	// A view has drawn the tap once it changes after its own command's first
	// answer (`nowview …`, `tail …`, `dayview …`): a clock ticking in it moves
	// its first and last change, never this.
	const drawn = {};
	// The nowview lays a crop of the day's data layer: `dayview --data-layer` is its command too.
	for (const [name, word] of [['nowview', 'dayview'], ['tail', 'tail'], ['dayview', 'dayview']]) {
		const own = runs.filter((r) => r.settled && wordOf(r.argv) === word);
		if (!own.length) continue;
		// The first of them: the strip and the nowview each draw from one of several layers.
		const answered = Math.min(...own.map((r) => r.settled));
		const frame = Math.min(...own.map((r) => r.frame ?? r.settled));
		// Its change in that frame; failing one, the next (a pane that awaits more first).
		const paint = paints.find((p) => p.name === name && p.at >= answered && p.at <= frame + 1) ?? paints.find((p) => p.name === name && p.at >= answered);
		drawn[name] = paint ? round(paint.at - t0) : null;
	}
	return {
		paints: paints.map((p) => `${p.name}@${round(p.at - t0)}`).join(' '),
		changes: Object.fromEntries(Object.entries(Object.groupBy(paints, (p) => p.name)).map(([n, all]) => [n, all.length])),
		firsts,
		lasts,
		drawn,
		peerMs,
		dials: dials.map((d) => ({ sent: round(d.sent - t0), settled: round(d.settled - t0) })),
		commands: runs.filter((r) => r.settled).map(cmd)
	};
}

// ---- a tap while a sync session runs --------------------------------------------

// A tap's push is a session, and the peer pushes back: a second tap soon after
// meets one in the worker's queue, one process at a time.
if (DURING) {
	report.during = [];
	for (let i = 0; i < PRESSES; i++) {
		// A tap of its own to start a session, not recorded: the next tap is the one measured.
		await tap(page);
		await hidOf(page);
		await page.waitForFunction(
			// A session process running, not queued: everything ranked ahead of it has answered.
			// Any of them: since da-tvv4j a session runs no `sync status`, the one this waited for.
			() =>
				window.__log.runs.some((r) => r.session && !r.settled) &&
				window.__log.runs.every((r) => r.session || r.settled),
			null,
			{ timeout: 60_000, polling: 5 }
		);
		const { wall } = await tap(page);
		const hid = await hidOf(page);
		const reached = await arrival(hid, Date.now() + 60_000);
		await quiet(page, 1500);
		const got = await page.evaluate(() => JSON.parse(JSON.stringify(window.__log)));
		report.during.push(shapeOf(got, reached === null ? null : reached - wall));
	}
}

// ---- leaving straight after the tap -------------------------------------------

if (LEAVE) {
	for (const how of ['close', 'freeze']) {
		for (const after of [0, 250, 1000, 3000]) {
			const { wall } = await tap(page);
			const hid = await hidOf(page).catch(() => null);
			const since = Date.now() - wall;
			if (after > since) await sleep(after - since);
			const left = Date.now() - wall;
			let cdp;
			if (how === 'close') await page.close();
			else {
				cdp = await context.newCDPSession(page);
				await cdp.send('Page.setWebLifecycleState', { state: 'frozen' });
			}
			const reached = hid && (await arrival(hid, Date.now() + 20_000));
			report.leave.push({ how, after, left, hid, peerMs: reached ? reached - wall : null });
			log(`  ${how} ${left} ms after the tap: ${reached ? `A held it ${reached - wall} ms after the tap` : 'A never got it (20 s)'}`);
			if (how === 'close') page = await openTab(false);
			else {
				await cdp.send('Page.setWebLifecycleState', { state: 'active' });
				const later = hid && (await arrival(hid, Date.now() + 60_000));
				report.leave.at(-1).afterThawMs = later ? later - wall : null;
				await quiet(page, 1500);
			}
		}
	}
}

await context.close();
await server.close();
stopAll();
held.close();
rmSync(profile, { recursive: true, force: true });

// ---- the report -----------------------------------------------------------------

const P = report.presses;
const med = (pick) => {
	const xs = P.slice(1).map(pick).filter((x) => x != null);
	return xs.length ? round(median(xs)) : null;
};
log(`\n── ${P.length} taps on «${FIELD}», ms from the tap (first tap, then the median of the rest) ──`);
const row = (name, first, rest) => log(`  ${name.padEnd(28)} ${String(first ?? '—').padStart(7)} ${String(rest ?? '—').padStart(9)}`);
row('effect', 'first', 'median');
row('button flips', P[0]?.firsts['button-flipped'], med((p) => p.firsts['button-flipped']));
row('output section (first)', P[0]?.firsts.output, med((p) => p.firsts.output));
for (const name of ['nowview', 'tail', 'dayview'])
	row(`${name} drawn`, P[0]?.drawn[name], med((p) => p.drawn[name]));
row('the peer holds the entry', P[0]?.peerMs, med((p) => p.peerMs));
const table = (label, list) => {
	if (!list?.length) return;
	const m = (pick) => {
		const xs = list.map(pick).filter((x) => x != null);
		return xs.length ? round(median(xs)) : null;
	};
	log(`\n── ${label}: median of ${list.length} taps ──`);
	row('button flips', null, m((p) => p.firsts['button-flipped']));
	row('output section (first)', null, m((p) => p.firsts.output));
	for (const name of ['nowview', 'tail', 'dayview']) row(`${name} drawn`, null, m((p) => p.drawn[name]));
	row('the peer holds the entry', null, m((p) => p.peerMs));
};
table('a tap while a sync session runs', report.during);
const last = P.at(-1);
log(`  (DOM changes per region, last tap: ${JSON.stringify(last.changes)})`);
log(`\n── tap ${P.length}: every command, ms from the tap ──`);
for (const c of last.commands)
	log(`  ${c.kind.padEnd(8)} ${c.argv.padEnd(60)} sent ${String(c.sent).padStart(6)}  done ${String(c.settled).padStart(6)}  ran ${String(c.ran).padStart(5)}`);
for (const d of last.dials) log(`  dial     sent ${d.sent}  done ${d.settled}`);
if (JSON_OUT) writeFileSync(JSON_OUT, JSON.stringify(report, null, '\t'));
