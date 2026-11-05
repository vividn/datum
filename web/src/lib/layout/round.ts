// A tutorial game's round as its panes share it (da-uzyih.4908): the game in
// one, the collect widget filtered to its fields in another (da-2s9im,
// vividn, 2026-11-05: «the widgets in the tutorial should actually use the
// collect widget … filtered to each game's set»). The round scores what is
// logged in its fields while it runs, whatever logged it — the collect
// widget, the bar, a typed line — read back from the database, so all it
// keeps is the presses still landing. A mis-tap is the bar's `undo`, as
// anywhere (da-uzyih.4910), and a refusal is the bar's to say, as for any
// collect widget.

import { writable } from 'svelte/store';
import type { Phase } from '#lib/tutorial.ts';
import { parseEcho, type EchoDoc } from '#lib/echo.ts';
import { onPress } from './engine.js';

export class Round {
	/** The game box's phase: the switch game starts itself at rest. */
	readonly phase = writable<Phase>('neutral');
	#pending: Promise<unknown>[] = [];

	/**
	 * While a lesson's game is mounted, until the function returned is called:
	 * every press kept until it lands, and the entries it made in `fields` told
	 * to `wrote`, the switch game starting on one.
	 */
	watch(fields: string[], wrote?: (docs: EchoDoc[]) => void): () => void {
		return onPress((result) =>
			this.#pending.push(
				result.then((r) => {
					const docs = r.code === 0 ? (parseEcho(r.stdout) ?? []).filter((d) => d.created && fields.includes(d.field ?? '')) : [];
					if (docs.length) wrote?.(docs);
				})
			)
		);
	}

	begin(): void {
		this.#pending = [];
	}

	/** Once every press so far has landed. */
	settled(): Promise<unknown> {
		return Promise.allSettled(this.#pending);
	}
}

/** One round per lesson for the page's life: the lessons are a fixed few, and each game's start begins its round afresh. */
const rounds = new Map<string, Round>();

/** The lesson's one round, shared by its game and its fields. */
export function roundOf(lesson: string): Round {
	if (!rounds.has(lesson)) rounds.set(lesson, new Round());
	return rounds.get(lesson)!;
}
