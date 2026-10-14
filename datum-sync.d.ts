/* tslint:disable */
/* eslint-disable */
/**
 * The `ReadableStreamType` enum.
 *
 * *This API requires the following crate features to be activated: `ReadableStreamType`*
 */

type ReadableStreamType = "bytes";

export class IntoUnderlyingByteSource {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    cancel(): void;
    pull(controller: ReadableByteStreamController): Promise<any>;
    start(controller: ReadableByteStreamController): void;
    readonly autoAllocateChunkSize: number;
    readonly type: ReadableStreamType;
}

export class IntoUnderlyingSink {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    abort(reason: any): Promise<any>;
    close(): Promise<any>;
    write(chunk: any): Promise<any>;
}

export class IntoUnderlyingSource {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    cancel(): void;
    pull(controller: ReadableStreamDefaultController): Promise<any>;
}

/**
 * One database's endpoint, and the CLI that is its store.
 */
export class Node {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * The next incoming connection; `undefined` once the endpoint closed.
     */
    accept(): Promise<Pending | undefined>;
    /**
     * Where a peer reaches this endpoint, as the JSON `sync_peers` keeps.
     */
    addr(): string;
    /**
     * Bind the endpoint whose key is `key` (hex, `datum sync
     * endpoint-key`), on n0's relays or the one `relay` names. `datum`
     * and `schema` are what the CLI runs (`sync status --json`'s `this`).
     */
    static bind(key: string, relay: string | null | undefined, label: string, datum: string, schema: number, run: Function, progress: Function): Promise<Node>;
    close(): Promise<void>;
    /**
     * `datum sync chain connect [--label]`: redeem `ticket` as `label`,
     * which the admit records, and sync.
     */
    connect(ticket: string, label?: string | null): Promise<string>;
    /**
     * Sync with the peer at `addr` (the JSON `sync_peers` keeps); the
     * report, as JSON.
     */
    dial(addr: string): Promise<string>;
    id(): string;
    /**
     * Serve `pending`; its report, as JSON.
     */
    serve(pending: Pending): Promise<string>;
    /**
     * The ticket for an invite whose secret is `secret` — this endpoint's
     * address, then `also`'s (a JSON list of addresses, a chain's members),
     * and `about` (JSON, [`About`]).
     */
    ticket(secret: string, also?: string | null, about?: string | null): string;
}

/**
 * An incoming connection, not yet served.
 */
export class Pending {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
}

/**
 * What a ticket or join link says of itself ([`About`], as JSON): what a
 * page asks before it redeems one (da-l9kdr). Never the secret.
 */
export function about(ticket: string): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_node_free: (a: number, b: number) => void;
    readonly __wbg_pending_free: (a: number, b: number) => void;
    readonly about: (a: number, b: number, c: number) => void;
    readonly node_accept: (a: number) => number;
    readonly node_addr: (a: number, b: number) => void;
    readonly node_bind: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number) => number;
    readonly node_close: (a: number) => number;
    readonly node_connect: (a: number, b: number, c: number, d: number, e: number) => number;
    readonly node_dial: (a: number, b: number, c: number) => number;
    readonly node_id: (a: number, b: number) => void;
    readonly node_serve: (a: number, b: number) => number;
    readonly node_ticket: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => void;
    readonly __wbg_intounderlyingbytesource_free: (a: number, b: number) => void;
    readonly __wbg_intounderlyingsink_free: (a: number, b: number) => void;
    readonly __wbg_intounderlyingsource_free: (a: number, b: number) => void;
    readonly intounderlyingbytesource_autoAllocateChunkSize: (a: number) => number;
    readonly intounderlyingbytesource_cancel: (a: number) => void;
    readonly intounderlyingbytesource_pull: (a: number, b: number) => number;
    readonly intounderlyingbytesource_start: (a: number, b: number) => void;
    readonly intounderlyingbytesource_type: (a: number) => number;
    readonly intounderlyingsink_abort: (a: number, b: number) => number;
    readonly intounderlyingsink_close: (a: number) => number;
    readonly intounderlyingsink_write: (a: number, b: number) => number;
    readonly intounderlyingsource_cancel: (a: number) => void;
    readonly intounderlyingsource_pull: (a: number, b: number) => number;
    readonly ring_core_0_17_14__bn_mul_mont: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly __wasm_bindgen_func_elem_14726: (a: number, b: number, c: number, d: number) => void;
    readonly __wasm_bindgen_func_elem_14728: (a: number, b: number, c: number, d: number) => void;
    readonly __wasm_bindgen_func_elem_6035: (a: number, b: number, c: number) => void;
    readonly __wasm_bindgen_func_elem_3529: (a: number, b: number, c: number) => void;
    readonly __wasm_bindgen_func_elem_7744: (a: number, b: number, c: number) => void;
    readonly __wasm_bindgen_func_elem_5833: (a: number, b: number) => void;
    readonly __wasm_bindgen_func_elem_6943: (a: number, b: number) => void;
    readonly __wasm_bindgen_func_elem_7007: (a: number, b: number) => void;
    readonly __wasm_bindgen_func_elem_14582: (a: number, b: number) => void;
    readonly __wbindgen_export: (a: number, b: number) => number;
    readonly __wbindgen_export2: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_export3: (a: number) => void;
    readonly __wbindgen_export4: (a: number, b: number) => void;
    readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
    readonly __wbindgen_export5: (a: number, b: number, c: number) => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
