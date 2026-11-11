// datum's action URLs (da-uzl16). vividn, 2026-11-11: «can we allow buttons
// with a limited action vocabulary (like changing tabs, logging data, etc?)
// inside text widgets», and «maybe using links or something that embed
// actions (probably nice to be able to do actions using urls inside datum
// anyway)».
//
// One form wherever datum reads a link: `datum.day/act/<kind>/…`, or inside
// a page `act:<kind>/…`, which means the same. A text widget and a guide draw
// one as a button (#lib/markdown.ts, `act`), whose tap is the reader's yes;
// opened from outside, `/act/…` asks before it runs (routes/act). The
// vocabulary is small on purpose, and `run` logs and does nothing else.

import type { ActKind } from './act.js';
import { logs } from './commands.js';
import { shellSplit } from './shellSplit.js';

export type Action = { kind: 'tab' | 'place'; name: string } | { kind: 'field'; name: string } | { kind: 'run'; line: string };

/**
 * Every kind, with what it does and an example: the whole vocabulary, which
 * the text widget's guide lists (registry.ts) and a test holds it to. `field`
 * is an EMBED, the component itself, and no action (`EMBEDS` in markdown.ts).
 */
export const KINDS_OF_ACTION = {
	tab: { does: 'goes to that tab, opening it if it is not open', example: '[Morning](act:tab/morning)' },
	place: { does: 'opens a place, a widget on its own (a modal on a desktop, a tab on a phone): sync, settings, backups, help, docs or any widget by its name', example: '[Open Sync](act:place/sync)' },
	run: {
		does: 'runs one logging line as the bar would: moment, on, off, change, record, remark or allday, with the flags that say what the entry is (its time, -k, -c, -l, -L, -m, -T, --if-changed, --dry-run), nothing else',
		example: '[Coffee](act:run?line=moment+coffee)'
	},
	field: { does: "draws that field's collect button, which logs on a tap and opens on a hold", example: '[coffee](act:field/coffee)' }
} as const;

/** What a tap on each action does, by #lib/act.ts's kinds: a run runs, a place opens over the page, a tab takes its place. */
export const TAP: Record<Exclude<Action['kind'], 'field'>, ActKind> = { run: 'run', place: 'open', tab: 'show' };

const HOST = 'https://datum.day';
const PATH = /^\/act\/(tab|place|field)\/([^/]+)$|^\/act\/run$/;

/** The action `href` names — `act:…`, `/act/…` or `https://datum.day/act/…` — or null for any other link. */
export function actionOf(href: string): Action | null {
	try {
		const url = new URL(href.startsWith('act:') ? `/act/${href.slice(4)}` : href, HOST);
		const m = url.origin === HOST ? url.pathname.match(PATH) : null;
		if (!m) return null;
		if (!m[1]) {
			const line = url.searchParams.get('line')?.trim();
			return line ? { kind: 'run', line } : null;
		}
		return { kind: m[1] as 'tab' | 'place' | 'field', name: decodeURIComponent(m[2]) };
	} catch {
		return null;
	}
}

/** What `action` does, in words: a button's title, and what an action from outside asks. */
export const said = (action: Action): string =>
	action.kind === 'run'
		? `run ${action.line}`
		: action.kind === 'tab'
			? `go to the tab ${action.name}`
			: action.kind === 'place'
				? `open ${action.name}`
				: `${action.name}'s collect button`;

/**
 * The flags a run may carry, each with whether it takes a value: those that
 * say what the entry is (its time, keys, comment, the state left, untimed) and
 * how the answer is shown, from `datum help logging`. Any other writes more
 * than the entry (`--alias … --alias-overwrite`, `--id … --conflict replace`,
 * `-u`, `--setup`) or names another database (`--db`), so it is refused.
 */
const LONG: Record<string, boolean> = {
	yesterday: false,
	quick: false,
	date: true,
	time: true,
	timezone: true,
	'full-day': false,
	key: true,
	comment: true,
	remainder: true,
	'string-remainder': false,
	'comment-remainder': false,
	'string-state': false,
	'last-state': true,
	'ensure-last-state': true,
	moment: false,
	'no-timestamp': false,
	'if-changed': false,
	'dry-run': false,
	show: true,
	json: false
};
const SHORT: Record<string, boolean> = { y: false, q: false, d: true, t: true, z: true, D: false, k: true, c: true, R: true, S: false, C: false, I: false, l: true, L: true, m: false, T: false };

/** The first flag of a side's words after its command that a run may not carry, or null. A value is the flag's; after `--` every word is as typed. */
function strayFlag(words: string[]): string | null {
	for (let i = 0; i < words.length; i++) {
		const w = words[i];
		if (w === '--') return null;
		if (!w.startsWith('-') || w === '-') continue;
		if (w.startsWith('--')) {
			const name = w.slice(2).split('=')[0];
			if (!Object.hasOwn(LONG, name)) return `--${name}`;
			if (LONG[name] && !w.includes('=')) i++;
			continue;
		}
		// A cluster, `-yq`; a flag taking a value takes the rest of it (`-t10`), or else the next word (`-t -5`).
		for (let j = 1; j < w.length; j++) {
			if (!Object.hasOwn(SHORT, w[j])) return `-${w[j]}`;
			if (!SHORT[w[j]]) continue;
			if (j === w.length - 1) i++;
			break;
		}
	}
	return null;
}

/**
 * Why `action` is refused, or null: a `run` whose every side, a `,` token
 * between them (a quoted `","` too, as the CLI reads it), is a command that
 * logs and the flags that say what its entry is (`LONG`, `SHORT`). Any other
 * command, an alias's included, may be anything (`delete`, `db purge`), and
 * any other flag writes more than the entry, so each is refused, by name.
 */
export function refusal(action: Action): string | null {
	if (action.kind !== 'run') return null;
	const sides: string[][] = [[]];
	for (const w of shellSplit(action.line)) w === ',' ? sides.push([]) : sides.at(-1)!.push(w);
	for (const [head = '', ...rest] of sides) {
		if (!logs(head)) return `${head || 'nothing'} is not a logging command: a run takes moment, on, off, change, record, remark or allday`;
		const flag = strayFlag(rest);
		if (flag) return `${flag} is not a flag a run takes: only those saying what the entry is, its time, keys, comment and the state it leaves`;
	}
	return null;
}

/** Do `action`, a refused one nothing; an embed is no action. */
export async function perform(action: Action): Promise<void> {
	if (refusal(action)) return;
	if (action.kind === 'run') return void (await (await import('./layout/engine.js')).press(shellSplit(action.line), action.line));
	const shell = await import('./layout/shell.js');
	if (action.kind === 'tab') await shell.open(action.name);
	else if (action.kind === 'place') await shell.openPlace(action.name);
}
