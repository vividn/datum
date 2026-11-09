// datum.day/install.sh (static/install.sh, da-vpbg3), run by `sh` in a
// scratch HOME against a stub release: a `file://` directory holding a
// script per platform and a SHA256SUMS, and `uname` stubbed on PATH to be
// the machine asked about. What is checked is which binary it picks and
// that it installs nothing whose checksum does not match. `minisign` is
// stubbed too, on a PATH entry of its own (`signer`): it says what it was
// asked, and passes a signature unless STUB_BAD_SIGNATURE is set and the
// .minisig is there, so a test leaving `signer` off PATH is a machine
// without minisign.

import test, { describe } from 'node:test';
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { createServer } from 'node:http';
import { createHash } from 'node:crypto';
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const script = fileURLToPath(new URL('../static/install.sh', import.meta.url));
const ASSETS = ['datum-x86_64-linux', 'datum-aarch64-linux', 'datum-x86_64-macos', 'datum-aarch64-macos'];
const sha256 = (text) => createHash('sha256').update(text).digest('hex');

// A binary that says which asset it is when asked for its version, and
// what else it was asked.
const binary = (asset) =>
	`#!/bin/sh\ncase "$1" in --version) echo "datum 3.0.1-beta (${asset})" ;; *) echo "datum ran: $*" ;; esac\n`;

function scratch({ sums = (asset) => sha256(binary(asset)) } = {}) {
	const root = mkdtempSync(join(tmpdir(), 'datum-install-'));
	const release = join(root, 'release');
	const bin = join(root, 'bin');
	mkdirSync(release);
	mkdirSync(bin);
	mkdirSync(join(root, 'home'));
	// Each binary under its build's name, as release/publish.sh uploads it.
	for (const asset of ASSETS) writeFileSync(join(release, `${asset}-0123abc`), binary(asset));
	const lines = ASSETS.map((asset) => [sums(asset), asset]).filter(([sum]) => sum);
	writeFileSync(join(release, 'SHA256SUMS'), lines.map(([sum, asset]) => `${sum}  ${asset}-0123abc\n`).join(''));
	writeFileSync(join(release, 'SHA256SUMS-0123abc.minisig'), 'a signature\n');
	writeFileSync(join(bin, 'uname'), '#!/bin/sh\ncase "$1" in -s) echo "$STUB_S" ;; -m) echo "$STUB_M" ;; esac\n');
	chmodSync(join(bin, 'uname'), 0o755);
	const signer = join(root, 'signer');
	mkdirSync(signer);
	writeFileSync(join(signer, 'minisign'), '#!/bin/sh\necho "minisign $*" >&2\n[ -f "$3.minisig" ] && [ -z "$STUB_BAD_SIGNATURE" ]\n');
	chmodSync(join(signer, 'minisign'), 0o755);
	return { root, release, bin, signer, home: join(root, 'home') };
}

function install(box, s, m, env = {}) {
	const result = spawnSync('sh', [script, '--no-completions'], {
		encoding: 'utf8',
		env: {
			PATH: `${box.bin}:${box.signer}:${process.env.PATH}`,
			HOME: box.home,
			XDG_CONFIG_HOME: join(box.home, '.config'),
			DATUM_RELEASE_URL: `file://${box.release}`,
			STUB_S: s,
			STUB_M: m,
			...env
		}
	});
	return { ...result, installed: join(env.DATUM_INSTALL_DIR ?? join(box.home, '.local/bin'), 'datum') };
}

