const DEVICE_ID_KEY = 'datum-device-id';

/**
 * This browser's device id, minted on first use and kept in localStorage.
 *
 * Kept if it can be: a store the browser blocks (a locked-down profile,
 * some private windows) throws on the property itself, and losing the id
 * across reloads is a much smaller thing than every command in the app
 * failing to run.
 */
let sessionDeviceId: string | undefined;
export function deviceId(): string {
	const minted = () => (sessionDeviceId ??= 'web-' + Math.random().toString(36).slice(2, 10));
	try {
		const existing = localStorage.getItem(DEVICE_ID_KEY);
		if (existing) return existing;
		const id = minted();
		localStorage.setItem(DEVICE_ID_KEY, id);
		return id;
	} catch {
		return minted();
	}
}
