// Easy Sync's harness (da-6kpj5): a local iroh relay, and native CLI copies
// that sync through it. Hermetic — nothing reaches n0.
//
// The relay is iroh's own `iroh-relay` 1.3.0, the release n0 builds, pinned
// and sha256-checked into target/toolchains/ the way build-cli-wasm.mjs
// fetches its tools, and run in `--dev` mode (plain HTTP) on a port of its
// own per file, since files run side by side. A browser endpoint reaches it
// over a WebSocket; a native one is pointed at it by `DATUM_SYNC_RELAY`, and
// datum.day by `localStorage['datum.syncRelay']` (`relayed`).

import { execFile, execFileSync, spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import { mkdtemp, rm } from 'node:fs/promises';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { until } from './until.mjs';

const repo = fileURLToPath(new URL('../../..', import.meta.url));

const RELAY = {
	url: 'https://github.com/n0-computer/iroh/releases/download/v1.3.0/iroh-relay-v1.3.0-x86_64-unknown-linux-musl.tar.gz',
	sha256: '677f4c62342a6ba8044459b5fd4302f2b1dcb8402542072e3a4ade5039bc0b9e',
	dir: 'iroh-relay-v1.3.0'
};

async function relayBinary() {
	const home = join(repo, 'target', 'toolchains', RELAY.dir);
	const bin = join(home, 'iroh-relay');
	if (existsSync(bin)) return bin;
	mkdirSync(home, { recursive: true });
	const res = await fetch(RELAY.url);
	if (!res.ok) throw new Error(`${RELAY.url}: HTTP ${res.status}`);
	const tarball = Buffer.from(await res.arrayBuffer());
	const actual = createHash('sha256').update(tarball).digest('hex');
	if (actual !== RELAY.sha256) throw new Error(`${RELAY.url}: sha256 ${actual}, pinned ${RELAY.sha256}`);
	writeFileSync(`${bin}.tar.gz`, tarball);
	execFileSync('tar', ['-xzf', `${bin}.tar.gz`, '-C', home]);
	return bin;
}

function freePort() {
	return new Promise((resolve, reject) => {
		const s = createServer().listen(0, '127.0.0.1', () => {
			const { port } = s.address();
			s.close(() => resolve(port));
		});
		s.on('error', reject);
	});
}

/** A relay of this file's own: `{ url, stop }`. */
export async function startRelay() {
	const bin = await relayBinary();
	const dir = await mkdtemp(join(tmpdir(), 'datum-relay-'));
	const port = await freePort();
	const config = join(dir, 'relay.toml');
	writeFileSync(config, `http_bind_addr = "127.0.0.1:${port}"\nenable_metrics = false\n`);
	const proc = spawn(bin, ['--dev', '--config-path', config], { stdio: 'ignore' });
	const url = `http://127.0.0.1:${port}`;
	await until(() => fetch(url).then(() => true, () => false), Boolean);
	return {
		url,
		async stop() {
			proc.kill();
			await rm(dir, { recursive: true, force: true });
		}
	};
}

/** `withApp`'s `prepare` for an app that syncs through `relay`. */
export const relayed = (relay) => (context) =>
	context.addInitScript((url) => localStorage.setItem('datum.syncRelay', url), relay.url);

/**
 * The native `datum` a sync test pairs with: this tree's debug build first,
 * which CI's cargo step has just made — a release build left from another
 * day would test another day's CLI.
 */
function datumBinary() {
	if (process.env.DATUM_BIN) return process.env.DATUM_BIN;
	// Where `cargo build` put it: CARGO_TARGET_DIR, relative to the repo root
	// when relative, as mq-check runs cargo there (da-0cdib).
	const target = resolve(repo, process.env.CARGO_TARGET_DIR || 'target');
	for (const profile of ['debug', 'release']) {
		const candidate = join(target, profile, 'datum');
		if (existsSync(candidate)) return candidate;
	}
	throw new Error('no `datum` binary found — run `cargo build -p datum-cli` or set DATUM_BIN');
}

/**
 * A native copy of a database, on `relay`. `run(argv, stdin, env)` resolves
 * to its `{ code, stdout, stderr }`; `spawn(argv)` starts a long-running one
 * (`sync run`, `sync invite`) and collects what it prints; `count(field)`
 * is how many entries of `field` it holds; `close` stops every process and
 * removes the copy.
 */
export async function nativeCopy(relay) {
	const bin = datumBinary();
	const home = await mkdtemp(join(tmpdir(), 'datum-native-'));
	const env = {
		...process.env,
		HOME: home,
		DATUM_HOME: home,
		DATUM_DB: join(home, 'datum.db'),
		DATUM_SYNC_RELAY: relay.url,
		DATUM_SYNC_TIMEOUT_S: '20',
		TZ: 'UTC',
		NO_COLOR: '1'
	};
	const running = new Set();
	const self = {
		run: (argv, stdin, more = {}) =>
			new Promise((resolve) => {
				const child = execFile(bin, argv, { env: { ...env, ...more }, encoding: 'utf8' }, (e, stdout, stderr) =>
					resolve({ code: e ? (e.code ?? 1) : 0, stdout: stdout.trim(), stderr: stderr.trim() })
				);
				child.stdin.end(stdin ?? '');
			}),
		spawn(argv) {
			const child = spawn(bin, argv, { env, stdio: ['ignore', 'pipe', 'pipe'] });
			const out = { stdout: '', stderr: '', child };
			child.stdout.on('data', (d) => (out.stdout += d));
			child.stderr.on('data', (d) => (out.stderr += d));
			running.add(child);
			child.on('exit', () => running.delete(child));
			return out;
		},
		count: async (field) => Number((await self.run(['count', field])).stdout),
		verify: async () => (await self.run(['sync', 'verify'])).stdout,
		async close() {
			for (const child of running) child.kill();
			await rm(home, { recursive: true, force: true });
		}
	};
	return self;
}

/**
 * Pair the browser tab `app` with `native` through a sync chain, as `datum
 * sync invite` and `connect` do — the native copy inviting, which starts an
 * open chain, the page redeeming — and return the page's report of that
 * first sync.
 */
export async function pairWith(app, native) {
	const serving = native.spawn(['sync', 'invite']);
	const said = await until(() => /datum sync connect (\S+)/.exec(serving.stdout), Boolean);
	return app.page.evaluate((t) => window.datum.sync.connect('datum', t), said[1]);
}

/** A tab's `sync verify`. */
export const verifyOf = async (app) => (await app.run(['sync', 'verify'])).stdout;

/** How many entries of `field` a tab's database holds. */
export const countOf = async (app, field) => Number((await app.run(['count', field])).stdout);

/** The databases whose sync `app`'s tab leads. */
export const leadsOf = (app) => app.page.evaluate(() => window.datum.sync.leads());