describe('install.sh picks the binary for the machine', () => {
	for (const [s, m, asset] of [
		['Linux', 'x86_64', 'datum-x86_64-linux'],
		['Linux', 'amd64', 'datum-x86_64-linux'],
		['Linux', 'aarch64', 'datum-aarch64-linux'],
		['Darwin', 'arm64', 'datum-aarch64-macos'],
		['Darwin', 'x86_64', 'datum-x86_64-macos']
	]) {
		test(`${s} ${m} gets ${asset}`, () => {
			const box = scratch();
			const run = install(box, s, m);
			assert.equal(run.status, 0, run.stderr);
			assert.equal(readFileSync(run.installed, 'utf8'), binary(asset));
			// It names what it installed and where, and that the place is not on PATH.
			for (const fact of [`datum 3.0.1-beta (${asset})`, run.installed]) assert.ok(run.stdout.includes(fact), `${fact}: ${run.stdout}`);
			assert.match(run.stdout, /\bPATH\b/);
		});
	}

	test('the binary then sets itself up, with the arguments install.sh was given', () => {
		const run = install(scratch(), 'Linux', 'x86_64');
		assert.equal(run.status, 0, run.stderr);
		assert.match(run.stdout, /^datum ran: install --no-completions$/m);
	});

	test('$DATUM_INSTALL_DIR is where it goes, and on PATH it says nothing of PATH', () => {
		const box = scratch();
		const dir = join(box.root, 'elsewhere');
		const run = install(box, 'Linux', 'x86_64', { DATUM_INSTALL_DIR: dir, PATH: `${box.bin}:${box.signer}:${dir}:${process.env.PATH}` });
		assert.equal(run.status, 0, run.stderr);
		assert.equal(run.installed, join(dir, 'datum'));
		assert.equal(readFileSync(run.installed, 'utf8'), binary('datum-x86_64-linux'));
		assert.doesNotMatch(run.stdout, /PATH/);
	});

	for (const [s, m] of [
		['FreeBSD', 'x86_64'],
		['Linux', 'riscv64']
	]) {
		test(`${s} ${m} is refused, naming the build from a checkout`, () => {
			const box = scratch();
			const run = install(box, s, m);
			assert.notEqual(run.status, 0);
			assert.ok(run.stderr.includes(`${s} ${m}`), `it names the machine: ${run.stderr}`);
			assert.match(run.stderr, /\bcheckout\b/, 'and where to build one');
			assert.equal(existsSync(run.installed), false);
		});
	}
});

describe('install.sh tries a download again', () => {
	// da-dbe3g: GitHub answered 504 for the latest SHA256SUMS on and off for
	// hours. The release served over HTTP, each file's first request refused
	// with a 503, and the install gets through on the second.
	test('a 503 answered once is tried again, for each file', async () => {
		const box = scratch();
		const refused = [];
		const server = createServer((req, res) => {
			if (!refused.includes(req.url)) {
				refused.push(req.url);
				return res.writeHead(503).end();
			}
			const file = join(box.release, decodeURIComponent(req.url));
			if (!existsSync(file)) return res.writeHead(404).end();
			res.writeHead(200).end(readFileSync(file));
		});
		await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
		try {
			const sh = spawn('sh', [script, '--no-completions'], {
				env: {
					PATH: `${box.bin}:${box.signer}:${process.env.PATH}`,
					HOME: box.home,
					XDG_CONFIG_HOME: join(box.home, '.config'),
					DATUM_RELEASE_URL: `http://127.0.0.1:${server.address().port}`,
					STUB_S: 'Linux',
					STUB_M: 'x86_64'
				}
			});
			let stderr = '';
			sh.stderr.on('data', (d) => (stderr += d));
			const status = await new Promise((resolve) => sh.on('close', resolve));
			assert.equal(status, 0, stderr);
			assert.equal(readFileSync(join(box.home, '.local/bin/datum'), 'utf8'), binary('datum-x86_64-linux'));
			assert.deepEqual(refused, ['/SHA256SUMS', '/SHA256SUMS-0123abc.minisig', '/datum-x86_64-linux-0123abc']);
		} finally {
			server.close();
			rmSync(box.root, { recursive: true, force: true });
		}
	});
});

