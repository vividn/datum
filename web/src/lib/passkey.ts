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
	return (await made(label)).passkey;
}

/** A new passkey, asked for PRF where `prf`: its credential, and its public key as `admin add` takes it (the create alone says it). */
async function made(label: string, prf = false): Promise<{ cred: PublicKeyCredential; passkey: string }> {
	const cred = (await navigator.credentials.create({
		publicKey: {
			rp: { id: RP_ID, name: 'datum' },
			user: { id: crypto.getRandomValues(new Uint8Array(16)), name: label, displayName: label },
			challenge: crypto.getRandomValues(new Uint8Array(32)),
			pubKeyCredParams: [
				{ type: 'public-key', alg: -8 },
				{ type: 'public-key', alg: -7 }
			],
			authenticatorSelection: { residentKey: 'required', userVerification: 'required' },
			extensions: prf ? { prf: {} } : undefined
		}
	})) as PublicKeyCredential | null;
	const response = cred?.response as AuthenticatorAttestationResponse | undefined;
	const key = response?.getPublicKey();
	if (!cred || !response || !key) throw new Error('no passkey was made');
	return { cred, passkey: JSON.stringify({ alg: response.getPublicKeyAlgorithm(), publicKey: b64url(key), label }) };
}

/** An assertion's signed parts: what `signedBy` checks a passkey admin's key against. */
export type Proof = { data: Uint8Array<ArrayBuffer>; sig: Uint8Array<ArrayBuffer>; id: ArrayBuffer };

/** The SPKI prefix of each passkey admin key's kind (`chain::passkey_admin_key`), and its COSE alg. */
const SPKI: Record<string, [string, number]> = {
	es256: ['3059301306072a8648ce3d020106082a8648ce3d030107034200', -7],
	eddsa: ['302a300506032b6570032100', -8]
};

/** The admin key (`es256:…`, `eddsa:…`) of a passkey as `admin add` takes it, its JSON, as `chain::passkey_admin_key` makes it; null if it reads as none. */
export function adminKeyOf(passkey: string): string | null {
	try {
		const { alg, publicKey } = JSON.parse(passkey) as { alg: number; publicKey: string };
		const kind = Object.keys(SPKI).find((k) => SPKI[k][1] === alg);
		if (!kind) return null;
		const spki = unb64url(publicKey);
		const head = SPKI[kind][0].match(/../g)!.map((h) => parseInt(h, 16));
		return head.every((b, i) => spki[i] === b) ? `${kind}:${b64url(spki.subarray(head.length))}` : null;
	} catch {
		return null;
	}
}

/** A chain's passkey admin key (`es256:…`, `eddsa:…`) as `admin add` takes it, or null for any other key. */
export function adminPasskey(key: string, label: string): string | null {
	const [kind, raw] = key.split(':');
	const [prefix, alg] = SPKI[kind] ?? [];
	if (!prefix || !raw) return null;
	const der = Uint8Array.from(prefix.match(/../g)!, (h) => parseInt(h, 16));
	return JSON.stringify({ alg, publicKey: b64url(new Uint8Array([...der, ...unb64url(raw)])), label });
}

/**
 * Whether the passkey admin `key` made the assertion `p`: so a key read from
 * storage proves itself before it becomes an admin, and a sign-in recognises
 * its passkey among a chain's admins on a device that did not make it (a
 * passkey synced to it), with no prompt more.
 */
export async function signedBy(key: string, p: Proof): Promise<boolean> {
	const [kind, raw] = key.split(':');
	try {
		if (kind === 'eddsa') {
			const k = await crypto.subtle.importKey('raw', unb64url(raw), { name: 'Ed25519' }, false, ['verify']);
			return await crypto.subtle.verify({ name: 'Ed25519' }, k, p.sig, p.data);
		}
		if (kind !== 'es256') return false;
		const k = await crypto.subtle.importKey('raw', unb64url(raw), { name: 'ECDSA', namedCurve: 'P-256' }, false, ['verify']);
		// WebAuthn's ES256 signature is DER; WebCrypto's is r ‖ s, 32 bytes each.
		const rs = new Uint8Array(64);
		for (let at = 2, half = 0; half < 2; half++) {
			const n = p.sig[at + 1];
			const v = p.sig.subarray(at + 2 + Math.max(0, n - 32), at + 2 + n);
			rs.set(v, 32 * (half + 1) - v.length);
			at += 2 + n;
		}
		return await crypto.subtle.verify({ name: 'ECDSA', hash: 'SHA-256' }, k, rs, p.data);
	} catch {
		return false;
	}
}

