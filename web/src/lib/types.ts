export interface DatumTime {
	utc: string; // ISO 8601
	offsetMinutes: number;
	timezone: string;
}

export type Verb = 'occur' | 'start' | 'stop' | 'switch' | 'note';

export interface Entry {
	id: string;
	slug: string;
	contentHash: string;
	field: string;
	verb: Verb;
	value?: unknown;
	extraData?: unknown;
	unit?: string;
	timeStart?: DatumTime;
	timeEnd?: DatumTime;
	lat?: number;
	lon?: number;
	location?: string;
	createdAt: string;
	modifiedAt: string;
	deviceId: string;
}

export interface ParsedInput {
	verb: Verb;
	field: string;
	value?: unknown;
	unit?: string;
	extraData?: Record<string, unknown>;
	timeOffset?: number; // minutes offset from now
	timeAbsolute?: string; // HH:MM
}
