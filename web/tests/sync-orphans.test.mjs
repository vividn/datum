// The sync harness's children die with the run that started them (da-sfe2t).
// The relay and a long-running native `datum` are stopped by an `after()`,
// and a run killed before it — a timeout, a cancelled job, a retired can —
// left them running under PID 1, one relay for 1h47m on 2026-10-02. So a
// node process starts both through the harness and is SIGKILLed, and
// neither may outlive it.

import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { tied } from './support/sync.mjs';

const SYNC = new URL('./support/sync.mjs', import.meta.url).href;
const UNTIL = new URL('./support/until.mjs', import.meta.url).href;

// What a test file does: a relay, a copy on it, and a `sync chain invite`
// serving until it is redeemed. It prints the two pids and waits to be killed.
const RUN = `
const { startRelay, nativeCopy } = await import(${JSON.stringify(SYNC)});
const { until } = await import(${JSON.stringify(UNTIL)});
const relay = await startRelay();
const serving = (await nativeCopy(relay)).spawn(['sync', 'chain', 'invite']);
await until(() => serving.stdout.includes('datum sync chain connect'), Boolean);
console.log(JSON.stringify({ relay: relay.pid, datum: serving.child.pid }));
setInterval(() => {}, 1 << 30);
`;

// Running and not a zombie: one reparented to a PID 1 that never reaps is
// dead all the same.
async function alive(pid) {
	if (process.platform !== 'linux') {
		try {
			return process.kill(pid, 0);
		} catch {
			return false;
		}
	}
	try {
		const stat = await readFile(`/proc/${pid}/stat`, 'utf8');
		return stat.slice(stat.lastIndexOf(')') + 2)[0] !== 'Z';
	} catch {
		return false;
	}
}

const living = async (pids) => (await Promise.all(Object.entries(pids).map(async ([k, p]) => (await alive(p)) && k))).filter(Boolean);

test(
	'the relay and a native `datum` die with a SIGKILLed test run',
	{ todo: !tied && `no parent-death signal on ${process.platform}: the children outlive the run` },
	async () => {
		// The run's own temp dir, so what it leaves behind goes with this one.
		const tmp = await mkdtemp(join(tmpdir(), 'datum-orphans-'));
		const run = spawn(process.execPath, ['--input-type=module', '-e', RUN], {
			env: { ...process.env, TMPDIR: tmp },
			stdio: ['ignore', 'pipe', 'inherit']
		});
		let pids;
		try {
			let out = '';
			run.stdout.on('data', (d) => (out += d));
			const exited = new Promise((resolve) => run.on('exit', resolve));
			await Promise.race([
				new Promise((resolve) => run.stdout.on('data', () => out.includes('\n') && resolve())),
				exited.then(() => assert.fail(`the run exited before starting both: ${out}`))
			]);
			pids = JSON.parse(out);
			assert.deepEqual(await living(pids), ['relay', 'datum'], 'both are running before the kill');

			run.kill('SIGKILL');
			await exited;
			const deadline = Date.now() + 5000;
			let left = await living(pids);
			while (left.length && Date.now() < deadline) {
				await new Promise((resolve) => setTimeout(resolve, 100));
				left = await living(pids);
			}
			assert.deepEqual(left, [], `alive 5 s after their run was SIGKILLed: ${JSON.stringify(pids)}`);
		} finally {
			run.kill('SIGKILL');
			for (const pid of Object.values(pids ?? {})) {
				try {
					process.kill(pid, 'SIGKILL');
				} catch {}
			}
			await rm(tmp, { recursive: true, force: true });
		}
	}
);
