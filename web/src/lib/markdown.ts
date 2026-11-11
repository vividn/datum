// The subset of Markdown datum's docs are written in, to HTML (da-0d8l6),
// for the docs widget. vividn, 2026-11-05: «you should be able to explore
// the docs without leaving the app». The docs use headings, paragraphs,
// lists (nested, numbered), tables, fenced code, rules, links, images,
// `code`, **bold** and *italics*; this is those and no more, written
// here rather than taken as a dependency because it is this small.
//
// Everything is escaped before any markup is added, so a doc's text is never
// HTML: `<file>` in a code span reads as written. A link's and an image's
// target go through the caller's `link` and `image`, which decide where they
// lead, and only for a scheme it keeps: a link http, https or mailto, an image
// http or https, read as the browser reads it, a relative one as https. Any
// other is drawn as its text, so no doc carries a `javascript:` URL into the
// page however it is written (da-hse1u). A heading gets GitHub's anchor, so
// `#a-heading` lands on it.

export interface Targets {
	/** The attributes of a link to `href`, as written in the doc: `href="…"` and whatever else. */
	link: (href: string) => string;
	/** The URL an image at `src` is drawn from. */
	image: (src: string) => string;
}

const escape = (text: string): string =>
	text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

/** GitHub's anchor for a heading: lowercase, punctuation dropped, a space a `-`. */
export const anchor = (heading: string): string =>
	heading
		.toLowerCase()
		.replace(/[`*]/g, '')
		.replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
		.replace(/[^\p{L}\p{N}\s_-]/gu, '')
		.trim()
		.replace(/\s/g, '-');

/** Targets that lead out of the page: a link opens in a new tab, an image is drawn from where it says. */
export const OUTWARD: Targets = {
	link: (href) => `href="${href.replace(/"/g, '&quot;')}" target="_blank" rel="noopener"`,
	image: (src) => src
};

const LINKS = ['http:', 'https:', 'mailto:'];
const IMAGES = ['http:', 'https:'];

/** Whether `target`, read as the browser reads it, has one of `schemes`. */
function keeps(target: string, schemes: string[]): boolean {
	try {
		return schemes.includes(new URL(target, 'https://doc/').protocol);
	} catch {
		return false;
	}
}

