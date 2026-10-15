// A fake WebDAV server for the backups tests (da-bmxbb): a real HTTP server
// on another origin than the app's, so the browser does real CORS and real
// preflights against it, and every request's Basic auth is checked.
//
// Only what the destination uses: PUT, GET, HEAD and DELETE of a file and MKCOL
// of a folder, under `/dav/`, which exists. A PUT into a folder that does
// not exist is refused 409, as RFC 4918 has it. A refusal carries the CORS
// headers too, as a server configured by the widget's note does.

import { createServer } from 'node:http';

export const LOGIN = { user: 'ana', password: 'app-pass-ü' };

/**
 * Start one. `server.files` is every file by path; `server.folders` the
 * folders; `server.requests` is every request that reached it, `METHOD
 * path`, in order; `server.cors = false` stops sending CORS headers.
 */
export async function startWebDav() {
	const files = new Map();
	const folders = new Set(['/dav/']);
	const requests = [];
	const state = { cors: true };
	const expected = `Basic ${Buffer.from(`${LOGIN.user}:${LOGIN.password}`).toString('base64')}`;
	const parent = (path) => path.replace(/[^/]+\/?$/, '');

	const server = createServer(async (req, res) => {
		const reply = (status, body = '') => {
			res.writeHead(status);
			res.end(body);
		};
		if (state.cors && req.headers.origin) res.setHeader('access-control-allow-origin', req.headers.origin);
		if (req.method === 'OPTIONS') {
			if (!state.cors) return reply(403);
			res.setHeader('access-control-allow-methods', 'GET, HEAD, PUT, DELETE, MKCOL');
			res.setHeader('access-control-allow-headers', 'Authorization');
			return reply(200);
		}
		const chunks = [];
		for await (const chunk of req) chunks.push(chunk);
		const path = decodeURIComponent(new URL(req.url, 'http://x').pathname);
		requests.push(`${req.method} ${path}`);
		if (req.headers.authorization !== expected) return reply(401);
		if (req.method === 'MKCOL') {
			if (folders.has(path) || files.has(path.replace(/\/$/, ''))) return reply(405);
			if (!folders.has(parent(path))) return reply(409);
			folders.add(path.replace(/\/?$/, '/'));
			return reply(201);
		}
		if (req.method === 'PUT') {
			if (!folders.has(parent(path))) return reply(409);
			files.set(path, Buffer.concat(chunks));
			return reply(201);
		}
		if (req.method === 'GET') return files.has(path) ? reply(200, files.get(path)) : reply(404);
		if (req.method === 'DELETE') return files.delete(path) ? reply(204) : reply(404);
		if (req.method === 'HEAD') {
			if (!files.has(path)) return reply(404);
			res.setHeader('content-length', files.get(path).length);
			return reply(200);
		}
		return reply(405);
	});
	await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));

	return {
		root: `http://127.0.0.1:${server.address().port}/dav/`,
		set cors(on) {
			state.cors = on;
		},
		files,
		folders,
		requests,
		close: () => new Promise((resolve) => server.close(resolve))
	};
}
