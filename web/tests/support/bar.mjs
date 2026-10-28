// Moving the bottom bar the way a reader does (da-ckw3).
//
// There is no `^` button any more. vividn, 2026-09-18: "The ^ button obscures
// the text. Let's just get rid of this button and instead just put a little
// carrat above the input bar to suggest that it can be pulled up." So the
// chevron on the bar's top rule is a HINT — `pointer-events: none`, out of
// the Tab order — and the GESTURE is the whole of how the bar moves.
//
// Which means a test that wants the output open has to open it the way a
// finger does, and this is that gesture. It dispatches synthetic
// `PointerEvent`s, which carry no time between them, so `settle` reads no
// velocity and applies its let-go rules — which is what these callers want:
// a drag a little way up opens to the notch, a drag a little way down shuts.
// A test about the FLING needs a real touch instead (`swipe` in ./layout.mjs
// goes through CDP, which does take real time).
//
// ONE STAMP FOR THE WHOLE GESTURE (da-31464). An event is stamped when it
// is constructed, so "no time between them" held only while the loop ran
// inside `MIN_MS` (#lib/grip.ts, 8ms). On a starved thread the dozen
// dispatches — each a `moveTo` and a store write — took longer than that,
// the release read 40px over ~10ms as a 4 px/ms throw, and a drag meant to
// open the output flung the bar to the ceiling: every pane cut (da-1x6ro),
// a fold 40px down that left it open (da-jegw1), a log left at 120px
// (da-7nkdb). So every event carries its pointerdown's stamp, and the
// speed is zero by construction rather than by the box being quick.

const BAR = '.prompt-bar';

/** Drag `selector` by `dy` px (negative opens), as a finger without a flick. */
export function dragOn(page, selector, dy, steps = 10) {
	return page.evaluate(
		([selector, dy, steps]) => {
			const el = document.querySelector(selector);
			const r = el.getBoundingClientRect();
			const x = r.left + r.width / 2;
			const y = r.top + r.height / 2;
			const stamp = performance.now();
			const send = (type, at) => {
				const event = new PointerEvent(type, {
					pointerId: 3,
					pointerType: 'touch',
					isPrimary: true,
					clientX: x,
					clientY: at,
					bubbles: true,
					cancelable: true
				});
				Object.defineProperty(event, 'timeStamp', { value: stamp });
				el.dispatchEvent(event);
			};
			send('pointerdown', y);
			for (let i = 1; i <= steps; i += 1) send('pointermove', y + (dy * i) / steps);
			send('pointerup', y + dy);
		},
		[selector, dy, steps]
	);
}

/** The bar itself, dragged so. */
export const dragBar = (page, dy, steps) => dragOn(page, BAR, dy, steps);

/**
 * Wait until the bar RESTS where a drag with no speed in it puts it: at the
 * notch — the output whole and no log — or shut.
 *
 * A place and not a pause (da-31464). This used to wait for five frames of
 * the bar's own height holding still, which a bar gliding up into the LOG
 * also does — the log grows under the bar, not in it — so a fling passed
 * the wait, and the panes a test measured next were cut or not by how fast
 * its own round trips were (da-1x6ro). A bar that went anywhere else now
 * times out here, at the drag that sent it.
 */
const rests = async (page, open) => {
	await page.waitForFunction(
		(open) => {
			const style = getComputedStyle(document.querySelector('.shell'));
			const px = (name) => parseFloat(style.getPropertyValue(name)) || 0;
			const out = document.querySelector('.prompt-bar .section.output');
			const want = open ? out?.getBoundingClientRect().height : 0;
			const there = want !== undefined && px('--log-height') < 0.5 && Math.abs(px('--shown') - want) < 1;
			// For five frames running: a glide on its way up into the log
			// passes the notch for a frame, and one read would call it there.
			window.__rests = there ? (window.__rests ?? 0) + 1 : 0;
			return window.__rests > 4;
		},
		open,
		{ polling: 'raf' }
	);
	await page.evaluate(() => delete window.__rests);
};

/** The output section showing, whatever this screen's default is. */
export async function unfold(page) {
	if (await page.locator(`${BAR} .section.output`).count()) return;
	await dragBar(page, -40);
	await rests(page, true);
}

/** And away again. */
export async function fold(page) {
	if (!(await page.locator(`${BAR} .section.output`).count())) return;
	await dragBar(page, 40);
	await page.locator(`${BAR} .section.output`).waitFor({ state: 'detached' });
	await rests(page, false);
}

/**
 * Drag the bar by `dy` px WITH TIME IN IT, so the release carries speed.
 *
 * `dragBar` above dispatches its whole run of moves synchronously, so
 * every one carries the same stamp and `settle` reads no velocity — which
 * is exactly what its callers want. A test about MOMENTUM needs the
 * opposite, and `swipe` (./layout.mjs, through CDP) is the other way to
 * get it. This is the cheap one: the moves are a frame apart, so the
 * gesture measures about `dy / (steps * gap)` px/ms and the bar glides on
 * after the finger.
 *
 * It settles at `now + speed * TAU`, so the GLIDE's own length is
 * `3 * distance / speed` = `3 * TAU` whatever the speed turned out to be
 * — which is how a test can ask about `TAU` without pinning a velocity
 * this machine may not produce (da-o2wm8).
 */
export function timedDrag(page, dy, steps = 6, gap = 16) {
	return page.evaluate(
		([dy, steps, gap]) =>
			new Promise((done) => {
				const el = document.querySelector('.prompt-bar');
				const r = el.getBoundingClientRect();
				const x = r.left + r.width / 2;
				const y = r.top + r.height / 2;
				const send = (type, at) =>
					el.dispatchEvent(
						new PointerEvent(type, {
							pointerId: 7,
							pointerType: 'touch',
							isPrimary: true,
							clientX: x,
							clientY: at,
							bubbles: true,
							cancelable: true
						})
					);
				send('pointerdown', y);
				let i = 0;
				const step = () => {
					i += 1;
					send('pointermove', y + (dy * i) / steps);
					if (i < steps) return void setTimeout(step, gap);
					setTimeout(() => {
						send('pointerup', y + dy);
						done();
					}, gap);
				};
				setTimeout(step, gap);
			}),
		[dy, steps, gap]
	);
}
