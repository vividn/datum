// Not a test: the day strip's two timings, for da-iu9i's PR body.
//
//   node tests/support/measure-strip.mjs
//
// 1. A RESIZE: from the moment the strip's box changes to the first band
//    painted at the new size, and to the last of them. That is the number
//    one-picture-per-day exists for.
// 2. A COMMAND: how long the strip's refresh takes, and how many CLI
//    processes it costs — the other side of the same trade.

import { startApp, stopApp, withApp } from './app.mjs';

const VIEWPORT = { width: 1280, height: 1000 };

await startApp();
try {
	await withApp(
		async (app) => {
			const { page } = app;
			await page.locator('.band[data-days-ago="0"] svg.svg').waitFor();
			await app.idle();

			// ── a resize ────────────────────────────────────────────────
			const seen = await app.spyOnRunner();
			const resize = await page.evaluate(async () => {
				const strip = document.querySelector('.strip');
				const before = new Map(
					[...strip.querySelectorAll('.band')].map((b) => [
						b.dataset.daysAgo,
						b.querySelector('svg.svg')?.getAttribute('width')
					])
				);
				const started = performance.now();
				// A narrower box: every band's argv changes, so every one is
				// re-asked. The pane's cell is the layout tree's flex item.
				document.querySelector('.cell.dayview').style.width = '64%';
				const fresh = () =>
					[...strip.querySelectorAll('.band')].filter(
						(b) => b.querySelector('svg.svg')?.getAttribute('width') !== before.get(b.dataset.daysAgo)
					).length;
				const total = before.size;
				let first = null;
				for (;;) {
					await new Promise(requestAnimationFrame);
					const n = fresh();
					if (n > 0 && first === null) first = performance.now() - started;
					if (n === total) return { first, last: performance.now() - started, bands: total };
					if (performance.now() - started > 30000) return { first, last: null, bands: total };
				}
			});
			await app.idle();
			const drawn = (await seen()).filter((a) => a[0] === 'dayview' && !a.includes('--data-layer'));
			console.log(
				`resize: first band ${resize.first?.toFixed(0)}ms, all ${resize.bands} bands ${resize.last?.toFixed(0)}ms, ${drawn.length} dayview runs`
			);

			// ── a command ───────────────────────────────────────────────
			const after = await app.spyOnRunner();
			const began = Date.now();
			await app.submit('occur coffee');
			await app.idle();
			const cost = Date.now() - began;
			const runs = await after();
			console.log(
				`a command: ${cost}ms to a settled page, ${runs.filter((a) => a[0] === 'dayview' && !a.includes('--data-layer')).length} dayview runs of ${runs.length} runs in all`
			);
		},
		{ viewport: VIEWPORT }
	);
} finally {
	await stopApp();
}
