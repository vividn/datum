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

const BAR = '.prompt-bar';

/** Drag the bar by `dy` px (negative opens), as a finger without a flick. */
export function dragBar(page, dy, steps = 10) {
	return page.evaluate(
		([dy, steps]) => {
			const el = document.querySelector('.prompt-bar');
			const r = el.getBoundingClientRect();
			const x = r.left + r.width / 2;
			const y = r.top + r.height / 2;
			const send = (type, at) =>
				el.dispatchEvent(
					new PointerEvent(type, {
						pointerId: 3,
						pointerType: 'touch',
						isPrimary: true,
						clientX: x,
						clientY: at,
						bubbles: true,
						cancelable: true
					})
				);
			send('pointerdown', y);
			for (let i = 1; i <= steps; i += 1) send('pointermove', y + (dy * i) / steps);
			send('pointerup', y + dy);
		},
		[dy, steps]
	);
}

/** Wait until the bar has come to rest: two reads of its height the same. */
export const barSettled = (page) =>
	page.evaluate(
		() =>
			new Promise((done) => {
				let last = -1;
				let same = 0;
				const tick = () => {
					const now = Math.round(document.querySelector('.prompt-bar').getBoundingClientRect().height);
					same = now === last ? same + 1 : 0;
					last = now;
					if (same > 4) return done();
					requestAnimationFrame(tick);
				};
				tick();
			})
	);

/** The output section showing, whatever this screen's default is. */
export async function unfold(page) {
	if (await page.locator(`${BAR} .section.output`).count()) return;
	await dragBar(page, -40);
	await page.locator(`${BAR} .section.output`).waitFor();
	await barSettled(page);
}

/** And away again. */
export async function fold(page) {
	if (!(await page.locator(`${BAR} .section.output`).count())) return;
	await dragBar(page, 40);
	await page.locator(`${BAR} .section.output`).waitFor({ state: 'detached' });
	await barSettled(page);
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
