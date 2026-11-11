// A field is made and edited in the field widget (da-fu29p): in its place —
// a tab, or on a desktop the same widget in the place modal (da-jozgv) — or
// in a pane someone put it in. FIELD_FORM finds its form, and not the copy a
// tab slides in as (`.beside`), which is thrown away when it lands
// (da-6du3o): what is typed there is lost.

import { shellSplit } from '../../src/lib/shellSplit.ts';

export const FIELD_FORM = '.field-form:not(.beside *)';

/** The panel of the tab in front, the one there is: a tap or a key changes it, never a swipe (da-ooflv). */
export const PANEL = `${FIELD_FORM} [data-panel]`;

/**
 * A new field's Save as a preview or an echo reads it — ONE `setup F
 * --from-json J --track` or `--ignore` (da-2pbvd) — as the line without J,
 * and J parsed: the shell's quoting of the JSON is none of a test's business.
 */
export function declared(text) {
	const argv = shellSplit(text.trim());
	const [json] = argv.splice(3, 1);
	return { line: argv.join(' '), spec: JSON.parse(json) };
}

/** Any copy of the form, the sliding one too: what must be gone before another opens. */
const ANY = '.field-form';

/**
 * Open the field widget on `field` as the console does — none is a new
 * field, `verb` its kind — once any place before it has gone: a form still
 * open in one, as Save leaves it (da-hda4j), is left first, and a place
 * left slides back before its tab closes. `saved` is `datum.field`'s answer, what the
 * place's next save ran or null when left, BOXED: awaiting this must not
 * await the save. Nor may a test that never awaits it be failed by it: a
 * page closed under a test that threw rejects it with «Target page closed»,
 * which node would report in place of what the test threw (da-w3y9q).
 * `brought` opens it as the hold → setup path does (da-29s6c): Save goes back.
 */
export async function openField(page, field, verb, brought = false) {
	if (await page.locator(FIELD_FORM).count()) await page.evaluate(() => window.datum.shell.leavePlace());
	await page.locator(ANY).waitFor({ state: 'detached' });
	const saved = page.evaluate(([f, v, b]) => window.datum.field(f, v, { brought: b }), [field, verb ?? null, brought]);
	saved.catch(() => {});
	return { saved };
}

/**
 * Leave the place, as its × or the back does: a new field's edits are
 * dropped, as its Cancel drops them, and a spec's saved (da-hda4j). Never by
 * the Cancel button: a new field's Save remounts the form on the spec it
 * made, which has none, and a click on it then waits on a button the
 * remount took away.
 */
export const leaveField = (page) => page.evaluate(() => window.datum.shell.leavePlace());

/**
 * Give the open form's field the image `svg` as its symbol (da-rpfws): its
 * symbol tile opens the picker, whose «image (svg)» takes the file, and the
 * tile shows it once read: this image, since a field may show another
 * already, and the file is read, and the picker closed, a moment later (da-28k9h).
 */
export async function pickImage(page, name, svg) {
	const tile = page.locator(`${FIELD_FORM} .tile[aria-expanded][aria-label^="the field's symbol:"]`);
	await tile.click();
	await page.locator(`${FIELD_FORM} .picker input[type="file"]`).setInputFiles({ name, mimeType: 'image/svg+xml', buffer: Buffer.from(svg) });
	const src = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
	await page.locator(`${FIELD_FORM} .tile[aria-label="the field's symbol: image"][data-icon] img[src="${src}"]`).waitFor();
}

/** The `field:` line's modal (da-4ta1k): what a field is, a new one's name, and the fields there are. */
export const NAMING = `${FIELD_FORM} dialog[data-picker]`;

/** Open the modal from the `field:` line, as «track something» does; the × of a field's pill first, to start over. */
export async function trackSomething(page) {
	const clear = page.locator(`${FIELD_FORM} .name-line .pill button.clear`);
	if (await clear.count()) await clear.click();
	await page.locator(`${FIELD_FORM} button.track-something`).click();
	await page.locator(`${NAMING} input.field`).waitFor();
}

/** Name a field as a person does: «track something», the name typed into its modal, Enter. A field there is opens. */
export async function nameField(page, field) {
	await trackSomething(page);
	await page.locator(`${NAMING} input.field`).fill(field);
	await page.locator(`${NAMING} input.field`).press('Enter');
	await page.locator(NAMING).waitFor({ state: 'detached' });
}

/** The field the form is on, as its pill names it: `''` while the line says «track something». */
export async function fieldNamed(page) {
	await page.locator(`${FIELD_FORM} .name-line`).waitFor();
	const pill = page.locator(`${FIELD_FORM} .name-line .pill`);
	return (await pill.count()) ? pill.getAttribute('data-field') : '';
}

/** The setup modal's own row of its commands, on a desktop, where the modal covers the bar (da-r674m). */
const MODAL = 'dialog.sheet[data-place="setup"]';
const IN_MODAL = `${MODAL} .setup-commands`;

/**
 * Where the setup widget's commands are (da-r674m): the setup modal's row
 * where one is open, else the ❯'s palette, opened if it is not.
 */
export async function openPalette(page) {
	if (await page.locator(MODAL).count()) return page.locator(IN_MODAL);
	// By the ❯'s state, not the palette's: one still sliding away after a run is there, and closed.
	const shut = page.locator('.prompt-bar button[aria-controls="bar-commands"][aria-expanded="false"]');
	if (await shut.count()) await shut.click();
	const bar = page.locator('#bar-commands');
	await bar.waitFor();
	return bar;
}

/** The setup command `name`, or with none its primary: save, or start tracking, which the form's Save was. */
const one = (name) => (name ? `[data-setup="${name}"]` : '[data-setup][data-primary]');
export const SETUP = (name) => `:is(#bar-commands, ${IN_MODAL}) ${one(name)}`;

/** Press one of the setup widget's commands, where they are. */
export async function setupCommand(page, name) {
	await (await openPalette(page)).locator(one(name)).click();
}

/** The setup commands, primary first and undo apart, each `[name, enabled]`. */
export async function setupCommands(page) {
	const at = await openPalette(page);
	await at.locator(one()).waitFor();
	return at.locator('[data-setup]:not([data-setup="undo"])').evaluateAll((els) => els.map((el) => [el.dataset.setup, !el.disabled]));
}