/** One line's inline markup. */
function inline(text: string, to: Targets): string {
	const held: string[] = [];
	const hold = (html: string) => `\u0000${held.push(html) - 1}\u0000`;
	let out = escape(text).replace(/(`+)(.+?)\1/g, (_, __, code: string) => hold(`<code>${code.trim() || code}</code>`));
	out = out
		.replace(/!\[([^\]]*)\]\(([^)\s]+)\)/g, (_, alt: string, src: string) =>
			hold(keeps(unescaped(src), IMAGES) ? `<img alt="${alt}" src="${escape(to.image(unescaped(src)))}">` : alt)
		)
		.replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, (_, label: string, href: string) =>
			keeps(unescaped(href), LINKS) ? `<a ${to.link(unescaped(href))}>${label}</a>` : label
		)
		.replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
		.replace(/(^|[^\w*])\*(?!\s)(.+?)\*(?!\w)/g, '$1<em>$2</em>');
	return out.replace(/\u0000(\d+)\u0000/g, (_, i: string) => held[Number(i)]);
}

/** A target as the doc wrote it, back from its escaped form. */
const unescaped = (text: string): string => text.replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&');

const FENCE = /^\s*(`{3,}|~{3,})/;
const HEADING = /^(#{1,6})\s+(.*?)(?:\s+#+)?\s*$/;
const ITEM = /^(\s*)([-*+]|\d+[.)])\s+/;
const RULE = /^\s*([-*_])(\s*\1){2,}\s*$/;
const ROW = /^\s*\|/;

/** Whether `line` starts a block of its own, so a paragraph ends before it. */
const starts = (line: string) => FENCE.test(line) || HEADING.test(line) || ITEM.test(line) || RULE.test(line) || ROW.test(line);

/** A table row's cells, its outer pipes dropped. */
const cells = (row: string): string[] =>
	row
		.trim()
		.replace(/^\||\|$/g, '')
		.split('|')
		.map((c) => c.trim());

/** `markdown`'s blocks, for lines already split. */
function blocks(lines: string[], to: Targets): string {
	const out: string[] = [];
	let i = 0;
	while (i < lines.length) {
		const line = lines[i];
		if (!line.trim()) {
			i++;
			continue;
		}
		const fence = line.match(FENCE);
		if (fence) {
			const indent = line.length - line.trimStart().length;
			const code: string[] = [];
			// It closes on a line of its own character alone, at least as long as it opened.
			const closes = (l: string) => l.trim().startsWith(fence[1]) && /^(`+|~+)$/.test(l.trim());
			for (i++; i < lines.length && !closes(lines[i]); i++) code.push(lines[i].slice(Math.min(indent, lines[i].length - lines[i].trimStart().length)));
			i++;
			out.push(`<pre><code>${escape(code.join('\n'))}</code></pre>`);
			continue;
		}
		const heading = line.match(HEADING);
		if (heading) {
			const level = heading[1].length;
			out.push(`<h${level} id="${escape(anchor(heading[2]))}">${inline(heading[2], to)}</h${level}>`);
			i++;
			continue;
		}
		if (RULE.test(line)) {
			out.push('<hr>');
			i++;
			continue;
		}
		if (ROW.test(line) && i + 1 < lines.length && /^\s*\|?\s*:?-+:?\s*(\|\s*:?-+:?\s*)*\|?\s*$/.test(lines[i + 1])) {
			const head = cells(line);
			const rows: string[][] = [];
			for (i += 2; i < lines.length && ROW.test(lines[i]); i++) rows.push(cells(lines[i]));
			const tr = (row: string[], tag: string) => `<tr>${row.map((c) => `<${tag}>${inline(c, to)}</${tag}>`).join('')}</tr>`;
			out.push(`<table><thead>${tr(head, 'th')}</thead><tbody>${rows.map((r) => tr(r, 'td')).join('')}</tbody></table>`);
			continue;
		}
		const item = line.match(ITEM);
		if (item) {
			const indent = item[1].length;
			const ordered = /\d/.test(item[2]);
			const items: string[] = [];
			while (i < lines.length) {
				const at = lines[i].match(ITEM);
				if (!at || at[1].length !== indent || /\d/.test(at[2]) !== ordered) break;
				// The item's own lines: its first, then every one indented past its marker, blank lines between them included.
				const body = [lines[i].slice(at[0].length)];
				const width = at[0].length;
				const deeper = (l: string) => l.length - l.trimStart().length > indent;
				for (i++; i < lines.length; i++) {
					const next = lines[i];
					if (next.trim() ? !deeper(next) : !deeper(lines[i + 1] ?? '')) break;
					body.push(next.slice(Math.min(width, next.length - next.trimStart().length)));
				}
				// A tight item is its text, with no paragraph around it.
				const html = blocks(body, to);
				items.push(`<li>${body.some((l) => !l.trim()) ? html : html.replace(/^<p>([\s\S]*?)<\/p>/, '$1')}</li>`);
			}
			const start = ordered ? Number.parseInt(item[2], 10) : 1;
			out.push(ordered ? `<ol${start === 1 ? '' : ` start="${start}"`}>${items.join('')}</ol>` : `<ul>${items.join('')}</ul>`);
			continue;
		}
		const para: string[] = [];
		for (; i < lines.length && lines[i].trim() && (para.length === 0 || !starts(lines[i])); i++) para.push(lines[i].trim());
		out.push(`<p>${inline(para.join(' '), to)}</p>`);
	}
	return out.join('\n');
}

/** `source`, a doc in datum's Markdown, as HTML: its links and images led where `to` says. */
export const markdown = (source: string, to: Targets): string => blocks(source.replace(/\r\n?/g, '\n').split('\n'), to);
