// The field-name mirror (`fieldName.ts`) and `shellJoin`, in node. What the
// form and the typed line DO with them is `fieldname.test.mjs` and
// `fields-keys.test.mjs`; this file is the rule and the quoting alone,
// held to the fixtures the Rust side reads (da-7h73h).

import test, { describe } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { resolveLibAsSource } from './support/libhook.mjs';

resolveLibAsSource(import.meta.url);
const { shellJoin, shellSplit } = await import('../src/lib/shellSplit.ts');
const { fieldNameOk, suggestFieldName } = await import('../src/lib/fieldName.ts');

const fixture = (name) => JSON.parse(readFileSync(fileURLToPath(new URL(`../../tests/fixtures/${name}`, import.meta.url)), 'utf8'));
const { cases } = fixture('field-name.json');

// The mirror itself, in node: the form's verdict and its offer, case by
// case, and the quoting every typed line with a field in it goes through.
describe('fieldName.ts and shellJoin', () => {
	test('judge every shared case as the parser does', () => {
		for (const c of cases) {
			assert.equal(fieldNameOk(c.field), c.ok, c.name);
			assert.equal(suggestFieldName(c.field), c.suggest, c.name);
		}
	});

	test('shellJoin is shell_join: a spaced field is one quoted token, and every argv round-trips', () => {
		assert.equal(shellJoin(['occur', 'my coffee', 'amount=']), 'occur "my coffee" amount=');
		assert.equal(shellJoin(['a"b\\c', '', '#x']), '"a\\"b\\\\c" "" "#x"');
		const { cases: lines } = fixture('shell-split.json');
		for (const c of [...lines, ...cases.map((f) => ({ name: f.name, argv: ['occur', f.field] }))])
			assert.deepEqual(shellSplit(shellJoin(c.argv)), c.argv, c.name);
	});
});
