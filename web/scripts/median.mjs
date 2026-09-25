/**
 * The true median, averaging the two middle values of an even-length run.
 * Not the upper middle, which is what `Math.floor(n / 2)` returns:
 * measure-press's concurrency control medians EIGHT runs, where that read
 * 51.6 ms against a true 49.8 — a latency figure biased upwards, flattering
 * the very gap the control exists to show.
 */
export const median = (xs) => {
	const sorted = [...xs].sort((a, b) => a - b);
	if (sorted.length === 0) return 0;
	const middle = sorted.length >> 1;
	return sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
};
