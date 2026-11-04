// The switch lesson's game starts itself (da-uzyih.4867): as its cue says,
// by switching `room` to the room the person stands in.

import { until } from './until.mjs';

const SWITCH = '[data-pane="switch-lesson"]';
/** Its fields, a pane of their own under the game (da-uzyih.4908). */
const FIELDS = '[data-pane="switch-fields"]';

/** How many entries `room` holds. */
export const roomEntries = (page) =>
	page.evaluate(() => window.datum.runQuiet(['tail', 'room', '--json', '-n', '200']).then((r) => JSON.parse(r.stdout).rows.length));

/** A tap of `room`'s button, then of `room` among its states, landed. */
export async function switchRoom(page, room, pane = FIELDS) {
	const before = await roomEntries(page);
	await page.locator(`${pane} .fields button[data-field="room"]`).click();
	await page.locator(`${pane} .fields .ring button[data-state="${room}"]`).click();
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
