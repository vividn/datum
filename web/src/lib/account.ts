// The account a passkey signed this tab in to (da-nf1p4): what the CLI
// gives a device to keep of it, `{"key", "seal"}`, held in this tab's memory
// alone and never stored, so a reload or a sign-out ends it. Every command
// `run` sends carries it as `DATUM_ACCOUNT`: enough to read the account's
// record, and nothing that signs for it. The passkey's PRF output, from
// which the CLI derives the account key, is asked for each act that signs
// (an admin act, a sign-in's joins, a CLI's login) and dropped after it
// (da-k04lr).

import { writable, type Readable } from 'svelte/store';

let kept: string | null = null;
const signed = writable<{ key: string } | null>(null);

/** The account key this tab is signed in to, `ed25519:<hex>`, or null. */
export const account: Readable<{ key: string } | null> = signed;

/** What `run` adds to a command's environment. */
export const accountEnv = (): string[] => (kept ? [`DATUM_ACCOUNT=${kept}`] : []);

/** Signed in, keeping `held` (the CLI's `kept`), whose account key is `key`; null signs out. */
export function setAccount(held: string | null, key?: string): void {
	kept = held;
	signed.set(held && key ? { key } : null);
}
