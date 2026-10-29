// What a finger's drag across the day strip costs the page, driven by CDP
// touch events as a phone's would arrive (da-uovtn): the probe's
// (`measure-pans.mjs`) and the test's (`panfree.test.mjs`).
//
// Two readings of one drag. The FRAMES: each animation frame's place of
// the finger and of the band under it, and when the stack first scrolled —
// read in the page, by the layout boxes, so they measure what is drawn.
// And the TRACE: what the renderer's main thread did meanwhile — paints,
// style recalcs and the elements each restyled, layouts, long tasks — read
// with no box asked for, so the reading adds no layout of its own.

import assert from 'node:assert/strict';

/** A point every two minutes in each of three fields, from 01:00 today: thousands of marks on today's band. */
export async function seedMarks(app) {
	const backup = await app.run(['db', 'backup', '-']);
	assert.equal(backup.code, 0, backup.stderr);
	const header = JSON.parse(backup.stdout.split('\n')[0]);
	const end = Date.parse(new Date(await app.page.evaluate(() => Date.now())).toISOString().slice(0, 10) + 'T14:55:00Z');
	const count = 3 * 30 * 14;
	const lines = [JSON.stringify({ ...header, backup_time: new Date(end).toISOString(), source_db: 'seed', entry_count: count })];
	for (let i = 0; i < count; i++) {
		const t = new Date(end - Math.floor(i / 3) * 120_000).toISOString();
		const field = ['tick', 'tock', 'tack'][i % 3];
		lines.push(
			JSON.stringify({
				hid: 'pans' + i.toString().padStart(8, '0'),
				slug: `${field}:${t}`,
				content_hash: i.toString(16).padStart(64, '0'),
				field,
				verb: 'point',
				state: null,
				dur: 'MOMENT',
				following_state: null,
				data: null,
				occur_time: { utc: t, offset_minutes: 0, timezone: 'UTC' },
				create_time: t,
				modify_time: t,
				client_id: '01M281T64122KN04Z1VN626W9J',
				id_structure: '%occur_time%',
				raw_input: `occur ${field}`,
				parser_version: 2
			})
		);
	}
	const result = await app.run(['db', 'restore', '-', '--merge'], lines.join('\n') + '\n');
	assert.equal(result.code, 0, result.stderr);
	await app.reload();
	// The marks, not the band: a reload draws today from its picture kept
	// before the seed until today's run lands (da-8xy6u).
	await app.page.locator('.band[data-days-ago="0"] [data-field="tick"]').first().waitFor({ state: 'attached' });
	if (await app.page.locator('.nowview-band').count())
		await app.page.locator('.nowview-band [data-field="tick"]').first().waitFor({ state: 'attached' });
	await app.idle();
}

/** A frame's worth of wall time, in ms, which is what a move is paced at. */
export const FRAME_MS = 16;

/** The renderer events a pan is judged by (`devtools.timeline`). */
const CATEGORIES = ['devtools.timeline', 'disabled-by-default-devtools.timeline', 'toplevel'];

/**
 * Drive one finger from `from` along `moves` steps of `by` px each, a
 * frame apart, and lift it unless `lift` is false. Through CDP, which runs the touch through the
 * browser's own input path — a non-passive listener, `touch-action`, the
 * compositor's scroll — where a synthetic `TouchEvent` would skip it.
 */
export async function touchDrag(cdp, from, by, moves, { lift = true } = {}) {
	const at = (i) => [{ x: from.x + by.x * i, y: from.y + by.y * i }];
	await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: at(0) });
	for (let i = 1; i <= moves; i++) {
		const sent = Date.now();
		await cdp.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: at(i) });
		await new Promise((done) => setTimeout(done, Math.max(0, FRAME_MS - (Date.now() - sent))));
	}
	if (lift) await liftFinger(cdp);
}

/** Lift the finger `touchDrag` left down. */
export const liftFinger = (cdp) => cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });

/**
 * Watch the page's frames while `act` runs: every `touchmove` and `scroll`
 * the page hears, with its time, and once a frame the finger's last place
 * and the left edge of the band picture under `band` (a selector); and
 * what `act` returned, as `result`.
 */
