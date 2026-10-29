// The page's last layer between an SVG the CLI printed and the DOM: what
// `inlineSvg` keeps of a picture. Imports nothing, so #lib/dayLayers.ts
// sanitizes a held data layer once, when it is parsed (da-mi2s6), and a
// day strip band laid out from it goes to the page as a node, never as
// text parsed again.

/**
 * `svg` with nothing kept that could run, in place. The markup is datum's
 * own, but it carries whatever text was logged, so: no script, no foreign
 * content, no `on…` handler, no link that leaves the picture, no `<style>`
 * and no animation that writes a handler or a link — and, in a field's
 * icon, nothing that could fetch. No `<title>` either: its text becomes its
 * mark's `data-title` and, as the `<title>` was, its accessible name.
 */
export function sanitizeSvg(svg: Element): void {
	// A field's icon (da-45wgz) is a person's own SVG, vetted by the CLI;
	// this is the second layer, with the CLI's rules: no `<style>` (it is
	// page-wide once inlined), and no `<set>`/`<animate>` that writes a
	// handler or a link later.
	for (const el of svg.querySelectorAll('script, foreignObject, style')) el.remove();
	for (const el of svg.querySelectorAll('set, animate, animateMotion, animateTransform')) {
		if (/^on|href$/i.test(el.getAttribute('attributeName') ?? '')) el.remove();
	}
	for (const title of svg.querySelectorAll('title')) {
		const mark = title.closest('[data-state]');
		mark?.setAttribute('data-title', title.textContent ?? '');
		mark?.setAttribute('aria-label', title.textContent ?? '');
		title.remove();
	}
	for (const el of [svg, ...svg.querySelectorAll('*')]) {
		for (const { name, value } of [...el.attributes]) {
			if (/^on/i.test(name) || (/href$/i.test(name) && !value.startsWith('#'))) {
				el.removeAttribute(name);
			}
		}
	}
	// A field's icon is the one part a person wrote: nothing in it that could
	// fetch, as the CLI's `check_icon` holds it (`fetches` there).
	for (const el of svg.querySelectorAll('defs > svg[id^="icon-"], defs > svg[id^="icon-"] *')) {
		for (const { name, value } of [...el.attributes]) if (fetches(value)) el.removeAttribute(name);
	}
}

/** `text` as an `<svg>` for the page, `sanitizeSvg`'s, or `null` when it is not one. */
export function inlineSvg(text: string): SVGSVGElement | null {
	const doc = new DOMParser().parseFromString(text, 'image/svg+xml');
	const svg = doc.documentElement;
	if (!(svg instanceof SVGSVGElement) || doc.querySelector('parsererror')) return null;
	sanitizeSvg(svg);
	return document.importNode(svg, true);
}

/** The functions an icon draws with; `url()` is let through only to a `#fragment`. */
const DRAWS = new Set(['matrix', 'translate', 'scale', 'rotate', 'skewx', 'skewy', 'rgb', 'rgba', 'hsl', 'hsla']);

/**
 * Whether an attribute value could make the browser fetch (da-45wgz): any
 * CSS escape — `\75 rl(` IS `url(` — and any function but a transform's, a
 * color's or a `url(#…)`, since `image-set('…')` fetches with no `url(`.
 */
function fetches(value: string): boolean {
	const v = value.toLowerCase();
	if (v.includes('\\')) return true;
	for (const m of v.matchAll(/([a-z0-9-]*)\s*\(\s*['"]?/g)) {
		if (m[1] === 'url' ? !v.startsWith('#', m.index + m[0].length) : !DRAWS.has(m[1])) return true;
	}
	return false;
}
