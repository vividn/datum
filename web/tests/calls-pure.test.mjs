// Every command the page sends on its own still parses, and its JSON still
// has the keys the page reads — in node, with no browser (da-1y9gi).
//
// vividn, 2026-10-08: «a (quick) extra test to make sure every wasm call that
// the frontend is valid … could prevent drift when for example the rust api
// changes». The calls are `$lib/calls.ts`'s exports; each runs, with the
// samples below, through the same wasm CLI the page runs, on a copy of one
// seeded database, as `runners/wasi.ts` runs it. A usage error — a flag,
// subcommand or argument the CLI no longer takes — fails it, and so does a
// key the page reads that no answer carries, naming the files that send it.
// The keys are every one `Reads` — the types the call sites parse each
// answer into — names, walked by the TypeScript checker (da-qy4w0).
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
import ts from 'typescript';
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
	const vars = ['HOME=/data', 'DATUM_HOME=/data', 'DATUM_DB=/data/datum.db', 'TZ=UTC', `DATUM_CLIENT_ID=${client}`, 'FORCE_COLOR=1'];
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

// ---- what the page reads -------------------------------------------------------

/**
 * `Reads`, as each call's paths: dotted, with `[]` after a segment for any
 * element of an array (a leading one: the answer itself is one) and `{}`
 * for any value of a map. Every key of every type, nested — each member
 * of a union's — but under a property tagged `@shallow`; an array or a map
 * of plain values is its key alone, and a type met again inside itself
 * stops there. A type the checker cannot see into (`any`: an import it
 * could not resolve) fails, rather than passing with nothing to check.
 */
function readsOf(file) {
	const program = ts.createProgram([file], {
		strict: true,
		noEmit: true,
		skipLibCheck: true,
		target: ts.ScriptTarget.ESNext,
		module: ts.ModuleKind.ESNext,
		moduleResolution: ts.ModuleResolutionKind.Bundler,
		paths: { '$lib/*': [join(web, 'src/lib/*')] }
	});
	const checker = program.getTypeChecker();
	const exported = checker.getExportsOfModule(checker.getSymbolAtLocation(program.getSourceFile(file)));
	const reads = checker.getDeclaredTypeOfSymbol(exported.find((s) => s.name === 'Reads'));
	let call;
	const walk = (type, key, seen) => {
		type = checker.getNonNullableType(type);
		assert.ok(!(type.flags & ts.TypeFlags.Any), `Reads.${call}: «${key}» is any`);
		if (type.isUnion()) return [...new Set(type.types.flatMap((t) => walk(t, key, seen)))];
		if (seen.includes(type)) return [key];
		const keyed = type.flags & ts.TypeFlags.Object || type.isIntersection();
		const props = keyed ? checker.getPropertiesOfType(type) : [];
		// An array's elements, or the values of a map with no key of its own: named only where they have keys.
		const each = checker.isArrayType(type)
			? [checker.getTypeArguments(type)[0], '[]']
			: keyed && !props.length && checker.getIndexInfosOfType(type)[0]
				? [checker.getIndexInfosOfType(type)[0].type, '{}']
				: null;
		if (each) {
			const paths = walk(each[0], key + each[1], [...seen, type]);
			return paths.length === 1 && paths[0] === key + each[1] ? [key] : paths;
		}
		if (!props.length) return [key];
		return props
			.flatMap((p) =>
				p.getJsDocTags().some((tag) => tag.name === 'shallow')
					? [p.name]
					: walk(checker.getTypeOfSymbol(p), p.name, [...seen, type])
			)
			.map((path) => (key ? `${key}.${path}` : path));
	};
	return Object.fromEntries(
		checker.getPropertiesOfType(reads).map((p) => [(call = p.name), walk(checker.getTypeOfSymbol(p), '', [])])
	);
}
const READS = readsOf(join(web, 'src/lib/calls.ts'));

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
// Every key a spec leaves out when empty, on a spec that has it.
const SPEC = { verb: 'occur', color: null, description: null, allowed_states: null, state_colors: {} };
const SIZE = { required: false, value_type: 'string', in_slug: false, default: 'big' };
seed(
	calls.specArgv({
		...SPEC,
		field: 'coffee',
		glyph: 'c',
		glyph_by: { key: 'size', map: { big: 'C' } },
		icon: '<svg viewBox="0 0 1 1"/>',
		remainder: 'note',
		string_remainder: true,
		subfields: { size: SIZE }
	})
);
seed(calls.specArgv({ ...SPEC, field: 'mood', verb: 'switch', never_false: true, relative_to: 'coffee', state_descriptions: { happy: 'up' }, state_glyphs: { happy: 'h' }, state_lookup_field: 'feeling', comment_remainder: true }));
seed(['alias', 'set', 'cof', 'occur coffee']);
seed(calls.savePlanArgv(PLAN, false));
seed(calls.newLayoutArgv(LAYOUT, 'k'));
seed(calls.updateLayoutArgv({ ...LAYOUT, root: { kind: 'split' } }, 'k'));
// Started by another client: `sync status` lists it as a member besides this copy.
seed(calls.newInviteArgv(), { client: 'phone' });
// An admin it adds and removes, which leaves a notice of the unlock, and one
// it adds again, locking it: each from a key file made in the stage.
const KEYS = seed(calls.adminAddArgv('/data/stage/first.key'), { client: 'phone', stage: {} }).stage;
seed([...calls.adminRemoveArgv('first.key', true), '--admin-key', '/data/stage/first.key'], { client: 'phone', stage: KEYS });
seed(calls.adminAddArgv('/data/stage/admin.key'), { client: 'phone', stage: {} });
const STAGE = seed(calls.backupToArgv(PLAN), { stage: {} }).stage;
const FILE = Object.keys(STAGE).find((file) => file !== 'index.json');
const BACKUP = seed(calls.backupArgv()).stdout;
const KIT = readFileSync(join(web, '../kits/sample.toml'), 'utf8');
// A kit with a widget, and a chore it lists, done today so its row is painted.
seed([...calls.kitInstallArgv(), '--yes'], { stdin: readFileSync(join(web, '../kits/chores.toml'), 'utf8') });
seed(['newchore', 'vacuum', '7']);
seed(['vacuum']);
const MEMBER = JSON.parse(seed(calls.syncStatusArgv()).stdout).members[0].endpoint;
const HID = JSON.parse(seed(calls.layoutsArgv()).stdout).rows.find((r) => r.data?.name).hid;

