// The logo (da-y992): the database's NAME in Gemunu Libre, the menu of databases,
// and the page — accent and grounds — painted in the colour of the one it
// is on.
//
// vividn, 2026-09-16 22:26: "The datum logo, we should remove the '.day' and
// give it a nice font, maybe we start with Geo. then make this logo actually
// a dropdown for selecting different dbs […] And the base color of the app
// changes depending on the db you select. That way it is both a logo and
// functional." And on PR #25: "the whole logo should actually just be the
// database name"; "when hovering over the "datum" word make a little down
// arrow appear next to it, but only show when hovering over it"; "change the
// subtle background color too, not just the accent color"; the beta marker
// up to the top bar by the gear (and on 10-14 onto the gear itself); "let's
// use Gemunu Libre"; and then "Let's increase the font weight to 300".
//
// Every claim is read off the PAGE: what the mark says and in what face,
// what the menu offers and where choosing goes, what colour the app is then
// painted, and where the beta mark went. The hash is asserted only for what
// da-wikou promised of it: a name keeps its hue, and the ranges hold.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { focusTop, sharedApp, startApp, stopApp, withApp } from './support/app.mjs';
import { until } from './support/until.mjs';
import { resolveLibAsSource } from './support/libhook.mjs';

resolveLibAsSource();
const { colorOf, SAT_MIN, SHIFT_MIN, shiftMax } = await import('../src/lib/database.ts');

before(startApp);
after(stopApp);

// ONE APP per screen where tests share their options (da-6b3dj, sharedApp in
// support/app.mjs): Playwright's 1280x720 and a 390x640 phone on `datum`.
// `solo` empties only the page's own database,
// so a database another test seeded is still in the logo's menu — none of
// the shared tests counts that list after another has grown it. What still
// opens an app of its own is at a `?db` or screen of its own, watches the
// requests (`prepare`), or moves the page to another database, reloads or
// opens a second tab: a shared app keeps its query, so it would stay there.
const plain = sharedApp();
const phone = sharedApp({ viewport: { width: 390, height: 640 } });

const BRAND = '.prompt-bar .brand';
const MENU = '[role="menu"][aria-label="databases"]';

/** Open the logo's menu and return its items: label and whether it is the page's. */
async function openMenu(page) {
	await page.locator(BRAND).click();
	await page.locator(MENU).waitFor();
	return () =>
		page.locator(`${MENU} button`).evaluateAll((all) =>
			all.map((el) => ({
				label: el.textContent.replace(/[✓ ]/g, '').trim(),
				checked: el.getAttribute('aria-checked')
			}))
		);
}

/** The app is up again after a navigation the PAGE started. */
async function upAgain(page) {
	await page.locator('.prompt-bar input:not([disabled])').waitFor();
	await page.waitForFunction(() => typeof window.datum?.run === 'function');
}

/**
 * The colour this page PAINTS for `token`, as `rgb(...)`.
 *
 * Through an element that uses it, never `getPropertyValue(token)`: a custom
 * property computes to its declared token — `hsl(var(--hue) …)` with the hue
 * substituted — and nothing resolves it to a colour until something is drawn
 * in it. A contrast check fed that string reads "240 100 74.3" as an rgb
 * triple and answers nonsense.
 */
const paintOf = (page, token) =>
	page.evaluate((t) => {
		const probe = document.createElement('span');
		probe.style.color = `var(${t})`;
		document.body.append(probe);
		const painted = getComputedStyle(probe).color;
		probe.remove();
		return painted;
	}, token);

const accentOf = (page) => paintOf(page, '--accent');

