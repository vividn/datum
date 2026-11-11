// A passkey as the sync chain's admin key (da-bjxts): WebAuthn in the page,
// relying party `datum.day`, as `crates/datum-core/src/db/sync/chain.rs`
// verifies it. The page makes no record and checks no signature: the CLI
// builds the record and prints its approve link, and takes the code back
// (`datum sync approve <link>`), refusing one the chain would not take.

const RP_ID = 'datum.day';

const b64url = (bytes: ArrayBuffer | Uint8Array): string =>
	btoa(String.fromCharCode(...new Uint8Array(bytes)))
		.replace(/\+/g, '-')
		.replace(/\//g, '_')
		.replace(/=+$/, '');

const unb64url = (s: string): Uint8Array<ArrayBuffer> =>
	Uint8Array.from(atob(s.replace(/-/g, '+').replace(/_/g, '/')), (c) => c.charCodeAt(0));

/** The fragment of an approve link: the record, canonical JSON, base64url. */
const fragment = (link: string): string => link.slice(link.lastIndexOf('#') + 1).trim();

/** What an approve link's fragment reads as: a record, or a batch's list of records as canonical JSON text (da-7bjai). */
function carried(link: string): unknown {
	try {
		return JSON.parse(new TextDecoder().decode(unb64url(fragment(link))));
	} catch {
		return null;
	}
}

const isRecord = (v: unknown): v is Record<string, unknown> => !!v && typeof v === 'object' && !Array.isArray(v);

/** Every record an approve link carries: its one, or a batch's. */
export function approveRecords(link: string): Record<string, unknown>[] | null {
	const v = carried(link);
	if (isRecord(v)) return [v];
	if (!Array.isArray(v) || !v.length || !v.every((t) => typeof t === 'string')) return null;
	try {
		const records = v.map((t) => JSON.parse(t));
		return records.every(isRecord) ? records : null;
	} catch {
		return null;
	}
}

const hex = (b: ArrayBuffer): string => [...new Uint8Array(b)].map((x) => x.toString(16).padStart(2, '0')).join('');

/**
 * The challenge for `link`: a record's digest, SHA-256 of the fragment's bytes, the link carrying the
 * canonical record; a batch's, SHA-256 of the JSON list of each record text's SHA-256, hex
 * (`chain::batch`), so one approval names exactly the records shown.
 */
async function challengeOf(link: string): Promise<ArrayBuffer> {
	const bytes = unb64url(fragment(link));
	const v = carried(link);
	if (!Array.isArray(v)) return crypto.subtle.digest('SHA-256', bytes);
	const enc = new TextEncoder();
	const digests = await Promise.all(v.map(async (t: string) => hex(await crypto.subtle.digest('SHA-256', enc.encode(t)))));
	return crypto.subtle.digest('SHA-256', enc.encode(JSON.stringify(digests)));
}

/** The approve link in what a command wrote, if it stopped at one. */
export function approveLink(text: string): string | null {
	return text.match(/https:\/\/datum\.day\/sync\/approve#[A-Za-z0-9_-]+/)?.[0] ?? null;
}

/**
 * A new passkey, as `datum sync admin add` takes it:
 * `{"alg", "publicKey", "label"}` (`chain::passkey_admin_key`).
 */
export async function createPasskey(label: string): Promise<string> {
	const cred = (await navigator.credentials.create({
		publicKey: {
			rp: { id: RP_ID, name: 'datum' },
			user: { id: crypto.getRandomValues(new Uint8Array(16)), name: label, displayName: label },
			challenge: crypto.getRandomValues(new Uint8Array(32)),
			pubKeyCredParams: [
				{ type: 'public-key', alg: -8 },
				{ type: 'public-key', alg: -7 }
			],
			authenticatorSelection: { residentKey: 'required', userVerification: 'required' }
		}
	})) as PublicKeyCredential | null;
	const response = cred?.response as AuthenticatorAttestationResponse | undefined;
	const key = response?.getPublicKey();
	if (!response || !key) throw new Error('no passkey was made');
	return JSON.stringify({ alg: response.getPublicKeyAlgorithm(), publicKey: b64url(key), label });
}

/**
 * Approve the record of `link` with a passkey: the approval code the CLI
 * takes, base64url of `u16 len ‖ authenticatorData ‖ u16 len ‖
 * clientDataJSON ‖ signature` (lengths big-endian), as
 * `chain::verify_assertion` parses it. The challenge is `challengeOf` the link.
 */
export async function approve(link: string): Promise<string> {
	const challenge = await challengeOf(link);
	const cred = (await navigator.credentials.get({
		publicKey: { rpId: RP_ID, challenge, userVerification: 'required' }
	})) as PublicKeyCredential | null;
	const r = cred?.response as AuthenticatorAssertionResponse | undefined;
	if (!r) throw new Error('the passkey gave no approval');
	const parts = [new Uint8Array(r.authenticatorData), new Uint8Array(r.clientDataJSON)];
	const out: number[] = [];
	for (const p of parts) out.push(p.length >> 8, p.length & 0xff, ...p);
	out.push(...new Uint8Array(r.signature));
	return b64url(new Uint8Array(out));
}

/**
 * What a passkey's WebAuthn PRF is asked of to sign in (da-nf1p4), as
 * `chain::ACCOUNT_PRF_SALT`: the same 32 bytes come back on every device the
 * passkey is synced to, and the CLI derives the account's keys from them.
 */
const PRF_SALT = new TextEncoder().encode('datum.day account v1');

/** Said when this browser or passkey gives no PRF: invites stay the way in. */
export const NO_PRF =
	'This browser or passkey can’t sign in to sync (it gives no WebAuthn PRF). Passkeys in iCloud Keychain (iOS 18.4, macOS 15.4 and later) and Google Password Manager can; Windows Hello’s stay on one device. Add a device with an invite instead.';

/** False when the browser says it has no PRF; true when it says it has, or does not say. */
export async function prfOffered(): Promise<boolean> {
	const caps = await (
		PublicKeyCredential as unknown as { getClientCapabilities?: () => Promise<Record<string, boolean>> }
	)
		.getClientCapabilities?.()
		.catch(() => undefined);
	return caps?.['extension:prf'] !== false;
}

/**
 * Sign in with a passkey: the PRF output of an assertion, hex, which only the
 * page holds, for the session. `make` names a new passkey made first; even
 * then the secret comes from an assertion, since an authenticator need not
 * answer PRF as it makes one. Throws `NO_PRF` where the passkey gives none.
 */
export async function signIn(make?: string): Promise<string> {
	let allow: PublicKeyCredentialDescriptor[] | undefined;
	if (make) {
		const cred = (await navigator.credentials.create({
			publicKey: {
				rp: { id: RP_ID, name: 'datum' },
				user: { id: crypto.getRandomValues(new Uint8Array(16)), name: make, displayName: make },
				challenge: crypto.getRandomValues(new Uint8Array(32)),
				pubKeyCredParams: [
					{ type: 'public-key', alg: -8 },
					{ type: 'public-key', alg: -7 }
				],
				authenticatorSelection: { residentKey: 'required', userVerification: 'required' },
				extensions: { prf: {} }
			}
		})) as PublicKeyCredential | null;
		if (!cred) throw new Error('no passkey was made');
		if (cred.getClientExtensionResults().prf?.enabled === false) throw new Error(NO_PRF);
		allow = [{ type: 'public-key', id: cred.rawId }];
	}
	const cred = (await navigator.credentials.get({
		publicKey: {
			rpId: RP_ID,
			challenge: crypto.getRandomValues(new Uint8Array(32)),
			userVerification: 'required',
			allowCredentials: allow,
			extensions: { prf: { eval: { first: PRF_SALT } } }
		}
	})) as PublicKeyCredential | null;
	const first = cred?.getClientExtensionResults().prf?.results?.first;
	if (!first) throw new Error(NO_PRF);
	const bytes = ArrayBuffer.isView(first) ? new Uint8Array(first.buffer, first.byteOffset, first.byteLength) : new Uint8Array(first);
	return hex(bytes.slice().buffer);
}
