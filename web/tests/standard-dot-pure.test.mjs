// The placement editor draws a dot's box at the CLI's standard size (da-a87dh):
// Placement.svelte keeps its own `STANDARD_DOT`, which must be the one
// datum-layout draws by, or the box and the dot it frames part.

import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const read = (path) => readFileSync(new URL(path, import.meta.url), 'utf8');
const dot = (text, pattern) => {
	const m = text.match(pattern);
	assert.ok(m, `no STANDARD_DOT matching ${pattern}`);
	return Number(m[1]);
};

test("the placement editor's standard dot is the CLI's", () => {
	const cli = dot(read('../../crates/datum-layout/src/dayview.rs'), /pub const STANDARD_DOT: f64 = ([\d.]+);/);
	const web = dot(read('../src/lib/Placement.svelte'), /const STANDARD_DOT = ([\d.]+);/);
	assert.equal(web, cli);
});
