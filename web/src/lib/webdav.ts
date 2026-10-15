// A WEBDAV folder as a backup destination (da-bmxbb): Nextcloud, ownCloud,
// many NAS boxes. A folder URL, a user and an (app) password, sent as HTTP
// Basic auth; the files are `<folder>/<file>`, and the folder is made
// (MKCOL) the first time a write finds it missing.
//
// As with S3 the browser talks to the server itself, so the server must
// allow this page's origin (CORS, `corsHeaders`) — which most WebDAV servers
// do not out of the box. Kept free of the app, so node can import it.

import { probe, type StepResult } from './probe.js';

export interface WebDavConfig {
	/** The folder, e.g. `https://cloud.example.com/remote.php/dav/files/<user>/datum-backups/`. */
	url?: string;
	user?: string;
	password?: string;
}

const folder = (c: WebDavConfig) => (c.url?.trim() ?? '').replace(/\/*$/, '/');

/** HTTP Basic, of the UTF-8 bytes: `btoa` takes only Latin-1. One byte at a time, so no length overflows a call's arguments. */
const basic = (c: WebDavConfig) =>
	`Basic ${btoa(Array.from(new TextEncoder().encode(`${c.user?.trim() ?? ''}:${c.password ?? ''}`), (b) => String.fromCharCode(b)).join(''))}`;

/**
 * Why `url` cannot be the folder for a page served over `page` (its
 * protocol, `https:`), or '' when it can. An https page's request to an
 * http:// address is blocked by the browser as mixed content, localhost
 * aside, and would carry the password unencrypted besides: without this the
 * block reads as a CORS refusal, and no header could help.
 */
export function unusable(url: string, page: string): string {
	let u: URL;
	try {
		u = new URL(url.trim());
	} catch {
		return 'folder URL: not a whole URL — it starts with https://';
	}
	if (u.protocol !== 'https:' && u.protocol !== 'http:') return `folder URL: ${u.protocol}// is not a web address — use the server's https:// address`;
	if (u.protocol === 'http:' && page === 'https:' && !['localhost', '127.0.0.1', '[::1]'].includes(u.hostname))
		return "folder URL: an https page can't reach an http:// folder (the browser blocks it), and the password would travel unencrypted — use the server's https:// address";
	return '';
}

/** A WebDAV refusal, with what it most likely means. */
export class WebDavError extends Error {
	status: number;
	constructor(status: number, message: string) {
		super(message);
		this.status = status;
	}
}

/** What a refused request most likely means, in the reader's terms. */
function explain(status: number, statusText: string): string {
	if (status === 401) return 'credentials: the user or password is wrong (401)';
	if (status === 403) return 'permission: this user may not do this in this folder (403)';
	if (status === 404) return 'folder URL: there is nothing at this URL (404)';
	if (status === 409) return 'folder URL: the folder above this one does not exist (409)';
	if (status === 507) return 'space: the server is full (507)';
	return `${status} ${statusText}`.trim();
}

/** Send one request; a refusal throws a `WebDavError` naming its likely cause. */
async function send(c: WebDavConfig, method: string, url: string, body?: Uint8Array): Promise<Response> {
	const why = unusable(c.url ?? '', typeof location === 'undefined' ? '' : location.protocol);
	if (why) throw new WebDavError(0, why);
	let response: Response;
	try {
		// Never the browser's HTTP cache: a run's re-read of the index must see another device's put.
		response = await fetch(url, { method, body: body as BodyInit | undefined, headers: { authorization: basic(c) }, cache: 'no-store' });
	} catch {
		// The browser hides WHY a cross-origin request got no response.
		throw new WebDavError(0, 'CORS: the server does not allow this page — send the headers below — or the folder URL is wrong, or this device is offline');
	}
	if (response.ok) return response;
	throw new WebDavError(response.status, explain(response.status, response.statusText));
}

/** The destination: `get`, `put`, `remove`, `size` on files in the folder. */
export function webdav(c: WebDavConfig) {
	const at = (file: string) => folder(c) + encodeURIComponent(file);
	return {
		async get(file: string): Promise<Uint8Array | null> {
			try {
				return new Uint8Array(await (await send(c, 'GET', at(file))).arrayBuffer());
			} catch (e) {
				if (e instanceof WebDavError && e.status === 404) return null;
				throw e;
			}
		},
		async put(file: string, bytes: Uint8Array): Promise<void> {
			try {
				await send(c, 'PUT', at(file), bytes);
			} catch (e) {
				// 409: the folder is not there yet. Make it (405: it is, after all) and put again.
				if (!(e instanceof WebDavError && e.status === 409)) throw e;
				await send(c, 'MKCOL', folder(c)).catch((m) => {
					if (!(m instanceof WebDavError && m.status === 405)) throw m;
				});
				await send(c, 'PUT', at(file), bytes);
			}
		},
		async remove(file: string): Promise<void> {
			await send(c, 'DELETE', at(file)).catch((e) => {
				if (!(e instanceof WebDavError && e.status === 404)) throw e;
			});
		},
		async size(file: string): Promise<number | null> {
			const length = (await send(c, 'HEAD', at(file))).headers.get('content-length');
			return length === null ? null : Number(length);
		}
	};
}

/** Write, read and delete a small probe file in the folder — everything a plan does. */
export const testConnection = (c: WebDavConfig): Promise<StepResult[]> => probe(webdav(c));

/** The response headers the server must send for this page, on every response — a refusal too. */
export const corsHeaders = (origin: string): string =>
	[
		`Access-Control-Allow-Origin: ${origin}`,
		'Access-Control-Allow-Methods: GET, HEAD, PUT, DELETE, MKCOL',
		'Access-Control-Allow-Headers: Authorization',
		'Access-Control-Max-Age: 3000'
	].join('\n');
