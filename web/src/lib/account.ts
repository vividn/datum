// The account a passkey signed this tab in to (da-nf1p4): what the CLI
// gives a device to keep of it, `{"key", "seal"}`, kept in this browser
// until «Sign out», so a reload, an upgrade or a new tab stays signed in
// (da-wsx4k), as a terminal's `datum sync login` keeps it on disk. Every
// command `run` sends carries it as `DATUM_ACCOUNT`: enough to read the
// account's record, and nothing that signs for it. The passkey's PRF output,
// from which the CLI derives the account key, is asked for each act that
// signs (an admin act, a sign-in's joins, a CLI's login) and dropped after
// it, never stored (da-k04lr).
//
// Beside it, by account key, the public key of the passkey it signs in with,
// as `datum sync admin add` takes it: the chain's admin passkey is the
// sign-in passkey, with no second credential (da-wsx4k). A public key signs
// nothing either, so it stays past a sign-out. But what is stored, anything
// that ran on the origin could have written: the key is an admin only once
// it is PROVEN, in this tab's memory, by its create or by an assertion made
// since the tab loaded (`provenPasskey`, `prove`); stored alone it is a hint.

import { writable, type Readable } from 'svelte/store';

const KEPT = 'datum.account';
const PASSKEYS = 'datum.accountPasskeys';

type Kept = { held: string; key: string };

function load<T>(name: string, fallback: T): T {
	try {
		return JSON.parse(localStorage.getItem(name) ?? 'null') ?? fallback;
	} catch {
		return fallback;
	}
}

function store(name: string, value: unknown): void {
	try {
		if (value === null) localStorage.removeItem(name);
		else localStorage.setItem(name, JSON.stringify(value));
	} catch {
		// Not kept: this tab stays signed in until it reloads.
	}
}

let kept = load<Kept | null>(KEPT, null);
const signed = writable<{ key: string } | null>(kept && { key: kept.key });

/** The account key this tab is signed in to, `ed25519:<hex>`, or null. */
export const account: Readable<{ key: string } | null> = signed;

/** The sign-in passkey of the account signed in, as `admin add` takes it (`{"alg", "publicKey", "label"}`), as stored: a hint, unproven. */
export const accountPasskey = (): string | null => (kept && load<Record<string, string>>(PASSKEYS, {})[kept.key]) || null;

/** The passkeys a create or an assertion in this tab proved, in its memory alone. */
const proven = new Set<string>();

/** Mark `passkey` proven, by its create or an assertion it made in this tab. */
export const prove = (passkey: string): void => void proven.add(passkey);

/** [`accountPasskey`], where this tab has proven it: the only form that becomes an admin. */
export const provenPasskey = (): string | null => {
	const p = accountPasskey();
	return p && proven.has(p) ? p : null;
};

/** Remember `passkey`, proven, as the sign-in passkey of `key`. */
export function keepPasskey(key: string, passkey: string): void {
	store(PASSKEYS, { ...load(PASSKEYS, {}), [key]: passkey });
	prove(passkey);
}

/** What `run` adds to a command's environment: the kept form, naming its passkey where proven, which a chain started signed in takes as an admin. */
export function accountEnv(): string[] {
	if (!kept) return [];
	let named = kept.held;
	try {
		const passkey = provenPasskey();
		if (passkey) named = JSON.stringify({ ...JSON.parse(kept.held), passkey: JSON.parse(passkey) });
	} catch {
		// A stored passkey that does not read: the account without it.
	}
	return [`DATUM_ACCOUNT=${named}`];
}

/** Signed in, keeping `held` (the CLI's `kept`), whose account key is `key`; null signs out, in every tab. */
export function setAccount(held: string | null, key?: string): void {
	kept = held && key ? { held, key } : null;
	store(KEPT, kept);
	signed.set(kept && { key: kept.key });
}

// Another tab signed in or out, or came to know the account's passkey.
if (typeof window !== 'undefined')
	window.addEventListener('storage', (e) => {
		if (e.key !== KEPT && e.key !== PASSKEYS && e.key !== null) return;
		kept = load<Kept | null>(KEPT, null);
		signed.set(kept && { key: kept.key });
	});
