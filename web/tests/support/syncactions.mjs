// What more than one of the Sync widget's action files needs: `syncactions`,
// `syncactions-leave` and `syncactions-purge` (da-xwjul).

import assert from 'node:assert/strict';
import { generateKeyPairSync } from 'node:crypto';

export const WIDGET = 'section.pane[data-title="sync"]';
export const DESKTOP = { width: 1280, height: 800 };
export const AT_DATUM_DAY = { origin: 'https://datum.day', viewport: DESKTOP };

export const endpointId = () =>
	generateKeyPairSync('ed25519').publicKey.export({ format: 'der', type: 'spki' }).subarray(-32).toString('hex');

/** `datum <argv>` in the page, whatever it exits with. */
export const runIn = (app, argv, stdin = '') => app.page.evaluate(([a, s]) => window.datum.runQuiet(a, s), [argv, stdin]);

export async function cli(app, argv, stdin) {
	const r = await runIn(app, argv, stdin);
	assert.equal(r.code, 0, `${argv.join(' ')}: ${r.stderr}`);
	return r.stdout;
}

/** `home` in an open chain with a member `label` beside this copy, admitted as a session admits one; its endpoint. */
export async function openChain(app, label = 'phone') {
	const { secret } = JSON.parse(await cli(app, ['--db', 'home', 'sync', 'new-invite', '--expires', '1d']));
	const phone = endpointId();
	await cli(app, ['--db', 'home', 'sync', 'handshake', phone, '--label', label, '--addr', '{}', '--invite', secret, '--chain', '']);
	return phone;
}

export const members = async (app) => JSON.parse(await cli(app, ['--db', 'home', 'sync', 'chain', 'list', '--json']));

/** A passkey the page can make and use, with the user verified; `more`, its other options (`hasPrf`). The CDP session comes back. */
export async function authenticator(page, more = {}) {
	const cdp = await page.context().newCDPSession(page);
	await cdp.send('WebAuthn.enable');
	const { authenticatorId } = await cdp.send('WebAuthn.addVirtualAuthenticator', {
		options: {
			protocol: 'ctap2',
			transport: 'internal',
			hasResidentKey: true,
			hasUserVerification: true,
			isUserVerified: true,
			automaticPresenceSimulation: true,
			...more
		}
	});
	return { cdp, authenticatorId };
}

/** Lock `home`'s chain with a new passkey, under the widget's grid. */
export async function addAdmin(page) {
	await page.locator(`${WIDGET} .chain[data-db="home"]`).getByRole('button', { name: 'add admin' }).click();
	await page.locator('dialog.ask input').fill('my phone');
	await page.locator('dialog.ask').getByRole('button', { name: 'add' }).click();
	await page.locator(`${WIDGET} .chain[data-db="home"] .admin`).waitFor();
}

/** `act` chosen in the dropdown of `label`'s grid cell in `db` (da-5wlnr: the grid is the one list of devices). */
export const cellAct = (page, label, db, act) => page.locator(`${WIDGET} .grid select[aria-label="${label} in ${db}"]`).selectOption(act);

/** The grid's device rows' labels. */
export const gridLabels = (page) => page.locator(`${WIDGET} .grid tbody th`).allInnerTexts();
