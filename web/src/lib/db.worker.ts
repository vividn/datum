// The dedicated worker that runs datum.
//
// A worker because that is the only place OPFS works: a sync access handle,
// which is what lets a synchronous WASI process read and write a file, is
// exposed to dedicated workers only. On the main thread the database could
// never be anything but in-memory, and every entry died on reload (da-5iqw).
//
// What runs here is the real `datum` CLI, one process per command — see
// #lib/cli.ts. This file is only the door: a request in, a process's
// outcome out, and a word to the page when storage changes under it.

import { onBlocked, onStorage, probe, runCli, storageNow } from './cli.js';
import { onUpgrade } from './wasiFs.js';
import type { DbMessage, DbRequest, DbResponse } from './dbProtocol.js';

/**
 * The worker global, typed for the two calls this file makes: TypeScript
 * resolves `self` against lib.dom here, where it is a `Window` whose
 * `postMessage` takes a target origin.
 */
const ctx = self as unknown as {
	postMessage(message: DbMessage): void;
	addEventListener(type: 'message', listener: (event: MessageEvent<DbRequest>) => void): void;
};

async function serve(request: DbRequest): Promise<DbResponse> {
	try {
		if (request.op === 'open') return { id: request.id, ok: true, storage: await probe() };
		// stderr as it is written, not only at exit: the page shows progress.
		const cli = await runCli(
			request.argv,
			request.env,
			request.stdin,
			(text) => ctx.postMessage({ notice: 'stderr', id: request.id, text }),
			request.key,
			request.stage,
			request.pane,
			request.session
		);
		if (!cli) return { id: request.id, ok: true, storage: storageNow() };
		return { id: request.id, ok: true, storage: cli.storage, cli };
	} catch (e) {
		return { id: request.id, ok: false, error: e instanceof Error ? e.message : String(e) };
	}
}

onStorage((storage) => ctx.postMessage({ notice: 'storage', storage }));
onUpgrade(() => ctx.postMessage({ notice: 'upgrading' }));
onBlocked((waited) => ctx.postMessage({ notice: 'blocked', waited }));

ctx.addEventListener('message', (event: MessageEvent<DbRequest>) => {
	void serve(event.data).then((response) => ctx.postMessage(response));
});