describe('install.sh refuses a binary SHA256SUMS does not vouch for', () => {
	test('a checksum mismatch installs nothing and keeps the datum already there', () => {
		const box = scratch({ sums: (asset) => sha256(asset === 'datum-x86_64-linux' ? 'something else' : binary(asset)) });
		const old = join(box.home, '.local/bin/datum');
		mkdirSync(join(box.home, '.local/bin'), { recursive: true });
		writeFileSync(old, 'the old one');
		const run = install(box, 'Linux', 'x86_64');
		assert.notEqual(run.status, 0);
		// It names the binary and both checksums: the one it got and the one SHA256SUMS lists.
		for (const fact of ['datum-x86_64-linux', sha256(binary('datum-x86_64-linux')), sha256('something else')]) {
			assert.ok(run.stderr.includes(fact), `${fact}: ${run.stderr}`);
		}
		assert.equal(readFileSync(old, 'utf8'), 'the old one');
	});

	test('a release from before build names, listing only the fixed names, installs', () => {
		const box = scratch();
		for (const asset of ASSETS) writeFileSync(join(box.release, asset), binary(asset));
		writeFileSync(join(box.release, 'SHA256SUMS'), ASSETS.map((asset) => `${sha256(binary(asset))}  ${asset}\n`).join(''));
		writeFileSync(join(box.release, 'SHA256SUMS.minisig'), 'a signature\n');
		const run = install(box, 'Linux', 'x86_64');
		assert.equal(run.status, 0, run.stderr);
		assert.equal(readFileSync(run.installed, 'utf8'), binary('datum-x86_64-linux'));
	});

	test('a binary SHA256SUMS does not list installs nothing', () => {
		const box = scratch({ sums: (asset) => (asset === 'datum-aarch64-macos' ? null : sha256(binary(asset))) });
		const run = install(box, 'Darwin', 'arm64');
		assert.notEqual(run.status, 0);
		assert.match(run.stderr, /\bdatum-aarch64-macos\b/, 'it names the binary SHA256SUMS lacks');
		assert.equal(existsSync(run.installed), false);
	});
});

// The pinned key is release/minisign.pub's; datum-commands' install.rs
// holds every copy of it to that file.
const KEY = readFileSync(fileURLToPath(new URL('../../release/minisign.pub', import.meta.url)), 'utf8').split('\n')[1];
const hostMinisign = spawnSync('sh', ['-c', 'command -v minisign']).status === 0;

describe('install.sh checks SHA256SUMS is signed by the release key, with minisign', () => {
	test('it asks minisign with the pinned key, and installs on a good signature', () => {
		const run = install(scratch(), 'Linux', 'x86_64');
		assert.equal(run.status, 0, run.stderr);
		assert.match(run.stderr, new RegExp(`^minisign -Vq -m \\S+/SHA256SUMS -P ${KEY.replace(/[+/]/g, '\\$&')}$`, 'm'));
		assert.equal(existsSync(run.installed), true);
	});

	test('a signature that does not verify installs nothing and keeps the datum already there', () => {
		const box = scratch();
		const old = join(box.home, '.local/bin/datum');
		mkdirSync(join(box.home, '.local/bin'), { recursive: true });
		writeFileSync(old, 'the old one');
		const run = install(box, 'Linux', 'x86_64', { STUB_BAD_SIGNATURE: '1' });
		assert.notEqual(run.status, 0);
		assert.match(run.stderr, /\bSHA256SUMS\b.*\bsignature\b/, 'it names what failed to verify');
		assert.equal(readFileSync(old, 'utf8'), 'the old one');
	});

	test('a release with no signature installs nothing', () => {
		const box = scratch();
		rmSync(join(box.release, 'SHA256SUMS-0123abc.minisig'));
		const run = install(box, 'Linux', 'x86_64');
		assert.notEqual(run.status, 0);
		assert.match(run.stderr, /\S+\/SHA256SUMS-0123abc\.minisig\b/, "it names the build's signature it could not download");
		assert.equal(existsSync(run.installed), false);
	});

	test('without minisign it says so, and how to check by hand, and installs', { skip: hostMinisign && 'minisign is installed here' }, () => {
		const box = scratch();
		const run = install(box, 'Linux', 'x86_64', { PATH: `${box.bin}:${process.env.PATH}` });
		assert.equal(run.status, 0, run.stderr);
		assert.ok(run.stderr.includes(`minisign -Vm SHA256SUMS -x SHA256SUMS-0123abc.minisig -P ${KEY}`), `the command to check it by hand: ${run.stderr}`);
		assert.equal(existsSync(run.installed), true);
	});
});

