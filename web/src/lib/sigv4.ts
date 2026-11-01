// AWS Signature Version 4 for S3, on WebCrypto, held to AWS's published S3
// vectors in `tests/s3-pure.test.mjs`. It signs every header it is given and
// never reads a body: `x-amz-content-sha256` says what the payload is, and
// unless a caller says otherwise it is `UNSIGNED-PAYLOAD`.
//
// No imports, so the tests' fake S3 can check a signature with it as well.

export interface SigV4Request {
	url: string;
	method?: string;
	headers?: Record<string, string>;
	key: string;
	secret: string;
	region: string;
	/** `YYYYMMDDTHHMMSSZ`; blank is now. */
	datetime?: string;
}

const utf8 = (text: string) => new TextEncoder().encode(text);

const hmac = async (key: BufferSource, text: string) =>
	crypto.subtle.sign('HMAC', await crypto.subtle.importKey('raw', key, { name: 'HMAC', hash: 'SHA-256' }, false, ['sign']), utf8(text));

const hex = (bytes: ArrayBuffer) => [...new Uint8Array(bytes)].map((b) => b.toString(16).padStart(2, '0')).join('');

/** RFC 3986's escaping: `encodeURIComponent` leaves these five bare. */
const encode = (text: string) => encodeURIComponent(text).replace(/[!'()*]/g, (c) => `%${c.charCodeAt(0).toString(16).toUpperCase()}`);

/** S3's canonical path is the object's key escaped once: a `+` in a path is a space, and a stray `%` stands for itself. */
function canonicalPath(pathname: string): string {
	let path = pathname;
	try {
		path = decodeURIComponent(pathname.replace(/\+/g, ' '));
	} catch {}
	return encode(path).replace(/%2F/g, '/');
}

/** Sorted by key: `s3.ts` never names one twice. */
const canonicalQuery = (params: URLSearchParams) =>
	[...params]
		.map(([k, v]) => [encode(k), encode(v)])
		.sort(([a], [b]) => (a < b ? -1 : 1))
		.map((pair) => pair.join('='))
		.join('&');

/** The request with its `x-amz-*` and `authorization` headers set. */
export async function signV4({ url: href, method = 'GET', headers: given, key, secret, region, datetime }: SigV4Request) {
	const url = new URL(href);
	const when = datetime || new Date().toISOString().replace(/[:-]|\.\d{3}/g, '');
	const headers = new Headers(given);
	headers.delete('host');
	if (!headers.has('x-amz-content-sha256')) headers.set('x-amz-content-sha256', 'UNSIGNED-PAYLOAD');
	headers.set('x-amz-date', when);

	const names = ['host', ...headers.keys()].sort();
	const signedHeaders = names.join(';');
	const canonicalHeaders = names.map((n) => `${n}:${n === 'host' ? url.host : (headers.get(n) ?? '').replace(/\s+/g, ' ')}`).join('\n');
	const canonical = [
		method,
		canonicalPath(url.pathname),
		canonicalQuery(url.searchParams),
		`${canonicalHeaders}\n`,
		signedHeaders,
		headers.get('x-amz-content-sha256')
	].join('\n');

	const date = when.slice(0, 8);
	const scope = `${date}/${region}/s3/aws4_request`;
	const toSign = ['AWS4-HMAC-SHA256', when, scope, hex(await crypto.subtle.digest('SHA-256', utf8(canonical)))].join('\n');
	let signing = await hmac(utf8(`AWS4${secret}`), date);
	for (const part of [region, 's3', 'aws4_request']) signing = await hmac(signing, part);

	headers.set('authorization', `AWS4-HMAC-SHA256 Credential=${key}/${scope}, SignedHeaders=${signedHeaders}, Signature=${hex(await hmac(signing, toSign))}`);
	return { method, url, headers };
}
