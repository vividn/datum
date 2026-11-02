// The repo's kits, served by datum.day (da-fj3v1): each `kits/<name>.toml` at
// `/kits/<name>.toml`, byte for byte, and a listing at `/kits/` naming each with
// the version and description of its `[kit]` table. vite.config.ts emits them
// into the client build, so they deploy with the site.

import { readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';

/**
 * A key's string in the `[kit]` table: a basic string (`"…"`, read as JSON,
 * whose escapes TOML's are) or a literal one (`'…'`). Anything else, a
 * multi-line string say, is refused, so the listing never quietly lacks it.
 * @param {string} toml @param {string} key @param {string} file
 */
function kitKey(toml, key, file) {
	const table = toml.match(/^\[kit\][ \t]*\r?\n([\s\S]*?)(?=^\[|$(?![\s\S]))/m)?.[1] ?? '';
	const value = table.match(new RegExp(`^${key}[ \\t]*=[ \\t]*("(?:[^"\\\\\\n]|\\\\.)*"|'[^'\\n]*')`, 'm'))?.[1];
	if (value === undefined) throw new Error(`kits/${file}: no one-line ${key} in its [kit] table`);
	return value.startsWith("'") ? value.slice(1, -1) : JSON.parse(value);
}

/** @param {string} dir the repo's `kits/` @returns {{ file: string, bytes: Buffer, version: string, description: string }[]} */
export function repoKits(dir) {
	return readdirSync(dir)
		.filter((file) => file.endsWith('.toml'))
		.sort()
		.map((file) => {
			const bytes = readFileSync(path.join(dir, file));
			const toml = bytes.toString('utf8');
			return { file, bytes, version: kitKey(toml, 'version', file), description: kitKey(toml, 'description', file) };
		});
}

/** @param {string} text */
const escape = (text) =>
	text.replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c] ?? c);

/** The listing at `/kits/`. @param {ReturnType<typeof repoKits>} kits */
export function kitsIndex(kits) {
	const rows = kits
		.map(
			({ file, version, description }) =>
				`<dt><a href="${escape(file)}">${escape(file)}</a> ${escape(version)}</dt>\n<dd>${escape(description)}</dd>`
		)
		.join('\n');
	return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>datum kits</title>
<style>body{font:16px/1.5 system-ui,sans-serif;max-width:48rem;margin:2rem auto;padding:0 1rem}dd{margin:0 0 1rem}</style>
</head>
<body>
<h1>datum kits</h1>
<p><code>curl -fsSLO https://datum.day/kits/&lt;name&gt;.toml &amp;&amp; datum kit install &lt;name&gt;.toml</code> downloads a kit and installs it, asking before it adds an index; <code>--dry-run</code> says first what it brings.</p>
<dl>
${rows}
</dl>
</body>
</html>
`;
}
