// The wasi runner — the real `datum` CLI behind the seam (da-1vat, route C).
//
// Behind a flag while the spike measures it: `?wasi` holds the database
// files in the owner worker, `?wasi=command` takes them per command (see
// $lib/cli.ts). Without the flag the TypeScript stand-in stays the default.
//
// The CLI keeps its own database, so under the flag the regions that read
// the stand-in's tables directly — the widget bar, the tail pane — do not
// see what it logs. The REPL, the dayview strip and nowview go through
// `run(argv)` and do.

import { announceChange } from '../changes.js';
import { runCli } from '../db.js';
import { deviceId } from '../deviceId.js';
import type { RunResult } from '../run.js';

/**
 * `datum <argv>` under WASI. What the native CLI reads from its host, the
 * page supplies: the zone (WASI has no zone database of the host's), this
 * browser's device id, and colour, since there is no tty to detect.
 */
export async function wasiRun(argv: string[]): Promise<RunResult> {
	const env = [
		`TZ=${Intl.DateTimeFormat().resolvedOptions().timeZone}`,
		`DATUM_DEVICE_ID=${deviceId()}`,
		'DATUM_COLOR=1'
	];
	const lock = new URLSearchParams(location.search).get('wasi') === 'command' ? 'command' : 'owner';
	const result = await runCli(argv, env, lock);
	if (result.changed) announceChange();
	return result;
}
