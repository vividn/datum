// The docs widget's Markdown (da-0d8l6), in node, with no browser: the subset
// datum's docs are written in, each block and inline form, its text escaped,
// and every doc under `docs/` drawn with no table whose rows disagree.

import test from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { anchor, markdown } from '../src/lib/markdown.ts';

const to = { link: (href) => `href="${href}"`, image: (src) => `/img/${src}` };
const md = (text) => markdown(text, to);

test('headings carry GitHub anchors, and text is escaped before markup', () => {
	assert.equal(md('## What `datum install` sets up'), '<h2 id="what-datum-install-sets-up">What <code>datum install</code> sets up</h2>');
	assert.equal(anchor('First of all: why keep data?'), 'first-of-all-why-keep-data');
	assert.equal(md('a <b> & `<file>`'), '<p>a &lt;b&gt; &amp; <code>&lt;file&gt;</code></p>');
});

test('inline: links, images, bold, italics, and a link around code', () => {
	assert.equal(
		md('see [`x`](a.md#b), ![pic](img/d.svg), **bold** and *it*, 2 * 3 * 4'),
		'<p>see <a href="a.md#b"><code>x</code></a>, <img alt="pic" src="/img/img/d.svg">, <strong>bold</strong> and <em>it</em>, 2 * 3 * 4</p>'
	);
});

test('a link keeps http, https or mailto, an image http or https; any other is its text', () => {
	assert.equal(md('[y](https://example.org) [m](mailto:a@b.c) [r](a.md) ![i](http://x/p.png)'), '<p><a href="https://example.org">y</a> <a href="mailto:a@b.c">m</a> <a href="a.md">r</a> <img alt="i" src="/img/http://x/p.png"></p>');
	for (const bad of ['javascript:alert(1)', 'JavaScript:alert(1)', '\u0001javascript:alert(1)', 'data:text/html,x', 'vbscript:x']) {
		assert.equal(md(`[x](${bad})`).replace(/\)/g, ''), '<p>x</p>', bad);
		assert.equal(md(`![x](${bad})`).replace(/\)/g, ''), '<p>x</p>', bad);
	}
	assert.equal(md('![x](data:image/png;base64,AA) ![](mailto:a@b.c)'), '<p>x </p>');
});

test('a paragraph joins its lines; a code block keeps them, escaped', () => {
	assert.equal(md('one\ntwo\n\n```bash\na < b\n  c\n```'), '<p>one two</p>\n<pre><code>a &lt; b\n  c</code></pre>');
});

test('a fence closes on its own character, at least as long, and on nothing else', () => {
	assert.equal(md('````\n```\n~~~\n````\nafter'), '<pre><code>```\n~~~</code></pre>\n<p>after</p>');
	assert.equal(md('```\n```js\n```'), '<pre><code>```js</code></pre>');
});

test('lists: tight items, continuation lines, nesting and numbering', () => {
	assert.equal(md('- a\n  more\n  - b\n- c'), '<ul><li>a more\n<ul><li>b</li></ul></li><li>c</li></ul>');
	assert.equal(md('3. x\n4. y'), '<ol start="3"><li>x</li><li>y</li></ol>');
});

test('tables, with a header and a rule', () => {
	assert.equal(
		md('| a | b |\n| - | :-: |\n| `c` | d |\n\n---'),
		'<table><thead><tr><th>a</th><th>b</th></tr></thead><tbody><tr><td><code>c</code></td><td>d</td></tr></tbody></table>\n<hr>'
	);
});

test('every doc draws, each table with as many cells a row as its header', () => {
	const dir = new URL('../../docs/', import.meta.url);
	const docs = readdirSync(dir, { recursive: true }).filter((f) => f.endsWith('.md'));
	assert.ok(docs.includes('README.md') && docs.length > 10, `the docs read: ${docs}`);
	for (const doc of docs) {
		const html = md(readFileSync(new URL(doc, dir), 'utf8'));
		assert.match(html, /^<h1/, `${doc} opens on its title`);
		for (const table of html.match(/<table>[\s\S]*?<\/table>/g) ?? []) {
			const widths = table.match(/<tr>[\s\S]*?<\/tr>/g).map((row) => row.match(/<t[hd]>/g).length);
			assert.equal(new Set(widths).size, 1, `${doc}: a table's rows are ${widths}`);
		}
	}
});
