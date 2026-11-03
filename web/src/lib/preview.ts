// What collect would log, before it is (da-axrdv).
//
// vividn, 2026-11-03: «the nowview could show a preview of what would
// actually be logged (somehow with dotted outline or something to show it
// is not yet logged …) Getting this feedback loop tight and fast is
// important.» The entry is the CLI's: the line collect would run, run with
// `--dry-run` under `DATUM_SHOW=json`, whose DRYRUN document says the time
// and length the entry would take — a field whose spec changes the entry
// included. Every picture draws it (`drawPreview` in #lib/hover.ts) in the
// field's own lane, white and dotted: yellow dotted is now's alone.

import { writable } from 'svelte/store';
import { dryRunArgv } from './calls.js';
import { parseEcho, type EchoDoc } from './echo.js';
import { run, runKey } from './run.js';

/** The entry a dry run says a line would write. */
export interface Preview {
	field: string;
	/** Its time, in ms. */
	at: number;
	/** The length that ends at it, in seconds; none for a moment or a switch from `at` on. */
	dur: number | null;
	/** A moment: a point, not a stretch. */
	point: boolean;
}

export const preview = writable<Preview | null>(null);

const key = runKey();
let asked = 0;

/** Set `preview` to what `argv` would write, or to none for `null` or a line the CLI refuses. The newest ask wins. */
export async function previewOf(argv: string[] | null): Promise<void> {
	const mine = ++asked;
	if (!argv) return preview.set(null);
	const { code, stdout } = await run(dryRunArgv(argv), undefined, undefined, key, undefined, ['DATUM_SHOW=json']);
	if (mine !== asked) return;
	const doc = code === 0 ? (parseEcho(stdout)?.[0] as (EchoDoc & { at?: string | null; dur_seconds?: number | null }) | undefined) : undefined;
	const at = Date.parse(doc?.at ?? '');
	preview.set(doc?.field && Number.isFinite(at) ? { field: doc.field, at, dur: doc.dur_seconds ?? null, point: doc.shape === 'MOMENT' } : null);
}
