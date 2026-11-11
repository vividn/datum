// The account a passkey signed this tab in to (da-nf1p4): its PRF output,
// kept in this tab's memory alone and never stored, so a reload or a
// sign-out ends it. Every command `run` sends carries it as `DATUM_ACCOUNT`,
// from which the CLI derives the account key (`chain::account_keys`): an
// admin of each chain the account makes, which signs that chain's acts and
// the invite this device redeems to join one.

import { writable, type Readable } from 'svelte/store';

let prf: string | null = null;
const signed = writable<{ key: string } | null>(null);

/** The account key this tab is signed in to, `ed25519:<hex>`, or null. */
export const account: Readable<{ key: string } | null> = signed;

/** What `run` adds to a command's environment. */
export const accountEnv = (): string[] => (prf ? [`DATUM_ACCOUNT=${prf}`] : []);

/** Signed in with `secret`, whose account key is `key`; null signs out. */
export function setAccount(secret: string | null, key?: string): void {
	prf = secret;
	signed.set(secret && key ? { key } : null);
}