// release/publish.sh (da-mzhau) against a fake `gh` holding the release in a
// directory: each uploaded asset a file under release/, which install.sh
// reads, and assets.json each asset's id, state and name, as GitHub lists
// them. The upload FAKE_GH_CUT names is cut short as GitHub's was: gh deletes
// the asset it clobbers, and the new one is left a hidden `starter`, which a
// later upload of that name fails on. minisign is faked too: a signature is
// the hash of what it signs, so a SHA256SUMS beside another's signature is
// refused.
const publishScript = fileURLToPath(new URL('../../release/publish.sh', import.meta.url));
const fakeGh = `#!${process.execPath}
const fs = require('fs'), path = require('path');
const dir = process.env.FAKE_GH, file = path.join(dir, 'assets.json'), release = path.join(dir, 'release');
const state = fs.existsSync(file) ? JSON.parse(fs.readFileSync(file, 'utf8')) : null;
const save = () => fs.writeFileSync(file, JSON.stringify(state));
const exit = (code, why) => { if (why) console.error(why); process.exit(code); };
const args = process.argv.slice(2);
if (args[0] === 'api') {
  const at = args.find((a, i) => i > 0 && !a.startsWith('-') && !['-X', '--jq'].includes(args[i - 1]));
  if (at.includes('/git/ref/tags/')) exit(0);
  if (!state) exit(1, 'HTTP 404');
  if (args.includes('DELETE')) {
    const i = state.assets.findIndex((a) => a.id === Number(at.split('/').pop()));
    if (i < 0) exit(1, 'HTTP 404');
    if (state.assets[i].state === 'uploaded') fs.rmSync(path.join(release, state.assets[i].name));
    state.assets.splice(i, 1);
    save();
    exit(0);
  }
  if (at.endsWith('/releases/tags/latest')) exit(0, void state.assets.forEach((a) => console.log([a.id, a.state, a.name].join('\\t'))));
} else if (args[0] === 'release' && ['view', 'edit'].includes(args[1])) {
  exit(state ? 0 : 1);
} else if (args[0] === 'release' && ['create', 'upload'].includes(args[1])) {
  if (args[1] === 'create' ? state : !state) exit(1, 'fake gh: ' + args.join(' '));
  const now = state ?? { next: 1, assets: [] };
  fs.mkdirSync(release, { recursive: true });
  for (const f of args.slice(3)) {
    if (f.startsWith('--')) break;
    const name = path.basename(f);
    const had = now.assets.findIndex((a) => a.name === name);
    if (had >= 0 && now.assets[had].state !== 'uploaded') exit(1, 'HTTP 422: ' + name + ' already_exists');
    if (had >= 0) {
      fs.rmSync(path.join(release, name));
      now.assets.splice(had, 1);
    }
    const id = now.next++;
    if (name === process.env.FAKE_GH_CUT) {
      now.assets.push({ id, name, state: 'starter' });
      fs.writeFileSync(file, JSON.stringify(now));
      exit(1, 'cut short: ' + name);
    }
    fs.copyFileSync(f, path.join(release, name));
    now.assets.push({ id, name, state: 'uploaded' });
    fs.writeFileSync(file, JSON.stringify(now));
  }
  // gh's own retry meeting an asset GitHub already took, FAKE_GH_TAKEN_TIMES times.
  const taken = process.env.FAKE_GH_TAKEN, count = path.join(dir, 'taken');
  const times = fs.existsSync(count) ? Number(fs.readFileSync(count, 'utf8')) : 0;
  if (taken && args.slice(3).some((f) => path.basename(f) === taken) && times < Number(process.env.FAKE_GH_TAKEN_TIMES ?? 1)) {
    fs.writeFileSync(count, String(times + 1));
    exit(1, 'HTTP 422: Validation Failed (ReleaseAsset.name already exists: ' + taken + ')');
  }
  exit(0);
}
exit(1, 'fake gh: ' + args.join(' '));
`;
const fakeMinisign = `#!${process.execPath}
const fs = require('fs'), crypto = require('crypto');
const a = process.argv.slice(2), val = (f) => a[a.indexOf(f) + 1];
const signed = 'signed ' + crypto.createHash('sha256').update(fs.readFileSync(val('-m'))).digest('hex') + '\\n';
if (a[0] === '-S') process.exit(void fs.writeFileSync(val('-x'), signed) ?? 0);
const sig = val('-m') + '.minisig';
process.exit(fs.existsSync(sig) && fs.readFileSync(sig, 'utf8') === signed ? 0 : 1);
`;

