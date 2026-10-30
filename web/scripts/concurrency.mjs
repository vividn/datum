// How many test files `pnpm test` runs at once — the arithmetic, and the runs
// already on the box, so a test can import it without spawning the suite.

import { mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
//
// Cores were the only signal until 2026-09-15, when seven cans each ran the
// web suite at six files on the same 24-core box: load average 71/83/88, 160
// Chromium processes, and the merge queue's own check crawling under them —
// nothing landed on main for 70 minutes. Six is right for one suite on an idle
// box and wrong for seven sharing it, so the load the box already carries is
// read too (da-x38v).
//
// The budget is in files, not cores: a file under test is about two cores'
// worth (a Chromium and the node driving it), so the box gets at most
// `cores / 2` files, and each unit of load average already on the box costs
// half a file off `cores - 1`. An idle 24-core box still gets six (the cap),
// an idle 4-core CI runner gets two, not three — three was six cores of work
// on four, the cores term charging one core a file while the load term
// charged two (da-j2okg) — and a box at load 88 on 24 cores gets one.
//
// The «two cores a file» coefficient is one observation from that box, not a
// measurement (vremya, da-x38v): the inputs are printed on the run's first
// line so a red or slow run says what it ran at, and this is the number to
// doubt first if a small runner gets SLOWER under the load term.
//
// An override is a ceiling, not an exact number: it gets no more files than
// the cores less two (da-dwq7g, da-gimld). Load is left out on purpose — the
// override exists because a CI job's load average is the whole host's, not its
// lane's — but cores are not: CI's 3 on the 3-core forge lane was six cores of
// work on three, and gets one. Not half the cores, as with no override: the
// 6-core CI lane measured 1.2 cores a file at three files (run 5177, lane CPU
// 4920 s over 4107 s of file walls), so four is ~4.8 cores of its six.
// Nor is it held to the cap, which is for a box cans share: an override is the
// box's own choice — 12 on the 16-core staging ship (da-7u6ig).
export function filesAtOnce(cores, load, cap = 6, override) {
	if (Number.isFinite(override) && override > 0) return Math.max(1, Math.min(Math.floor(override), cores - 2));
	return Math.max(1, Math.min(cap, Math.floor(cores / 2), Math.floor(cores - 1 - Math.max(0, load) / 2)));
}

const alive = (pid) => {
	try {
		process.kill(pid, 0);
		return true;
	} catch (e) {
		return e.code === 'EPERM'; // another user's
	}
};

// Suites that start together each read the load before any of them has added
// to it: three cans' gates on an idle 24-core box each read about 0 and took
// six files, and the load average, a minute's mean, says so only after (da-ueire).
// So each run writes the files it takes into `dir`, one entry per pid, and a
// run reads the entries of the live runs before it chooses, under a lock, so
// two runs never choose at once. `decide(others)` gets the files they took and
// returns this run's; test.mjs charges each two units of load, as a file costs.
// What it cannot read or write costs nothing: the run decides on the load alone.
export function claim(dir, decide) {
	const lock = join(dir, 'lock');
	try {
		mkdirSync(dir, { recursive: true });
		for (;;) {
			try {
				mkdirSync(lock);
				break;
			} catch (e) {
				if (e.code !== 'EEXIST') throw e;
				// A run killed while it held the lock, a few milliseconds, held it for good.
				if (Date.now() - statSync(lock, { throwIfNoEntry: false })?.mtimeMs > 5_000) rmSync(lock, { recursive: true, force: true });
				Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 20);
			}
		}
	} catch {
		return decide(0);
	}
	try {
		let others = 0;
		for (const f of readdirSync(dir).filter((f) => /^\d+$/.test(f) && Number(f) !== process.pid)) {
			const entry = join(dir, f);
			try {
				if (!alive(Number(f))) rmSync(entry, { force: true }); // a run that died without its exit
				else others += Number(readFileSync(entry, 'utf8')) || 0;
			} catch {} // gone as it was read, or not this user's to remove
		}
		const mine = join(dir, String(process.pid));
		process.on('exit', () => rmSync(mine, { force: true }));
		const n = decide(others);
		writeFileSync(mine, String(n));
		return n;
	} finally {
		rmSync(lock, { recursive: true, force: true });
	}
}
