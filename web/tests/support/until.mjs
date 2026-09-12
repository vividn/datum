// The suite's one hand-rolled wait.
//
// Defined three times before this file (layouts, storage, shell), which is
// why it is here. `fields.test.mjs` had a fourth thing under the same name
// — a different signature waiting on one button's computed style — and it
// stayed there, renamed, because it is not this.
//
// The cap is a HANG-STOP, not a speed assertion. It exists so a condition
// that will never come true reports itself instead of sitting until the
// runner's own 120 s budget expires. 30 s is the suite's one tier, which is
// also playwright's own default for `waitFor` / `waitForFunction` — so a
// playwright wait writes no `timeout:` at all and this one matches it.

/** Poll `read` until `ok` accepts what it says, or give up loudly. */
export async function until(read, ok, timeout = 30000) {
	const deadline = Date.now() + timeout;
	for (;;) {
		const seen = await read();
		if (ok(seen)) return seen;
		if (Date.now() > deadline) throw new Error(`timed out waiting; last saw: ${JSON.stringify(seen)}`);
		await new Promise((resolve) => setTimeout(resolve, 100));
	}
}