// ---- what each call is asked with ------------------------------------------------

/** Arguments for every call that takes some: one list per run. A call with none is run once, bare. */
const SAMPLES = {
	helpArgv: [[], ['db backup']],
	tailArgv: [[{}], [{ field: 'mood', count: 5, above: HID }]],
	showAllArgv: [[statsArgv(DEFAULT_STATS)], [statsArgv(DEFAULT_STATS, 'mood')], [statsArgv({ range: 'all' }, 'coffee')]],
	statsArgv: [[DEFAULT_STATS], [{ range: 'all' }, 'coffee']],
	specShowArgv: [['coffee'], ['mood']],
	renameFieldArgv: [['coffee', 'drink', true], ['mood', 'feeling']],
	bandArgv: [[400, 40], [400, 40, 3]],
	geometryArgv: [[400, 40]],
	nowviewSpec: [['30m'], ['0', new Date().toISOString(), true]],
	pressArgv: [
		[{ field: 'coffee', shape: { kind: 'point' } }],
		[{ field: 'walk', shape: { kind: 'state' }, current: true }],
		[{ field: 'work', shape: { kind: 'state' }, current: false }],
		[{ field: 'mood', shape: { kind: 'switch' } }, 'happy']
	],
	declareArgv: [
		['tea', { verb: 'occur', color: '', description: '', states: [] }],
		[
			'tea',
			{
				verb: 'switch',
				color: '#00ff00',
				description: '',
				states: [{ name: 'hot', color: '#ff0000', description: '' }],
				keys: [
					{ name: 'cups', kind: 'default', value: '1', value_type: 'number', in_slug: false },
					{ name: 'brand', kind: 'required', value: '', value_type: 'any', in_slug: true },
					{ name: 'note', kind: 'optional', value: '', value_type: 'any', in_slug: false },
					{ name: 'size', kind: 'assign', value: 'big', value_type: 'any', in_slug: false },
					{ name: 'words', kind: 'optional', value: '', value_type: 'any', in_slug: false, rest: 'SR' }
				]
			},
			true
		]
	],
	specArgv: [[{ field: 'coffee', verb: 'occur', color: '#123456', description: 'beans', allowed_states: null, state_colors: {} }]],
	renameArgv: [['phone'], ['phone', MEMBER]],
	unsetRenameArgv: [[]],
	labelCheckArgv: [['kitchen']],
	revokeArgv: [[MEMBER]],
	revertArgv: [[MEMBER], [MEMBER, true]],
	ackArgv: [['unlock'], [MEMBER]],
	adminAddArgv: [['not-a-key']],
	adminRemoveArgv: [['not-a-key'], ['not-a-key', true]],
	approveArgv: [['https://datum.day/sync/approve#nothing']],
	qrArgv: [['hello']],
	pausedArgv: [[true], [false]],
	dbPurgeArgv: [[]],
	dbPurgeProbeArgv: [[], [['--leave', '--force']], [['--rejoin']]],
	dbPurgeChainArgv: [[], [true]],
	restorePreviewArgv: [[calls.restoreArgv()], [[...calls.restoreArgv(), '--hard', '--leave']], [[...calls.restoreArgv(), '--hard', '--chain']], [calls.restoreStagedArgv(FILE)]],
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
	deleteSampleArgv: [[[HID]]],
	kitWidgetArgv: [['chorelist'], ['chorelist --all']]
};