function releaseBox() {
	const box = scratch();
	box.fake = join(box.root, 'github');
	box.release = join(box.fake, 'release');
	mkdirSync(box.fake);
	writeFileSync(join(box.bin, 'gh'), fakeGh);
	writeFileSync(join(box.signer, 'minisign'), fakeMinisign);
	chmodSync(join(box.bin, 'gh'), 0o755);
	chmodSync(join(box.signer, 'minisign'), 0o755);
	return box;
}

// The binaries `sha` built, published to the box's release.
function publish(box, sha, env = {}, tag) {
	const dist = mkdtempSync(join(box.root, 'dist-'));
	for (const asset of ASSETS) writeFileSync(join(dist, asset), built(sha, asset));
	return spawnSync('bash', [publishScript, dist, ...(tag ? [tag] : [])], {
		encoding: 'utf8',
		env: {
			PATH: `${box.bin}:${box.signer}:${process.env.PATH}`,
			FAKE_GH: box.fake,
			GH_REPO: 'vividn/datum',
			GITHUB_SHA: sha,
			MINISIGN_SECRET_KEY: 'a key',
			...env
		}
	});
}
const built = (sha, asset = 'datum-x86_64-linux') => binary(`${asset} ${sha.slice(0, 7)}`);
const assetsOf = (box) => JSON.parse(readFileSync(join(box.fake, 'assets.json'), 'utf8')).assets;
const [A, B, C, D] = ['a', 'b', 'c', 'd'].map((c) => c.repeat(40));