export async function framesOf(page, band, act) {
	await page.evaluate((band) => {
		const seen = (window.__pans = { start: null, moves: [], scrolls: [], frames: [] });
		const stack = document.querySelector('.stack');
		let finger = null;
		const onMove = (e) => {
			finger = e.touches[0].clientX;
			seen.moves.push({ t: e.timeStamp, x: finger, y: e.touches[0].clientY });
		};
		const onScroll = () => seen.scrolls.push({ t: performance.now(), top: stack.scrollTop });
		const onStart = (e) => (seen.start = { t: e.timeStamp, x: (finger = e.touches[0].clientX) });
		addEventListener('touchstart', onStart, { capture: true, passive: true });
		addEventListener('touchmove', onMove, { capture: true, passive: true });
		// The finger's place as the page acts on it: a pointer event comes
		// ahead of its touch event, and the pan moves on the pointer's.
		const onPointer = (e) => (finger = e.clientX);
		addEventListener('pointermove', onPointer, { capture: true, passive: true });
		stack?.addEventListener('scroll', onScroll, { passive: true });
		const frame = (t) => {
			if (!window.__pans) return;
			const svg = document.querySelector(`${band} svg.svg`);
			seen.frames.push({ t, finger, left: svg?.getBoundingClientRect().left ?? null, top: stack?.scrollTop ?? null });
			requestAnimationFrame(frame);
		};
		requestAnimationFrame(frame);
		seen.stop = () => {
			removeEventListener('touchstart', onStart, { capture: true });
			removeEventListener('touchmove', onMove, { capture: true });
			removeEventListener('pointermove', onPointer, { capture: true });
			stack?.removeEventListener('scroll', onScroll);
		};
	}, band);
	const result = await act();
	await new Promise((done) => setTimeout(done, 200));
	const seen = await page.evaluate(() => {
		const seen = window.__pans;
		seen.stop();
		window.__pans = null;
		return { start: seen.start, moves: seen.moves, scrolls: seen.scrolls, frames: seen.frames };
	});
	return { ...seen, result };
}

/**
 * The main thread's work while `act` runs, from a trace of the renderer
 * that runs `page` (its `CrRendererMain`) and the page's own counters.
 */
