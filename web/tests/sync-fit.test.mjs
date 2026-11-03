// The Sync widget fits a phone and is legible (da-yoef7). vividn, on a
// phone: «all sorts of width issues, where there is horizontal overflow.
// the buttons are really ugly and are actually unredable (white text on
// white button?)». Measured on b51eb233: button text at a contrast of 1.1,
// a 459px table on a 390px screen, and `copy` at x=−13 once sync was on.
//
// At each screen and in each state: the page does not scroll sideways, no
// element in the widget stands past its box, and every visible button's
// text has a contrast of 4.5 against what it is drawn on.
//
// `SYNC_SHOTS=<dir>` also saves each state as a picture, for the PR.

import test, { after, before, describe } from 'node:test';
import assert from 'node:assert/strict';
import { generateKeyPairSync } from 'node:crypto';
import { join } from 'node:path';
import { devices } from 'playwright';
import { startApp, stopApp, withApp } from './support/app.mjs';

before(startApp);
after(stopApp);

const WIDGET = 'section.pane[data-title="sync"]';
const SCREENS = [
	{ name: 'phone', options: { device: devices['iPhone 12'], viewport: { width: 390, height: 844 } } },
	{ name: 'desktop', options: { viewport: { width: 1280, height: 800 } } }
];

const endpointId = () =>
	generateKeyPairSync('ed25519').publicKey.export({ format: 'der', type: 'spki' }).subarray(-32).toString('hex');

async function cli(app, argv) {
	const r = await app.page.evaluate((a) => window.datum.runQuiet(a), argv);
	assert.equal(r.code, 0, `${argv.join(' ')}: ${r.stderr}`);
	return r.stdout;
}

/** A device admitted to `datum`'s chain as a session would admit it, and how its last session went. */
async function admit(app, label, ...how) {
	const { secret } = JSON.parse(await cli(app, ['sync', 'new-invite', '--expires', '1d']));
	const id = endpointId();
	await cli(app, ['sync', 'handshake', id, '--label', label, '--addr', '{}', '--invite', secret, '--chain', '']);
	if (how.length) await cli(app, ['sync', 'record-session', id, ...how]);
}

