// A tutorial game's round as its two panes share it (da-uzyih.4908): the
// game in one, its fields in the other (vividn, 2026-10-29: «Let's have the
// game and the fields widgets be separate widgets here»). Each tap is a
// `press`, as any widget's; what it wrote is read back from the database
// when the round is scored, so all a round keeps is the writes still
// landing and the last failure. A mis-tap is the bar's `undo`, as anywhere
// (da-uzyih.4910: no Undo of the game's own).

import { writable } from 'svelte/store';
import type { Phase } from '#lib/tutorial.ts';
import { press } from './engine.js';

export class Round {
	readonly failed = writable('');
	/** The game box's phase: the switch game's fields are cued at rest. */
	readonly phase = writable<Phase>('neutral');
	/** Told of each tap that landed: the switch game starts on one. */
	wrote?: (argv: string[]) => void;
	#pending: Promise<unknown>[] = [];

	begin(): void {
		this.#pending = [];
		this.failed.set('');
	}

	/** A write that fails is said. */
	tap(argv: string[]): void {
		const fail = (why: string) => this.failed.set(why);
		this.#pending.push(
			press(argv).then(
				(result) => (result.code !== 0 ? fail(result.stderr) : this.wrote?.(argv)),
				(e) => fail((e as Error).message)
			)
		);
	}

	/** Once every write so far has landed. */
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