/** WCAG contrast between two `rgb(...)` strings. */
function contrast(a, b) {
	const channel = (c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
	const luminance = (color) => {
		const [r, g, b] = color.match(/\d+(\.\d+)?/g).map((n) => channel(Number(n) / 255));
		return 0.2126 * r + 0.7152 * g + 0.0722 * b;
	};
	const [x, y] = [luminance(a), luminance(b)];
	return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
}

/** Log one row into `db` without moving the page. */
const seed = (app, db, field) =>
	app.run(['--db', db, 'occur', field, '-t', '0700']).then((r) => assert.equal(r.code, 0, r.stderr));

describe('the logo', () => {
	test('reads the database’s name in Gemunu Libre 300, served from this origin and never from Google', async () => {
		const asked = [];
		await withApp(
			async (app) => {
				const { page } = app;
				// The default database is called `datum`, and that is the only
				// reason the mark reads `datum` here.
				assert.equal(await page.locator(BRAND).innerText(), 'datum');

				// The face the browser actually resolved, not the stack asked for.
				await page.evaluate(() => document.fonts.ready);
				assert.match(
					await page.locator(BRAND).evaluate((el) => getComputedStyle(el).fontFamily),
					/^"?Gemunu Libre"?/
				);
				assert.ok(
					await page.evaluate(() => document.fonts.check('300 1.45rem "Gemunu Libre"')),
					'Gemunu Libre 300 is not loaded'
				);
				// Asked for BY WEIGHT, so nothing is synthesised: the face is 300 alone.
				assert.equal(
					await page.locator(BRAND).evaluate((el) => getComputedStyle(el).fontWeight),
					'300'
				);

				// Local-first: the app must look the same offline, so the file
				// is ours and nothing was fetched from Google to get it.
				assert.ok(
					asked.some((url) => url.endsWith('/fonts/gemunu-libre-latin-300.woff2')),
					`the vendored face was never fetched: ${asked.join(' ')}`
				);
				// One weight ships: the one the mark uses.
				assert.ok(
					!asked.some((url) => /gemunu-libre-latin-200/.test(url)),
					'the 200 weight is still fetched'
				);
				// And the face it replaced is gone, not merely unused.
				assert.ok(!asked.some((url) => /\/fonts\/geo-/.test(url)), 'the old Geo face was fetched');
				const google = asked.filter((url) => /fonts\.(googleapis|gstatic)\.com/.test(url));
				assert.deepEqual(google, [], 'the app went to Google for a font');
			},
			{ prepare: (context) => context.on('request', (r) => asked.push(r.url())) }
		);
		await withApp(
			async ({ page }) => {
				// vividn: "when you're in the finance db, there is no datum word
				// down there at all".
				assert.equal(await page.locator(BRAND).innerText(), 'finance');
				assert.equal(await page.locator('.prompt-bar').getByText('datum', { exact: true }).count(), 0);
				// The bare name says which; the label says what it is too.
				const label = await page.locator(BRAND).getAttribute('aria-label');
				assert.ok(label.includes('finance') && label !== 'finance', label);
			},
			{ query: '?db=finance' }
		);
	});

	test('a down arrow shows beside the name on hover and on keyboard focus, and moves nothing', async () => {
		await withApp(async ({ page }) => {
			const arrow = () =>
				page.locator(BRAND).evaluate((el) => Number(getComputedStyle(el, '::after').opacity));
			const nameBox = () => page.locator(`${BRAND} .name`).boundingBox();

			await page.mouse.move(640, 300);
			const rest = await nameBox();
			await until(arrow, (o) => o === 0);

			await page.locator(BRAND).hover();
			await until(arrow, (o) => o === 1);
			assert.deepEqual(await nameBox(), rest, 'the name moved when the arrow appeared');
			// Drawn content, not text: the mark still reads the name alone.
			assert.equal(await page.locator(BRAND).innerText(), 'finance');

			await page.mouse.move(640, 300);
			await until(arrow, (o) => o === 0);

			// The keyboard's hover. Focus from a click would not be
			// `:focus-visible`, so the focus comes by Tab from the prompt's
			// neighbour — the page body — until it lands on the mark.
			await focusTop(page);
			for (let i = 0; i < 80; i++) {
				await page.keyboard.press('Tab');
				if (await page.locator(BRAND).evaluate((el) => el === document.activeElement)) break;
			}
			await until(arrow, (o) => o === 1);
		}, { query: '?db=finance', reducedMotion: 'reduce' });
	});

	test('a long name steps its type down, then ellipsizes — and costs the prompt nothing', async (t) => {
		const LONG = 'household-accounts-2026';
		const measure = (page) =>
			page.evaluate(() => {
				const box = (s) => document.querySelector(s).getBoundingClientRect();
				const [mark, block, prompt] = [box('.prompt-bar .brand'), box('.prompt-bar .brand-block'), box('.prompt-bar .prompt')];
				return {
					inside: mark.left >= block.left - 0.5 && mark.right <= block.right + 0.5,
					height: mark.height,
					prompt: prompt.width,
					size: parseFloat(getComputedStyle(document.querySelector('.prompt-bar .brand')).fontSize)
				};
			});
		for (const viewport of [{ width: 1280, height: 900 }, { width: 412, height: 563 }]) {
			await t.test(`${viewport.width}x${viewport.height}`, async () => {
				let short;
				await withApp(async ({ page }) => (short = await measure(page)), { viewport, query: '?db=work' });
				await withApp(
					async ({ page }) => {
						const long = await measure(page);
						assert.ok(long.inside, 'the mark is outside the brand block');
						assert.ok(long.height >= 24, `the mark is ${long.height}px tall`);
						assert.ok(long.size < short.size, 'a long name is drawn as large as a short one');
						assert.ok(
							long.prompt >= short.prompt - 1,
							`the prompt lost ${short.prompt - long.prompt}px to the name`
						);
						// Cut on screen, whole where it is read.
						assert.ok(
							await page.locator(`${BRAND} .name`).evaluate((el) => el.scrollWidth > el.clientWidth),
							'the name was not cut'
						);
						assert.ok((await page.locator(BRAND).getAttribute('title')).includes(LONG), 'the title does not hold the whole name');
						// One phrasing, for the pointer and for a screen reader alike.
						assert.equal(
							await page.locator(BRAND).getAttribute('aria-label'),
							await page.locator(BRAND).getAttribute('title')
						);
					},
					{ viewport, query: `?db=${LONG}` }
				);
			});
		}
	});

	test('opens a menu of the databases, the page’s own checked, without `_internal`', async () => {
		await plain.solo(async (app) => {
			const { page } = app;
			await seed(app, 'other', 'coffee');
			const items = await openMenu(page);

			// The pool is re-read as the logo is pressed, so the database
			// seeded a moment ago is in the list the open menu is showing.
			await until(items, (all) => all.some((i) => i.label === 'other'));
			const all = await items();
			assert.deepEqual(
				all.map((i) => i.label),
				['datum', 'other', 'new database'],
				'the menu is not the pool plus the one way to grow it'
			);
			assert.deepEqual(
				all.filter((i) => i.checked === 'true').map((i) => i.label),
				['datum'],
				'the page’s own database is not the checked one'
			);
			// `_internal` is on the device — the layouts live there — and is
			// never a database the page may be pointed at.
			assert.ok(
				!all.some((i) => i.label === '_internal'),
				'the app’s own database is offered'
			);
		});
	});

	test('choosing one goes there, and the page then reads it', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seed(app, 'other', 'coffee');
			const items = await openMenu(page);
			await until(items, (all) => all.some((i) => i.label === 'other'));

			await page.locator(`${MENU} button`, { hasText: 'other' }).click();
			await page.waitForURL(/[?&]db=other\b/);
			await upAgain(page);

			// The tail is `other`'s rows, and the menu now checks `other`.
			await until(
				() => app.paneTable('tail').then((t) => t.rows.flat().join(' ')),
				(shown) => /coffee/.test(shown)
			);
			const after = await openMenu(page);
			assert.deepEqual(
				(await after()).filter((i) => i.checked === 'true').map((i) => i.label),
				['other']
			);
		});
	});

	test('a tab that switched hears the database it moved to, not the one it left', async () => {
		// The switch is a FULL navigation on purpose (vremya on #22): two
		// listeners are keyed to the page's database once, at mount — the
		// root frame's and the shell's — so a tab that changed database
		// under its own feet would go on re-reading for the one it left.
		// This is that, from the reader's side: what the tail does when
		// each of the two databases is written to by another tab.
		await withApp(async (app) => {
			const { page } = app;
			await seed(app, 'other', 'coffee');
			const items = await openMenu(page);
			await until(items, (all) => all.some((i) => i.label === 'other'));
			await page.locator(`${MENU} button`, { hasText: 'other' }).click();
			await page.waitForURL(/[?&]db=other\b/);
			await upAgain(page);

			const shown = () => app.paneTable('tail').then((t) => t.rows.flat().join(' '));
			await until(shown, (rows) => /coffee/.test(rows));

			// The database this tab LEFT, then the one it is on — in that
			// order, from one peer, so the second landing proves the first
			// was heard and ignored rather than merely slow.
			const peer = await app.newTab('cli?db=other');
			assert.equal((await peer.run(['--db', 'datum', 'occur', 'tea', '-t', '0800'])).code, 0);
			assert.equal((await peer.run(['occur', 'milk', '-t', '0900'])).code, 0);

			await until(shown, (rows) => /milk/.test(rows));
			assert.doesNotMatch(
				await shown(),
				/tea/,
				'the switched tab is still reading the database it left'
			);
		});
	});

	test('`new database` asks in the app’s own dialog, and a name that has never existed opens and keeps its first row', async () => {
		// vremya on #22: the first command on a database nobody has named
		// before is the case the worker persists lazily, so this is the one
		// that has to be read back after a reload.
		await withApp(async (app) => {
			const { page } = app;
			// vividn on PR #25: "lets use our own dialog" — so a native one is a
			// failure, not a thing to accept.
			const native = [];
			page.on('dialog', (d) => (native.push(d.type()), d.dismiss()));

			await openMenu(page);
			await page.locator(`${MENU} button`, { hasText: 'new database' }).click();
			const ask = page.locator('dialog.ask');
			await ask.waitFor();
			const field = ask.locator('input');
			assert.ok(await field.evaluate((el) => el === document.activeElement), 'the field does not have the focus');

			// Empty is refused quietly: Enter does nothing, the dialog stays.
			await field.press('Enter');
			assert.ok(await ask.evaluate((el) => el.open), 'an empty name closed the dialog');
			assert.equal(new URL(page.url()).searchParams.get('db'), null);

			// Trimmed, and Enter submits.
			await field.fill('  fresh  ');
			await field.press('Enter');
			await page.waitForURL(/[?&]db=fresh\b/);
			await upAgain(page);
			assert.deepEqual(native, [], 'the browser’s own dialog was used');

			assert.equal((await app.run(['occur', 'tea', '-t', '0800'])).code, 0);
			await app.reload();
			assert.deepEqual(
				(await app.entries()).map((e) => e.field),
				['tea'],
				'the first row of a brand-new database did not survive the reload'
			);
		});
	});

	test('the dialog closes on Escape and on cancel, goes nowhere, and gives the focus back to the mark', async () => {
		await plain.solo(async ({ page }) => {
			const before = page.url();
			for (const dismiss of [
				(ask) => ask.locator('input').press('Escape'),
				(ask) => ask.getByRole('button', { name: 'cancel' }).click()
			]) {
				await openMenu(page);
				await page.locator(`${MENU} button`, { hasText: 'new database' }).click();
				const ask = page.locator('dialog.ask');
				await ask.waitFor();
				await ask.locator('input').fill('never');
				await dismiss(ask);
				await ask.waitFor({ state: 'detached' });
				assert.equal(page.url(), before, 'a dismissed dialog navigated');
				assert.ok(
					await page.locator(BRAND).evaluate((el) => el === document.activeElement),
					'the focus did not come back to the mark'
				);
			}
		});
	});

	test('the menu is whole on the screen, however many databases arrive after it opened', async () => {
		// The list is read off OPFS while the menu is already up, so the menu
		// GROWS under itself — and it hangs off the bottom bar it opens from
		// unless the placement follows the size (PaneMenu's `place`). Held at
		// the phone, where there is least room, and stated as the rule: the
		// box lies inside the screen.
		await phone.solo(
			async (app) => {
				const { page } = app;
				await seed(app, 'other', 'coffee');
				await seed(app, 'work', 'standup');
				const items = await openMenu(page);
				await until(items, (all) => all.length === 4);
				// And a rendering step after the fourth arrived: `place` runs
				// from a ResizeObserver, which the browser delivers in the next
				// frame's rendering, so a read straight after the items land can
				// catch the menu grown and not yet moved — which a loaded lane
				// did, at 648 of 640 (da-31464).
				await page.evaluate(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))));

				const box = await page.locator(MENU).boundingBox();
				const screen = await page.evaluate(() => ({
					width: document.documentElement.clientWidth,
					height: document.documentElement.clientHeight
				}));
				assert.ok(box.y >= 0, `the menu starts at ${box.y}`);
				assert.ok(
					box.y + box.height <= screen.height,
					`the menu ends at ${box.y + box.height}, the screen at ${screen.height}`
				);
				assert.ok(box.x >= 0 && box.x + box.width <= screen.width, 'the menu is off the side');
			}
		);
	});

	test('Escape closes the menu and the logo keeps the focus', async () => {
		await plain.solo(async ({ page }) => {
			await openMenu(page);
			await page.keyboard.press('Escape');
			await page.locator(MENU).waitFor({ state: 'detached' });
			assert.ok(await page.locator(BRAND).evaluate((el) => el === document.activeElement));
		});
	});

	// The focus can leave an open menu without closing it — a focused item
	// the list re-renders away drops it on the body — and the menu's own
	// keys hear nothing then. Escape must still close it (da-8zpep: a CI
	// run where the menu stayed open through an Escape for 30s).
	test('Escape closes the menu when the focus has left it', async () => {
		await withApp(async ({ page }) => {
			await openMenu(page);
			await page.evaluate(() => document.activeElement.blur());
			await page.keyboard.press('Escape');
			await page.locator(MENU).waitFor({ state: 'detached' });
		});
	});

	test('it is a fingertip’s 24px on a phone', async () => {
		await phone.solo(
			async ({ page }) => {
				const box = await page.locator(BRAND).boundingBox();
				assert.ok(box.height >= 24, `the logo is ${box.height}px tall`);
				assert.ok(box.width >= 24, `the logo is ${box.width}px wide`);
			}
		);
	});
});

