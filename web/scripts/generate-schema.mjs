// Codegen: read the canonical SQLite schema from datum-core and emit
// web/src/lib/schema.generated.ts. Runs as a pre-step for dev/build/check
// so the PWA's CREATE TABLE statements never drift from the CLI's.
//
// Source: ../crates/datum-core/src/schema.sql
// Output: src/lib/schema.generated.ts
//
// The output is a single exported SCHEMA string. db.ts execs it verbatim
// against the WASM SQLite instance.

import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, '..', '..');
const schemaPath = resolve(repoRoot, 'crates/datum-core/src/schema.sql');
const outPath = resolve(here, '..', 'src/lib/schema.generated.ts');

const sql = readFileSync(schemaPath, 'utf8');
const escaped = sql.replace(/\\/g, '\\\\').replace(/`/g, '\\`').replace(/\$\{/g, '\\${');

const output = `// AUTO-GENERATED FROM crates/datum-core/src/schema.sql — DO NOT EDIT.
// Regenerate via \`npm run generate:schema\` (also runs on dev/build/check).

export const SCHEMA = \`${escaped}\`;
`;

mkdirSync(dirname(outPath), { recursive: true });
writeFileSync(outPath, output);
console.log(`generated ${outPath} (${sql.length} bytes of SQL)`);
