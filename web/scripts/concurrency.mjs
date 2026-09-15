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
// The budget is in files, not cores: the box is willing to be committed to
// `cores - 1` files, and a file under test is about two cores' worth (a
// Chromium and the node driving it), so each unit of load average already on
// the box costs half a file. An idle 24-core box still gets six, an idle
// 4-core CI runner still gets three, and a box at load 88 on 24 cores gets one.
//
// The «two cores a file» coefficient is one observation from that box, not a
// measurement (vremya, da-x38v): the inputs are printed on the run's first
// line so a red or slow run says what it ran at, and this is the number to
// doubt first if a small runner gets SLOWER under the load term.
export function filesAtOnce(cores, load, cap = 6, override) {
	if (Number.isFinite(override) && override > 0) return Math.max(1, Math.floor(override));
	return Math.max(1, Math.min(cap, Math.floor(cores - 1 - Math.max(0, load) / 2)));
}
