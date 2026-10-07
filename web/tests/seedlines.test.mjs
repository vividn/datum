// `seedLines` (support/app.mjs) refuses to seed in silence (da-pm48t).
//
// A seed line the CLI refused used to vanish: the quiet door's RunResult
// was dropped and the bar's echo looks the same either way, so the missing
// row surfaced tests later as a pane that "failed to draw" (da-edr7t). A
// duplicate `occur` at one `-t` is a refusal that says nothing on stderr —
// it exits 11 — which is why the message is built from the code. Both
// doors are held to it: the quiet one for every line but the last, and
// the bar, through its receipt, for the last.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, seedLines } from './support/app.mjs';
import { DESKTOP, ONE_PANE } from './support/layout.mjs';

before(startApp);
after(stopApp);

const shared = sharedApp(DESKTOP);
// `main` as a draft of ONE REPL pane: nothing here reads the day frame,
// and drawing it costs what a boot did (da-m1m9a).
const onSeam = (fn) => shared.solo(fn, ONE_PANE);

const TWICE = 'occur coffee -t 0900';

describe('seedLines — a refused line throws', () => {
	test('through the quiet door: names the line and its code', async () => {
		await onSeam(async (app) => {
			await assert.rejects(seedLines(app, [TWICE, TWICE, 'occur tea']), {
				message: `seed line \`${TWICE}\` exited 11`
			});
		});
	});

	test('through the bar: the last line is checked too', async () => {
		await onSeam(async (app) => {
			await assert.rejects(seedLines(app, [TWICE, TWICE]), {
				message: `seed line \`${TWICE}\` exited 11`
			});
		});
	});

	test('a seed that lands returns quietly', async () => {
		await onSeam(async (app) => {
			await seedLines(app, ['occur coffee -t 0900', 'occur tea -t 0905']);
			assert.equal((await app.entries()).length, 2);
		});
	});
});
