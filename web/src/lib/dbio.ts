// The REPL's file commands (da-podv): the only typed commands the web
// rewrites before they reach the CLI.
//
// A command whose stdin or stdout is a FILE never shows that file in the
// transcript — "nobody wants to deal with stdin/out on this little repl,
// that would be a terrible user interface" (vividn, 2026-09-11). So the
// browser's file dialog IS the command's stdin and a download IS its
// stdout, and the CLI runs `-` exactly as it would in a pipe. `db purge`,
// which wants a TTY to ask on, asks in a dialog instead.
//
// These are the whole table, on purpose: anything else typed runs as
// typed, and this is not the start of a shell. The input bar's main menu
// (after the shell, da-0a5o) calls `upload` / `download` / `purge`
// directly; a named target database would put `--db <name>` in front of
// the argv they run (the names are `DATABASES` in $lib/wasiFs.ts).

import ConfirmName from './ConfirmName.svelte';
import { saveFile } from './backup.js';
import { run, type RunResult } from './run.js';

/**
 * `argv` run the browser's way if it is one of the file commands, else
 * null for the caller to run as typed.
 *
 * Call it synchronously from the Enter handler: a file dialog opens only
 * inside the user's activation, which an `await` before it can spend
 * (Safari is strictest), and `upload` opens its dialog before its first.
 */
export function fileCommand(argv: string[]): Promise<RunResult> | null {
	const [db, sub, ...rest] = argv;
	if (db !== 'db') return null;
	// A PATH typed is not a path here — the dialog is the path, or names
	// the download — so what goes on is the flags, then `-`.
	const flags = rest.filter((t) => t.startsWith('-') && t !== '-' && !/^--(from|backup)=/.test(t));
	const path = rest.find((t) => !t.startsWith('-'));
	switch (sub) {
		case 'restore':
		case 'v2import':
			return upload([db, sub, ...flags, '-']);
		case 'backup':
			return download(path);
		case 'purge':
			// `--yes` typed by hand is the CLI's contract: straight through.
			return flags.includes('--yes') || flags.includes('-y') ? null : purge(flags);
		default:
			return null;
	}
}

/** Run `argv` (ending in `-`) with a file the reader picks as its stdin. */
export function upload(argv: string[]): Promise<RunResult> {
	const picker = Object.assign(document.createElement('input'), { type: 'file', hidden: true });
	document.body.append(picker);
	const chosen = new Promise<File | null>((resolve) => {
		picker.addEventListener('change', () => resolve(picker.files?.[0] ?? null));
		picker.addEventListener('cancel', () => resolve(null));
	});
	picker.click();
	return chosen.then(async (file) => {
		picker.remove();
		if (!file) return { argv, code: 0, stdout: 'no file chosen — nothing ran', stderr: '' };
		return run(argv, await file.text());
	});
}

/** `db backup -`, handed to the browser as a download; the transcript gets the header. */
export async function download(name?: string): Promise<RunResult> {
	const result = await run(['db', 'backup', '-']);
	if (result.code !== 0) return result;
	// `sv` writes a date as YYYY-MM-DD, in the reader's own day.
	name = !name || name === '-' ? `datum-${new Date().toLocaleDateString('sv')}.jsonl` : name;
	saveFile(result.stdout + '\n', name);
	const [header, ...entries] = result.stdout.split('\n');
	return { ...result, stdout: `${header}\n${entries.length} entries → ${name}` };
}

/** `db purge --yes`, once the reader has typed the database's name. */
export async function purge(flags: string[] = []): Promise<RunResult> {
	const confirmed = await new Promise<boolean>((resolve) => {
		const dialog = new ConfirmName({ target: document.body, props: { name: 'datum' } });
		dialog.$on('close', ({ detail }) => {
			dialog.$destroy();
			resolve(detail);
		});
	});
	if (!confirmed) {
		// The CLI's own words for a mismatch at its prompt.
		return { argv: ['db', 'purge'], code: 1, stdout: '', stderr: 'db purge: aborted (input did not match datum)' };
	}
	return run(['db', 'purge', '--yes', ...flags]);
}
