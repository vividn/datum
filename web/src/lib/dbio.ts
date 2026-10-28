// The REPL's file commands (da-podv): the only typed commands the web
// rewrites before they reach the CLI.
//
// A command whose stdin or stdout is a FILE never shows that file in the
// transcript — "nobody wants to deal with stdin/out on this little repl,
// that would be a terrible user interface" (vividn, 2026-09-11). So the
// browser's file dialog IS the command's stdin and a download IS its
// stdout, and the CLI runs `-` exactly as it would in a pipe. `db purge`,
// which wants a TTY to ask on, asks in a dialog instead, and so do
// `setup F --delete` (da-ez40f), `kit install`'s trust (da-axrk), a replacing `db restore`'s
// deletions (da-jrbwc), a `--hard` one's drops (da-qq7gb), a stale
// `_LAST`'s question (da-f5pg2, `asking`) and which of a repeated hour's
// two readings a time means (da-6i7l1, `asking`).
//
// Every line a person types consults it — the REPL's own runner and the
// bottom bar's through `press` — and nothing else: `run` is the seam and
// stays literal. These are the whole table, on purpose: anything else
// typed runs as typed, and this is not the start of a shell. A main menu
// on the bottom bar (#lib/PromptBar.svelte) would call `upload` /
// `download` / `purge` directly. They run through `run`, so the page's
// own database is already in front of them (da-vk1x, #lib/database.ts):
// `db backup -` on a `?db=other` page exports `other`.

import { writable } from 'svelte/store';
import { saveFile } from './backup.js';
import { backupArgv, chainListArgv, dbPurgeArgv, dbPurgeChainArgv, dbPurgeProbeArgv, restorePreviewArgv } from './calls.js';
import { choose, confirm } from './confirm.js';
import { runEchoed } from './echo.js';
import { fileArgv } from './fileargv.js';
import { FIELDS, STALE } from './layout/draft.js';
import { approve } from './passkey.js';
import { forget } from './cliStore.js';
import { dbOf, run, type CommandRunner, type RunResult } from './run.js';
import type { ChainList } from './syncView.js';

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
		// `--yes` typed by hand is the CLI's contract: straight through. So is
		// `--chain`, `sync chain purge --data` by another name
		// (da-2bb7z), which asks nothing and changes nothing without `--yes`.
		const yes = flags.some((f) => f === '--yes' || f === '-y');
		if (!yes && flags.includes('--chain')) return null;
		// A purge that ran takes what the page kept of the data with it.
		return (yes ? runEchoed(asking(run), argv) : purge(flags)).then((r) => {
			if (r.code === 0) forgetKept();
			return r;
		});
	}
	// The field is the line's one word that is not a flag, as the CLI reads
	// it; a `--delete` with none is the CLI's to refuse, unasked.
	const field = argv.slice(1).find((w) => !w.startsWith('-'));
	if (cmd === 'setup' && field && argv.includes('--delete')) return deleteSpec(argv, field);
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

/** `setup F --delete`, once the reader has said yes to it; its echo is the bar's DELETE. */
async function deleteSpec(argv: string[], field: string): Promise<RunResult> {
	const text = `Delete the spec of ${field}? Its entries stay, and one Undo brings the spec back.`;
	if (!(await confirm(text, 'delete'))) return nothing(argv, 'spec not deleted — nothing ran');
	return runEchoed(run, argv);
}

/** `datum`'s exit code for a question stdin did not answer (`EXIT_UNANSWERED`). */
export const UNANSWERED = 12;

/**
 * `run`, but a question the CLI could not ask on stdin — a stale `_LAST`
 * under `retime` / `update` (da-f5pg2) — is asked in a confirm, in the CLI's
 * own words: what it said on stderr, the rows it printed, then its question.
 * The line runs again with every answer so far on stdin, so a yes changes the entry
 * and a no prints the CLI's own «Aborted.». The answer carries the pin the
 * CLI printed after the question (da-84h6a): if another tab moved `_LAST`
 * meanwhile, the CLI reads no answer for an entry it never showed and asks
 * again, about the entry `_LAST` is now. A question whose answers the CLI
 * names after it (`DATUM_CHOOSE=`, a wall time a fall-back night repeats,
 * da-6i7l1) is a button an answer, and the line runs again with the env it
 * ran with, so a gesture's frozen instant is the answer's too.
 */
