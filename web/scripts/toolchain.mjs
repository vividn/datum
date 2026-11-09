// A pinned toolchain tarball, fetched, checked and unpacked: build-cli-wasm.mjs's
// `tool()`, here so tests/toolchain.test.mjs can drive it.
//
// DATUM_TOOLCHAIN_CACHE, a directory, is a read-through cache of the tarballs
// (da-waoz2). Staging run 11153 went red when all eight of `download()`'s tries
// at wasm-bindgen got a 504 from GitHub's release CDN, and the forge has no
// cache service, so every run fetched all three tools afresh. Set, `tool()`
// reads `<cache>/<the URL's last segment>` (a name carrying the version) before
// it fetches, and keeps a download there. A cached file is checked against the
// pinned sha256 like any download and ignored when it differs, which is what
// makes the directory safe whoever writes it; a write goes by a temp name and a
// rename, since jobs on one host share it, and one that fails is a warning.
// Unset or empty, nothing is read or written.

import { execFileSync } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

const sha = (bytes) => createHash('sha256').update(bytes).digest('hex');

/**
 * `url`'s bytes, asking up to eight times, 10 s more apart each time (4 min 40 s
 * in all): one HTTP 500 from GitHub failed a CI run (da-wh359), and 30 s of 504s
 * another (da-ui0ux). A 4xx is the URL's answer, so it is not asked again, but
 * 408 and 429 say to wait and are.
 */
export async function download(url, get = fetch) {
	for (let attempt = 1; ; attempt++) {
		console.log(`fetching ${url}`);
		try {
			const res = await get(url);
			if (res.status >= 400 && res.status < 500 && res.status !== 408 && res.status !== 429) throw Object.assign(new Error(`HTTP ${res.status}`), { final: true });
			if (!res.ok) throw new Error(`HTTP ${res.status}`);
			return Buffer.from(await res.arrayBuffer());
		} catch (e) {
			if (e.final || attempt === 8) throw new Error(`${url}: ${e.message}`);
			console.log(`${e.message}; again in ${attempt * 10}s`);
			await new Promise((r) => setTimeout(r, attempt * 10_000));
		}
	}
}

/** The cache's copy of `name` if it is `sha256`'s bytes, else undefined. */
function cached(cache, name, sha256) {
	const file = resolve(cache, name);
	try {
		if (!existsSync(file)) return;
		const bytes = readFileSync(file);
		const actual = sha(bytes);
		if (actual === sha256) return bytes;
		console.error(`${file}: sha256 ${actual}, pinned ${sha256}; ignored, fetching`);
	} catch (e) {
		console.error(`${file}: ${e.message}; ignored, fetching`);
	}
}

/** Keeps `bytes` in the cache as `name`, or warns that it could not. */
function keep(cache, name, bytes) {
	const file = resolve(cache, name);
	const temp = `${file}.${randomUUID()}.tmp`;
	try {
		writeFileSync(temp, bytes);
		renameSync(temp, file);
	} catch (e) {
		// `force` still throws under a file (ENOTDIR); existsSync never does.
		if (existsSync(temp)) rmSync(temp);
		console.error(`warning: ${name} not kept in DATUM_TOOLCHAIN_CACHE: ${e.message}`);
	}
}

/** The unpacked tool's directory under `toolchains`, downloading and verifying it first if needed. */
export async function tool({ url, sha256, dir }, toolchains, get = fetch) {
	const home = resolve(toolchains, dir);
	if (existsSync(home)) return home;
	mkdirSync(toolchains, { recursive: true });
	const name = url.split('/').pop();
	const tarball = resolve(toolchains, name);
	if (!existsSync(tarball)) {
		const cache = process.env.DATUM_TOOLCHAIN_CACHE;
		let bytes = cache && cached(cache, name, sha256);
		if (!bytes) {
			bytes = await download(url, get);
			if (cache && sha(bytes) === sha256) keep(cache, name, bytes);
		}
		writeFileSync(tarball, bytes);
	}
	const actual = sha(readFileSync(tarball));
	if (actual !== sha256) throw new Error(`${tarball}: sha256 ${actual}, pinned ${sha256}`);
	execFileSync('tar', ['-xzf', tarball, '-C', toolchains], { stdio: 'inherit' });
	return home;
}