/** What a call that reads its standard input is given. */
const STDIN = { mergeArgv: BACKUP, restoreArgv: BACKUP, restorePreviewArgv: BACKUP, kitInstallArgv: KIT };

/**
 * Run on a database of its own: an invite takes a member, and the seeded
 * copy is none — a client of its own re-keys it — so it starts a chain, as
 * a first invite does.
 */
const FRESH = new Set(['newInviteArgv']);

/**
 * Run on a database of conflicts (da-6cutb): `debug conflicts`, which the
 * seeded copy — in a chain, holding real entries — refuses.
 */
const CONFLICTED = new Map(DB_FILES.map((file) => [file, new MemoryHandle()]));
assert.equal(datum(CONFLICTED, ['debug', 'conflicts', '1']).code, 0);
const CLASH = JSON.parse(datum(CONFLICTED, calls.conflictsArgv()).stdout).find((e) => e.conflicts.state).hid;
SAMPLES.resolveArgv = [[CLASH], [CLASH, 'merge'], [CLASH, '', { state: 'b' }], [CLASH, 'use-a', { state: '"typed"' }]];
const ON_CONFLICTED = new Set(['conflictsArgv', 'resolveArgv']);

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

/** The values `segment` names in `value`: a key's, every element's for `[]`, every value's for `{}`. */
function at(value, segment) {
	const [, inner, of] = segment.match(/^(.*?)(\[\]|\{\})?$/);
	const keyed = (v) => v !== null && typeof v === 'object';
	if (!of) return !inner ? [value] : keyed(value) && inner in value ? [value[inner]] : [];
	return at(value, inner).flatMap((v) => (of === '[]' ? (Array.isArray(v) ? v : []) : keyed(v) && !Array.isArray(v) ? Object.values(v) : []));
}

/** Whether `path` (`readsOf`'s spelling) is found in `value`: in some element or value, for `[]` and `{}`. */
const has = (value, [segment, ...rest]) => segment === undefined || at(value, segment).some((v) => has(v, rest));

// ---- where each call is sent from -------------------------------------------------

const src = join(web, 'src');
const sources = readdirSync(src, { recursive: true })
	.filter((file) => /\.(ts|svelte)$/.test(file) && !file.includes('wasm/'))
	.map((file) => ({ file: relative(web, join(src, file)), text: readFileSync(join(src, file), 'utf8') }));
const sitesOf = (name) =>
	sources.filter(({ file, text }) => !file.endsWith('lib/calls.ts') && new RegExp(`\\b${name}\\b`).test(text)).map(({ file }) => file);

const USAGE = /^Error: (Invalid argument|Missing required key|Extra data with no matching key)/m;
const named = Object.entries(calls);

describe('every call the page sends', () => {
	for (const [name, value] of named) {
		test(name, () => {
			const sites = sitesOf(name);
			assert.ok(sites.length, `${name} is exported by calls.ts and sent from nowhere`);
			const at = `${name} (sent from ${sites.join(', ')})`;
			const answers = [];
			for (const argv of argvsOf(name, value)) {
				const handles = FRESH.has(name)
					? new Map(DB_FILES.map((file) => [file, new MemoryHandle()]))
					: copy(ON_CONFLICTED.has(name) ? CONFLICTED : seeded);
				const r = datum(handles, argv, { stdin: STDIN[name] ?? '', stage: { ...STAGE } });
				assert.doesNotMatch(r.stderr, USAGE, `${at}: datum ${argv.join(' ')}`);
				if (isJson(argv)) {
					assert.ok(READS[name], `${at} asks for JSON: name the type the page reads it as in Reads`);
					assert.equal(r.code, 0, `${at}: datum ${argv.join(' ')}: ${r.stderr}`);
				}
				// Some answer JSON unasked (`sync new-invite`), and some samples do not (a purge's `--yes`).
				if (READS[name] && r.code === 0) answers.push(parse(r.stdout));
			}
			const missing = (READS[name] ?? []).filter((path) => !answers.some((answer) => has(answer, path.split('.'))));
			assert.deepEqual(missing, [], `${at}: no answer has these keys, which the page reads`);
		});
	}

	// The check above is only as good as USAGE: a CLI that said it otherwise would pass every call.
	test('a flag, subcommand or argument the CLI does not take is seen as a usage error', () => {
		for (const argv of [['head', '--metrik', 'occur'], ['sync', 'chainz', 'list'], ['sync', 'chain', 'rename', 'a', 'b', 'c'], ['setup', 'show', 'mood']])
			assert.match(datum(copy(seeded), argv).stderr, USAGE, `datum ${argv.join(' ')}`);
	});

	test('Reads names only calls', () => {
		for (const name of Object.keys(READS)) assert.ok(name in calls, `Reads.${name} is no call`);
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
