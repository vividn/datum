// The app as something a phone can install.
//
// Installability is not a feature of the page — it is a claim the manifest
// makes and the browser checks, and until da-elii the claim was false: one
// icon entry, `sizes: "any"`, pointing at a 70-byte 1x1 PNG placeholder.
// Chrome wants a 192 and a 512; Android wants a `maskable` it can cut a
// circle out of without cutting into the picture; iOS reads none of the
// manifest and wants an `apple-touch-icon` link. So the assertions here are
// the installer's, not ours.
//
// Every icon is fetched over HTTP from the built site rather than read off
// disk, because the path in the manifest is the thing that breaks — a file
// that exists under a name the manifest does not use is the same outage as
// no file at all. And each one's declared `sizes` is held to the pixels in
// its own IHDR, since a manifest that lies about a size is how an installed
// app ends up with a blurred or a rejected icon.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { startApp, stopApp, withApp } from './support/app.mjs';

const web = fileURLToPath(new URL('..', import.meta.url));

const PNG_MAGIC = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

/**
 * The pixel dimensions a PNG declares in its own header.
 *
 * The IHDR chunk is the first one a PNG may have, so after the 8-byte
 * signature and the chunk's own length and type it is fixed: width at 16,
 * height at 20, both big-endian. No image library, and nothing this suite
 * would have to install.
 */
function pngSize(bytes) {
	assert.ok(bytes.subarray(0, 8).equals(PNG_MAGIC), 'not a PNG');
	return { width: bytes.readUInt32BE(16), height: bytes.readUInt32BE(20) };
}

before(startApp);
after(stopApp);

describe('the manifest', () => {
	test('declares an installable app, and every icon it names is really there', async () => {
		await withApp(async ({ page }) => {
			const origin = new URL(page.url()).origin;

			// The href the page itself gives, not one written down here: the
			// link in app.html is half of what a browser follows.
			const href = await page.getAttribute('link[rel="manifest"]', 'href');
			const response = await page.request.get(new URL(href, origin).href);
			assert.equal(response.status(), 200);
			const manifest = JSON.parse(await response.text());

			assert.equal(manifest.name, 'datum');
			assert.equal(manifest.display, 'standalone');
			assert.equal(manifest.start_url, '/');
			assert.equal(manifest.scope, '/');
			// `id` is Chrome's identity for an installed app. Left out it
			// defaults to `start_url`, and then a later `start_url` installs a
			// SECOND app beside the first rather than updating it.
			assert.equal(manifest.id, '/');

			// Chrome will not offer to install without a 192 and a 512 of
			// purpose `any`; Android clips an un-padded icon to its mask.
			const declared = manifest.icons.map((icon) => `${icon.sizes} ${icon.purpose}`);
			for (const wanted of ['192x192 any', '512x512 any', '192x192 maskable', '512x512 maskable']) {
				assert.ok(declared.includes(wanted), `no ${wanted} icon among ${declared.join(', ')}`);
			}

			for (const icon of manifest.icons) {
				const at = new URL(icon.src, origin).href;
				const got = await page.request.get(at);
				assert.equal(got.status(), 200, `${icon.src} did not fetch`);
				assert.equal(got.headers()['content-type'], 'image/png', `${icon.src} is not a PNG`);
				assert.equal(icon.type, 'image/png');

				// `any` and `maskable` are different pictures and the browser
				// picks between them by this word; anything else it ignores,
				// silently.
				assert.ok(['any', 'maskable'].includes(icon.purpose), `${icon.src}: ${icon.purpose}`);

				const { width, height } = pngSize(await got.body());
				assert.equal(`${width}x${height}`, icon.sizes, `${icon.src} is not its declared size`);
			}
		});
	});

	test('a path no icon uses is a 404 — so "every icon fetched" examined something', async () => {
		await withApp(async ({ page }) => {
			const origin = new URL(page.url()).origin;
			const got = await page.request.get(new URL('/icons/icon-256.png', origin).href);
			assert.equal(got.status(), 404);
		});
	});
});

describe('the icons outside the manifest', () => {
	test('iOS gets a 180x180 apple-touch-icon, and the tab a real favicon', async () => {
		await withApp(async ({ page }) => {
			const origin = new URL(page.url()).origin;

			for (const [rel, side] of [
				['apple-touch-icon', 180],
				['icon', 64]
			]) {
				const href = await page.getAttribute(`link[rel="${rel}"]`, 'href');
				assert.ok(href, `no <link rel="${rel}">`);
				const got = await page.request.get(new URL(href, origin).href);
				assert.equal(got.status(), 200, `${rel} did not fetch`);
				assert.deepEqual(pngSize(await got.body()), { width: side, height: side }, rel);
			}
		});
	});

	test('no 1x1 placeholder is left anywhere in static/', () => {
		const tracked = execFileSync('git', ['ls-files', '-z', '--', 'static'], {
			cwd: web,
			encoding: 'utf8'
		})
			.split('\0')
			.filter((name) => name.endsWith('.png'));
		assert.ok(tracked.length > 0, 'no PNGs tracked under static/');

		for (const name of tracked) {
			const bytes = fs.readFileSync(path.join(web, name));
			const { width, height } = pngSize(bytes);
			assert.ok(width > 1 && height > 1, `${name} is ${width}x${height}`);
		}
	});

	test('the service worker precaches them, so an installed app has them offline', () => {
		// `$service-worker`'s `files` is everything under static/, and
		// service-worker.ts spreads it into the precache list — so this is a
		// check that the icons are in static/ under the names the manifest
		// uses, not a second claim about the worker. If the precache ever
		// stops covering `files`, this is where it shows.
		const sw = fs.readFileSync(path.join(web, 'build', 'service-worker.js'), 'utf8');
		const manifest = JSON.parse(
			fs.readFileSync(path.join(web, 'build', 'manifest.json'), 'utf8')
		);
		const wanted = [...manifest.icons.map((icon) => icon.src), '/icons/apple-touch-icon.png'];
		assert.deepEqual(
			wanted.filter((src) => !sw.includes(`"${src}"`)),
			[],
			'not named in the precache list'
		);
	});
});
