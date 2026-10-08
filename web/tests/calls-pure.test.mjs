// Every command the page sends on its own still parses, and its JSON still
// has the keys the page reads — in node, with no browser (da-1y9gi).
//
// vividn, 2026-10-08: «a (quick) extra test to make sure every wasm call that
// the frontend is valid … could prevent drift when for example the rust api
// changes». The calls are `$lib/calls.ts`'s exports; each runs, with the
// samples below, through the same wasm CLI the page runs, on a copy of one
// seeded database, as `runners/wasi.ts` runs it. A usage error — a flag,
// subcommand or argument the CLI no longer takes — fails it, and so does a
// key in `READS` that no answer carries, naming the files that send it.
// What a call does at RUN time is not this test's business: a revoke of a
// member that is not there may fail, as long as it fails as a revoke.
//
// The last test keeps the list whole: a command spelled out at a `run`-like
// call site, instead of built in calls.ts, fails it.

import test, { describe } from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { File as MemFile, OpenFile, WASI } from '@bjorn3/browser_wasi_shim';
import { resolveLibAsSource } from './support/libhook.mjs';

resolveLibAsSource(import.meta.url);
const calls = await import('../src/lib/calls.ts');
const { DB_FILES, MemoryHandle, opfsDirectory, Sink, staged } = await import('../src/lib/wasiFs.ts');
const { paneArgv } = await import('../src/lib/panes.ts');
const { DEFAULT_STATS, statsArgv } = await import('../src/lib/stats.ts');

const web = fileURLToPath(new URL('..', import.meta.url));
const module = await WebAssembly.compile(readFileSync(join(web, 'src/lib/wasm/datum-cli.wasm')));

/** `datum <argv>` on `handles`, as `runners/wasi.ts` runs it. */
function datum(handles, argv, { stdin = '', stage, client = 'calls' } = {}) {
	const stdout = new Sink();
	const stderr = new Sink();
	const dir = opfsDirectory('/data', handles, stage);
	const fds = [new OpenFile(new MemFile(new TextEncoder().encode(stdin))), stdout, stderr, dir];
	const vars = ['HOME=/data', 'DATUM_HOME=/data', 'DATUM_DB=/data/datum.db', 'TZ=UTC', `DATUM_CLIENT_ID=${client}`, 'DATUM_COLOR=1'];
	const wasi = new WASI(['datum', ...argv], vars, fds, { debug: false });
	const code = wasi.start(new WebAssembly.Instance(module, { wasi_snapshot_preview1: wasi.wasiImport }));
	return { code, stdout: stdout.text(), stderr: stderr.text(), stage: staged(dir) };
}

const bytes = (handle) => {
	const buffer = new Uint8Array(handle.getSize());
	handle.read(buffer, { at: 0 });
	return buffer;
};
/** A copy of every file, so one call cannot change what the next one sees. */
function copy(handles) {
	return new Map(
		[...handles].map(([file, handle]) => {
			const fresh = new MemoryHandle();
			fresh.write(bytes(handle), { at: 0 });
			return [file, fresh];
		})
	);
}

// ---- the seeded database -----------------------------------------------------

const PLAN = { name: 'nightly', kind: 's3', config: {}, dbs: ['datum'], every: 86_400, slots: 7 };
const LAYOUT = { version: 1, name: 'desk', root: { kind: 'leaf' }, designedFor: 'desktop' };
const seeded = new Map(DB_FILES.map((file) => [file, new MemoryHandle()]));
const seed = (argv, opts) => {
	const r = datum(seeded, argv, opts);
	assert.equal(r.code, 0, `seed: datum ${argv.join(' ')}: ${r.stderr}`);
	return r;
};
seed(['start', 'work', '30m', '-t', '-240']);
seed(['occur', 'coffee', '-t', '-180']);
seed(['switch', 'mood', 'happy', '-t', '-120']);
seed(['switch', 'mood', 'calm', '-t', '-60']);
seed(['start', 'walk']);
seed(['track', 'coffee', '--verb', 'occur']);
seed(['alias', 'set', 'cof', 'occur coffee']);
seed(calls.savePlanArgv(PLAN, false));
seed(calls.newLayoutArgv(LAYOUT, 'k'));
seed(calls.updateLayoutArgv({ ...LAYOUT, root: { kind: 'split' } }, 'k'));
// Started by another client: `sync status` lists it as a member besides this copy.
seed(calls.newInviteArgv(), { client: 'phone' });
const STAGE = seed(calls.backupToArgv(PLAN), { stage: {} }).stage;
const FILE = Object.keys(STAGE).find((file) => file !== 'index.json');
const BACKUP = seed(calls.backupArgv()).stdout;
const KIT = readFileSync(join(web, '../kits/sample.toml'), 'utf8');
const MEMBER = JSON.parse(seed(calls.syncStatusArgv()).stdout).members[0].endpoint;
const HID = JSON.parse(seed(calls.layoutsArgv()).stdout).rows.find((r) => r.data?.name).hid;

