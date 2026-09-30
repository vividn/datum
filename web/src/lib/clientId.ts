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
