// The name on the tab.
//
// datum.day shipped with no `<title>` at all: the built index.html carried
// a charset, a viewport, an icon and a manifest, and nothing that named the
// app, so the tab read `www.datum.day` (da-kiio). The manifest's "name"
// covers the installed PWA and not the tab, which is why this survived
// being looked at.
//
// The assertion is against the *built* HTML rather than the running page
// on purpose. This is a static-hosted, client-rendered app: a title set
// after hydration leaves the tab nameless for as long as the JS takes to
// arrive, and the first impression happens inside that window. So the
// floor has to be in the very first bytes the browser receives, and that
// is a file this test can simply read — no browser, no server.

import test, { describe } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const web = fileURLToPath(new URL('..', import.meta.url));
const html = fs.readFileSync(path.join(web, 'build', 'index.html'), 'utf8');

describe('the served page', () => {
	test('names the app in its head, before any script runs', () => {
		const title = html.match(/<title[^>]*>([^<]*)<\/title>/i);
		assert.ok(title, 'built index.html has no <title> element');
		assert.equal(title[1].trim(), 'datum');

		// "Before any script runs" is the point: a <title> that only the
		// bundle produces is the bug this test is about.
		const head = html.slice(0, html.search(/<script/i));
		assert.match(head, /<title[^>]*>/i, '<title> comes after the first <script>');
	});
});