/** Today's palette, as the page painted it before da-y992. */
const DATUM = {
	'--page': 'rgb(10, 10, 26)',
	'--ground': 'rgb(13, 13, 26)',
	'--tint': 'rgb(30, 30, 56)',
	'--rule': 'rgb(42, 42, 74)',
	'--accent': 'rgb(124, 124, 255)',
	'--accent-bright': 'rgb(173, 173, 255)',
	'--muted': 'rgb(85, 85, 138)',
	'--quiet': 'rgb(68, 68, 102)',
	'--ink': 'rgb(200, 200, 232)',
	'--ink-bright': 'rgb(224, 224, 255)',
	'--raised': 'rgb(26, 26, 48)'
};

/** The colours that MEAN something, which no database moves. */
const MEANING = { '--alarm': 'rgb(232, 130, 12)', '--error': 'rgb(244, 100, 90)', '--warn': 'rgb(255, 209, 102)', '--track': 'rgb(38, 162, 105)' };

const paletteOf = async (page, tokens) =>
	Object.fromEntries(await Promise.all(Object.keys(tokens).map(async (t) => [t, await paintOf(page, t)])));

describe('the base colour follows the database', () => {
	test('the default database is the palette datum has always had, grounds and accent, to the byte', async () => {
		await plain.solo(async ({ page }) => {
			assert.deepEqual(await paletteOf(page, DATUM), DATUM);
			assert.deepEqual(await paletteOf(page, MEANING), MEANING);
		});
	});

	test('another database moves the accent AND the grounds, and not the colours that mean something', async () => {
		await withApp(async ({ page }) => {
			const other = await paletteOf(page, DATUM);
			for (const token of Object.keys(DATUM)) {
				assert.notEqual(other[token], DATUM[token], `${token} did not take the database's hue`);
			}
			assert.deepEqual(await paletteOf(page, MEANING), MEANING);
			// The page itself is drawn in it, not only the token.
			assert.equal(await page.evaluate(() => getComputedStyle(document.body).backgroundColor), other['--page']);
		}, { query: '?db=other' });
	});

	test('one database is one colour — across a reload, and across tabs', async () => {
		await withApp(async (app) => {
			const first = await accentOf(app.page);
			await app.reload();
			assert.equal(await accentOf(app.page), first, 'the colour changed under a reload');
			const peer = await app.newTab('?db=other');
			assert.equal(await accentOf(peer.page), first, 'two tabs of one database disagree');
			// And a different name is a different colour in the same breath.
			const elsewhere = await app.newTab('?db=work');
			assert.notEqual(await accentOf(elsewhere.page), first);
		}, { query: '?db=other' });
	});

	test('the shell writes the three numbers the name gives', async () => {
		await withApp(async ({ page }) => {
			const written = await page.evaluate(() => {
				const { style } = document.documentElement;
				return ['--hue', '--sat', '--shift'].map((p) => style.getPropertyValue(p));
			});
			const { hue, sat, shift } = colorOf('other');
			assert.deepEqual(written, [String(hue), String(sat), `${shift}%`]);
		}, { query: '?db=other' });
	});

	test('whatever color a name picks, text is legible on the tinted grounds', async () => {
		// Every hue, across the range of saturation and at each end of the
		// lightness it allows: a tinted GROUND is where a guarantee can quietly
		// stop being one, so this holds the page at every color a name can pick
		// rather than the few a handful of names hash to (da-kni8r, da-wikou).
		// A literal is a text color no token names.
		const grounds = ['--page', '--ground', '--tint', '--raised'];
		const LEGIBLE = [
			...['--ink', '--ink-bright', '--accent', '--accent-bright', '--warn'].flatMap((ink) => grounds.map((g) => [ink, g])),
			...['--error', '--track'].flatMap((ink) => [[ink, '--page'], [ink, '--ground']]),
			// A word under the prompt (CliEntry, Doc, RawLines), and the darkest
			// terminal color legible on a pane (ansi.ts).
			['#8888aa', '--ground'],
			['#6a8cff', '--ground'],
			// The ink on a fill.
			['--page', '--accent'],
			['--ground', '--accent'],
			['--page', '--error'],
			['--page', '#f0b429']
		];
		const FAINT = ['--muted', '--quiet'].flatMap((ink) => grounds.map((g) => [ink, g]));
		const tokens = [...new Set([...LEGIBLE, ...FAINT].flat())];
		const colors = [];
		for (let sat = 1; sat >= SAT_MIN - 0.001; sat -= 0.1) {
			for (const shift of [SHIFT_MIN, 0, shiftMax(sat)]) colors.push({ sat: sat.toFixed(1), shift });
		}
		await plain.solo(async ({ page }) => {
			const { own, painted } = await page.evaluate(
				({ tokens, colors }) => {
					// The tokens resolve at the root, so the numbers are turned there;
					// with the page's body out of the styling for the sweep, a turn
					// restyles the probes and not the whole app. All in one task, so
					// no frame is drawn and no observer sees it.
					const html = document.documentElement;
					const own = ['--hue', '--sat', '--shift'].map((p) => html.style.getPropertyValue(p));
					const display = document.body.style.display;
					const probes = tokens.map((t) => {
						const probe = document.createElement('span');
						probe.style.color = t.startsWith('--') ? `var(${t})` : t;
						html.append(probe);
						return probe;
					});
					const read = () => Object.fromEntries(probes.map((p, i) => [tokens[i], getComputedStyle(p).color]));
					const at = { datum: read() };
					document.body.style.display = 'none';
					for (const { sat, shift } of colors) {
						html.style.setProperty('--sat', sat);
						html.style.setProperty('--shift', `${shift}%`);
						for (let hue = 0; hue < 360; hue++) {
							html.style.setProperty('--hue', String(hue));
							at[`hue ${hue} sat ${sat} shift ${shift}`] = read();
						}
					}
					['--hue', '--sat', '--shift'].forEach((p, i) => html.style.setProperty(p, own[i]));
					document.body.style.display = display;
					probes.forEach((p) => p.remove());
					return { own, painted: at };
				},
				{ tokens, colors }
			);
			// The `datum` row is the name → color path only if the shell wrote
			// the numbers; with none inline it would be the stylesheet's.
			assert.deepEqual(own, ['240', '1', '0%'], 'the shell did not write datum its color');
			assert.equal(Object.keys(painted).length, 1 + colors.length * 360);
			const low = [];
			for (const [color, paint] of Object.entries(painted)) {
				for (const [ink, ground] of LEGIBLE) {
					const ratio = contrast(paint[ink], paint[ground]);
					if (!(ratio >= 4.5)) low.push(`${color}: ${ink} on ${ground} is ${ratio.toFixed(3)}:1`);
				}
				for (const [ink, ground] of FAINT) {
					const ratio = contrast(paint[ink], paint[ground]);
					const floor = contrast(painted.datum[ink], painted.datum[ground]);
					// What rounding a channel to 8 bits can move a ratio this low.
					if (!(ratio >= floor - 0.02)) low.push(`${color}: ${ink} on ${ground} is ${ratio.toFixed(3)}:1, datum's ${floor.toFixed(3)}:1`);
				}
			}
			assert.deepEqual(low, [], 'text is under its contrast');
		});
	});
});

