// A field is made and edited in the field widget (da-fu29p): in its place —
// a tab, or on a desktop the same widget in the place modal (da-jozgv) — or
// in a pane someone put it in. FIELD_FORM finds its form, and not the copy a
// tab slides in as (`.beside`), which is thrown away when it lands
// (da-6du3o): what is typed there is lost.

export const FIELD_FORM = '.field-form:not(.beside *)';

/** Any copy of the form, the sliding one too: what must be gone before another opens. */
const ANY = '.field-form';

/**
 * Open the field widget on `field` as the console does — none is a new
 * field, `verb` its kind — once any place before it has gone: a Done slides
 * back before its tab closes. `saved` is `datum.field`'s answer, what the
 * place's next save ran or null when left, BOXED: awaiting this must not
 * await the save.
 */
export async function openField(page, field, verb) {
	await page.locator(ANY).waitFor({ state: 'detached' });
	return { saved: page.evaluate(([f, v]) => window.datum.field(f, v), [field, verb]) };
}

/** Leave the place running nothing: its Cancel. */
export const cancelField = (page) => page.locator(`${FIELD_FORM} button.cancel`).click();
