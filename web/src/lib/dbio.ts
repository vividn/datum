// The REPL's file commands (da-podv): the only typed commands the web
// rewrites before they reach the CLI.
//
// A command whose stdin or stdout is a FILE never shows that file in the
// transcript — "nobody wants to deal with stdin/out on this little repl,
// that would be a terrible user interface" (vividn, 2026-09-11). So the
// browser's file dialog IS the command's stdin and a download IS its
// stdout, and the CLI runs `-` exactly as it would in a pipe. `db purge`,
// which wants a TTY to ask on, asks in a dialog instead, and so does
// `kit install`'s trust (da-axrk).
//
// Every prompt consults it — the REPL's, the input widget's, and the
// bottom bar's through `press` — and nothing else: `run` is the seam and
// stays literal. These are the whole table, on purpose: anything else
// typed runs as typed, and this is not the start of a shell. A main menu
// on the bottom bar ($lib/PromptBar.svelte) would call `upload` /
// `download` / `purge` directly; a named target database would put
// `--db <name>` in front of the argv they run (the names are `DATABASES`
// in $lib/wasiFs.ts).

import Confirm from './Confirm.svelte';
import { saveFile } from './backup.js';
import { fileArgv } from './fileargv.js';
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
	const [cmd, sub, ...flags] = argv;
	if (cmd === 'db' && sub === 'purge') {
		// `--yes` typed by hand is the CLI's contract: straight through.
		return flags.includes('--yes') || flags.includes('-y') ? null : purge(flags);
	}
	const file = fileArgv(argv);
	if (!file) return null;
	const ran =
		sub === 'backup' ? download(file.name, file.argv) : cmd === 'kit' ? kit(file.argv) : upload(file.argv);
	if (!file.paths.length) return ran;
	// A path typed is not a file here — the dialog is the file, or names
	// the download — and saying so is one quiet line.
	const note = `not a file in the browser: ${file.paths.join(', ')}`;
	return ran.then((r) => ({ ...r, stderr: [r.stderr, note].filter(Boolean).join('\n') }));
}

/** The file the reader picks in the browser's dialog, which opens before this returns. */
function pick(): Promise<File | null> {
	const picker = Object.assign(document.createElement('input'), { type: 'file', hidden: true });
	document.body.append(picker);
	const chosen = new Promise<File | null>((resolve) => {
		picker.addEventListener('change', () => resolve(picker.files?.[0] ?? null));
		picker.addEventListener('cancel', () => resolve(null));
	});
	picker.click();
	return chosen.finally(() => picker.remove());
}

/** What a command that ran nothing says, in one quiet line. */
const nothing = (argv: string[], stdout: string): RunResult => ({ argv, code: 0, stdout, stderr: '' });

/** Run `argv` (its source `-`) with a file the reader picks as its stdin. */
export function upload(argv: string[]): Promise<RunResult> {
	return pick().then(async (file) =>
		file ? run(argv, await file.text()) : nothing(argv, 'no file chosen — nothing ran')
	);
}

/**
 * `kit install -` with the kit the reader picks, once they have seen what
 * it brings (the CLI's own `--dry-run`) and said yes: the page's confirm is
 * the trust the CLI asks y/N for on a TTY, so `--yes` reaches the CLI only
 * after it — typed, it is dropped. A typed `--dry-run` just says what the
 * kit brings.
 */
export function kit(argv: string[]): Promise<RunResult> {
	if (argv.includes('--dry-run')) return upload(argv);
	argv = argv.filter((a) => a !== '--yes' && a !== '-y');
	return pick().then(async (file) => {
		if (!file) return nothing(argv, 'no file chosen — nothing ran');
		const toml = await file.text();
		const seen = await run([...argv, '--dry-run'], toml);
		if (seen.code !== 0) return seen;
		const text = `${seen.stdout.trim()}\n\nA kit can add SQL views and commands to your database; install only kits you trust.`;
		if (!(await confirm(text, 'install'))) return nothing(argv, 'kit not installed — nothing ran');
		return run([...argv, '--yes'], toml);
	});
}

/** Whether the reader pressed `action` in a dialog saying `text` (and typed `name`, if one is given). */
function confirm(text: string, action: string, name = ''): Promise<boolean> {
	return new Promise((resolve) => {
		const dialog = new Confirm({ target: document.body, props: { text, action, name } });
		dialog.$on('close', ({ detail }) => {
			dialog.$destroy();
			resolve(detail);
		});
	});
}

/** `db backup -`, handed to the browser as a download; the transcript gets the header. */
export async function download(name?: string, argv = ['db', 'backup', '-']): Promise<RunResult> {
	const result = await run(argv);
	if (result.code !== 0) return result;
	// `sv` writes a date as YYYY-MM-DD, in the reader's own day.
	name = !name || name === '-' ? `datum-${new Date().toLocaleDateString('sv')}.jsonl` : name;
	saveFile(result.stdout + '\n', name);
	const [header, ...entries] = result.stdout.split('\n');
	return { ...result, stdout: `${header}\n${entries.length} entries → ${name}` };
}

/** `db purge --yes`, once the reader has typed the database's name. */
export async function purge(flags: string[] = []): Promise<RunResult> {
	if (!(await confirm('This will destroy ALL data in datum.', 'purge', 'datum'))) {
		// The CLI's own words for a mismatch at its prompt.
		return { argv: ['db', 'purge'], code: 1, stdout: '', stderr: 'db purge: aborted (input did not match datum)' };
	}
	return run(['db', 'purge', '--yes', ...flags]);
}
