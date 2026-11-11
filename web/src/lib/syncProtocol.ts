// The messages between #lib/sync.ts (the page) and #lib/sync.worker.ts.

/** How one session went, as the wire crate reports it (`Report`). */
export interface SyncReport {
	label: string;
	id: string;
	added: boolean;
	redeemed: boolean;
	sent: number;
	got: number;
	conflicts: number;
	/** Whether the state hashes agree; null with one copy ahead of the other (da-t3ey2). */
	verified: boolean | null;
	/** The verify in words. */
	verify: string;
	/** It was no sync: this copy answered the peer's `sync diff` (da-n0ruf). */
	diffed: boolean;
	vector: Record<string, number>;
	/** The chain of one this copy left to redeem the invite, and the chain it joined (da-h7pnc). */
	left?: [string, string] | null;
	/**
	 * The two copies are on either side of a sync floor: only the chain's
	 * records moved, and this says which must update (da-52vdj).
	 */
	floor?: string | null;
	/**
	 * A join no member answered yet: kept as the database's pending join, which
	 * the leader redeems once one is online (da-4qpwf). Then `label` is the
	 * inviter's, as its link names it, and the rest is absent.
	 */
	pending?: true;
	/** What a member answered of a join it kept and did not admit (not holding the invite yet, paused), if one did. */
	said?: string | null;
}

/** What a ticket says of itself (the wire crate's `About`, da-l9kdr): never its secret. */
export interface TicketAbout {
	/** The inviting device's label. */
	from?: string;
	/** Its sync chain's id. */
	chain?: string;
	/** When the invite expires, unix seconds. */
	expires?: number;
	/** The inviting database's name (da-wm9wb). */
	db?: string;
	/** Whom the invite admits. */
	role?: 'member' | 'reader' | 'writer';
	/** A filtered reader's or writer's fields: a writer's may be none. */
	fields?: string[];
	/** Read, never written: whether the inviter's address names its relay, without which no other device reaches it. */
	relay?: boolean;
}

export type SyncRequest =
	| ({ id: number; db: string } & (
			| { op: 'start'; key: string; relay?: string; idle?: number; label: string; datum: string; schema: number; floor: number }
			| { op: 'dial'; addr: string }
			| { op: 'connect'; ticket: string; label?: string }
			/** Redeem the database's pending join, if a member answers: its report, or null with none (da-4qpwf). */
			| { op: 'redeem' }
			| { op: 'ticket'; secret: string; also?: string; about?: string }
			/** What a ticket or join link says of itself (`TicketAbout`); needs no endpoint. */
			| { op: 'about'; ticket: string }
			/** Send `message` to the `datum sync login` `link` names, from an endpoint under `key` (da-mzqs4); needs no database's. */
			| { op: 'login'; link: string; key: string; relay?: string; message: string }
	  ))
	/** A CLI process the worker asked for (`run`), and how it went. */
	| { ran: number; result: { code: number; stdout: string; stderr: string } };

export type SyncEvent =
	/** The answer to request `id`. */
	| { id: number; ok: true; value: unknown }
	| { id: number; ok: false; error: string }
	/** Run `datum <argv>` for a session's store, and answer `{ ran }`. */
	| { run: number; argv: string[]; stdin?: string }
	/** A session a peer dialed, which this endpoint served. */
	| { served: string; report: SyncReport }
	| { served: string; error: string }
	/** `done` of the `of` ops a session of `db`'s received are applied (da-89ujs). */
	| { progress: string; done: number; of: number }
	/** A session `peer` dialed begins (`on`) or ends: the peer is online while it lasts (da-eglw0). */
	| { serving: string; peer: string; on: boolean };
