// The REPL's file commands (da-podv): the only typed commands the web
// rewrites before they reach the CLI.
//
// A command whose stdin or stdout is a FILE never shows that file in the
// transcript — "nobody wants to deal with stdin/out on this little repl,
// that would be a terrible user interface" (vividn, 2026-09-11). So the
// browser's file dialog IS the command's stdin and a download IS its
// stdout, and the CLI runs `-` exactly as it would in a pipe. `db purge`,
// which wants a TTY to ask on, asks in a dialog instead, and so do
// `kit install`'s trust (da-axrk), a replacing `db restore`'s
// deletions (da-jrbwc) and a stale `_LAST`'s question (da-f5pg2, `asking`).
//
// Every line a person types consults it — the REPL's own runner and the
// bottom bar's through `press` — and nothing else: `run` is the seam and
// stays literal. These are the whole table, on purpose: anything else
// typed runs as typed, and this is not the start of a shell. A main menu
// on the bottom bar ($lib/PromptBar.svelte) would call `upload` /
// `download` / `purge` directly. They run through `run`, so the page's
// own database is already in front of them (da-vk1x, $lib/database.ts):
// `db backup -` on a `?db=other` page exports `other`.

import { writable } from 'svelte/store';
import { saveFile } from './backup.js';
import { confirm } from './confirm.js';
import { fileArgv } from './fileargv.js';
import { dbOf, run, type CommandRunner, type RunResult } from './run.js';

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
		sub === 'backup'
			? download(file.name, file.argv)
			: cmd === 'kit'
				? kit(file.argv)
				: sub === 'restore'
					? restore(file.argv)
					: upload(file.argv);
	if (!file.paths.length) return ran;
	// A path typed is not a file here — the dialog is the file, or names
	// the download — and saying so is one quiet line.
	const note = `not a file in the browser: ${file.paths.join(', ')}`;
	return ran.then((r) => ({ ...r, stderr: [r.stderr, note].filter(Boolean).join('\n') }));
}

/** `datum`'s exit code for a question stdin did not answer (`EXIT_UNANSWERED`). */
export const UNANSWERED = 12;

/**
 * `run`, but a question the CLI could not ask on stdin — a stale `_LAST`
 * under `retime` / `update` (da-f5pg2) — is asked in a confirm, in the CLI's
 * own words: what it said on stderr, the rows it printed, then its question.
 * The line runs again with the answer on stdin, so a yes changes the entry
 * and a no prints the CLI's own «Aborted.». The answer carries the pin the
 * CLI printed after the question (da-84h6a): if another tab moved `_LAST`
 * meanwhile, the CLI reads no answer for an entry it never showed and asks
 * again, about the entry `_LAST` is now.
 */
export const asking =
	(run: CommandRunner): CommandRunner =>
	async (argv, stdin, onStderr, key, stage, env = []) => {
		let result = await run(argv, stdin, onStderr, key, stage, env);
		while (result.code === UNANSWERED && stdin === undefined) {
			const said = result.stderr.trimEnd().split('\n');
			const pin = said.at(-1)?.startsWith('DATUM_LAST_SHOWN=') ? [said.pop()!] : [];
			const question = said.pop()?.trimEnd() ?? '';
			const text = [...said, result.stdout.trimEnd(), question].filter(Boolean).join('\n');
			const yes = await confirm(text, question.split(' ')[0].toLowerCase());
			result = await run(argv, yes ? 'y\n' : 'n\n', onStderr, key, stage, [...env, ...pin]);
		}
		return result;
	};

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

/**
 * The latest line an upload's command has said on stderr while it runs —
 * `db v2import` / `db restore` counting as they go (da-h9vj) — and '' once
 * it has exited.
 */
export const progress = writable('');

/** Run `argv` (its source `-`) with a file the reader picks as its stdin. */
export function upload(argv: string[]): Promise<RunResult> {
	return pick().then(async (file) => (file ? feed(argv, await file.text()) : nothing(argv, 'no file chosen — nothing ran')));
}

