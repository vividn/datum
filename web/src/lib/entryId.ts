import { ulid } from 'ulid';
import type { DatumTime } from './types.js';

export function generateId(): string {
	return ulid();
}

export function buildSlug(field: string, time?: DatumTime): string {
	if (time) {
		// Format: field:2024-03-16T09:15Z
		const dt = new Date(time.utc);
		const iso = dt.toISOString().replace(/\.\d{3}Z$/, 'Z');
		return `${field}:${iso}`;
	}
	const now = new Date().toISOString().replace(/\.\d{3}Z$/, 'Z');
	return `${field}:${now}`;
}

export async function contentHash(
	field: string,
	timeStart: DatumTime | undefined,
	timeEnd: DatumTime | undefined,
	value: unknown,
	extraData: unknown
): Promise<string> {
	const data = JSON.stringify({ field, timeStart, timeEnd, value, extraData });
	const encoded = new TextEncoder().encode(data);
	const hashBuffer = await crypto.subtle.digest('SHA-256', encoded);
	const hashArray = Array.from(new Uint8Array(hashBuffer));
	return hashArray.map((b) => b.toString(16).padStart(2, '0')).join('');
}