describe('release/publish.sh moves installs to a build only once it is all up', () => {
	const installs = (box) => {
		const run = install(box, 'Linux', 'x86_64');
		assert.equal(run.status, 0, run.stderr);
		return readFileSync(run.installed, 'utf8');
	};
	const published = (box, sha, env, tag) => {
		const run = publish(box, sha, env, tag);
		assert.equal(run.status, 0, run.stdout + run.stderr);
		return run;
	};

	test('a full run moves installs to its build, and the fixed names a datum before build names reads', () => {
		const box = releaseBox();
		for (const sha of [A, B]) {
			published(box, sha);
			assert.equal(installs(box), built(sha));
		}
		assert.equal(readFileSync(join(box.release, 'datum-x86_64-linux'), 'utf8'), built(B));
		assert.match(readFileSync(join(box.release, 'SHA256SUMS'), 'utf8'), /^\S+ {2}datum-x86_64-linux$/m);
	});

	test('a run cut short uploading its binaries leaves the previous build installable', () => {
		const box = releaseBox();
		published(box, A);
		const run = publish(box, B, { FAKE_GH_CUT: `datum-x86_64-macos-${B.slice(0, 7)}` });
		assert.notEqual(run.status, 0);
		assert.equal(existsSync(join(box.release, `datum-x86_64-linux-${B.slice(0, 7)}`)), true, "this build's own binary went up");
		assert.equal(installs(box), built(A));
	});

	test('a cache serving the previous SHA256SUMS beside the new signatures installs the previous build', () => {
		const box = releaseBox();
		published(box, A);
		const stale = readFileSync(join(box.release, 'SHA256SUMS'));
		published(box, B);
		writeFileSync(join(box.release, 'SHA256SUMS'), stale);
		assert.equal(installs(box), built(A));
	});

	test("gh's retry meeting an asset already taken is run once more, and a second failure moves nothing", () => {
		const box = releaseBox();
		published(box, A);
		const taken = `datum-aarch64-macos-${B.slice(0, 7)}`;
		published(box, B, { FAKE_GH_TAKEN: taken });
		const ids = assetsOf(box);
		const id = (name) => ids.find((a) => a.name === name).id;
		const own = ids.filter((a) => a.name.endsWith(`-${B.slice(0, 7)}`)).map((a) => a.id);
		assert.equal(own.length, ASSETS.length, `this build's binaries: ${ids.map((a) => a.name)}`);
		assert.ok(Math.max(...own, id(`SHA256SUMS-${B.slice(0, 7)}.minisig`)) < id('SHA256SUMS'), 'SHA256SUMS after the build-named assets');
		assert.ok(id('SHA256SUMS') < id('SHA256SUMS.minisig'), 'and its signature after it');
		assert.equal(installs(box), built(B));

		const run = publish(box, C, { FAKE_GH_TAKEN: `datum-aarch64-macos-${C.slice(0, 7)}`, FAKE_GH_TAKEN_TIMES: '3' });
		assert.notEqual(run.status, 0);
		assert.match(run.stderr, /already exists/);
		assert.equal(readFileSync(join(box.fake, 'taken'), 'utf8'), '3', 'the upload ran twice more, and stopped');
		assert.equal(installs(box), built(B));
	});

	test("a kept release's upload, run again, meets gh's 422 once and goes on, and stops at a second", () => {
		const box = releaseBox();
		const tag = 'v3.1.100';
		published(box, A, {}, tag);
		assert.equal(existsSync(join(box.release, 'datum-x86_64-linux')), false, 'a kept release, which holds no fixed names');
		const taken = `datum-aarch64-macos-${A.slice(0, 7)}`;
		const again = publish(box, A, { FAKE_GH_TAKEN: taken }, tag);
		assert.equal(again.status, 0, again.stdout + again.stderr);
		assert.equal(installs(box), built(A));

		const run = publish(box, A, { FAKE_GH_TAKEN: taken, FAKE_GH_TAKEN_TIMES: '3' }, tag);
		assert.notEqual(run.status, 0);
		assert.match(run.stderr, /already exists/);
		assert.equal(readFileSync(join(box.fake, 'taken'), 'utf8'), '3', 'the upload ran twice more, and stopped');
	});

	test("a 'starter' asset a run leaves blocks no later run", () => {
		const box = releaseBox();
		published(box, A);
		assert.notEqual(publish(box, B, { FAKE_GH_CUT: `datum-x86_64-linux-${B.slice(0, 7)}` }).status, 0);
		// Run again at the same commit: the same name, which the starter holds.
		published(box, B);
		assert.equal(installs(box), built(B));
		// The fixed name cut short, as on 11-04: installs move on, and the next run uploads it.
		const run = published(box, C, { FAKE_GH_CUT: 'datum-x86_64-linux' });
		assert.match(run.stdout, /::warning::/);
		assert.equal(installs(box), built(C));
		published(box, D);
		assert.equal(readFileSync(join(box.release, 'datum-x86_64-linux'), 'utf8'), built(D));
		assert.deepEqual(assetsOf(box).filter((a) => a.state !== 'uploaded'), []);
	});

	test('a prune that would delete the build SHA256SUMS names deletes nothing', () => {
		const box = releaseBox();
		for (const sha of [A, B, C]) published(box, sha);
		// Asset ids that no longer follow the builds' order, as a recreated release's might.
		const file = join(box.fake, 'assets.json');
		const state = JSON.parse(readFileSync(file, 'utf8'));
		for (const a of state.assets) a.id += 1000;
		writeFileSync(file, JSON.stringify(state));
		const run = publish(box, D);
		assert.notEqual(run.status, 0);
		assert.match(run.stdout, /::error::.*\bnothing pruned\b/);
		assert.equal(assetsOf(box).filter((a) => a.name.endsWith(A.slice(0, 7))).length, ASSETS.length, 'the oldest build is kept too');
		assert.equal(installs(box), built(D));
	});

	test('it keeps the newest three builds and deletes the rest', () => {
		const box = releaseBox();
		for (const sha of [A, B, C, D]) published(box, sha);
		const names = assetsOf(box).map((a) => a.name);
		const builds = new Set(names.map((n) => n.match(/^datum-[^-]+-[^-]+-([0-9a-f]+)$/)?.[1]).filter(Boolean));
		assert.deepEqual([...builds].sort(), [B, C, D].map((sha) => sha.slice(0, 7)));
		assert.equal(names.length, (ASSETS.length + 1) * 3 + ASSETS.length + 2, `three builds with their signatures, the fixed names and the signed SHA256SUMS: ${names}`);
		for (const sha of [B, C, D]) assert.ok(names.includes(`SHA256SUMS-${sha.slice(0, 7)}.minisig`), `${sha}'s signature: ${names}`);
		assert.equal(installs(box), built(D));
	});
});
