// The sync worker (da-6kpj5): datum.day's Easy Sync transport.
//
// WASI has no sockets, so the CLI cannot dial. This worker runs the wire
// crate (`crates/datum-sync-wire`) built for wasm32-unknown-unknown — iroh,
// relay-only in a browser — with one endpoint per synced database. It only
// moves bytes: every read and write of a session is a `datum sync …` command
// it hands back to the page, which runs it in the database worker like any
// other (#lib/sync.ts). There is no storage logic here, nor in TypeScript.
//
// The page decides when to dial; this file serves whatever comes in, and
// says so.

import init, { Node, about, type Pending } from './wasm/datum-sync.js';
import syncWasm from './wasm/datum-sync_bg.wasm?url';
import type { SyncEvent, SyncRequest } from './syncProtocol.js';

const ctx = self as unknown as {
	postMessage(message: SyncEvent): void;
	addEventListener(type: 'message', listener: (event: MessageEvent<SyncRequest>) => void): void;
};

const ready = init({ module_or_path: syncWasm });
const said = (e: unknown) => (e instanceof Error ? e.message : String(e));
/** Each database's endpoint, as it binds: a second `start` waits on the first. */
const nodes = new Map<string, Promise<Node>>();
/** The key each is bound with: a `start` with another (a leave rekeyed it) binds afresh. */
const keys = new Map<string, string>();
/** The CLI processes a store is waiting on, by number. */
const waiting = new Map<number, (result: { code: number; stdout: string; stderr: string }) => void>();
let nextRun = 0;

/** `datum --db <db> <argv>` in the page's database worker. */
function runner(db: string) {
	return (argv: string[], stdin?: string) =>
		new Promise((resolve) => {
			const run = ++nextRun;
			waiting.set(run, resolve);
			ctx.postMessage({ run, argv: ['--db', db, ...argv], stdin });
		});
}

/** Serve every connection `node` accepts until it closes. */
async function serve(db: string, node: Node): Promise<void> {
	for (let p: Pending | undefined; (p = await node.accept()); ) {
		node.serve(p).then(
			(report) => ctx.postMessage({ served: db, report: JSON.parse(report) }),
			(e) => ctx.postMessage({ served: db, error: said(e) })
		);
	}
}

async function answer(request: Exclude<SyncRequest, { ran: number }>): Promise<unknown> {
	await ready;
	if (request.op === 'about') return JSON.parse(about(request.ticket));
	if (request.op === 'start' && keys.get(request.db) !== request.key) {
		void nodes.get(request.db)?.then((old) => old.close(), () => {});
		const db = request.db;
		const progress = (done: number, of: number) => ctx.postMessage({ progress: db, done, of });
		const serving = (peer: string, on: boolean) => ctx.postMessage({ serving: db, peer, on });
		const { key, relay, idle, label, datum, schema, floor } = request;
		const binding = Node.bind(key, relay, idle, label, datum, schema, floor, runner(db), progress, serving);
		nodes.set(request.db, binding);
		keys.set(request.db, request.key);
		// A bind replaced while it was in flight (a leave rekeyed the db) clears nothing of its successor's.
		binding.then(
			(node) => serve(request.db, node),
			() => nodes.get(request.db) === binding && (nodes.delete(request.db), keys.delete(request.db))
		);
	}
	const node = await nodes.get(request.db);
	if (!node) throw new Error(`no endpoint for ${request.db}`);
	if (request.op === 'start') return node.id();
	switch (request.op) {
		case 'dial':
			return JSON.parse(await node.dial(request.addr));
		case 'connect':
			return JSON.parse(await node.connect(request.ticket, request.label));
		case 'ticket':
			return node.ticket(request.secret, request.also, request.about);
	}
}

ctx.addEventListener('message', ({ data }) => {
	if ('ran' in data) {
		waiting.get(data.ran)?.(data.result);
		waiting.delete(data.ran);
		return;
	}
	answer(data).then(
		(value) => ctx.postMessage({ id: data.id, ok: true, value }),
		(e) => ctx.postMessage({ id: data.id, ok: false, error: said(e) })
	);
});
