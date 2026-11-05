// The placement editor draws a dot's box at the CLI's standard size (da-a87dh):
// panes.ts keeps its own `STANDARD_DOT`, which Placement.svelte and collect's
// now strip draw by. It must be the one datum-layout draws by, or the box and
// the dot it frames part, and a tap's dot changes size as the layer lands.

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
	const web = dot(read('../src/lib/panes.ts'), /export const STANDARD_DOT = ([\d.]+);/);
	assert.equal(web, cli);
});
