// A game's «How to play» (da-op236). vividn, 2026-11-15: «add a "how to play"
// button to each game that starts a tour, explains what will happen and how to
// record the data», and «For the on/off tour mention that you are a
// multitasker, often reading and looking at TV and your phone at the same time».
//
// Four stops on the game's tab: the game, the collect widget under it, the
// nowview over it, then Start, or the box again for the change game, which
// starts itself. Its stops only show, so a tap in the tour moves it on and
// never starts a round.
//
// The tour's module is imported as the button is pressed: it reaches the
// widget registry, which draws this game's box.

import type { Stop } from './tour.js';

/** What each game's tour says at each stop. */
const SAY: Record<string, [game: string, collect: string, nowview: string, start: string]> = {
	moment: [
		'Things will appear on the table one at a time: a croissant, a cake or a bagel on the plate, a coffee or a tea beside it. Each one is a moment.',
		'When one appears, tap its button here. Each tap logs a moment at that instant.',
		'Your taps land here as dots as you make them.',
		'Press Start to play. At the end the game reads back what you logged and scores it.'
	],
	'on-off': [
		"You're a multitasker: you often read, watch TV and look at your phone, all at the same time. The book, the TV and the phone will each turn on and off, overlapping.",
		'When one turns on, tap its button; when it turns off, tap it again. Each stays on until you turn it off, and several can be on at once.',
		"Each one's period grows here while it's on.",
		'Press Start to play. At the end the game reads back what you logged and scores it.'
	],
	change: [
		'A person wanders from room to room, and out into the garden.',
		"Tap room, then the room they're in. Each change ends the last room, so you log only the moves.",
		'Each room is its own color here.',
		"The game starts itself once you log the room they're standing in."
	]
};

/** Start `lesson`'s tour, its game the box `game`: each stop whose element is drawn. */
export async function howToPlay(lesson: string, game: Element): Promise<void> {
	const say = SAY[lesson];
	if (!say) return;
	const { FIELDS, NOWVIEW, shown, tour } = await import('./tour.js');
	const [scene, collect, now, start] = say;
	const stops: Stop[] = [
		{ at: ':scope > .stage', in: game, say: scene },
		{ at: FIELDS, say: collect },
		{ at: NOWVIEW, say: now },
		lesson === 'change' ? { at: ':scope > .stage', in: game, say: start } : { at: '.go', in: game, say: start }
	];
	tour.set(stops.filter((s) => shown(s.at, s.in)));
}
