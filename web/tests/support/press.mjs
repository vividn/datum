// A press that travels, on a band that tells a pan from a range by WHEN it
// first does (`advance`, #lib/nowpoint.ts): the nowview's, and a dayview
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
export const noWait = () => {
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
 * before the first move when `hold`, and held still before the lift: a
 * mouse let go mid-move coasts on (da-w57do), and this drag lands where
 * it is put.
 *
 * With no `hold` the press is one that travels BEFORE the long press's wait
 * is up (`LONG_MS`, #lib/nowpoint.ts), and how long the first move takes to
 * arrive is the box's to say: on a loaded runner it came after the 400ms,
 * and a drag asked for as a pan was a range (da-9ol3b). So that press has no
 * wait to beat (`noWait`), and its moves are STAMPED as made within 80 ms
 * of it, never later than sent: a band reads a move's own `timeStamp` less
 * the press's (da-343zv), which the timer swallowed never was (da-z7nte).
 * One with a `hold` waits for the wait's mark, its moves stamped as sent.
 */
export async function drag(page, from, to, { hold = false, shift = false } = {}) {
	await page.mouse.move(from.x, from.y);
	if (shift) await page.keyboard.down('Shift');
	if (!hold) await page.evaluate(noWait);
	// By CDP, whose `timestamp` (s since the epoch) is the event's `timeStamp`.
	const cdp = await page.context().newCDPSession(page);
	const t0 = Date.now() / 1000;
	const mouse = (type, at, buttons, timestamp) =>
		cdp.send('Input.dispatchMouseEvent', { type, x: at.x, y: at.y, button: 'left', buttons, clickCount: type === 'mouseMoved' ? 0 : 1, modifiers: shift ? 8 : 0, timestamp });
	await mouse('mousePressed', from, 1, hold ? undefined : t0);
	if (hold) await longPressed(page);
	for (let i = 1; i <= 8; i++)
		await mouse('mouseMoved', { x: from.x + ((to.x - from.x) * i) / 8, y: from.y + ((to.y - from.y) * i) / 8 }, 1, hold ? undefined : Math.min(Date.now() / 1000, t0 + i / 100));
	// Not a wait for rest: the drag held still past WINDOW_MS (80 ms, #lib/grip.ts),
	// the time before a lift its speed is read over (`speedOver`), so the lift lets go
	// at none and the band does not coast. Real time, not the page's timers: a test may
	// hold those (`holdTimers`), and its frozen clock carries no speed anyway.
	await page.waitForTimeout(100);
	await mouse('mouseReleased', to, 0);
	await cdp.detach();
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

/**
 * One finger, by CDP: down, then moved in `steps`, then up — held until
 * the view's long press has fired between the down and the first move
 * when `hold`.
 */
export async function finger(page, from, to = from, { hold = false, steps = 8 } = {}) {
	const moves = to.x !== from.x || to.y !== from.y;
	// A drag with no hold has no wait to beat, as `drag`'s has not.
	if (moves && !hold) await page.evaluate(noWait);
	const cdp = await page.context().newCDPSession(page);
	const touch = (type, at) =>
		cdp.send('Input.dispatchTouchEvent', { type, touchPoints: at ? [{ x: at.x, y: at.y }] : [] });
	await touch('touchStart', from);
	if (hold) await longPressed(page);
	for (let i = 1; moves && i <= steps; i++)
		await touch('touchMove', { x: from.x + ((to.x - from.x) * i) / steps, y: from.y + ((to.y - from.y) * i) / steps });
	await touch('touchEnd');
	await cdp.detach();
}


/**
 * A mouse press at `at` whose move to `to` is MADE at once and HANDLED only
 * after `ms`, past the long press (da-343zv): a busy main thread, which ran
 * the long press's timer before the input waiting behind it. An event is
 * stamped when it is made, so the move says when the finger moved. The
 * view's `data-mode` once the wait is up and once the move is handled.
 */
export const lateMove = (page, view, at, to, ms) =>
	page.evaluate(
		async ([view, at, to, ms]) => {
			const mode = () => new Promise((done) => setTimeout(() => done(document.querySelector(view).dataset.mode)));
			const target = document.elementFromPoint(at.x, at.y);
			const init = (p, buttons = 1) => ({
				clientX: p.x,
				clientY: p.y,
				pointerId: 1,
				pointerType: 'mouse',
				isPrimary: true,
				buttons,
				bubbles: true,
				composed: true
			});
			target.dispatchEvent(new PointerEvent('pointerdown', init(at)));
			const move = new PointerEvent('pointermove', init(to));
			await new Promise((done) => setTimeout(done, ms));
			const held = await mode();
			target.dispatchEvent(move);
			const moved = await mode();
			target.dispatchEvent(new PointerEvent('pointerup', init(to, 0)));
			return { held, moved };
		},
		[view, at, to, ms]
	);