describe('a name gives its color', () => {
	test('datum is pinned, and the same name always gets the same color', () => {
		assert.deepEqual(colorOf('datum'), { hue: 240, sat: 1, shift: 0 });
		for (const name of ['work', 'other', 'db1']) assert.deepEqual(colorOf(name), colorOf(name));
	});

	test('a name keeps the hue it had', () => {
		// Written out from `hueOf` as it stood before da-wikou.
		for (const [name, hue] of Object.entries({ work: 264, other: 21, db1: 134, db2: 315, journal: 288, health: 39, 'é-notes': 40 })) assert.equal(colorOf(name).hue, hue, name);
	});

	const NAMES = Array.from({ length: 5000 }, (_, i) => (i % 2 ? `db${i}` : `${i.toString(36)}-notes`));

	test('saturation and lightness stay in their ranges, leaning to full saturation', () => {
		const sats = [];
		for (const name of NAMES) {
			const { sat, shift } = colorOf(name);
			assert.ok(sat >= SAT_MIN && sat <= 1, `${name}: sat ${sat}`);
			assert.ok(shift >= SHIFT_MIN && shift <= shiftMax(sat), `${name}: shift ${shift} at sat ${sat}`);
			sats.push(sat);
		}
		sats.sort((a, b) => a - b);
		const share = (under) => sats.filter((s) => s < under).length / sats.length;
		assert.ok(sats[sats.length >> 1] >= 0.8, `the median is ${sats[sats.length >> 1]}`);
		assert.ok(share(0.6) > 0.1 && share(0.6) < 0.25, `${share(0.6)} are under 0.6`);
	});

	test('neither saturation nor lightness follows the hue', () => {
		// The mean over each sixth of the wheel against the whole's.
		const mean = (of, names) => names.reduce((sum, n) => sum + of(colorOf(n)), 0) / names.length;
		for (const [of, near] of [[(c) => c.sat, 0.03], [(c) => c.shift, 0.3]]) {
			const whole = mean(of, NAMES);
			for (let sixth = 0; sixth < 6; sixth++) {
				const part = NAMES.filter((n) => Math.floor(colorOf(n).hue / 60) === sixth);
				assert.ok(Math.abs(mean(of, part) - whole) < near, `sixth ${sixth}: ${mean(of, part)} against ${whole}`);
			}
		}
		const siblings = new Set(['db1', 'db2', 'db3', 'db4'].map((n) => colorOf(n).sat));
		assert.ok(siblings.size > 1, 'names one digit apart share a saturation');
	});
});

