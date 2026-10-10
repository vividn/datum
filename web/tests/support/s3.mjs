// A fake S3 for the backups tests (da-pstzw): a real HTTP server on another
// origin than the app's, so the browser does real CORS and real preflights
// against it, and every request's SigV4 signature is checked — recomputed
// with the secret the server holds — the way S3 refuses a wrong secret.
//
// Path-style only (`/<bucket>/<key>`), and only what the destination uses:
// PUT, GET and DELETE of an object, and ListObjectsV2 on a bucket. CORS is
// S3's: a preflight is answered by echoing the headers it asks for, which
// is what makes `AllowedHeaders: ["*"]` cover `authorization`.

import { createServer } from 'node:http';
import { AwsV4Signer } from 'aws4fetch';

export const KEY = { key: 'AKIDFAKE', secret: 'fake-secret' };

/**
 * Start one. `buckets` are the buckets that exist; `server.objects(bucket)`
 * is one's contents; `server.requests` is every request that reached it,
 * `METHOD key`, in order; `server.cors = false` stops answering preflights;
 * `server.hold(match)` stops the first request `match(method, key)`
 * accepts until `release()`, and `reached` says it arrived.
 */
export async function startS3(buckets = ['backups']) {
	const store = new Map(buckets.map((b) => [b, new Map()]));
	const holds = [];
	const requests = [];
	const state = { cors: true };

	const reply = (res, status, body = '', headers = {}) => {
		res.writeHead(status, headers);
		res.end(body);
	};
	const error = (res, status, code) =>
		reply(res, status, `<?xml version="1.0" encoding="UTF-8"?><Error><Code>${code}</Code><Message>${code}</Message></Error>`, {
			'content-type': 'application/xml'
		});

	async function signed(req, url) {
		const auth = req.headers.authorization ?? '';
		const [, key, region] = auth.match(/Credential=([^/]+)\/\d+\/([^/]+)\/s3\/aws4_request/) ?? [];
		if (key !== KEY.key) return 'InvalidAccessKeyId';
		const names = auth.match(/SignedHeaders=([^,]+)/)?.[1].split(';') ?? [];
		const headers = Object.fromEntries(names.filter((n) => n !== 'host' && n !== 'x-amz-date').map((n) => [n, req.headers[n]]));
		const again = await new AwsV4Signer({
			method: req.method,
			url: url.href,
			headers,
			accessKeyId: KEY.key,
			secretAccessKey: KEY.secret,
			service: 's3',
			region,
			datetime: req.headers['x-amz-date']
		}).sign();
		return again.headers.get('authorization') === auth ? '' : 'SignatureDoesNotMatch';
	}

	const server = createServer(async (req, res) => {
		const origin = req.headers.origin;
		if (req.method === 'OPTIONS') {
			if (!state.cors) return reply(res, 403);
			return reply(res, 200, '', {
				'access-control-allow-origin': origin,
				'access-control-allow-methods': 'GET, PUT, DELETE',
				'access-control-allow-headers': req.headers['access-control-request-headers'] ?? '',
				'access-control-max-age': '0'
			});
		}
		if (state.cors && origin) res.setHeader('access-control-allow-origin', origin);
		const chunks = [];
		for await (const chunk of req) chunks.push(chunk);
		const url = new URL(req.url, `http://${req.headers.host}`);
		const [bucket, ...rest] = url.pathname.slice(1).split('/').map(decodeURIComponent);
		const key = rest.join('/');

		requests.push(`${req.method} ${key}`);
		const held = holds.find((h) => !h.done && h.match(req.method, key));
		if (held) {
			held.done = true;
			held.arrived();
			await held.released;
		}

		const refused = await signed(req, url);
		if (refused) return error(res, 403, refused);
		const objects = store.get(bucket);
		if (!objects) return error(res, 404, 'NoSuchBucket');
		if (!key && req.method === 'GET') {
			const prefix = url.searchParams.get('prefix') ?? '';
			const keys = [...objects.keys()].filter((k) => k.startsWith(prefix)).sort();
			return reply(res, 200, `<ListBucketResult>${keys.map((k) => `<Contents><Key>${k}</Key></Contents>`).join('')}</ListBucketResult>`, {
				'content-type': 'application/xml'
			});
		}
		if (req.method === 'PUT') return objects.set(key, Buffer.concat(chunks)), reply(res, 200);
		if (req.method === 'DELETE') return objects.delete(key), reply(res, 204);
		// As S3 answers: `Last-Modified` and no `Cache-Control`. An hour back, so
		// a browser's heuristic (RFC 9111 4.2.2) holds the object fresh for minutes.
		if (req.method === 'GET')
			return objects.has(key)
				? reply(res, 200, objects.get(key), { 'last-modified': new Date(Date.now() - 3_600_000).toUTCString() })
				: error(res, 404, 'NoSuchKey');
		return error(res, 405, 'MethodNotAllowed');
	});
	await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));

	return {
		endpoint: `http://127.0.0.1:${server.address().port}`,
		set cors(on) {
			state.cors = on;
		},
		objects: (bucket) => store.get(bucket),
		requests,
		hold(match) {
			const h = { match, done: false };
			h.reached = new Promise((r) => (h.arrived = r));
			h.released = new Promise((r) => (h.release = r));
			holds.push(h);
			return h;
		},
		close: () => new Promise((resolve) => server.close(resolve))
	};
}
