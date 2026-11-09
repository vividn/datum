// The switch lesson's game starts itself (da-uzyih.4867): as its cue says,
// by switching `room` to the room the person stands in.

import { ready } from './app.mjs';
import { until } from './until.mjs';

const SWITCH = '[data-pane="change-lesson"]';
/** Its fields, a pane of their own under the game (da-uzyih.4908). */
const FIELDS = '[data-pane="change-fields"]';

/** How many entries `room` holds. */
export const roomEntries = (page) =>
	page.evaluate(() => window.datum.runQuiet(['tail', 'room', '--json', '-n', '200']).then((r) => JSON.parse(r.stdout).rows.length));

/** A tap of `room`'s button, then of `room` among its states, landed. */
export async function switchRoom(page, room, pane = FIELDS) {
	const before = await roomEntries(page);
	await page.locator(`${pane} .fields button[data-field="room"]`).click();
	await page.locator(`${pane} .fields .room button[data-state="${room}"]`).click();
	await until(() => roomEntries(page), (n) => n === before + 1);
}

/** The room the cue names: where the person stands at rest, which starts the game. */
export const cuedRoom = (page, pane = SWITCH) => page.locator(`${pane} .game .cue b`).nth(1).innerText();

/** Start the switch game as a learner does, at the clock's instant: the room it began on. */
export async function startSwitch(page, pane = SWITCH) {
	const room = await cuedRoom(page, pane);
	await switchRoom(page, room);
	await page.locator(`${pane} .game[data-phase="playing"]`).waitFor();
	return room;
}

/** A row that is a field's TRACKED or IGNORED status (da-uzyih.4882), which a game writes as it begins and ends: no reading. */
export const isStatus = (row) => row.state === 'TRACKED' || row.state === 'IGNORED';

/** The game box in `pane` with its writes landed: a begin's TRACKED, a Reset's, before a test moves a held clock. */
export const settled = (page, pane) => page.locator(`${pane} .game[aria-busy="false"]`).waitFor();

/**
 * Each of `fields` as `datum now --json -A` holds it: `IGNORED`, `TRACKED`
 * (active, or tracked and off since a time), or `untracked` (a kit field
 * nothing has tracked yet).
 */
export async function statuses(app, fields) {
	const { active, tracked_false, ignored } = JSON.parse((await app.run(['now', '--json', '-A'])).stdout);
	const has = (list, field) => list.some((f) => f.field === field);
	return Object.fromEntries(
		fields.map((f) => [
			f,
			has(ignored, f) ? 'IGNORED' : has(active, f) || tracked_false.some((t) => t.field === f && t.since) ? 'TRACKED' : 'untracked'
		])
	);
}

// The tutorial*.test.mjs files' lessons, tabs and ways between databases (da-5rpcr).

// The lessons' layouts, by name, which their tabs say: the change lesson's is `change`, the glossary's word (da-e3rdf).
export const LESSON_NAMES = ['intro', 'moment', 'on-off', 'change', 'input-bar', 'tabs'];

/** What a game's tab says, its title (da-83y6w); another lesson's tab says its layout's name. */
export const GAME_TITLES = { moment: 'Moments in a Café', 'on-off': 'Turn the TV off', change: 'Change of scenery' };

/** The words on a lesson's tab. */
export const tabLabel = (name) => GAME_TITLES[name] ?? name;

/** The lesson a tab's words name, or the words: the tests speak of a lesson by its layout's name. */
const lessonOf = (label) => Object.keys(GAME_TITLES).find((name) => GAME_TITLES[name] === label) ?? label;

export const tabs = (page) =>
	page
		.locator('.root-bar [role="tab"]')
		.allInnerTexts()
		.then((all) => all.map(lessonOf));

export const front = (page) => page.locator('.root-bar [role="tab"][aria-selected="true"]').innerText().then(lessonOf);

/** Lesson `name`'s tab in the root bar, by its words alone (a changed tab's dot after them). */
export const lessonTab = (page, name) => page.locator('.root-bar [role="tab"]', { hasText: new RegExp(`^${tabLabel(name)}•?$`) });

/** Press `name` and wait for the page it loads, on database `to` (null: datum). */
export async function press(page, name, to) {
	await Promise.all([
		page.waitForURL((url) => url.searchParams.get('db') === to),
		page.getByRole('button', { name, exact: true }).click()
	]);
	await ready(page);
}

/** Choose `name` in the database switcher, the bottom bar's name, and wait for its page. */
export async function viaSwitcher(page, name, to) {
	await page.getByRole('button', { name: /choose a database/ }).click();
	await Promise.all([
		page.waitForURL((url) => url.searchParams.get('db') === to),
		page.getByRole('menuitemradio', { name, exact: true }).click()
	]);
	await ready(page);
}