/** Run `argv` with `stdin`, its stderr counting in `progress` while it runs. */
function feed(argv: string[], stdin: string): Promise<RunResult> {
	const said = (text: string) => {
		const line = text.trim().split(/[\r\n]+/).at(-1);
		if (line) progress.set(line);
	};
	return run(argv, stdin, said).finally(() => progress.set(''));
}

/**
 * `db restore -` with the backup the reader picks. A replace tombstones
 * what the backup lacks, and the CLI asks first only on a TTY (da-5fooh),
 * which the browser has not got: so the page asks, with the counts of the
 * CLI's own `--dry-run` on the same bytes, and runs it `--yes` on yes
 * (da-jrbwc). One that deletes nothing and warns of nothing, `--merge`,
 * `--dry-run` and a typed `--yes` run as asked.
 */
export function restore(argv: string[]): Promise<RunResult> {
	if (['--merge', '--dry-run', '--yes', '-y'].some((a) => argv.includes(a))) return upload(argv);
	return pick().then(async (file) => {
		if (!file) return nothing(argv, 'no file chosen — nothing ran');
		const backup = await file.text();
		const seen = await feed([...argv, '--dry-run', '--json'], backup);
		if (seen.code !== 0) return seen;
		const { would_delete, would_insert, would_update, warnings } = JSON.parse(seen.stdout);
		// A warning asks too, deleting or not (da-fnjku): a truncated backup into an empty db deletes nothing.
		if (!would_delete && !warnings.length) return feed(argv, backup);
		const text =
			(would_delete
				? `Restore replaces ${dbOf(argv)}: ${would_delete} entries will be deleted, ` +
					`${would_insert} added, ${would_update} changed. Undo reverts the whole restore.`
				: `Restore adds ${would_insert} entries and changes ${would_update}.`) +
			// Its warnings too (da-i4rqd): a header that disagrees with the lines is known before the yes.
			warnings.map((w: string) => ` ⚠ ${w}.`).join('');
		if (!(await confirm(text, 'restore'))) {
			// The CLI's own words for a no at its prompt.
			return { argv, code: 1, stdout: '', stderr: 'db restore: aborted (nothing changed; --merge deletes nothing)' };
		}
		return feed([...argv, '--yes'], backup);
	});
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

/**
 * `db backup -` into a file the reader names in the browser's SAVE picker
 * (da-9ow8m), or the download where there is no picker. The picker opens
 * first, inside the press: an `await` before it spends the activation.
 */
export async function saveBackup(): Promise<RunResult> {
	const argv = ['db', 'backup', '-'];
	const name = `${dbOf(argv)}-${new Date().toLocaleDateString('sv')}.jsonl`;
	const picker = (window as { showSaveFilePicker?: (o: { suggestedName: string }) => Promise<FileSystemFileHandle> })
		.showSaveFilePicker;
	if (!picker) return download(name);
	let file: FileSystemFileHandle;
	try {
		file = await picker({ suggestedName: name });
	} catch {
		return nothing(argv, 'no file chosen — nothing ran');
	}
	const result = await run(argv);
	if (result.code !== 0) return result;
	const out = await file.createWritable();
	await out.write(result.stdout + '\n');
	await out.close();
	const [header, ...entries] = result.stdout.split('\n');
	return { ...result, stdout: `${header}\n${entries.length} entries → ${file.name}` };
}

/** `db purge --yes`, once the reader has typed the database's name. */
export async function purge(flags: string[] = []): Promise<RunResult> {
	// The database this will really purge, never the word `datum` written
	// down here (vremya on #22). The dialog stands in for the CLI's TTY
	// prompt, and `run` puts the page's `--db` in front of the command
	// (da-vk1x) — so on a `?db=other` page the old dialog asked the reader
	// to type the name of a database that SURVIVED while the one they were
	// standing in was destroyed. `dbOf` reads the argv first, so a typed
	// `db purge --db x` names `x`.
	const db = dbOf(['db', 'purge', '--yes', ...flags]);
	if (!(await confirm(`This will destroy ALL data in ${db}.`, 'purge', db))) {
		// The CLI's own words for a mismatch at its prompt.
		return { argv: ['db', 'purge'], code: 1, stdout: '', stderr: `db purge: aborted (input did not match ${db})` };
	}
	return run(['db', 'purge', '--yes', ...flags]);
}
