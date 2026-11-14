// The helpers of the three nowview*.test.mjs files, split from nowview.test.mjs
// by topic so each stays well inside test.mjs's 300 s file cap (da-5rpcr): the
// nowview's pane, its handles and their wheel, and a pinch.

import { pane } from './layout.mjs';

export const PANE = 'section.pane[data-title="nowview"]';

/**
 * The handle of the axis `far` is of: a relative time's (`-30m`) is the relative
 * axis's, a clock's (`14:45`) the clock's. Each stands on its axis's rightmost
 * tick (da-273ug), so its words are that tick's, whatever the window.
 */
export const handle = (page, far, pane = PANE) => page.locator(`${pane} .nowview-handles .handle[data-axis="${/^t?[-+]/.test(far) ? 'rel' : 'abs'}"]`);

/** A handle's words: the time of the tick it stands on. */
export const handleWords = (page, far = '-', pane = PANE) => handle(page, far, pane).getAttribute('data-at');

/** A relative time's words (`-1h30m`, `t-45s`) as signed minutes. */
export const minutesIn = (words) => {
	const m = /^t?([-+])(.+)$/.exec(words);
	if (!m) return NaN;
	const parts = [...m[2].matchAll(/(\d+(?:\.\d+)?)([hms])/g)];
	if (parts.map((p) => p[0]).join('') !== m[2]) return NaN;
	return (m[1] === '-' ? -1 : 1) * parts.reduce((t, [, n, u]) => t + Number(n) * { h: 60, m: 1, s: 1 / 60 }[u], 0);
};

/** A notch of the open wheel, by its step. */
export const notch = (page, step) => page.locator(`.nowview-wheel .notch[data-step="${step}"]`);

/** Wait for the last nowview the seam was handed (`spyOnRunner`) to be `--history h`. */
export const asked = (page, h) =>
	page.waitForFunction((h) => {
		const argv = window.__datumSeen.filter((a) => a[0] === 'nowview').at(-1);
		return argv?.[argv.indexOf('--history') + 1] === h;
	}, h);

/**
 * The open wheel's held time and notch `k`'s, as minutes back from the anchor:
 * a step to it scales the window by `step / held` about the anchor.
 */
export const wheelStep = (page, k) =>
	page.locator('.nowview-wheel').evaluate(
		(w, k) => {
			const minutes = (words) => {
				const parts = [...words.replace(/^t?[-+]/, '').matchAll(/(\d+(?:\.\d+)?)([hms])/g)];
				return parts.reduce((t, [, n, u]) => t + Number(n) * { h: 60, m: 1, s: 1 / 60 }[u], 0);
			};
			const step = w.querySelector(`.notch[data-k="${k}"]`)?.dataset.step;
			return { held: minutes(w.querySelector('.notch.held').dataset.at), step: step && minutes(step), word: step };
		},
		k
	);

/** Wait for the last nowview the seam was handed (`spyOnRunner`) to reach back `m` minutes, to the second. */
export const askedMinutes = (page, m) =>
	page.waitForFunction((m) => {
		const argv = window.__datumSeen.filter((a) => a[0] === 'nowview').at(-1);
		const h = argv?.[argv.indexOf('--history') + 1] ?? '';
		const parts = [...h.matchAll(/(\d+(?:\.\d+)?)([hms])/g)];
		return parts.length > 0 && Math.abs(parts.reduce((t, [, n, u]) => t + Number(n) * { h: 60, m: 1, s: 1 / 60 }[u], 0) - m) <= 1 / 60;
	}, m);

/** The relative axis's handles' words, far first. */
export const handled = (page, pane = PANE) =>
	page.locator(`${pane} .nowview-handles .handle[data-axis="rel"]`).evaluateAll((hs) => hs.map((h) => h.dataset.at));

/**
 * Where the handles of the nowview in `pane` sit against its picture: the
 * far handle's words, and what is wrong — each handle must stand on its
 * axis's line, in the window, and its diamond lie over no label drawn. Waits
 * for the handles to be drawn.
 */
export async function beside(page, pane) {
	await page.locator(`${pane} .nowview-handles .handle`).first().waitFor();
	return page.locator(pane).evaluate((p) => {
		const r = (el) => el.getBoundingClientRect();
		const over = (a, b) => a.left < b.right - 0.5 && b.left < a.right - 0.5 && a.top < b.bottom - 0.5 && b.top < a.bottom - 0.5;
		const svg = p.querySelector('svg.svg');
		const frame = r(svg.closest('.picture'));
		const labels = [...svg.querySelectorAll('.tick-label, .clock-label')].filter((l) => getComputedStyle(l).visibility !== 'hidden');
		const wrong = [];
		const handles = [...p.querySelectorAll('.nowview-handles .handle')];
		for (const h of handles) {
			const box = r(h);
			const [middle, across] = [box.left + box.width / 2, box.top + box.height / 2];
			if (middle < frame.left || middle > frame.right) wrong.push(`${h.dataset.at} stands ${middle - frame.left}px along the window`);
			const line = r(svg.querySelector(h.dataset.axis === 'rel' ? 'path.axis' : 'path.clock-axis'));
			const off = across - (h.dataset.axis === 'rel' ? line.top : line.bottom);
			if (Math.abs(off) > 1) wrong.push(`${h.dataset.at} is ${off}px off its axis's line`);
			const diamond = { left: middle - 10, right: middle + 10, top: across - 5, bottom: across + 5 };
			// On a tick its label stands beside it, and the diamond may reach 2px
			// into the label's box across the axis, its descent, which no digit's ink reaches.
			const vertical = h.closest('.vertical');
			const into = (l) => (vertical ? Math.min(diamond.right, r(l).right) - Math.max(diamond.left, r(l).left) : Math.min(diamond.bottom, r(l).bottom) - Math.max(diamond.top, r(l).top));
			for (const l of labels) if (over(diamond, r(l)) && into(l) > 2) wrong.push(`${h.dataset.at} lies over ${l.textContent}`);
		}
		return { far: handles.find((h) => h.dataset.axis === 'rel')?.dataset.at, wrong };
	});
}

