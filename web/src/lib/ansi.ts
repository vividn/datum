// ANSI → styled spans, for CLI output rendered into the DOM.
//
// datum paints its output with truecolor SGR escapes
// (`crates/datum-commands/src/render/colors.rs`: `38;2;R;G;B` fg,
// `48;2;R;G;B` bg, plus bold / underline / bright-grey). A pane that
// dumped that text straight into the DOM would show the escape bytes,
// so every CLI render goes through here first.
//
// Everything that is not an SGR sequence is *dropped*, not rendered:
// `datum tail --watch` emits `\x1b[2J\x1b[H` between frames (see
// commands/tail.rs), and cursor moves mean nothing to a scrolling div.

/** One run of text sharing a single set of attributes. */
export interface AnsiSpan {
	text: string;
	/** CSS colour, or undefined for the pane's default. */
	fg?: string;
	bg?: string;
	bold?: boolean;
	dim?: boolean;
	italic?: boolean;
	underline?: boolean;
}

// The 16 basic colours, in the same dark-terminal register the panes use.
const BASIC = [
	'#1a1a2e',
	'#f4645a',
	'#4caf88',
	'#f0b429',
	'#6a8cff',
	'#c678dd',
	'#4dd0c4',
	'#c8c8e8',
	'#55558a',
	'#ff8a80',
	'#7ee0b0',
	'#ffd166',
	'#8fa8ff',
	'#e0a0f0',
	'#7fe8de',
	'#f0f0ff'
];

/** xterm-256 palette entry: 0–15 basic, 16–231 cube, 232–255 greys. */
function xterm256(n: number): string | undefined {
	if (n < 0 || n > 255) return undefined;
	if (n < 16) return BASIC[n];
	if (n < 232) {
		const c = n - 16;
		const step = (v: number) => (v === 0 ? 0 : 55 + v * 40);
		return rgb(step(Math.floor(c / 36)), step(Math.floor(c / 6) % 6), step(c % 6));
	}
	const grey = 8 + (n - 232) * 10;
	return rgb(grey, grey, grey);
}

function rgb(r: number, g: number, b: number): string {
	return `rgb(${r} ${g} ${b})`;
}

/**
 * Apply one SGR parameter run to `style`, consuming the extended-colour
 * arguments (`38;5;N`, `38;2;R;G;B`) it needs. Returns the index of the
 * last parameter consumed.
 */
function applySgr(style: AnsiSpan, params: number[], i: number): number {
	const p = params[i];
	if (p === 0) {
		for (const k of ['fg', 'bg', 'bold', 'dim', 'italic', 'underline'] as const) delete style[k];
	} else if (p === 1) style.bold = true;
	else if (p === 2) style.dim = true;
	else if (p === 3) style.italic = true;
	else if (p === 4) style.underline = true;
	else if (p === 22) {
		delete style.bold;
		delete style.dim;
	} else if (p === 23) delete style.italic;
	else if (p === 24) delete style.underline;
	else if (p === 39) delete style.fg;
	else if (p === 49) delete style.bg;
	else if (p >= 30 && p <= 37) style.fg = BASIC[p - 30];
	else if (p >= 40 && p <= 47) style.bg = BASIC[p - 40];
	else if (p >= 90 && p <= 97) style.fg = BASIC[p - 90 + 8];
	else if (p >= 100 && p <= 107) style.bg = BASIC[p - 100 + 8];
	else if (p === 38 || p === 48) {
		const key = p === 38 ? 'fg' : 'bg';
		if (params[i + 1] === 5) {
			style[key] = xterm256(params[i + 2]);
			return i + 2;
		}
		if (params[i + 1] === 2) {
			style[key] = rgb(params[i + 2] || 0, params[i + 3] || 0, params[i + 4] || 0);
			return i + 4;
		}
	}
	return i;
}

// One escape sequence: CSI (`\x1b[ … final`), OSC (`\x1b] … BEL|ST`), or a
// two-byte escape. Anything matched and not an SGR (`m`) is discarded.
const ESCAPE = /\x1b(?:\[([0-9;:?]*)([ -/]*)([@-~])|\][^\x07\x1b]*(?:\x07|\x1b\\)?|[@-Z\\-_])/g;

/** Split ANSI-painted text into styled spans. Escapes never reach the DOM. */
export function ansiSpans(text: string): AnsiSpan[] {
	const spans: AnsiSpan[] = [];
	const style: AnsiSpan = { text: '' };
	let at = 0;

	const flush = (upto: number) => {
		if (upto > at) spans.push({ ...style, text: text.slice(at, upto) });
	};

	ESCAPE.lastIndex = 0;
	for (let m = ESCAPE.exec(text); m; m = ESCAPE.exec(text)) {
		flush(m.index);
		at = m.index + m[0].length;
		// Only SGR (`m`) changes how the following text looks; a private
		// sequence (`\x1b[?25l`) is a terminal control, not a colour.
		if (m[3] !== 'm' || m[1]?.includes('?')) continue;
		const params = (m[1] === '' ? '0' : m[1]).split(';').map((p) => parseInt(p, 10) || 0);
		for (let i = 0; i < params.length; i++) i = applySgr(style, params, i);
	}
	flush(text.length);
	return spans;
}

/** The same text with every escape sequence removed. */
export function stripAnsi(text: string): string {
	return ansiSpans(text)
		.map((s) => s.text)
		.join('');
}