export const asking =
	(run: CommandRunner): CommandRunner =>
	async (argv, stdin, onStderr, key, stage, env = []) => {
		let result = await run(argv, stdin, onStderr, key, stage, env);
		// Every answer so far, a line each in the order asked, and the pin of
		// the `_LAST` it confirmed: a line may ask more than one question —
		// two wall times a fall-back night repeats (an `occur_time=` and a
		// `wake_time=`, da-job98), a stale `_LAST` and then which reading of
		// its new time (da-7bomd) — and each run asks the next only once the
		// earlier are answered.
		const answers: string[] = [];
		let pin: string[] = [];
		let confirmed = 0;
		while (result.code === UNANSWERED && stdin === undefined) {
			const said = result.stderr.trimEnd().split('\n');
			// A question with answers of its own, a wall time a fall-back
			// night repeats (da-6i7l1): a button each, the one pressed on
			// stdin, and nothing on stdin for cancel, which the CLI aborts.
			const options = said.at(-1)?.match(/^DATUM_CHOOSE=(.*)$/)?.[1].split('|');
			if (options) {
				said.pop();
				answers.push((await choose(said.join('\n'), options, 'cancel', 'run')) ?? '');
			} else {
				// Asked again with a pin in hand, `_LAST` moved (da-84h6a): the
				// CLI read no answer for it, so this one takes its place and
				// whatever followed, which was about the entry it was then.
				if (pin.length) answers.length = confirmed;
				confirmed = answers.length;
				pin = said.at(-1)?.startsWith('DATUM_LAST_SHOWN=') ? [said.pop()!] : [];
				const question = said.pop()?.trimEnd() ?? '';
				const text = [...said, result.stdout.trimEnd(), question].filter(Boolean).join('\n');
				answers.push((await confirm(text, question.split(' ')[0].toLowerCase())) ? 'y' : 'n');
			}
			const lines = answers.map((a) => `${a}\n`).join('');
			result = await run(argv, lines, onStderr, key, stage, [...env, ...pin]);
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

/** `db restore --dry-run --json`: what a restore would do; `dropped` only with `--hard`. */
export interface RestorePreview {
	would_delete: number;
	would_insert: number;
	would_update: number;
	warnings: string[];
	/** The live entries counted, which the run is pinned to; null for an empty backup. */
	fingerprint: string | null;
	dropped?: { entries?: number; tombstones?: number; history?: number };
	/** The sync chain a `--hard --chain` restores every copy of (da-ncmmj). */
	chain?: string;
	/**
	 * In a locked chain, the approve link of the order a passkey signs for
	 * the run (da-sd7xt).
	 * @shallow no seeded chain is locked.
	 */
	approve?: string | null;
}

/**
 * `db restore -` with the backup the reader picks. A replace tombstones
 * what the backup lacks, and the CLI asks first only on a TTY (da-5fooh),
 * which the browser has not got: so the page asks, with the counts of the
 * CLI's own `--dry-run` on the same bytes, and runs it `--yes` on yes
 * (da-jrbwc). One that deletes nothing and warns of nothing, `--merge`,
 * `--dry-run` and a typed `--yes` run as asked — but a `--hard` that is
 * not a dry run is `hard`'s, which asks whatever was typed.
 *
 * The run is pinned to the live entries the dry run counted (da-2zqad):
 * two tabs are two workers on one file, so another tab can write while the
 * question is open, and an unpinned yes would delete that write uncounted.
 * The CLI refuses a run whose database has changed since, and the page
 * counts again and asks again, even when the first count asked nothing.
 */
export function restore(argv: string[]): Promise<RunResult> {
	if (argv.includes('--hard') && !argv.includes('--dry-run')) return hard(argv);
	if (['--merge', '--dry-run', '--yes', '-y'].some((a) => argv.includes(a))) return upload(argv);
	return pick().then(async (file) => {
		if (!file) return nothing(argv, 'no file chosen — nothing ran');
		const backup = await file.text();
		for (let moved = ''; ; moved = '⚠ The database changed since it was counted. ') {
			const seen = await feed(restorePreviewArgv(argv), backup);
			if (seen.code !== 0) return seen;
			const { would_delete, would_insert, would_update, warnings, fingerprint } = JSON.parse(seen.stdout) as RestorePreview;
			const pin = fingerprint ? ['--if-unchanged', fingerprint] : [];
			// A warning asks too, deleting or not (da-fnjku): a truncated backup into an empty db deletes nothing.
			// So does a database that changed under a run, so a busy one never retries unattended.
			const asks = moved || would_delete || warnings.length;
			const text =
				moved +
				(would_delete
					? `Restore replaces ${dbOf(argv)}: ${would_delete} entries will be deleted, ` +
						`${would_insert} added, ${would_update} changed. Undo reverts the whole restore.`
					: `Restore adds ${would_insert} entries and changes ${would_update}.`) +
				// Its warnings too (da-i4rqd): a header that disagrees with the lines is known before the yes.
				warnings.map((w: string) => ` ⚠ ${w}.`).join('');
			if (asks && !(await confirm(text, 'restore'))) {
				// The CLI's own words for a no at its prompt.
				return { argv, code: 1, stdout: '', stderr: 'db restore: aborted (nothing changed; --merge deletes nothing)' };
			}
			const ran = await feed([...argv, ...(asks ? ['--yes'] : []), ...pin], backup);
			if (ran.code !== UNANSWERED) return ran;
		}
	});
}

/**
 * `db restore --hard -` with the backup the reader picks, once they have
 * read what the CLI's own `--dry-run` says it drops and typed the
 * database's name (da-qq7gb): the CLI's TTY prompt, as `purge` gets. It
 * cannot be undone, so a typed `--yes` is dropped, as `kit`'s is — the
 * CLI's refusal on no TTY points at it, and the yes reached nobody.
 *
 * The run is pinned to the live entries the dry run counted, as
 * `restore`'s is (da-h3yb3): a write from another tab since refuses it,
 * and the page counts again and asks again.
 */
function hard(argv: string[]): Promise<RunResult> {
	argv = argv.filter((a) => a !== '--yes' && a !== '-y');
	return pick().then(async (file) => {
		if (!file) return nothing(argv, 'no file chosen — nothing ran');
		const backup = await file.text();
		const db = dbOf(argv);
		for (let moved = ''; ; moved = '⚠ The database changed since it was counted. ') {
			const seen = await feed(restorePreviewArgv(argv), backup);
			if (seen.code !== 0) return seen;
			const { would_insert, dropped, warnings, fingerprint, chain, approve: link } = JSON.parse(seen.stdout) as RestorePreview;
			// An empty file is a backup of nothing, and replaces nothing: no fingerprint.
			const pin = fingerprint ? ['--if-unchanged', fingerprint] : [];
			// A locked chain's order takes an admin's signature (da-art6t): a
			// passkey's this page asks for and hands the run beside the backup on
			// its stdin (da-sd7xt), a key file's only the CLI reads — said before the name.
			const listed = chain ? await run(['--db', db, ...chainListArgv()]) : null;
			const { admins } = listed?.code === 0 ? (JSON.parse(listed.stdout) as ChainList) : { admins: [] };
			if (admins.length && admins.every((a) => a.key.startsWith('ed25519:'))) {
				const stderr =
					`db restore --hard --chain: sync chain ${chain} is locked by an admin key file, which only the CLI reads. Nothing changed. ` +
					'On a computer, `datum db restore --hard --chain <BACKUP> --admin-key <FILE>` restores every copy.';
				return { argv, code: 1, stdout: '', stderr };
			}
			const { entries = 0, tombstones = 0, history = 0 } = dropped ?? {};
			const text =
				moved +
				// `--chain` restores every copy, not this one alone (da-t26vv): the CLI's own words for it.
				(chain
					? `Every copy in sync chain ${chain} is restored to the backup (${would_insert} written), its own data dropped: ` +
						'an entry a copy made before the restore is dropped as it syncs, offline or not, and kept on that copy in ' +
						'<db>.wiped-<time>.jsonl, which `db restore <it> --merge` brings back; one made after it is kept. ' +
						'This copy drops '
					: `This replaces ALL data in ${db} with the backup (${would_insert} written); dropped: `) +
				`${entries} live, ${tombstones} tombstoned or superseded, ${history} in history. ` +
				'It cannot be undone: the history `undo` reads goes with it.' +
				(link ? ' The chain is locked, so an admin’s passkey approves it once the name is typed.' : '') +
				warnings.map((w: string) => ` ⚠ ${w}.`).join('');
			if (!(await confirm(text, chain ? 'restore every copy' : 'restore --hard', db))) {
				// The CLI's own words for a mismatch at its prompt.
				return { argv, code: 1, stdout: '', stderr: `db restore --hard: aborted (input did not match ${db})` };
			}
			let approval: string[] = [];
			if (link) {
				try {
					approval = ['--approve-link', link, '--approval-code', await approve(link)];
				} catch (e) {
					return { argv, code: 1, stdout: '', stderr: `db restore --hard --chain: no passkey approved it (${e}). Nothing changed.` };
				}
			}
			const ran = await feed([...argv, '--yes', ...pin, ...approval], backup);
			if (ran.code !== UNANSWERED) return ran;
		}
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
export async function download(name?: string, argv = backupArgv()): Promise<RunResult> {
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
	const argv = backupArgv();
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

/**
 * `db purge --yes`, once the reader has typed the database's name. A copy
 * in a sync chain is asked first whether it leaves the chain, rejoins it
 * or purges every copy in it (da-3ni5m): the CLI refuses naming `--leave`,
 * `--rejoin` and `--chain` (da-n0zx1), which reached the reader only after
 * they had typed the name for nothing (da-0hay9). A typed `--leave` or
 * `--rejoin` is the answer.
 */
export async function purge(flags: string[] = []): Promise<RunResult> {
	// The database this will really purge, never the word `datum` written
	// down here (vremya on #22). The dialog stands in for the CLI's TTY
	// prompt, and `run` puts the page's `--db` in front of the command
	// (da-vk1x) — so on a `?db=other` page the old dialog asked the reader
	// to type the name of a database that SURVIVED while the one they were
	// standing in was destroyed. `dbOf` reads the argv first, so a typed
	// `db purge --db x` names `x`.
	const db = dbOf(dbPurgeArgv(flags));
	// `sync chain list` answers `chain: null` in a copy in no chain, and one
	// joining a chain is in none: what the CLI's purge reads.
	const listed = await run(['--db', db, ...chainListArgv()]);
	const { chain, admins } = listed.code === 0 ? (JSON.parse(listed.stdout) as ChainList) : { chain: null, admins: [] };
	let way = flags.find((f) => f === '--leave' || f === '--rejoin')?.slice(2) ?? null;
	if (chain && !way) {
		// A locked chain's order takes an admin's signature (da-x5lw4): a
		// passkey's this page asks for (`syncAct`), a key file's only the CLI
		// reads — said before the name, which would otherwise be typed for nothing.
		const passkey = admins.some((a) => !a.key.startsWith('ed25519:'));
		const offered = !admins.length || passkey;
		const whole = !admins.length
			? 'chain wipes every copy in the chain, each as it next syncs.'
			: passkey
				? 'chain wipes every copy in the chain, each as it next syncs; the chain is locked, so an admin’s passkey approves it once the name is typed.'
				: 'the chain is locked by an admin key file, which only the CLI reads: `datum db purge --chain --admin-key <FILE>` wipes every copy in it.';
		way = await choose(
			`This copy is in sync chain ${chain}: leave tells the chain this copy leaves, then wipes it; ` +
				`rejoin wipes it and refetches everything from the chain, staying a member; ${whole}\n` +
				(offered ? 'Leave, rejoin, or purge the whole chain?' : 'Leave or rejoin?'),
			offered ? ['leave', 'rejoin', 'chain'] : ['leave', 'rejoin']
		);
		if (!way) return { argv: ['db', 'purge'], code: 1, stdout: '', stderr: 'db purge: aborted (none of leave, rejoin or chain)' };
		if (way === 'chain') return purgeChain(db);
		flags = [...flags, `--${way}`];
	}
	// What the CLI refuses before its name prompt — either flag out of a
	// chain, a rejoin with no member to refetch from or a change none holds
	// — it refuses off a TTY too: so the name is asked only for a purge
	// that will run.
	const probe = await run(dbPurgeProbeArgv(flags));
	if (!probe.stderr.includes('stdin is not a TTY')) return probe;
	const text =
		`This will destroy ALL data in ${db}, every entry and its history, and the copies of it kept before upgrades.` +
		(way === 'rejoin'
			? `\nIt stays a member of sync chain ${chain}, and refetches everything from it.`
			: way
				? `\nIt leaves sync chain ${chain} first.`
				: '') +
		`\n${CLEARS}`;
	if (!(await confirm(text, 'purge', db))) return mismatch(db);
	// A departure leaves only once a member holds it (da-dsob1), which this
	// page's sync delivers: each session it ends well is a reason to run the
	// purge again (da-a7kf7), as the sync widget's Leave does (da-2o19r).
	const { deliverMs, syncedSince, syncNow } = await import('./sync.js');
	const refused = (r: RunResult) => r.code !== 0 && r.stderr.includes('no member holds');
	const until = Date.now() + deliverMs();
	for (;;) {
		const since = Date.now();
		const r = await run(dbPurgeArgv(flags));
		if (!refused(r)) return r;
		progress.set('Telling the chain’s other devices that this copy is leaving…');
		const synced = await syncedSince(db, since, until - Date.now()).finally(() => progress.set(''));
		if (synced) continue;
		const unheard = `No device of sync chain ${chain} could be reached to receive this copy’s departure. Purge ${db} now, leaving the chain telling no one?`;
		if (!(await confirm(unheard, 'purge anyway'))) return r;
		// A member may have come up while the question was open (da-48g3o): one
		// sync, and the purge again, which forces only if still unheard.
		await syncNow(db);
		const again = await run(dbPurgeArgv(flags));
		return refused(again) ? run(dbPurgeArgv([...flags, '--force'])) : again;
	}
}

/** What a purge on this page clears beyond the database, in the dialog's words. */
const CLEARS = 'On this device it clears the command history and what the widgets last showed too.';

/** The CLI's own words for a mismatch at its prompt. */
const mismatch = (db: string): RunResult => ({ argv: ['db', 'purge'], code: 1, stdout: '', stderr: `db purge: aborted (input did not match ${db})` });

/**
 * Every copy in the chain purged (da-3ni5m), `db purge --chain`:
 * the CLI's own warning naming each copy, then the name, then the signed
 * order — with a passkey's approval when the chain is locked.
 */
async function purgeChain(db: string): Promise<RunResult> {
	const said = await run(['--db', db, ...dbPurgeChainArgv()]);
	if (said.code !== 0) return said;
	// All but the line naming the `--yes` that does it: here, typing the name.
	const warning = said.stdout
		.trimEnd()
		.split('\n')
		.filter((line) => !line.startsWith('nothing changed:'))
		.join('\n');
	if (!(await confirm(`${warning}\n${CLEARS}`, 'purge every copy', db))) return mismatch(db);
	const { syncAct } = await import('./syncView.js');
	return syncAct(db, dbPurgeChainArgv(true));
}

/** The REPL's scrollback and the widgets' last drawings, both the purged data's words. */
function forgetKept(): void {
	forget();
	try {
		localStorage.removeItem(STALE);
		localStorage.removeItem(FIELDS);
	} catch {
		// Refused: nothing was kept.
	}
}