/**
 * Click the handle of `at`'s axis (`handle`), and wait for its wheel. By the
 * mouse and not `locator.hover`: the wheel opens OVER the handle, which is the
 * point, and Playwright reads that as the handle not taking the pointer.
 */
export async function hover(page, at, pane = PANE) {
	const box = await handle(page, at, pane).boundingBox();
	await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
	await page.mouse.down();
	await page.mouse.up();
	await page.locator('.nowview-wheel .notch.held').waitFor();
}

/**
 * Wait for the window to reach back `far` from now: the picture redrawn for the
 * window asked, read off its own relative ticks, two of which give the time at
 * the window's far end, and the handles drawn on it.
 */
export const farIs = (page, far, pane = PANE) =>
	page.waitForFunction(
		([pane, want]) => {
			const p = document.querySelector(pane);
			const svg = p?.querySelector('svg.svg');
			if (!svg || !p.querySelector('.nowview-handles .handle')) return false;
			const y = !!svg.querySelector('[data-lo][data-axis="y"]');
			const along = (e) => {
				const b = e.getBoundingClientRect();
				return y ? b.top + b.height / 2 : b.left + b.width / 2;
			};
			const minutes = (words) => {
				const m = /^t?([-+])(.+)$/.exec(words);
				const parts = m ? [...m[2].matchAll(/(\d+(?:\.\d+)?)([hms])/g)] : [];
				if (!m || parts.map((q) => q[0]).join('') !== m[2]) return NaN;
				return (m[1] === '-' ? -1 : 1) * parts.reduce((t, [, n, u]) => t + Number(n) * { h: 60, m: 1, s: 1 / 60 }[u], 0);
			};
			const ticks = [...svg.querySelectorAll(svg.classList.contains('axis-live') ? '.live-axis .tick-label' : '.tick-label')]
				.filter((l) => getComputedStyle(l).visibility !== 'hidden' && l.previousElementSibling)
				.map((l) => ({ m: minutes(l.textContent), p: along(l.previousElementSibling) }))
				.filter((t) => Number.isFinite(t.m));
			if (ticks.length < 2) return false;
			const [a, b] = [ticks[0], ticks.at(-1)];
			const frame = svg.closest('.picture').getBoundingClientRect();
			const at = a.m + (((y ? frame.top : frame.left) - a.p) * (b.m - a.m)) / (b.p - a.p);
			return Math.abs(at - want) <= Math.max(Math.abs(want) * 0.02, 1 / 60);
		},
		[pane, minutesIn(far)]
	);

/**
 * Wait until each handle of the nowview in `pane` stands on a labelled tick of
 * its axis shown, within a pixel, and says its time (da-273ug): a relative
 * label's words, or a clock label's, which may be its seconds alone (`:30`).
 */
export const onTicks = (page, pane = PANE) =>
	page.waitForFunction((pane) => {
		const p = document.querySelector(pane);
		const hs = [...(p?.querySelectorAll('.nowview-handles .handle') ?? [])];
		return (
			hs.length > 0 &&
			hs.every((h) => {
				const y = !!h.closest('.vertical');
				const along = (e) => {
					const b = e.getBoundingClientRect();
					return y ? b.top + b.height / 2 : b.left + b.width / 2;
				};
				return [...p.querySelectorAll(`svg.svg ${h.dataset.axis === 'rel' ? '.tick-label' : '.clock-label'}`)].some(
					(l) =>
						getComputedStyle(l).visibility !== 'hidden' &&
						l.previousElementSibling &&
						Math.abs(along(l.previousElementSibling) - along(h)) <= 1 &&
						(h.dataset.at === l.textContent || h.dataset.at.endsWith(l.textContent))
				);
			})
		);
	}, pane);

/** A layout of one nowview pane, as the `main` tab's draft. */
export const soloNowview = (settings) =>
	JSON.stringify({
		version: 1,
		name: 'main',
		root: {
			id: 'only',
			tabs: [{ id: 'only-tab', content: { id: 'only-widget', kind: 'nowview', settings } }],
			active: 0,
			bar: { edge: 'top', align: 'start' }
		}
	});

/**
 * Two fingers over `box`, `from` px apart, drawn to `to` px apart.
 *
 * Through CDP, as every other touch in this suite: Playwright's
 * touchscreen only taps, and a pinch made of mouse events would be a test
 * of the wrong handler. `midway` is called with the fingers still down —
 * the gesture's preview is only on screen until they lift.
 */
export async function pinch(page, box, from, to, midway) {
	const y = box.y + box.height / 2;
	const x = box.x + box.width / 2;
	const cdp = await page.context().newCDPSession(page);
	const both = (apart) => [
		{ id: 1, x: x - apart / 2, y },
		{ id: 2, x: x + apart / 2, y }
	];
	const at = (type, touchPoints) => cdp.send('Input.dispatchTouchEvent', { type, touchPoints });
	try {
		// One finger, then the second: the fingers land as fingers do, and
		// the widget's count of them is what starts the pinch.
		await at('touchStart', [both(from)[0]]);
		await at('touchStart', both(from));
		for (let i = 1; i <= 6; i++) await at('touchMove', both(from + ((to - from) * i) / 6));
		await midway?.();
		await at('touchEnd', []);
	} finally {
		await cdp.detach();
	}
}

export const TOUCH = { device: { hasTouch: true, isMobile: true }, viewport: { width: 390, height: 640 } };