/**
 * Approve the record of `link` with a passkey: the approval code the CLI
 * takes, base64url of `u16 len ‖ authenticatorData ‖ u16 len ‖
 * clientDataJSON ‖ signature` (lengths big-endian), as
 * `chain::verify_assertion` parses it. The challenge is `challengeOf` the link.
 */
export async function approve(link: string): Promise<string> {
	return (await approveAs(link)).code;
}

/**
 * [`approve`], and the PRF output the passkey gave beside it, if it gave one
 * (da-k04lr): the one prompt approves as a passkey admin and as the account
 * key, an admin of each chain made signed in, whose signature the CLI makes
 * of the PRF output (`sync account --approve`). The code carries both,
 * joined by `.`, and the CLI keeps the one an admin's key verifies. `only`
 * names the passkey to ask, by its credential id; `proof` is the assertion's
 * signed parts, to check which passkey approved.
 */
export async function approveAs(link: string, only?: ArrayBuffer): Promise<{ code: string; prf: string | null; proof: Proof }> {
	const challenge = await challengeOf(link);
	const allowCredentials: PublicKeyCredentialDescriptor[] | undefined = only && [{ type: 'public-key', id: only }];
	const cred = (await navigator.credentials.get({
		publicKey: { rpId: RP_ID, challenge, userVerification: 'required', allowCredentials, extensions: { prf: { eval: { first: PRF_SALT } } } }
	})) as PublicKeyCredential | null;
	const r = cred?.response as AuthenticatorAssertionResponse | undefined;
	if (!r) throw new Error('the passkey gave no approval');
	const parts = [new Uint8Array(r.authenticatorData), new Uint8Array(r.clientDataJSON)];
	const out: number[] = [];
	for (const p of parts) out.push(p.length >> 8, p.length & 0xff, ...p);
	out.push(...new Uint8Array(r.signature));
	const webauthn = b64url(new Uint8Array(out));
	const prf = prfOf(cred);
	const proof = await proofOf(cred!);
	if (!prf) return { code: webauthn, prf, proof };
	const [{ run }, { accountApproveArgv }] = await Promise.all([import('./run.js'), import('./calls.js')]);
	const signed = await run(accountApproveArgv(link), undefined, undefined, undefined, undefined, [`DATUM_ACCOUNT=${prf}`]);
	const account = signed.code === 0 ? (JSON.parse(signed.stdout) as { code: string }).code : null;
	return { code: account ? `${account}.${webauthn}` : webauthn, prf, proof };
}

/** An assertion's signed parts: its authenticator data and its client data's SHA-256, its signature, and its credential id. */
async function proofOf(cred: PublicKeyCredential): Promise<Proof> {
	const r = cred.response as AuthenticatorAssertionResponse;
	const hash = new Uint8Array(await crypto.subtle.digest('SHA-256', r.clientDataJSON));
	return { data: new Uint8Array([...new Uint8Array(r.authenticatorData), ...hash]), sig: new Uint8Array(r.signature), id: cred.rawId };
}

/** The PRF output an assertion gave, hex, or null. */
function prfOf(cred: PublicKeyCredential | null): string | null {
	const first = cred?.getClientExtensionResults().prf?.results?.first;
	if (!first) return null;
	const bytes = ArrayBuffer.isView(first) ? new Uint8Array(first.buffer, first.byteOffset, first.byteLength) : new Uint8Array(first);
	return hex(bytes.slice().buffer);
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
 * Sign in with a passkey: the PRF output of an assertion, hex, which the
 * page holds only for the act it was asked for (da-k04lr). `make` names a new passkey made first; even
 * then the secret comes from an assertion, since an authenticator need not
 * answer PRF as it makes one. Throws `NO_PRF` where the passkey gives none.
 * Beside it, a new passkey's public key, for the chains' admin (da-wsx4k),
 * and the assertion's signed parts, to recognise one made elsewhere by.
 */
export async function signIn(make?: string): Promise<{ prf: string; passkey: string | null; proof: Proof }> {
	let allow: PublicKeyCredentialDescriptor[] | undefined;
	let passkey: string | null = null;
	if (make) {
		const m = await made(make, true);
		if (m.cred.getClientExtensionResults().prf?.enabled === false) throw new Error(NO_PRF);
		allow = [{ type: 'public-key', id: m.cred.rawId }];
		passkey = m.passkey;
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
	const prf = prfOf(cred);
	if (!prf) throw new Error(NO_PRF);
	return { prf, passkey, proof: await proofOf(cred!) };
}
