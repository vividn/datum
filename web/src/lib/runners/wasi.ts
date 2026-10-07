// The runner behind the seam: the real `datum` CLI (da-1vat, da-g8o8).
//
// One WASI process per command in the database worker ($lib/cli.ts). What
// the native CLI reads from its host, the page supplies: the zone (WASI has
// no zone database of the host's), this browser's client id, colour,
// since there is no tty to detect, and that an empty stdin answers no
// question (da-f5pg2, `asking` in $lib/dbio.ts).

import { runCli } from '../db.js';
import { clientId } from '../clientId.js';
import type { Stage } from '../dbProtocol.js';
import type { RunResult } from '../run.js';

/** `datum <argv>`, with `stdin` on its standard input; `onStderr` hears stderr as it is written. */
export async function wasiRun(
	argv: string[],
	stdin?: string,
	onStderr?: (text: string) => void,
	key?: string,
	stage?: Stage,
	extra: string[] = [],
	pane?: boolean,
	session?: boolean
): Promise<RunResult> {
	const env = [
		`TZ=${Intl.DateTimeFormat().resolvedOptions().timeZone}`,
		`DATUM_CLIENT_ID=${clientId()}`,
		'DATUM_COLOR=1',
		'DATUM_NO_DEFAULT_ANSWER=1',
		...extra
	];
	// The whole process record — its timings and storage ride along for
	// scripts/measure-wasi.mjs, unread by the app.
	const cli = await runCli(argv, env, stdin, onStderr, key, stage, session, pane);
	return cli ?? { argv, code: 0, stdout: '', stderr: '', superseded: true };
}
