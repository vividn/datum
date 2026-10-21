// A press that travels, on a band that tells a pan from a range by WHEN it
// first does (`advance`, $lib/nowpoint.ts): the nowview's, and a dayview
// strip's — where a finger that has waited no longer scrolls the stack either.

import { swipe } from './layout.mjs';

/**
 * The next press arms no wait: a timer set while a `pointerdown` is being
 * handled is not set, and the lift puts `setTimeout` back — a `pointercancel`
 * as well, which is how a touch the browser took for a scroll ends.
 *
 * EVERY timer a press handler arms is swallowed, not the long press's alone.
 * On the nowview's band and on a dayview band a press arms that one
 * (`LONG_MS`) and no other, so that is all it takes there; a second timer a
 * press armed would be swallowed too. Check which a press arms before using
 * this anywhere else.
 */
const noWait = () => {
	const real = window.setTimeout;
	window.setTimeout = (...a) => (window.event?.type === 'pointerdown' ? 0 : real(...a));
	for (const lift of ['pointerup', 'pointercancel'])
		addEventListener(lift, () => (window.setTimeout = real), { capture: true, once: true });
};

/**
 * The long press has fired: the wait's mark says so (`.held`). Waited for
 * and not timed, since a wait on Node's clock orders nothing on the page's,
 * whose timer a loaded runner fires late (da-mtsgy, #1001).
 */
export const longPressed = (page) => page.locator('.hold-mark.held').waitFor({ state: 'attached' });

/**
 * `ms` on the PAGE's clock, for a press that must last at least that long
 * where nothing on the page shows it has. The timer is set after the
 * press's own, so it fires after any the press armed for as long or less,
 * however late the page runs them.
 */
export const pageWait = (page, ms) => page.evaluate((ms) => new Promise((done) => setTimeout(done, ms)), ms);

/**
 * A press, a drag and a lift with the mouse, held past the long press
 * before the first move when `hold`.
 *
 * With no `hold` the press is one that travels BEFORE the long press's wait
 * is up (`LONG_MS`, $lib/nowpoint.ts), and how long the first move takes to
 * arrive is the box's to say: on a loaded runner it came after the 400ms,
 * and a drag asked for as a pan was a range (da-9ol3b). So that press has no
 * wait to beat (`noWait`); one with a `hold` waits for the wait's mark.
 */
export async function drag(page, from, to, { hold = false, shift = false } = {}) {
	await page.mouse.move(from.x, from.y);
	if (shift) await page.keyboard.down('Shift');
	if (!hold) await page.evaluate(noWait);
	await page.mouse.down();
	if (hold) await longPressed(page);
	await page.mouse.move(to.x, to.y, { steps: 8 });
	await page.mouse.up();
	if (shift) await page.keyboard.up('Shift');
}

/**
 * `swipe` (support/layout.mjs) as that press: a finger that travels before
 * the wait is up, however late the runner sends its first move.
 */
export async function swipeNoWait(page, ...swipeArgs) {
	await page.evaluate(noWait);
	await swipe(page, ...swipeArgs);
}
