// A field is made and edited in the field widget (da-fu29p): in its place —
// a tab, or on a desktop the same widget in the place modal (da-jozgv) — or
// in a pane someone put it in. FIELD_FORM finds its form, and not the copy a
// tab slides in as (`.beside`), which is thrown away when it lands
// (da-6du3o): what is typed there is lost.

import { shellSplit } from '../../src/lib/shellSplit.ts';

export const FIELD_FORM = '.field-form:not(.beside *)';

/**
 * A new field's Save as a preview or an echo reads it — ONE `spec set F
 * --from-json J --track` or `--ignore` (da-2pbvd) — as the line without J,
 * and J parsed: the shell's quoting of the JSON is none of a test's business.
 */
export function declared(text) {
	const argv = shellSplit(text.trim());
	const [json] = argv.splice(4, 1);
	return { line: argv.join(' '), spec: JSON.parse(json) };
}

/** Any copy of the form, the sliding one too: what must be gone before another opens. */
const ANY = '.field-form';

/**
 * Open the field widget on `field` as the console does — none is a new
 * field, `verb` its kind — once any place before it has gone: a Done slides
 * back before its tab closes. `saved` is `datum.field`'s answer, what the
 * place's next save ran or null when left, BOXED: awaiting this must not
 * await the save. Nor may a test that never awaits it be failed by it: a
 * page closed under a test that threw rejects it with «Target page closed»,
 * which node would report in place of what the test threw (da-w3y9q).
 */
export async function openField(page, field, verb) {
	await page.locator(ANY).waitFor({ state: 'detached' });
	const saved = page.evaluate(([f, v]) => window.datum.field(f, v), [field, verb]);
	saved.catch(() => {});
	return { saved };
}

/** Leave the place running nothing: its Cancel. */
export const cancelField = (page) => page.locator(`${FIELD_FORM} button.cancel`).click();
