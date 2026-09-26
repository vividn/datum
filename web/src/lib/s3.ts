// An S3-COMPATIBLE bucket as a backup destination (da-pstzw): AWS S3,
// Cloudflare R2, Backblaze B2, Wasabi, MinIO — one client, path-style URLs
// (`<endpoint>/<bucket>/<prefix><file>`), which every one of them serves.
//
// The browser talks to the bucket itself, so the bucket must allow this
// page's origin (CORS, `corsRules`); nothing else stands between them.
// Requests are signed with SigV4 by aws4fetch (a small, dependency-free
// signer), held to AWS's published S3 vectors in `tests/s3-pure.test.mjs`.
//
// Kept free of the app — no stores, no `run` — so node can import it.

import { AwsV4Signer } from 'aws4fetch';

export interface S3Config {
	/** Blank for AWS: `https://s3.<region>.amazonaws.com`. */
	endpoint?: string;
	bucket?: string;
	/** Blank means `us-east-1`; R2 takes `auto`. */
	region?: string;
	prefix?: string;
	key?: string;
	secret?: string;
}

export const DEFAULT_PREFIX = 'datum-backups/';

const region = (c: S3Config) => c.region?.trim() || 'us-east-1';

const base = (c: S3Config) =>
	`${(c.endpoint?.trim() || `https://s3.${region(c)}.amazonaws.com`).replace(/\/+$/, '')}/${encodeURIComponent(c.bucket?.trim() ?? '')}`;

const objectUrl = (c: S3Config, file: string) =>
	`${base(c)}/${`${c.prefix ?? ''}${file}`.split('/').map(encodeURIComponent).join('/')}`;

/** A request for `url`, signed with `c`'s key — `datetime` and `allHeaders` are for the test vectors. */
export function sign(
	c: S3Config,
	url: string,
	init: { method?: string; headers?: Record<string, string>; body?: BodyInit; datetime?: string; allHeaders?: boolean } = {}
) {
	return new AwsV4Signer({
		...init,
		url,
		accessKeyId: c.key?.trim() ?? '',
		secretAccessKey: c.secret?.trim() ?? '',
		service: 's3',
		region: region(c)
	}).sign();
}

/** An S3 refusal, with what it most likely means. */
export class S3Error extends Error {
	code: string;
	constructor(code: string, message: string) {
		super(message);
		this.code = code;
	}
}

/** What a failed request most likely means, in the reader's terms. */
function explain(c: S3Config, status: number, code: string, message: string): string {
	if (code === 'InvalidAccessKeyId' || code === 'SignatureDoesNotMatch')
		return `credentials: the access key id or secret is wrong (${code})`;
	if (code === 'AccessDenied') return 'credentials: the key is not allowed to do this in this bucket (AccessDenied)';
	if (code === 'NoSuchBucket') return `bucket name: there is no bucket «${c.bucket}» at this endpoint`;
	if (code === 'AuthorizationHeaderMalformed' || code === 'PermanentRedirect' || status === 301)
		return `region: the bucket is not in «${region(c)}» (${code || status})`;
	return `${status} ${code}${message ? `: ${message}` : ''}`.trim();
}

/** Send one signed request; a refusal throws an `S3Error` naming its likely cause. */
async function send(c: S3Config, method: string, url: string, body?: Uint8Array): Promise<Response> {
	let response: Response;
	try {
		const signed = await sign(c, url, { method, body: body as BodyInit | undefined });
		response = await fetch(signed.url, signed);
	} catch {
		// The browser hides WHY a cross-origin request got no response.
		throw new S3Error('', 'CORS: the bucket does not allow this page — paste the CORS rules below — or the endpoint URL is wrong, or this device is offline');
	}
	if (response.ok) return response;
	const xml = await response.text();
	const tag = (name: string) => xml.match(new RegExp(`<${name}>([^<]*)</${name}>`))?.[1] ?? '';
	throw new S3Error(tag('Code'), explain(c, response.status, tag('Code'), tag('Message')));
}

/** The destination: `get`, `put`, `remove` on files under the prefix. */
export function s3(c: S3Config) {
	return {
		async get(file: string): Promise<Uint8Array | null> {
			try {
				return new Uint8Array(await (await send(c, 'GET', objectUrl(c, file))).arrayBuffer());
			} catch (e) {
				if (e instanceof S3Error && e.code === 'NoSuchKey') return null;
				throw e;
			}
		},
		put: async (file: string, bytes: Uint8Array) => void (await send(c, 'PUT', objectUrl(c, file), bytes)),
		remove: async (file: string) => void (await send(c, 'DELETE', objectUrl(c, file))),
		/** The keys under the prefix that start with `start` — the first thousand. */
		async list(start = ''): Promise<string[]> {
			const url = `${base(c)}?list-type=2&prefix=${encodeURIComponent(`${c.prefix ?? ''}${start}`)}`;
			const xml = await (await send(c, 'GET', url)).text();
			return [...xml.matchAll(/<Key>([^<]*)<\/Key>/g)].map((m) => m[1]);
		}
	};
}

export type Step = 'write' | 'list' | 'read' | 'delete';

/** One step of `testConnection`: done, or failed and why. */
export interface StepResult {
	step: Step;
	ok: boolean;
	cause?: string;
}

/**
 * Write, list, read and delete a small probe object under the prefix —
 * everything a plan does — stopping at the first step that fails.
 */
export async function testConnection(c: S3Config): Promise<StepResult[]> {
	const bucket = s3(c);
	const file = `datum-probe-${Date.now().toString(36)}.txt`;
	const bytes = new TextEncoder().encode('datum.day connection test\n');
	const steps: [Step, () => Promise<unknown>][] = [
		['write', () => bucket.put(file, bytes)],
		[
			'list',
			async () => {
				// Listed by its own name, so the thousand-key page cannot leave it out.
				if (!(await bucket.list(file)).includes(`${c.prefix ?? ''}${file}`)) throw new Error('the probe written is not listed');
			}
		],
		[
			'read',
			async () => {
				const back = await bucket.get(file);
				if (!back || new TextDecoder().decode(back) !== new TextDecoder().decode(bytes)) throw new Error('the probe read back differs');
			}
		],
		['delete', () => bucket.remove(file)]
	];
	const results: StepResult[] = [];
	for (const [step, go] of steps) {
		try {
			await go();
			results.push({ step, ok: true });
		} catch (e) {
			results.push({ step, ok: false, cause: e instanceof Error ? e.message : String(e) });
			break;
		}
	}
	return results;
}

/** The CORS rules a bucket needs for this page, in the JSON S3 and R2 take. LIST is a GET on the bucket. */
export const corsRules = (origin: string): string =>
	JSON.stringify(
		[{ AllowedOrigins: [origin], AllowedMethods: ['GET', 'PUT', 'DELETE'], AllowedHeaders: ['*'], MaxAgeSeconds: 3000 }],
		null,
		2
	);