describe('the beta mark', () => {
	// A little banner ON the gear (da-eue7y). vividn, 2026-10-14: "let's move
	// the "beta" tag to just be a little banner on the gear button? Right
	// now on mobile it takes up way too much space in the header bar. move
	// the beta info message into the gear icon by the version number". It
	// was a `beta` mark and an ⓘ of their own before the gear (da-y992).
	test('is a little banner on the gear, with no width of its own in the root bar, and taps go through it', async () => {
		for (const viewport of [{ width: 1280, height: 900 }, { width: 390, height: 640 }, { width: 320, height: 664 }]) {
			const open = viewport.width === 390 ? phone.solo : (fn) => withApp(fn, { viewport });
			await open(async ({ page }) => {
				const at = await page.evaluate(() => {
					const box = (s) => document.querySelector(s)?.getBoundingClientRect().toJSON() ?? null;
					const banner = box('.root-bar .gear .beta');
					const hit = banner && document.elementFromPoint(banner.x + banner.width / 2, banner.y + banner.height / 2);
					return {
						banner,
						words: document.querySelector('.root-bar .gear .beta')?.textContent,
						gear: box('.root-bar .gear'),
						end: box('.root-bar .end'),
						pair: document.querySelectorAll('.root-bar .tag, .root-bar .hint-button').length,
						tapped: hit?.closest('.root-bar button.gear') !== null,
						screen: document.documentElement.clientWidth
					};
				});
				const size = `${viewport.width}x${viewport.height}`;
				assert.equal(at.words, 'beta', `${size}: the gear wears no beta banner`);
				assert.equal(at.pair, 0, `${size}: the beta pair is still in the root bar`);
				// Small, and on the gear: across its foot, inside its width.
				assert.ok(at.banner.width <= at.gear.width && at.banner.height <= 12, `${size}: the banner is ${at.banner.width}x${at.banner.height}`);
				assert.ok(at.banner.left >= at.gear.left && at.banner.right <= at.gear.right, `${size}: the banner is off the gear's sides`);
				assert.ok(at.banner.top > at.gear.top + at.gear.height / 2 && at.banner.top < at.gear.bottom, `${size}: the banner is not on the gear's foot`);
				assert.ok(at.tapped, `${size}: a tap on the banner does not reach the gear`);
				// The bar's right end holds nothing while sync and the acts have
				// nothing to say (da-6no9e); the gear on the screen at the narrowest.
				assert.ok(at.end.width <= 0.5, `${size}: the bar's end is ${at.end.width}px wide`);
				assert.ok(at.gear.right <= at.screen, `${size}: the gear is off the screen`);
			});
		}
	});

	test("its word is in the gear's menu, under the version, and whole on a phone", async () => {
		await phone.solo(async ({ page }) => {
			await page.locator('.root-bar button.gear').click();
			const at = await page.locator('[role="menu"]').evaluate((menu) => ({
				items: [...menu.children].map((el) => el.textContent.trim()),
				noteAt: [...menu.children].indexOf(menu.querySelector('.note')),
				note: menu.querySelector('.note').getBoundingClientRect().toJSON(),
				// An item of the menu as the version is: disabled, not walked to.
				role: [menu.querySelector('.note').getAttribute('role'), menu.querySelector('.note').getAttribute('aria-disabled')],
				width: document.documentElement.clientWidth,
				height: document.documentElement.clientHeight
			}));
			const stamp = at.items.findIndex((t) => /^3\.0\.\d+-beta · /.test(t));
			assert.ok(stamp >= 0, `no version in ${at.items}`);
			assert.equal(at.noteAt, stamp + 1, `the note is not under the version: ${at.items}`);
			assert.ok(at.items[at.noteAt], 'the note says nothing');
			assert.deepEqual(at.role, ['menuitem', 'true']);
			assert.ok(at.note.left >= 0 && at.note.right <= at.width, `the note is off the side: ${at.note.left}..${at.note.right}`);
			assert.ok(at.note.bottom <= at.height, 'the note is off the bottom');
			await page.keyboard.press('Escape');
			await page.locator('[role="menu"]').waitFor({ state: 'detached' });
		});
	});
});
