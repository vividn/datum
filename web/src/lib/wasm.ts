// The browser half of datum's wasm modules.
//
// Two of the app's regions are datum's own Rust rather than a second
// implementation of it in TypeScript: the dayview/nowview render
// (`datum-layout`, da-6h6p) and the parser (`datum-parse`, da-7scm). Both
// are `wasm32-unknown-unknown` cdylibs behind the same five-function C ABI,
// so the loading, the copy in and the copy out live here once.
//
//   alloc(len) -> ptr          reserve `len` bytes to write the request into
//   dealloc(ptr, len)          release them
//   <entry>(ptr, len) -> ptr   JSON request -> JSON response, held module-side
//   response_len() -> len      the length of that response
//   response_clear()           drop it once we have copied it out
//
// The response is parked in the module rather than packed into the return
// value because a pointer and a length do not fit in one `i32`. See the
// crate-level docs in crates/datum-{layout,parse}-wasm/src/lib.rs.

import { base } from '$app/paths';

/** The exports every one of these modules has. */
interface Abi {
	memory: WebAssembly.Memory;
	alloc(len: number): number;
	dealloc(ptr: number, len: number): void;
	response_len(): number;
	response_clear(): void;
	[entry: string]: unknown;
}

/**
 * A module's single JSON entry point, instantiated on first call and
 * reused: these modules are pure and hold no state between calls.
 *
 * `file` is the artifact's name under `static/`, `entry` the export that
 * takes the request. A failed load does not poison later attempts — the
 * pane shows the error and the next call tries again.
 */
export function jsonModule<Request, Response>(
	file: string,
	entry: string
): (request: Request) => Promise<Response> {
	let loading: Promise<Abi> | null = null;

	const load = (): Promise<Abi> => {
		// No imports to satisfy — a `wasm32-unknown-unknown` cdylib with no
		// `extern` calls has no import section — so instantiate takes {}.
		loading ??= fetch(`${base}/${file}`)
			.then((response) => {
				if (!response.ok) throw new Error(`${file}: HTTP ${response.status}`);
				return response.arrayBuffer();
			})
			.then((bytes) => WebAssembly.instantiate(bytes, {}))
			.then((result) => result.instance.exports as unknown as Abi)
			.catch((e) => {
				loading = null;
				throw e;
			});
		return loading;
	};

	return async (request: Request): Promise<Response> => {
		const wasm = await load();
		const call = wasm[entry] as (ptr: number, len: number) => number;
		const body = new TextEncoder().encode(JSON.stringify(request));

		const ptr = wasm.alloc(body.length);
		try {
			// A fresh view every time: `alloc` can grow the memory, which
			// detaches any ArrayBuffer taken before it.
			new Uint8Array(wasm.memory.buffer, ptr, body.length).set(body);
			const out = call(ptr, body.length);
			const len = wasm.response_len();
			const text = new TextDecoder().decode(
				new Uint8Array(wasm.memory.buffer, out, len).slice()
			);
			wasm.response_clear();
			return JSON.parse(text) as Response;
		} finally {
			wasm.dealloc(ptr, body.length);
		}
	};
}
