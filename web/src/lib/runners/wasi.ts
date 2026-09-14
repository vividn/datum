// The runner behind the seam: the real `datum` CLI (da-1vat, da-g8o8).
//
// One WASI process per command in the database worker ($lib/cli.ts). What
// the native CLI reads from its host, the page supplies: the zone (WASI has
// no zone database of the host's), this browser's device id, and colour,
// since there is no tty to detect.

import { runCli } from '../db.js';
import { deviceId } from '../deviceId.js';
import type { RunResult } from '../run.js';

/** `datum <argv>`, with `stdin` on its standard input; `onStderr` hears stderr as it is written. */
export async function wasiRun(
	argv: string[],
	stdin?: string,
	onStderr?: (text: string) => void
): Promise<RunResult> {
	const env = [
		`TZ=${Intl.DateTimeFormat().resolvedOptions().timeZone}`,
		`DATUM_DEVICE_ID=${deviceId()}`,
		'DATUM_COLOR=1'
	];
	// The whole process record — its timings and storage ride along for
	// scripts/measure-wasi.mjs, unread by the app.
	return runCli(argv, env, stdin, onStderr);
}
