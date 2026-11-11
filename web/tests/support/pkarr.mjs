// A pkarr relay (da-rnxp4), the account record's: routed in the browser for
// a page, and served on a loopback port for a native `datum` (da-mzqs4), both
// doors onto one store.

import { createPublicKey, verify } from 'node:crypto';
import { createServer } from 'node:http';

const PKARR = 'https://pkarr.test/pkarr';

/**
 * A PUT is kept only if it is what n0's relay would take (BEP44: signed by
 * the key the path names over `3:seqi<t>e1:v<n>:<packet>`, a packet of 1000
 * bytes at most), and a GET answers it or 404. `prepare` routes every context
 * it is given to it; `serve()` opens a port for a native copy, its URL what
 * `DATUM_SYNC_PKARR` names, and `stop()` closes it.
 */
export function pkarrRelay() {
	const held = new Map();
	const z32 = (s) => {
		let bits = '';
		for (const c of s) bits += 'ybndrfg8ejkmcpqxot1uwisza345h769'.indexOf(c).toString(2).padStart(5, '0');
		return Buffer.from(bits.slice(0, 256).match(/.{8}/g).map((b) => parseInt(b, 2)));
	};
	/** A PUT of `body` under `name`, kept if it verifies: whether it did. */
	const put = (name, body) => {
		const key = createPublicKey({ key: { kty: 'OKP', crv: 'Ed25519', x: z32(name).toString('base64url') }, format: 'jwk' });
		const packet = body.subarray(72);
		const signed = Buffer.concat([Buffer.from(`3:seqi${body.readBigUInt64BE(64)}e1:v${packet.length}:`), packet]);
		const ok = packet.length <= 1000 && verify(null, signed, key, body.subarray(0, 64));
		if (ok) held.set(name, body);
		return ok;
	};
	const cors = { 'access-control-allow-origin': '*', 'access-control-allow-methods': 'GET, PUT', 'access-control-allow-headers': '*' };
	const prepare = async (context) => {
		await context.addInitScript((url) => localStorage.setItem('datum.pkarrRelay', url), PKARR);
		await context.route(`${PKARR}/*`, async (route) => {
			const req = route.request();
			const name = new URL(req.url()).pathname.split('/').pop();
			if (req.method() === 'OPTIONS') return route.fulfill({ status: 204, headers: cors });
			if (req.method() === 'GET') return held.has(name) ? route.fulfill({ status: 200, headers: cors, body: held.get(name) }) : route.fulfill({ status: 404, headers: cors });
			return route.fulfill({ status: put(name, req.postDataBuffer()) ? 200 : 400, headers: cors });
		});
	};
	let server;
	const serve = async () => {
		server = createServer((req, res) => {
			const name = req.url.split('/').pop();
			const chunks = [];
			req.on('data', (c) => chunks.push(c));
			req.on('end', () => {
				if (req.method === 'GET' && held.has(name)) return res.writeHead(200).end(held.get(name));
				if (req.method === 'GET') return res.writeHead(404).end();
				res.writeHead(put(name, Buffer.concat(chunks)) ? 200 : 400).end();
			});
		});
		await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
		return `http://127.0.0.1:${server.address().port}/pkarr`;
	};
	const stop = () => new Promise((resolve) => (server ? server.close(resolve) : resolve()));
	return { held, prepare, serve, stop };
}