/** What the bead measures, in the page: the overflow, what stands outside the widget, and each button's contrast. */
const measure = (page) =>
	page.locator(`${WIDGET} .sync`).evaluate((sync) => {
		const parse = (color) => (color.match(/[\d.]+/g) ?? []).map(Number);
		const channel = (c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
		const luminance = ([r, g, b]) => 0.2126 * channel(r / 255) + 0.7152 * channel(g / 255) + 0.0722 * channel(b / 255);
		// The first ancestor that paints: a button's own ground, or what shows through it.
		const behind = (el) => {
			for (let node = el; node; node = node.parentElement) {
				const bg = parse(getComputedStyle(node).backgroundColor);
				if (bg[3] === undefined || bg[3] > 0) return bg.slice(0, 3);
			}
			return [0, 0, 0];
		};
		const ratio = (fg, bg) => {
			const [light, dark] = [luminance(fg), luminance(bg)].sort((a, b) => b - a);
			return (light + 0.05) / (dark + 0.05);
		};
		const box = sync.getBoundingClientRect();
		const shown = [...sync.querySelectorAll('*')].filter((el) => {
			const r = el.getBoundingClientRect();
			return r.width > 0 && r.height > 0;
		});
		const root = document.documentElement;
		return {
			scroll: [root.scrollWidth, root.clientWidth],
			outside: shown
				.map((el) => ({ el, r: el.getBoundingClientRect() }))
				.filter(({ r }) => r.left < box.left - 0.5 || r.right > box.right + 0.5)
				.map(({ el, r }) => `${el.tagName.toLowerCase()}.${el.className} «${el.textContent.trim().slice(0, 20)}» ${Math.round(r.left)}–${Math.round(r.right)}`),
			buttons: shown
				.filter((el) => el.tagName === 'BUTTON')
				.map((el) => ({ label: el.textContent.trim(), contrast: Math.round(ratio(parse(getComputedStyle(el).color), behind(el)) * 100) / 100 }))
		};
	});

async function fits(page, state, screen) {
	const m = await measure(page);
	assert.equal(m.scroll[0], m.scroll[1], `${state}: the page scrolls sideways`);
	assert.deepEqual(m.outside, [], `${state}: past the widget's box`);
	assert.ok(m.buttons.length, `${state}: no button measured`);
	const faint = m.buttons.filter((b) => b.contrast < 4.5);
	assert.deepEqual(faint, [], `${state}: a button under 4.5`);
	if (process.env.SYNC_SHOTS) await page.screenshot({ path: join(process.env.SYNC_SHOTS, `${screen}-${state}.png`) });
	return m;
}

describe('the sync widget fits its screen, legibly', () => {
	for (const screen of SCREENS)
		test(`${screen.name}: off, adding a device, and on`, async () => {
			await withApp(async (app) => {
				const { page } = app;
				await cli(app, ['--db', 'work', 'occur', 'tea']);
				await app.goto('/sync');
				const widget = page.locator(WIDGET);
				const turnOn = widget.getByRole('button', { name: 'Turn on sync' });
				await turnOn.waitFor();
				// Drawn before the databases are read: measure what it settles to.
				await widget.locator('.others li[data-db="work"]').waitFor();
				const off = await fits(page, 'off', screen.name);
				assert.deepEqual(off.buttons.map((b) => b.label), ['?', 'ⓘ', 'Turn on sync'], 'off: the guide (da-oyvsz), the legend, and the one act');

				await turnOn.click();
				const offer = widget.locator('.offer');
				await offer.locator('.qr svg').waitFor();
				// The ticket is behind its disclosure; the panel's acts are at its bottom.
				assert.equal(await offer.locator('.ticket').isVisible(), false);
				await fits(page, 'adding', screen.name);
				const own = await offer.locator('.actions button').allInnerTexts();
				assert.deepEqual(own.filter((l) => l !== 'Share'), ['Copy link', 'Done']);
				await offer.locator('summary').click();
				assert.match(await offer.locator('.ticket').innerText(), /^datum sync chain join 'https?:\/\/[^\s']+\/sync\/join#[^\s']+'$/);
				await fits(page, 'terminal', screen.name);
				await offer.getByRole('button', { name: 'Done' }).click();

				await admit(app, 'laptop', '--vector', '{}');
				await admit(app, 'tablet', '--error', 'connection refused by the relay');
				await widget.locator('.devices li').nth(2).waitFor();
				await widget.getByRole('button', { name: /^More/ }).click();
				await widget.locator('.more .chain').waitFor();
				await fits(page, 'on', screen.name);
			}, screen.options);
		});

	test('Add device opens the panel with Copy link, Share where the browser shares, and Done', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await page.addInitScript(() => {
					navigator.share = (data) => ((window.__shared = data), Promise.resolve());
				});
				await cli(app, ['sync', 'new-invite']);
				await app.goto('/sync');
				const widget = page.locator(WIDGET);
				await widget.getByRole('button', { name: 'Add device' }).click();
				const offer = widget.locator('.offer');
				await offer.locator('.qr svg').waitFor();
				assert.deepEqual(await offer.locator('.actions button').allInnerTexts(), ['Copy link', 'Share', 'Done']);
				assert.match(await offer.locator('.head').innerText(), /\b\d\d:\d\d\b/, 'the expiry time');
				await offer.getByRole('button', { name: 'Share' }).click();
				const shared = await page.waitForFunction(() => window.__shared);
				assert.match((await shared.jsonValue()).url, /\/sync\/join#\S+$/);
				await offer.getByRole('button', { name: 'Done' }).click();
				await widget.getByRole('button', { name: 'Add device' }).waitFor();
			},
			{ viewport: { width: 390, height: 844 } }
		);
	});

	// The browser's own focus ring is near-black, which on this page is no ring at all.
	test('a button focused by the keyboard is drawn in the accent', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await cli(app, ['sync', 'new-invite']);
				await app.goto('/sync');
				const widget = page.locator(WIDGET);
				const accent = await widget.evaluate((e) => {
					const probe = e.appendChild(document.createElement('i'));
					probe.style.color = 'var(--accent)';
					const color = getComputedStyle(probe).color;
					probe.remove();
					return color;
				});
				const focused = (l, prop) =>
					l.evaluate((e, prop) => (e.focus(), e.matches(':focus-visible') && getComputedStyle(e)[prop]), prop);
				const rename = widget.getByRole('button', { name: 'Rename this device' });
				await rename.waitFor();
				assert.equal(await focused(rename, 'borderTopColor'), accent);
				assert.equal(await focused(widget.locator('.legend .info'), 'outlineColor'), accent);
			},
			{ viewport: { width: 390, height: 844 } }
		);
	});
});
