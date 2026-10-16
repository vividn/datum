// How many test files `pnpm test` runs at once — the arithmetic, alone, so a
// test can import it without spawning the suite.
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
export function filesAtOnce(cores, load, cap = 6, override) {
	if (Number.isFinite(override) && override > 0) return Math.max(1, Math.min(Math.floor(override), cap, cores - 2));
	return Math.max(1, Math.min(cap, Math.floor(cores / 2), Math.floor(cores - 1 - Math.max(0, load) / 2)));
}
