// The helpers of the three nowview*.test.mjs files, split from nowview.test.mjs
// by topic so each stays well inside test.mjs's 300 s file cap (da-5rpcr): the
// nowview's pane, its handles and their wheel, and a pinch.

import { pane } from './layout.mjs';

export const PANE = 'section.pane[data-title="nowview"]';

/** The handle of the window reaching back `far`. */
/**
 * The words of the handle of a window on now reaching back `far`: a relative
 * handle stands halfway to now and names the time there. A clock's stand as they are.
 */
export function diamond(far) {
	const m = /^(t?[-+])(\d+)([smh])$/.exec(far);
	if (!m) return far;
	const half = (Number(m[2]) * { s: 1, m: 60, h: 3600 }[m[3]]) / 2;
	const s = half < 600 ? half : Math.round(half / 60) * 60;
	return m[1] + (s % 3600 === 0 ? `${s / 3600}h` : s < 60 ? `${s}s` : s % 60 ? `${Math.floor(s / 60)}m${s % 60}s` : `${s / 60}m`);
}

export const handle = (page, far, pane = PANE) => page.locator(`${pane} .nowview-handles .handle[data-at="${diamond(far)}"]`);

/** A notch of the open wheel, by its step. */
export const notch = (page, step) => page.locator(`.nowview-wheel .notch[data-step="${step}"]`);

/** Wait for the last nowview the seam was handed (`spyOnRunner`) to be `--history h`. */
export const asked = (page, h) =>
	page.waitForFunction((h) => {
		const argv = window.__datumSeen.filter((a) => a[0] === 'nowview').at(-1);
		return argv?.[argv.indexOf('--history') + 1] === h;
	}, h);

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
			for (const l of labels) if (over(diamond, r(l))) wrong.push(`${h.dataset.at} lies over ${l.textContent}`);
		}
		return { far: handles.find((h) => h.dataset.axis === 'rel')?.dataset.at, wrong };
	});
}

/**
 * Click the handle reading `at`, and wait for its wheel. By the mouse and
 * not `locator.hover`: the wheel opens OVER the handle, which is the point,
 * and Playwright reads that as the handle not taking the pointer.
 */
export async function hover(page, at, pane = PANE) {
	const box = await handle(page, at, pane).boundingBox();
	await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
	await page.mouse.down();
	await page.mouse.up();
	await page.locator(`.nowview-wheel .notch.held[data-at="${diamond(at)}"]`).waitFor();
}

/** Wait for the far handle to read `at`: the picture redrawn for the window asked. */
export const farIs = (page, at, pane = PANE) =>
	page.waitForFunction(
		([pane, at]) => document.querySelector(`${pane} .nowview-handles .handle`)?.dataset.at === at,
		[pane, diamond(at)]
	);

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
