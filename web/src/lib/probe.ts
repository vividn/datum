// «Test connection» for a backup destination ($lib/s3.ts, $lib/webdav.ts):
// everything a plan does, done once to a small probe file. Kept free of the
// app, so node can import it.

export type Step = 'write' | 'list' | 'read' | 'delete';

/** One step of `probe`: done, or failed and why. */
export interface StepResult {
	step: Step;
	ok: boolean;
	cause?: string;
}

interface Probed {
	get(file: string): Promise<Uint8Array | null>;
	put(file: string, bytes: Uint8Array): Promise<void>;
	remove(file: string): Promise<void>;
}

/**
 * Write, list (when `listed` is given), read and delete a probe file at
 * `dest`, stopping at the first step that fails.
 */
export async function probe(dest: Probed, listed?: (file: string) => Promise<boolean>): Promise<StepResult[]> {
	const file = `datum-probe-${Date.now().toString(36)}.txt`;
	const bytes = new TextEncoder().encode('datum.day connection test\n');
	const steps: [Step, () => Promise<unknown>][] = [['write', () => dest.put(file, bytes)]];
	if (listed)
		steps.push([
			'list',
			async () => {
				if (!(await listed(file))) throw new Error('the probe written is not listed');
			}
		]);
	steps.push(
		[
			'read',
			async () => {
				const back = await dest.get(file);
				if (!back || new TextDecoder().decode(back) !== new TextDecoder().decode(bytes)) throw new Error('the probe read back differs');
			}
		],
		['delete', () => dest.remove(file)]
	);
	const results: StepResult[] = [];
	for (const [step, go] of steps) {
		try {
			await go();
			results.push({ step, ok: true });
		} catch (e) {
			results.push({ step, ok: false, cause: e instanceof Error ? e.message : String(e) });
			break;
		}
	}
	return results;
}
