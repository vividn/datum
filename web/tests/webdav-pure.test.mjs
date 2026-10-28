// Which folder URLs the WebDAV destination (#lib/webdav.ts, da-bmxbb) refuses
// before asking: the fake server is http, as the app under test is, so a
// mixed-content block cannot be shown in the browser tests (vremya on #750).

import test from 'node:test';
import assert from 'node:assert/strict';
import { resolveLibAsSource } from './support/libhook.mjs';

resolveLibAsSource();
const { unusable } = await import('../src/lib/webdav.ts');

const MIXED = /^folder URL: an https page can't reach an http:\/\/ folder/;

test('an https page refuses an http:// folder — a NAS on the LAN above all — but not one on this machine', () => {
	for (const url of ['http://192.168.1.10/dav/', 'http://nas.local/remote.php/dav/files/ana/', ' HTTP://NAS.LOCAL/dav ']) assert.match(unusable(url, 'https:'), MIXED, url);
	for (const url of ['http://localhost:8080/dav/', 'http://127.0.0.1/dav/', 'http://[::1]/dav/', 'https://nas.local/dav/'])
		assert.equal(unusable(url, 'https:'), '', url);
	assert.equal(unusable('http://192.168.1.10/dav/', 'http:'), '');
});

test('a URL that is not a whole web address is named as the folder URL, not CORS', () => {
	assert.match(unusable('nas.local/dav/', 'https:'), /^folder URL: not a whole URL/);
	assert.match(unusable('', 'https:'), /^folder URL: not a whole URL/);
	assert.match(unusable('ftp://nas.local/dav/', 'https:'), /^folder URL: ftp:\/\/ is not a web address/);
});