export async function traceOf(page, cdp, act) {
	const counters = async () => {
		const { metrics } = await cdp.send('Performance.getMetrics');
		return Object.fromEntries(metrics.map((m) => [m.name, m.value]));
	};
	await cdp.send('Performance.enable');
	const events = [];
	const collect = ({ value }) => events.push(...value);
	cdp.on('Tracing.dataCollected', collect);
	await cdp.send('Tracing.start', { categories: CATEGORIES.join(','), transferMode: 'ReportEvents' });
	const was = await counters();
	await act();
	const is = await counters();
	const done = new Promise((resolve) => cdp.once('Tracing.tracingComplete', resolve));
	await cdp.send('Tracing.end');
	await done;
	cdp.off('Tracing.dataCollected', collect);

	// The page's own renderer: the main thread whose frame is this page's.
	const main = events.find(
		(e) =>
			e.name === 'thread_name' &&
			e.args?.name === 'CrRendererMain' &&
			events.some((f) => f.pid === e.pid && f.tid === e.tid && f.name === 'UpdateLayoutTree')
	);
	const on = events.filter((e) => main && e.pid === main.pid && e.tid === main.tid);
	const named = (name) => on.filter((e) => e.name === name);
	// A complete event carries its duration; a begin/end pair is matched in order.
	const spans = (name) => {
		const out = [];
		const open = [];
		for (const e of named(name)) {
			if (e.ph === 'X') out.push({ ...e, dur: e.dur ?? 0 });
			else if (e.ph === 'B') open.push(e);
			else if (e.ph === 'E' && open.length) {
				const b = open.pop();
				out.push({ ...b, dur: e.ts - b.ts, args: { ...b.args, ...e.args } });
			}
		}
		return out;
	};
	// Where each paint and layout was, by the pane and the band it is in:
	// a node name alone does not tell one picture from another.
	const where = new Map();
	const ids = [
		...new Set([
			...named('Paint').map((e) => e.args?.data?.nodeId),
			...spans('Layout').map((e) => e.args?.endData?.layoutRoots?.[0]?.nodeId)
		])
	].filter(Boolean);
	if (ids.length) {
		await cdp.send('DOM.getDocument', { depth: 0 });
		const { nodeIds } = await cdp.send('DOM.pushNodesByBackendIdsToFrontend', { backendNodeIds: ids });
		for (const [i, nodeId] of nodeIds.entries()) {
			const place = await cdp
				.send('DOM.resolveNode', { nodeId })
				.then(({ object }) =>
					cdp.send('Runtime.callFunctionOn', {
						objectId: object.objectId,
						returnByValue: true,
						functionDeclaration: `function () {
							const band = this.closest?.('.band')?.dataset.daysAgo;
							const pane = this.closest?.('section.pane')?.dataset.title;
							return (pane ? pane + ' ' : '') + (band !== undefined ? 'band ' + band + ' ' : '');
						}`
					})
				)
				.then((r) => r.result.value, () => '');
			where.set(ids[i], place);
		}
	}
	const placed = (id, name) => (where.get(id) ?? '') + name;
	const restyles = spans('UpdateLayoutTree').map((e) => e.args?.elementCount ?? e.args?.beginData?.elementCount ?? 0);
	const tasks = spans('RunTask').map((e) => e.dur / 1000);
	// Chrome's compositing update, every frame anything on the page changed:
	// linear in the paint chunks, so in the elements with a transform of their own (da-syaz8).
	const layerize = spans('Layerize').map((e) => e.dur / 1000);
	return {
		layerize: { runs: layerize.length, ms: layerize.reduce((a, b) => a + b, 0), most: Math.max(0, ...layerize) },
		paints: named('Paint').length,
		/** What each paint painted: the element whose layer it was. */
		painted: named('Paint').map((e) => placed(e.args?.data?.nodeId, e.args?.data?.nodeName ?? '')),
		/** What each layout began at. */
		laidOut: spans('Layout').map((e) => {
			const root = e.args?.endData?.layoutRoots?.[0];
			return placed(root?.nodeId, root?.nodeName ?? '');
		}),
		restyled: { most: Math.max(0, ...restyles), total: restyles.reduce((a, b) => a + b, 0), runs: restyles.length },
		layouts: is.LayoutCount - was.LayoutCount,
		recalcs: is.RecalcStyleCount - was.RecalcStyleCount,
		taskMs: (is.TaskDuration - was.TaskDuration) * 1000,
		/**
		 * The main thread's tasks in its own CPU time (`tdur`): what
		 * `taskMs` measures less the time the thread waited to be run,
		 * which on a loaded box is most of what it adds.
		 */
		threadMs: spans('RunTask').reduce((sum, e) => sum + (e.tdur ?? e.dur), 0) / 1000,
		scriptMs: (is.ScriptDuration - was.ScriptDuration) * 1000,
		longTasks: tasks.filter((d) => d > 50).map(Math.round),
		mostTaskMs: Math.round(Math.max(0, ...tasks))
	};
}

/**
 * Of a drag's frames: when the stack first moved after the first touchmove
 * (vertical), when the band first moved (horizontal), and how far each
 * frame's band strayed from the finger since the band first moved.
 */
export function readFrames({ start, moves, scrolls, frames }) {
	const first = moves[0]?.t ?? NaN;
	const scrolled = scrolls[0]?.t ?? NaN;
	const left0 = frames.find((f) => f.t <= first && f.left !== null)?.left ?? frames.find((f) => f.left !== null)?.left ?? null;
	const moved = frames.find((f) => f.t >= first && f.left !== null && Math.abs(f.left - left0) > 0.5);
	const x0 = start?.x;
	// The band's travel against the finger's, every frame from the band's
	// first move to the finger's last: a jump is a frame where they part.
	const last = moves.at(-1)?.t ?? 0;
	const strays = frames
		.filter((f) => moved && f.t >= moved.t && f.t <= last && f.finger !== null)
		.map((f) => Math.abs(f.left - left0 - (f.finger - x0)));
	return {
		toScrollMs: scrolled - first,
		movesBeforeScroll: moves.filter((m) => m.t < scrolled).length,
		toBandMs: moved ? moved.t - first : NaN,
		strays,
		mostStray: Math.max(0, ...strays)
	};
}
