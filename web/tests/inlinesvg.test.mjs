// `inlineSvg` ($lib/hover.ts) is the page's last layer between an SVG the
// CLI printed and the DOM. Since da-45wgz that SVG can carry a person's own
// icon, which the CLI vets when it is saved and again when it is drawn; this
// holds the page to the same rules, in a real browser's DOMParser, for
// whatever gets past both: no `<style>` (page-wide once inlined), and no
// `<set>`/`<animate>` that writes a handler or a link later.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { stripTypeScriptTypes } from 'node:module';
import { chromium } from 'playwright';

// The function's own source, and its helpers', types stripped: hover.ts's imports reach
// SvelteKit's `$app`, which node cannot load, and `inlineSvg` needs none of
// them — only the page's DOMParser.
const hover = fs.readFileSync(new URL('../src/lib/hover.ts', import.meta.url), 'utf8');
const from = hover.indexOf('export function inlineSvg');
// With the helpers that follow it, down to the end of `fetches`.
const to = hover.indexOf('\n}\n', hover.indexOf('function fetches', from)) + 2;
const source = stripTypeScriptTypes(hover.slice(from, to)).replace('export ', '');

let browser;
before(async () => (browser = await chromium.launch()));
after(() => browser?.close());

/** `text` through `inlineSvg` in the page, as the markup it inlines. */
async function inlined(text) {
	const page = await browser.newPage();
	try {
		return await page.evaluate(
			([src, text]) => new Function(`${src}\nreturn inlineSvg;`)()(text)?.outerHTML ?? null,
			[source, text]
		);
	} finally {
		await page.close();
	}
}

// The icon is the one part of the picture a person wrote, so it is the part
// held to what an icon draws with (da-45wgz): no CSS escape — `\75 rl(`
// IS `url(` — no function but a transform's or a colour's, and a `url()`
// only to a `#fragment`. Chromium fetched every one refused here.
const ICON = (inner) =>
	'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><defs>' +
	`<svg id="icon-coffee" viewBox="0 0 1 1">${inner}</svg>` +
	'</defs><rect class="mark" data-note="see url(http://example.com/n)"/></svg>';

test('an icon that reaches the page loses its style and any animation of a handler or a link', async () => {
	const out = await inlined(
		ICON(
			'<style>body{display:none}</style>' +
				'<a href="#x"><set attributeName="href" to="javascript:x()"/></a>' +
				'<rect><animate attributeName="onclick" to="x()"/><animate attributeName="x" to="1"/></rect>'
		)
	);
	assert.doesNotMatch(out, /<style|javascript|onclick/);
	assert.match(out, /<animate attributeName="x"/, 'an animation of anything else stays');
});

test('an icon loses every value that could fetch, however it is spelled, and keeps what it draws with', async () => {
	const out = await inlined(
		ICON(
			'<rect fill="url(https://example.com/a)" style="fill:URL( \'https://example.com/b\')"/>' +
				'<rect style="fill: \\75 rl(https://example.com/c)"/><rect fill="\\75 rl(https://example.com/d)"/>' +
				'<rect style="mask-image: image-set(\'https://example.com/e\' 1x)"/>' +
				'<rect mask="-webkit-image-set(\'https://example.com/f\' 1x)"/>' +
				'<rect class="kept" fill="url(#g)" stroke="rgb(1,2,3)" transform="translate(1 0) rotate (90)"/>'
		)
	);
	const icon = out.slice(out.indexOf('id="icon-coffee"'), out.indexOf('</defs>'));
	assert.doesNotMatch(icon, /example\.com|\\/);
	assert.match(icon, /fill="url\(#g\)" stroke="rgb\(1,2,3\)" transform="translate\(1 0\) rotate \(90\)"/);
	// datum's own markup is not an icon: a hover title that mentions a
	// url( is text a person logged, and it stays.
	assert.match(out, /data-note="see url\(http:\/\/example.com\/n\)"/);
});