// ---- what each call is asked with ------------------------------------------------

/** Arguments for every call that takes some: one list per run. A call with none is run once, bare. */
const SAMPLES = {
	helpArgv: [[], ['db backup']],
	lastDotsArgv: [['coffee']],
	tailArgv: [[{}], [{ field: 'mood', count: 5, before: new Date().toISOString() }]],
	showAllArgv: [[statsArgv(DEFAULT_STATS)], [statsArgv(DEFAULT_STATS, 'mood')]],
	statsArgv: [[DEFAULT_STATS], [{ range: 'all' }, 'coffee']],
	specShowArgv: [['coffee']],
	bandArgv: [[400, 40], [400, 40, 3]],
	geometryArgv: [[400, 40]],
	nowviewSpec: [['30m'], ['0', new Date().toISOString(), true]],
	pressArgv: [
		[{ field: 'coffee', shape: { kind: 'point' } }],
		[{ field: 'walk', shape: { kind: 'state' }, current: true }],
		[{ field: 'work', shape: { kind: 'state' }, current: false }],
		[{ field: 'mood', shape: { kind: 'switch' } }, 'happy']
	],
	trackArgv: [
		['tea', 'occur'],
		[
			'tea',
			'switch',
			[{ name: 'hot', color: '#ff0000' }],
			'#00ff00',
			[
				{ name: 'cups', kind: 'default', value: '1' },
				{ name: 'brand', kind: 'required', value: '' },
				{ name: 'note', kind: 'optional', value: '' },
				{ name: 'size', kind: 'assign', value: 'big' }
			]
		]
	],
	specArgv: [[{ field: 'coffee', verb: 'occur', color: '#123456', description: 'beans', allowed_states: null, state_colors: {} }]],
	renameArgv: [['phone'], ['phone', MEMBER]],
	revokeArgv: [[MEMBER]],
	purgeArgv: [[MEMBER], [MEMBER, true]],
	ackArgv: [['unlock'], [MEMBER]],
	adminAddArgv: [['not-a-key']],
	adminRemoveArgv: [['not-a-key'], ['not-a-key', true]],
	approveArgv: [['https://datum.day/sync/approve#nothing']],
	qrArgv: [['hello']],
	pausedArgv: [[true], [false]],
	dbPurgeArgv: [[]],
	restorePreviewArgv: [[calls.restoreArgv()], [[...calls.restoreArgv(), '--hard']], [calls.restoreStagedArgv(FILE)]],
	restoreStagedArgv: [[FILE]],
	savePlanArgv: [[PLAN, false], [PLAN, true]],
	deletePlanArgv: [[PLAN.name]],
	backupToArgv: [[PLAN]],
	planReachArgv: [[PLAN, 86_400]],
	newLayoutArgv: [[{ ...LAYOUT, name: 'phone' }], [{ ...LAYOUT, name: 'phone' }, 'k']],
	updateLayoutArgv: [[LAYOUT], [LAYOUT, 'k']],
	layoutHistoryArgv: [[LAYOUT.name]],
	layoutUndoArgv: [[LAYOUT.name], [LAYOUT.name, 1]],
	deleteLayoutArgv: [[HID]],
	deleteSampleArgv: [[[HID]]]
};

/** What a call that reads its standard input is given. */
const STDIN = { mergeArgv: BACKUP, restoreArgv: BACKUP, restorePreviewArgv: BACKUP, kitInstallArgv: KIT };

/**
 * Run on a database of its own: an invite takes a member, and the seeded
 * copy is none — a client of its own re-keys it — so it starts a chain, as
 * a first invite does.
 */
const FRESH = new Set(['newInviteArgv']);

/** A pane's box, which a sized pane's argv is given as `CliPane.svelte` gives it. */
const BOX = { width: 600, height: 300 };

/** Every argv `name` is sent as. */
function argvsOf(name, value) {
	const runs = typeof value === 'function' ? (SAMPLES[name] ?? (value.length ? [] : [[]])) : [[]];
	assert.ok(runs.length, `${name} takes arguments: give it a sample in SAMPLES`);
	return runs.map((args) => {
		const built = typeof value === 'function' ? value(...args) : value;
		return Array.isArray(built) ? built : paneArgv(built.argv, BOX);
	});
}

