// A field's spec is a PLACE (da-hbrfg): the `spec` widget in a `spec:<field>`
// tab, or on a desktop the same widget in the place modal (da-jozgv). The
// Track dialog that declares a field is the same form, so SPEC finds any of
// them — and not the widget's host, which is classed by its kind, nor the
// copy a tab slides in as (`.beside`), which is thrown away when it lands
// (da-6du3o): what is typed there is lost.

export const SPEC = '.spec:is(dialog, .inline):not(.beside *)';

/** Any copy of the form, the sliding one too: what must be gone before another opens. */
const ANY = '.spec:is(dialog, .inline)';

/**
 * Open `field`'s spec as the console does, once any place before it has
 * gone — a Done slides back before its tab closes. `saved` is `datum.spec`'s
 * answer, what Done ran or null when left, BOXED: awaiting this must not
 * await the save.
 */
export async function openSpec(page, field) {
	await page.locator(ANY).waitFor({ state: 'detached' });
	return { saved: page.evaluate((f) => window.datum.spec(f), field) };
}

/** Leave the place running nothing: its Cancel. */
export const cancelSpec = (page) => page.locator(`${SPEC} button.cancel`).click();
