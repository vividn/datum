// What the closed output's tab says about a command: always six characters
// (da-k5kwu). vividn, 2026-10-30: «If any of the ACTIONS are FAILED or or
// ERROR! those should take precedence to be seen on the tab. Otherwise show
// the first three letters of ACTION 1, and two letters of ACTION 2 e.g,
// UPD,SW (in their respective colors). for 3 2,1,1; for 4 or more: 1,1,1+».

export interface Word {
	text: string;
	color?: string;
}

/** How much of each word a line of that many actions shows. */
const CUTS = [[6], [3, 2], [2, 1, 1], [1, 1, 1]];

export function tabWord(words: Word[], failed: boolean): { parts: Word[]; more: boolean } {
	const bad = words.find((w) => w.text === 'FAILED' || w.text === 'ERROR!');
	if (bad) return { parts: [bad], more: false };
	if (failed) return { parts: [{ text: 'ERROR!' }], more: false };
	const cuts = CUTS[Math.min(words.length, CUTS.length) - 1] ?? [];
	return {
		parts: cuts.map((n, i) => ({ ...words[i], text: words[i].text.slice(0, n) })),
		more: words.length >= CUTS.length
	};
}
