// The argv a file command runs in the browser (#lib/dbio.ts, da-1vz2):
// every option and its value as typed, in order, with only the input
// source — the positional FILE, or `--from`'s value — as `-`. Pure and
// import-free, so node tests it as it is.

/**
 * The CLI's own options for these commands (datum-parse's
 * `parse/migrate_io.rs` and `parse/kit.rs`), true for one that takes a
 * value. Anything not here goes on as typed, for the CLI to judge.
 */
const OPTIONS: Record<string, Record<string, boolean>> = {
	'db v2import': { '--from': true, '--backup': true, '--skip-log': true, '--dry-run': false },
	'db restore': { '--merge': false, '--force': false, '--dry-run': false, '--yes': false, '--json': false, '--cluster': false },
	'db backup': { '--overwrite': false },
	'kit install': { '--force': false, '--yes': false, '--dry-run': false }
};

export interface FileArgv {
	/** What runs: the source as `-`, or `-` last when none was typed. */
	argv: string[];
	/** `db backup`'s PATH, which names the download. */
	name?: string;
	/** Paths typed, which are not files here: a FILE the dialog replaces, `--skip-log`'s. */
	paths: string[];
}

/** `argv` rewritten if it is `db v2import` / `restore` / `backup` or `kit install`, else null. */
export function fileArgv(argv: string[]): FileArgv | null {
	const [cmd, sub, ...rest] = argv;
	const options = OPTIONS[`${cmd} ${sub}`];
	if (!options) return null;
	const out = [cmd, sub];
	const paths: string[] = [];
	let name: string | undefined;
	let sourced = false;
	for (let i = 0; i < rest.length; i++) {
		const tok = rest[i];
		if (tok === '-' || !tok.startsWith('-')) {
			if (sub === 'backup' && name === undefined) name = tok;
			else if (tok !== '-') paths.push(tok);
			out.push('-');
			sourced = true;
			continue;
		}
		const eq = tok.startsWith('--') ? tok.indexOf('=') : -1;
		const flag = eq < 0 ? tok : tok.slice(0, eq);
		if (!options[flag]) {
			out.push(tok);
			continue;
		}
		out.push(flag);
		const value = eq < 0 ? rest[++i] : tok.slice(eq + 1);
		if (value === undefined) continue;
		if (value !== '-') paths.push(value);
		if (flag === '--skip-log') {
			out.push(value);
		} else {
			out.push('-');
			sourced = true;
		}
	}
	if (!sourced) out.push('-');
	return { argv: out, name, paths };
}
