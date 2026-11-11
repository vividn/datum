const CLIENT_ID_KEY = 'datum-client-id';
/** Where the id lived before da-lfsei renamed it, read once so a browser keeps its id. */
const OLD_KEY = 'datum-device-id';

/**
 * This browser's client id — one per profile, not per machine — minted on
 * first use and kept in localStorage.
 *
 * Kept if it can be: a store the browser blocks (a locked-down profile,
 * some private windows) throws on the property itself, and losing the id
 * across reloads is a much smaller thing than every command in the app
 * failing to run.
 */
let sessionClientId: string | undefined;
export function clientId(): string {
	const minted = () => (sessionClientId ??= 'web-' + Math.random().toString(36).slice(2, 10));
	try {
		const existing = localStorage.getItem(CLIENT_ID_KEY);
		if (existing) return existing;
		const id = localStorage.getItem(OLD_KEY) || minted();
		localStorage.setItem(CLIENT_ID_KEY, id);
		localStorage.removeItem(OLD_KEY);
		return id;
	} catch {
		return minted();
	}
}

const DEVICE_KEY = 'datum-device-key';

/**
 * This browser's device key (da-n0uj1.2): an Ed25519 secret, hex, one per
 * profile like the client id and kept beside it, which the CLI reads as
 * `DATUM_DEVICE_KEY` to sign that each database's endpoint here is this
 * device's. Cleared site data makes a new device; a store the browser
 * blocks gives one for the session.
 */
let sessionDeviceKey: string | undefined;
export function deviceKey(): string {
	const minted = () =>
		(sessionDeviceKey ??= Array.from(crypto.getRandomValues(new Uint8Array(32)), (b) =>
			b.toString(16).padStart(2, '0')
		).join(''));
	try {
		const existing = localStorage.getItem(DEVICE_KEY);
		if (existing) return existing;
		const key = minted();
		localStorage.setItem(DEVICE_KEY, key);
		return key;
	} catch {
		return minted();
	}
}

const USERNAME_KEY = 'datum-username';
const DEVICE_NAME_KEY = 'datum-device-name';

/** What this browser keeps under `key`; empty is none, as is a store the browser blocks. */
function kept(key: string): string {
	try {
		return localStorage.getItem(key) ?? '';
	} catch {
		return '';
	}
}

/** Keep `value` under `key`; empty forgets it. Blocked, it is kept for nothing later, as for none. */
function keep(key: string, value: string): void {
	try {
		if (value.trim()) localStorage.setItem(key, value.trim());
		else localStorage.removeItem(key);
	} catch {
		// Blocked.
	}
}

/**
 * The username this browser's person gave (da-xza37), offered in every sync
 * chain a database here enters and described as its `user`: the CLI reads it
 * as `DATUM_USERNAME`.
 */
export const username = (): string => kept(USERNAME_KEY);

/** Keep `name` as this browser's username; empty forgets it. */
export const setUsername = (name: string): void => keep(USERNAME_KEY, name);

/**
 * The name this browser's person gave this device in Sync (da-5wlnr), kept
 * for a database in no chain yet: its chain starts with it when the
 * database is first shared. Empty is none: the chain assembles one.
 */
export const deviceName = (): string => kept(DEVICE_NAME_KEY);

export const setDeviceName = (name: string): void => keep(DEVICE_NAME_KEY, name);