const isJson = (argv) => argv.includes('--json') || argv.some((w, i) => w === '--show' && /^(json|all)$/.test(argv[i + 1]));

/** The JSON in `stdout`: all of it, or — after `--show all`'s table — its last line; null for none. */
function parse(stdout) {
	for (const text of [stdout, stdout.trimEnd().split('\n').at(-1)]) {
		try {
			return JSON.parse(text);
		} catch {
			// Not this one.
		}
	}
	return null;
}

/** Whether `path` (`READS`' spelling) is found in `value`: some element, for `[]`. */
function has(value, [segment, ...rest]) {
	if (segment === undefined) return true;
	const many = segment.endsWith('[]');
	const key = many ? segment.slice(0, -2) : segment;
	if (key && !(value !== null && typeof value === 'object' && key in value)) return false;
	const next = key ? value[key] : value;
	return many ? Array.isArray(next) && next.some((item) => has(item, rest)) : has(next, rest);
}

// ---- where each call is sent from -------------------------------------------------

const src = join(web, 'src');
const sources = readdirSync(src, { recursive: true })
	.filter((file) => /\.(ts|svelte)$/.test(file) && !file.includes('wasm/'))
	.map((file) => ({ file: relative(web, join(src, file)), text: readFileSync(join(src, file), 'utf8') }));
const sitesOf = (name) =>
	sources.filter(({ file, text }) => !file.endsWith('lib/calls.ts') && new RegExp(`\\b${name}\\b`).test(text)).map(({ file }) => file);

const USAGE = /^Error: (Invalid argument|Missing required key|Extra data with no matching key)/m;
const named = Object.entries(calls).filter(([name]) => name !== 'READS');

describe('every call the page sends', () => {
	for (const [name, value] of named) {
		test(name, () => {
			const sites = sitesOf(name);
			assert.ok(sites.length, `${name} is exported by calls.ts and sent from nowhere`);
			const at = `${name} (sent from ${sites.join(', ')})`;
			const answers = [];
			for (const argv of argvsOf(name, value)) {
				const handles = FRESH.has(name) ? new Map(DB_FILES.map((file) => [file, new MemoryHandle()])) : copy(seeded);
				const r = datum(handles, argv, { stdin: STDIN[name] ?? '', stage: { ...STAGE } });
				assert.doesNotMatch(r.stderr, USAGE, `${at}: datum ${argv.join(' ')}`);
				if (isJson(argv)) {
					assert.ok(calls.READS[name], `${at} asks for JSON: name what the page reads of it in READS`);
					assert.equal(r.code, 0, `${at}: datum ${argv.join(' ')}: ${r.stderr}`);
				}
				// Some answer JSON unasked (`spec show`, `sync new-invite`), and some samples do not (a purge's `--yes`).
				if (calls.READS[name] && r.code === 0) answers.push(parse(r.stdout));
			}
			for (const path of calls.READS[name] ?? [])
				assert.ok(answers.some((answer) => has(answer, path.split('.'))), `${at}: no answer has «${path}», which the page reads`);
		});
	}

	// The check above is only as good as USAGE: a CLI that said it otherwise would pass every call.
	test('a flag, subcommand or argument the CLI does not take is seen as a usage error', () => {
		for (const argv of [['head', '--metrik', 'occur'], ['sync', 'chainz', 'list'], ['sync', 'chain', 'rename', 'a', 'b', 'c'], ['spec', 'show']])
			assert.match(datum(copy(seeded), argv).stderr, USAGE, `datum ${argv.join(' ')}`);
	});

	test('READS names only calls', () => {
		for (const name of Object.keys(calls.READS)) assert.ok(name in calls, `READS.${name} is no call`);
	});
});

test('no call site spells a command out instead of building it in calls.ts', () => {
	const grammar = JSON.parse(datum(copy(seeded), calls.grammarArgv()).stdout);
	const words = new Set(['help', ...grammar.flatMap((c) => [c.name, ...c.aliases])]);
	// The page's ways of running argv: `run` itself, and the wrappers over it.
	const site =
		/\b(?:(?:run|cli|must|step|feed|stdoutOf|internal|datum|data|restoreFile|json(?:<[^>]*>)?)\(|syncAct\([^,()]+,|exec\([^[()]*)\s*\[?\s*(?:'--db',\s*[^,]+,\s*)?'([a-z][a-z-]*)'/g;
	const spelled = sources.flatMap(({ file, text }) =>
		file.endsWith('lib/calls.ts')
			? []
			: [...text.matchAll(site)].filter((m) => words.has(m[1])).map((m) => `${file}:${text.slice(0, m.index).split('\n').length}: ${m[0]}`)
	);
	assert.deepEqual(spelled, []);
});
