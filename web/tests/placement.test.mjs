// The field form's Placement tab (da-btyvj): drag a field's lane on
// today's real dayview band, optionally anchored to a neighbour (⚓).
//
// Every assertion reads what was WRITTEN — `setup F --json`, `fields --json`,
// `dayview --show json` — and never the pixels a drag leaves behind: a
// driven drag sends its moves faster than frames are drawn, so a picture
// read after it can look exactly like «the feature is missing».

import test, { after, before, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp } from './support/app.mjs';
import { ONE_PANE } from './support/layout.mjs';
import { editedSpec, placeOf } from '../src/lib/track.ts';
import { FIELD_FORM as MODAL, leaveField, openField } from './support/field.mjs';

/** The box a tap draws round a mark (da-r6wwe): none at rest. */
const BOX = `${MODAL} .placement .box`;
/** A field's marks on the band, the ones a press takes. */
const marks = (field) => `${MODAL} .placement .band g.field[data-field="${field}"] :is(.block, .point-bg)`;
/** The band's day height in pixels (`H` in Placement.svelte): 32px is 0.2 of a day. */
const H = 160;
const round = (n) => Math.round(n * 1000) / 1000;

describe('the spec it saves', () => {
	const base = {
		field: 'coffee', verb: 'occur', color: null, description: null,
		allowed_states: null, state_colors: {}, y: 0.3, height: 0.2
	};
	const edit = (place) => ({ verb: 'occur', color: '', description: '', states: [], place });

	test('untouched, the placement goes back as it came — unset keys stay unset', () => {
		assert.deepEqual(placeOf(base), { y: 0.3, height: 0.2 });
		assert.deepEqual(editedSpec(base, edit(placeOf(base))), base);
		const bare = { ...base, y: undefined, height: undefined };
		assert.equal('y' in editedSpec(bare, edit(placeOf(bare))), false);
	});

	test('tied writes the triple and drops y/height; freed drops the triple', () => {
		const tied = editedSpec(base, edit({ relative_to: 'mood', anchor_pos_ratio: 1, anchor_size_ratio: 0.5 }));
		assert.equal('y' in tied || 'height' in tied, false);
		assert.deepEqual([tied.relative_to, tied.anchor_pos_ratio, tied.anchor_size_ratio], ['mood', 1, 0.5]);
		const freed = editedSpec(tied, edit({ y: 0.1, height: 0.2 }));
		assert.equal('relative_to' in freed || 'anchor_pos_ratio' in freed || 'anchor_size_ratio' in freed, false);
		assert.deepEqual([freed.y, freed.height], [0.1, 0.2]);
	});

	test("an occur dot's size is placement too: kept when set, dropped when the tab leaves it at 1", () => {
		const sized = { ...base, glyph_scale: 2, glyph_aspect: 0.5 };
		assert.deepEqual(placeOf(sized), { y: 0.3, height: 0.2, glyph_scale: 2, glyph_aspect: 0.5 });
		const back = JSON.parse(JSON.stringify(editedSpec(sized, edit({ y: 0.3, height: 0.2, glyph_scale: undefined }))));
		assert.equal('glyph_scale' in back || 'glyph_aspect' in back, false);
	});
});

describe('the Placement tab', () => {
	before(startApp);
	after(stopApp);
	const shared = sharedApp();
	// `main` as a draft of ONE REPL pane: nothing here reads the day frame,
	// and drawing it costs what a boot did (da-m1m9a).
	const onSeam = (fn) => shared.solo(fn, ONE_PANE);

	const spec = async (app, field) => JSON.parse((await app.run(['setup', field, '--json'])).stdout);
	const lane = async (app, field) =>
		JSON.parse((await app.run(['fields', '--json'])).stdout).find((f) => f.field === field);
	const setPlace = async (app, field, place) => {
		const r = await app.run(['setup', field, '--from-json', JSON.stringify({ ...(await spec(app, field)), ...place })]);
		assert.equal(r.code, 0, r.stderr);
	};

	/** Open `field`'s spec, its placement on Basics in view (da-i9upr); resolves to `{ saved }`, the pending save — boxed, or awaiting this would await the save. */
	async function openPlacement(page, field) {
		const { saved } = await openField(page, field);
		await page.locator(`${MODAL} .placement .band svg`).waitFor();
		await page.locator(`${MODAL} .placement .band`).evaluate((el) => el.scrollIntoView({ block: 'center' }));
		return { saved };
	}

	/** The middle of `field`'s latest mark: read in one go, as a redraw replaces the marks. */
	async function middle(page, field) {
		const at = await page.waitForFunction((sel) => {
			const r = [...document.querySelectorAll(sel)].at(-1)?.getBoundingClientRect();
			return r && [r.x + r.width / 2, r.y + r.height / 2];
		}, marks(field));
		return at.jsonValue();
	}

	/** Press at `x`,`y` and drag by `dx`,`dy`. */
	async function pull(page, [x, y], dx, dy) {
		await page.mouse.move(x, y);
		await page.mouse.down();
		await page.mouse.move(x + dx / 2, y + dy / 2);
		await page.mouse.move(x + dx, y + dy);
		await page.mouse.up();
	}

	/** Press the middle of `field`'s latest mark and drag it by `dy`. */
	const drag = async (page, field, dy) => pull(page, await middle(page, field), 0, dy);

	/** Tap `field`'s latest mark, which draws the box round it. */
	async function tap(page, field) {
		const [x, y] = await middle(page, field);
		await page.mouse.click(x, y);
		await page.locator(BOX).waitFor();
	}

	/** Where a handle of the box is drawn: `top`/`bottom` on a bar, `sx,sy` on a dot. */
	async function handle(page, name) {
		const r = await page.locator(`${BOX} [data-handle="${name}"]`).boundingBox();
		return [r.x + r.width / 2, r.y + r.height / 2];
	}

	/** What the band holds over its picture besides the fields' names: nothing at rest, whatever it is called. */
	const overlays = (page) =>
		page.locator(`${MODAL} .placement .band`).evaluate((band) => [...band.children].filter((c) => !(c instanceof SVGElement) && !c.matches('.name, .own')).length);

	/** The marks' top and bottom on the page. */
	const extent = (page, field) =>
		page.locator(marks(field)).evaluateAll((els) => {
			const rs = els.map((b) => b.getBoundingClientRect());
			return [Math.min(...rs.map((r) => r.top)), Math.max(...rs.map((r) => r.bottom))];
		});

	/** Click the eye until it says `label`, and wait for the band drawn around what it names (`dayview --context`). */
	async function turn(page, label) {
		const eye = page.locator(`${MODAL} .placement button.around`);
		for (let i = 0; i < 3 && (await eye.innerText()).trim() !== label; i++) await eye.click();
		assert.equal((await eye.innerText()).trim(), label);
		const context = { context: 'none', now: 'now', '12 hours': '12h' }[label];
		await page.locator(`${MODAL} .placement .band svg[data-context="${context}"]`).waitFor();
	}

	/** Save, and leave the place: Save stays on the spec (da-hda4j). */
	async function done(page, saved) {
		await page.locator(`${MODAL} button.commit`).click();
		const result = await saved;
		assert.equal(result.code, 0, result.stderr);
		await leaveField(page);
	}

	test('a field nobody placed says so, and dragging writes `y`; an edge writes `height`', async () => {
		await onSeam(async (app) => {
			const { page } = app;
			await app.run(['track', 'coffee', '--verb', 'start']);
			await app.run(['start', 'coffee', '-t', '-30']);
			const hashed = await lane(app, 'coffee');
			assert.equal((await spec(app, 'coffee')).y, undefined, 'nothing chose a lane yet');

			let { saved } = await openPlacement(page, 'coffee');
			assert.match(await page.locator(`${MODAL} .readout`).innerText(), /nobody chose this/);
			// Where the hash put it is where the lane is drawn.
			const band = await page.locator(`${MODAL} .placement .band`).boundingBox();
			const [top] = await extent(page, 'coffee');
			assert.ok(Math.abs(top - band.y - 1 - hashed.y * H) < 1.5, `${top - band.y} vs ${hashed.y * H}`);
			assert.equal(await overlays(page), 0, 'no box at rest');

			// A tap draws a thin box round the bar, hinted at its top and bottom centre, and moves nothing.
			await tap(page, 'coffee');
			assert.match(await page.locator(`${MODAL} .readout`).innerText(), /nobody chose this/);
			const look = await page.locator(BOX).evaluate((box) => {
				const hints = [...box.querySelectorAll('.handle')].map((h) => {
					const r = h.getBoundingClientRect();
					return { name: h.dataset.handle, x: r.x + r.width / 2, y: r.y + r.height / 2, drawn: parseFloat(getComputedStyle(h, '::after').width) };
				});
				const r = box.getBoundingClientRect();
				return { border: parseFloat(getComputedStyle(box).borderTopWidth), x: r.x + r.width / 2, top: r.top, bottom: r.bottom, hints };
			});
			assert.ok(look.border >= 1 && look.border <= 2, `a thin line: ${look.border}px`);
			assert.deepEqual(look.hints.map((h) => h.name), ['top', 'bottom']);
			for (const [h, y] of [[look.hints[0], look.top], [look.hints[1], look.bottom]]) {
				// Placed in the box's padding, inside its line: within the line's width of its edge.
				assert.ok(Math.abs(h.x - look.x) < 1 && Math.abs(h.y - y) <= look.border + 0.5, `${h.name} at the centre of its edge: ${[h.x, h.y]} vs ${[look.x, y]}`);
				assert.ok(h.drawn > 0 && h.drawn <= 8, `${h.name} drawn small: ${h.drawn}px`);
			}
			await page.mouse.click(band.x + 4, band.y + (hashed.y < 0.5 ? H - 4 : 4));
			assert.equal(await page.locator(BOX).count(), 0, 'a tap off the field clears it');

			const dir = hashed.y < 0.5 ? 1 : -1;
			await drag(page, 'coffee', dir * 32);
			assert.doesNotMatch(await page.locator(`${MODAL} .readout`).innerText(), /nobody chose/);
			await done(page, saved);
			let shown = await spec(app, 'coffee');
			assert.equal(shown.y, round(hashed.y + dir * 0.2));
			assert.equal(shown.height, 0.12);

			({ saved } = await openPlacement(page, 'coffee'));
			await tap(page, 'coffee');
			await pull(page, await handle(page, 'bottom'), 0, 16);
			await done(page, saved);
			const moved = shown.y;
			shown = await spec(app, 'coffee');
			assert.equal(shown.y, moved, 'an edge leaves the lane where it is');
			assert.equal(shown.height, round(0.12 + 0.1));
			assert.equal(shown.relative_to, undefined);
		});
	});

	// da-1r506: a `y` with no `height` is its band's CENTRE, so the lane is
	// where `fields --json` resolves it, half a default height above `y`.
	test('a `y` set with no `height` is drawn where the band is, centred on it', async () => {
		await onSeam(async (app) => {
			const { page } = app;
			await app.run(['track', 'coffee', '--verb', 'start']);
			await setPlace(app, 'coffee', { y: 0.5 });
			await app.run(['start', 'coffee', '-t', '-30']);
			const resolved = await lane(app, 'coffee');
			assert.equal(round(resolved.y), 0.44, 'the band spans 0.44–0.56');

			const { saved } = await openPlacement(page, 'coffee');
			const band = await page.locator(`${MODAL} .placement .band`).boundingBox();
			const [top] = await extent(page, 'coffee');
			assert.ok(Math.abs(top - band.y - 1 - resolved.y * H) < 1.5, `${top - band.y} vs ${resolved.y * H}`);
			assert.match(await page.locator(`${MODAL} .readout`).innerText(), /y 0\.44/);
			await leaveField(page);
			assert.equal(await saved, null);
		});
	});

	// da-gq232: a field the database knows nothing of has no row in `fields
	// --json`, and its box stood at the top of the day.
	test('a new field is boxed where its name will put it, and follows the name as it is typed', async () => {
		await onSeam(async (app) => {
			const { page } = app;
			const { saved } = await openField(page, 'brandnew', 'start');
			await page.locator(`${MODAL} .placement .band svg`).waitFor();
			/** The sample drawn (da-srrqd) comes to stand at `name`'s lane, as `fields NAME` resolves it, and the field's label beside it. */
			const boxedAt = async (name) => {
				const [row] = JSON.parse((await app.run(['fields', name, '--json'])).stdout);
				await page
					.waitForFunction(
						([name, px, mid]) => {
							const blocks = [...document.querySelectorAll(`.placement .band g.field[data-field="${name}"] rect.block`)];
							const top = Math.min(...blocks.map((b) => Number(b.getAttribute('y'))));
							return Math.abs(parseFloat(document.querySelector('.placement .own')?.style.top) - mid) < 0.5 && Math.abs(top - px) < 1.5;
						},
						[name, row.y * H, (row.y + row.height / 2) * H],
						{ timeout: 5000 }
					)
					.catch(() => assert.fail(`never: ${name}'s box and sample at y ${row.y}`));
				return row.y;
			};
			assert.ok((await boxedAt('brandnew')) > 0.05, 'not the top of the day');
			await page.locator(`${MODAL} input.field`).fill('walkabout');
			const y = await boxedAt('walkabout');
			assert.match(await page.locator(`${MODAL} .readout`).innerText(), /nobody chose this/);

			await done(page, saved);
			const declared = await lane(app, 'walkabout');
			assert.equal((await spec(app, 'walkabout')).y, undefined, 'saved with no lane of its own');
			assert.equal(declared.y, y);
		});
	});

	const FORM_OF = (field) => `.field-form[aria-label="the spec of ${field}"]:not(.beside *)`;
	const PICKER = `${MODAL} .placement select[aria-label="anchor to a field"]`;

	// da-w5ruf: anchoring is asked for. A field allows it, or names what is
	// anchored to it; the ⚓ picker offers only those, and «⚓ mood ×» opens
	// mood's setup on «mood» and frees the lane on ×.
	test('the ⚓ picker offers only the fields that allow anchoring, and is not drawn when none do', async () => {
		await onSeam(async (app) => {
			const { page } = app;
			for (const f of ['mood', 'energy', 'coffee']) await app.run(['track', f, '--verb', 'switch']);
			await setPlace(app, 'mood', { y: 0.2, height: 0.25 });
			await setPlace(app, 'energy', { y: 0.5, height: 0.2 });

			let { saved } = await openPlacement(page, 'coffee');
			assert.equal(await page.locator(PICKER).count(), 0, 'no field allows anchoring');
			await leaveField(page);
			await saved;

			// mood allows it by its checkbox, which the save stores on its spec.
			({ saved } = await openPlacement(page, 'mood'));
			await page.locator(`${MODAL} .placement input[name="anchorable"]`).check();
			await done(page, saved);
			assert.equal((await spec(app, 'mood')).anchorable, true);

			({ saved } = await openPlacement(page, 'coffee'));
			const offered = await page.locator(`${PICKER} option`).evaluateAll((os) => os.map((o) => o.value));
			assert.deepEqual(offered, ['', 'mood'], 'energy does not allow it');
			await leaveField(page);
			await saved;
		});
	});

	test('a field with fields anchored to it names them as links, and has no checkbox', async () => {
		await onSeam(async (app) => {
			const { page } = app;
			for (const f of ['body', 'mood', 'energy']) await app.run(['track', f, '--verb', 'switch']);
			await setPlace(app, 'body', { y: 0.2, height: 0.5 });
			await setPlace(app, 'mood', { relative_to: 'body', anchor_pos_ratio: 0, anchor_size_ratio: 0.5 });
			await setPlace(app, 'energy', { relative_to: 'body', anchor_pos_ratio: 0.5, anchor_size_ratio: 0.5 });

			await openPlacement(page, 'body');
			assert.equal(await page.locator(`${MODAL} .placement input[name="anchorable"]`).count(), 0);
			assert.match(await page.locator(`${MODAL} .anchorees`).innerText(), /⚓ anchored here: energy, mood/);
			await page.locator(`${MODAL} .anchorees button`, { hasText: 'mood' }).click();
			await page.locator(FORM_OF('mood')).waitFor();
			assert.match(await page.locator(`${MODAL} .placement .anchor`).innerText(), /⚓ body\s*×/);
			await leaveField(page);
		});
	});

	test('«⚓ body ×»: body opens its setup, × frees the lane where it stands', async () => {
		await onSeam(async (app) => {
			const { page } = app;
			for (const f of ['body', 'mood']) await app.run(['track', f, '--verb', 'switch']);
			await setPlace(app, 'body', { y: 0.2, height: 0.5 });
			await setPlace(app, 'mood', { relative_to: 'body', anchor_pos_ratio: 0.5, anchor_size_ratio: 0.2 });

			let { saved } = await openPlacement(page, 'mood');
			await page.locator(`${MODAL} .placement .anchor button.link`).click();
			await page.locator(FORM_OF('body')).waitFor();
			await leaveField(page);
			await saved;

			({ saved } = await openPlacement(page, 'mood'));
			await page.locator(`${MODAL} .placement button.unanchor`).click();
			assert.equal(await page.locator(`${MODAL} .placement .anchor`).count(), 0);
			await done(page, saved);
			const shown = await spec(app, 'mood');
			assert.equal(shown.relative_to, undefined);
			// body 0.2 + 0.5 × 0.5, 0.5 × 0.2 tall.
			assert.deepEqual([shown.y, shown.height], [0.45, 0.1]);
		});
	});

	test("anchored, the band is the anchor's lane at full scale: a drag writes the ratios, and the lane moves with its anchor", async () => {
		await onSeam(async (app) => {
			const { page } = app;
			await app.run(['track', 'mood', '--verb', 'switch']);
			await setPlace(app, 'mood', { y: 0.2, height: 0.25, anchorable: true });
			await app.run(['track', 'coffee', '--verb', 'start']);
			await setPlace(app, 'coffee', { y: 0.6, height: 0.1 });
			await app.run(['start', 'coffee', '-t', '-30']);

			const { saved } = await openPlacement(page, 'coffee');
			await page.locator(PICKER).selectOption('mood');
			// 0.6 was under mood: it is put inside it, at its foot, 0.1 of 0.25 tall.
			assert.match(await page.locator(`${MODAL} .readout`).innerText(), /on mood · ×0\.4/);
			// The bar's box is drawn in mood's frame: 0.4 of the band, at its foot.
			await tap(page, 'coffee');
			const band = await page.locator(`${MODAL} .placement .band`).boundingBox();
			const box = await page.locator(BOX).boundingBox();
			assert.ok(Math.abs(box.height - 0.4 * H) < 1.5, `the lane is 0.4 of mood, drawn ${box.height}px of ${H}`);
			assert.ok(Math.abs(box.y + box.height - (band.y + 1 + H)) < 1.5, 'at the foot of the band');
			// Up 32px is up 0.2 of mood's lane, not of the day.
			await drag(page, 'coffee', -32);
			await done(page, saved);

			const shown = await spec(app, 'coffee');
			assert.equal(shown.y, undefined);
			assert.equal(shown.height, undefined);
			assert.deepEqual([shown.relative_to, shown.anchor_pos_ratio, shown.anchor_size_ratio], ['mood', 0.4, 0.4]);

			const drawnAt = async () => {
				const layout = JSON.parse((await app.run(['dayview', '--day-height', '100', '--show', 'json'])).stdout);
				return layout.days.at(-1).fields.find((f) => f.field === 'coffee').cross_offset;
			};
			assert.ok(Math.abs((await drawnAt()) - 30) < 1e-6, 'mood 0.2 + 0.25 × 0.4');
			await setPlace(app, 'mood', { y: 0.5 });
			assert.ok(Math.abs((await drawnAt()) - 60) < 1e-6, 'the anchor moved, and so did coffee');

			// Anchored to a field with no spec, the resolver draws it free: the tab
			// says both — what the spec names, and where it is actually drawn.
			await setPlace(app, 'coffee', { relative_to: 'nobody' });
			const { saved: dismissed } = await openPlacement(page, 'coffee');
			assert.match(await page.locator(`${MODAL} .placement .anchor`).innerText(), /⚓ nobody/);
			assert.match(await page.locator(`${MODAL} .readout`).innerText(), /anchored to nobody, which has no spec/);
			await leaveField(page);
			assert.equal(await dismissed, null);
		});
	});

	// da-c1b6m: an occur field's box is drawn round the dot tapped
	// (da-r6wwe), its corners hinted. A drag of a dot moves the lane, a
	// handle sizes the dot about its centre, and the CLI draws the dot
	// exactly as tall as the box — here, and in every view: the standard
	// dot, whatever the lane (da-ijjjn).
	test("an occur field's tapped dot is boxed, its handles size it; a drag of it moves it; lock aspect keeps its shape and reset makes it round again", async () => {
		await onSeam(async (app) => {
			const { page } = app;
			await app.run(['track', 'coffee', '--verb', 'occur']);
			await setPlace(app, 'coffee', { y: 0.3, height: 0.2 });
			// Not today: today's band draws its sample dots instead (da-srrqd), the box round the latest.
			await app.run(['occur', 'coffee', '-d', '-2', '-t', '1000']);

			let { saved } = await openPlacement(page, 'coffee');
			const dots = page.locator(`${MODAL} .band g.field[data-field="coffee"] .point-fg`);
			assert.equal(await dots.count(), 8, 'the sample');
			assert.equal(await page.locator(`${MODAL} input[type="range"]`).count(), 0, 'no sliders');
			assert.equal(await overlays(page), 0, 'no box at rest');
			await tap(page, 'coffee');
			assert.equal(await page.locator(`${BOX}.dot`).count(), 1, 'the box is the dot, not the lane');
			const hints = await page.locator(`${BOX} .handle`).evaluateAll((all) =>
				all.map((h) => [h.dataset.handle, getComputedStyle(h, '::after').content === 'none' ? 0 : parseFloat(getComputedStyle(h, '::after').width)])
			);
			assert.deepEqual(hints.filter(([, w]) => w > 0).map(([n]) => n).sort(), ['-1,-1', '-1,1', '1,-1', '1,1'], 'the corners hinted, the edges not');
			for (const [n, w] of hints) assert.ok(w <= 8, `${n} drawn small: ${w}px`);
			assert.ok(parseFloat(await page.locator(BOX).evaluate((b) => getComputedStyle(b).borderTopWidth)) <= 2, 'a thin line');
			const near = (a, b, what) => assert.ok(Math.abs(a - b) < 1, `${what}: ${a} vs ${b}`);
			/** The box and the dot it is drawn round, each as its centre and size: read at once, as a redraw replaces the dots. */
			const read = () =>
				page.evaluate(() => {
					const at = (el) => {
						const r = el.getBoundingClientRect();
						return { x: r.x + r.width / 2, y: r.y + r.height / 2, w: r.width, h: r.height };
					};
					const box = document.querySelector('.placement .box');
					const dot = [...document.querySelectorAll('.placement .band g.field[data-field="coffee"] .point-fg')].at(-1);
					return { box: at(box), dot: at(dot) };
				});
			const fits = async (w, h, what) => {
				const { box: b, dot: d } = await read();
				near(b.h, h, `${what}: box height`);
				near(b.w, w, `${what}: box width`);
				near(d.h, h, `${what}: dot height`);
				near(d.w, w, `${what}: dot width`);
				near(d.x, b.x, `${what}: centred across`);
				near(d.y, b.y, `${what}: centred down`);
			};
			/** Drag the handle on sides `sx`,`sy` by `dx`,`dy`. */
			const size = async (sx, sy, dx, dy) => pull(page, await handle(page, `${sx},${sy}`), dx, dy);
			// The lane is 0.2 of the 160px day; the dot the standard 0.09 of it.
			const D = 0.09 * 160;
			await fits(D, D, 'as saved');

			await size(0, 1, 0, 8);
			await fits(D, D + 16, 'the bottom edge grows it about its centre, its width alone');

			await page.locator(`${MODAL} input[name="lock_aspect"]`).check();
			await size(1, 0, 8, 0);
			const [w, h] = [D + 16, ((D + 16) * (D + 16)) / D];
			await fits(w, h, 'locked, the right edge grows both');

			const before = (await read()).box.y;
			await drag(page, 'coffee', -16);
			near((await read()).box.y, before - 16, 'a drag of the dot moves it up');

			await done(page, saved);
			let shown = await spec(app, 'coffee');
			assert.deepEqual([shown.y, shown.height], [0.2, 0.2], 'moving writes the lane; sizing leaves its height');
			assert.ok(Math.abs(shown.glyph_scale - h / D) < 0.01, `scale ${shown.glyph_scale}`);
			assert.ok(Math.abs(shown.glyph_aspect - w / h) < 0.01, `aspect ${shown.glyph_aspect}`);

			// Opened again, the CLI draws it at the saved size: what the box showed.
			({ saved } = await openPlacement(page, 'coffee'));
			await tap(page, 'coffee');
			await fits(w, h, 'drawn by the CLI');
			await page.locator(`${MODAL} button.reset`).click();
			await fits(D, D, 'reset');
			await done(page, saved);
			shown = await spec(app, 'coffee');
			assert.equal('glyph_scale' in shown || 'glyph_aspect' in shown, false, JSON.stringify(shown));
		});
	});

	// da-srrqd, vividn, 2026-10-22: «They should actually be inside the placement configuration tool, so that
	// you can see the dots/states/bars and what they look like as real data as you're aligning it».
	test('a field with nothing today is drawn as samples in its lane, as the form holds it; one with entries today, as those', async () => {
		await onSeam(async (app) => {
			const { page } = app;
			await app.run(['track', 'mood', '--verb', 'switch', '--states', 'happy,sad']);
			await setPlace(app, 'mood', { y: 0.3, height: 0.2 });
			const blocks = page.locator(`${MODAL} .band g.field[data-field="mood"] rect.block`);
			const states = () => blocks.evaluateAll((all) => [...new Set(all.map((b) => b.dataset.state))].sort());
			const until = (what, test, arg = null) =>
				page.waitForFunction(test, arg, { timeout: 5000 }).catch(() => assert.fail(`never: ${what}`));

			let { saved } = await openPlacement(page, 'mood');
			assert.deepEqual(await states(), ['happy', 'sad'], 'its states, taking turns');
			await tap(page, 'mood');
			const lane = await page.locator(BOX).boundingBox();
			const drawn = async () => {
				const all = await blocks.evaluateAll((els) => els.map((b) => b.getBoundingClientRect()).map((r) => [r.top, r.bottom]));
				return [Math.min(...all.map((r) => r[0])), Math.max(...all.map((r) => r[1]))];
			};
			const [top, bottom] = await drawn();
			assert.ok(Math.abs(top - lane.y) < 1.5 && Math.abs(bottom - lane.y - lane.height) < 1.5, `${[top, bottom]} in ${lane.y}+${lane.height}`);

			// A state's colour is drawn as the form changes it, before any save.
			const happy = page.locator(`${MODAL} .state`).nth(0);
			assert.equal(await happy.locator('input.name').inputValue(), 'happy');
			await happy.locator('.swatch[aria-expanded]').click();
			const pick = happy.locator('.set .swatch').nth(3);
			const color = await pick.getAttribute('data-color');
			await pick.click();
			await until(
				`happy drawn in ${color}`,
				(color) => {
					const all = [...document.querySelectorAll('.placement .band rect.block[data-state="happy"]')];
					return all.length > 0 && all.every((b) => b.getAttribute('fill') === color);
				},
				color
			);
			// Moved, the samples are drawn again in the lane where it is let go.
			await page.locator(`${MODAL} .placement .band`).evaluate((el) => el.scrollIntoView({ block: 'center' }));
			await drag(page, 'mood', 32);
			await until('the samples drawn again where the lane was let go', () => {
				const svg = document.querySelector('.placement .band svg');
				const ys = [...svg.querySelectorAll('g.field[data-field="mood"] rect.block')].map((b) => Number(b.getAttribute('y')));
				const at = document.querySelector('.placement .box').getBoundingClientRect().top - svg.getBoundingClientRect().top;
				return ys.length > 0 && Math.abs(Math.min(...ys) - at) < 1.5 && Math.abs(at - 80) < 1.5;
			});
			await leaveField(page);
			assert.equal(await saved, null);
			assert.equal((await app.run(['tail', 'mood', '--json'])).stdout.includes('"happy"'), false, 'no sample was written');

			// With an entry in the last twelve hours, that is what «12 hours» draws: one stretch of happy, no sample.
			await app.run(['switch', 'mood', 'happy', '-t', '-30']);
			({ saved } = await openPlacement(page, 'mood'));
			await turn(page, '12 hours');
			assert.deepEqual([await blocks.count(), await states()], [1, ['happy']]);
			await leaveField(page);
			assert.equal(await saved, null);
		});
	});

	// da-jc8ic, vividn, 2026-10-26: «default should be to show no contextual data, just a sample of the field being
	// set … Clicking it changes [now], which then shows current now state of everything … another click -> [12 hours]
	// which shows all the data from the last 12 hours (no errors showed here though please). If any occurences of the
	// field actually happen in the last 12 hours, actually use these to do the placement».
	test('the eye cycles context, now and 12 hours: the sample alone, every state now, the last twelve hours', async () => {
		await onSeam(async (app) => {
			const { page } = app;
			await app.run(['switch', 'mood', 'happy', '-t', '-300']);
			await app.run(['start', 'work', '-t', '-120']);
			await app.run(['occur', 'coffee', '-t', '-60']);
			// A close with no open, which `check integrity` flags and today's dayview marks.
			await app.run(['end', 'lamp', '-t', '-30']);
			assert.match((await app.run(['dayview'])).stdout, /integrity-error/);
			await app.run(['track', 'pill', '--verb', 'occur']);
			await app.run(['track', 'light', '--verb', 'start']);
			await app.run(['track', 'weather', '--verb', 'switch', '--states', 'sun,rain']);

			/** What the band draws: each field's marks, their states, whether they name an entry, and its integrity marks. */
			const picture = (field) =>
				page.locator(`${MODAL} .placement .band svg`).evaluate((svg, field) => {
					const width = svg.getBoundingClientRect().width;
					const groups = [...svg.querySelectorAll('g.field')];
					return {
						fields: groups.map((g) => g.dataset.field).sort(),
						others: Object.fromEntries(
							groups
								.filter((g) => g.dataset.field !== field)
								.map((g) => [
									g.dataset.field,
									[...g.querySelectorAll('rect.block')].map((b) => Math.round(b.getBoundingClientRect().width - width))
								])
						),
						own: svg.querySelectorAll(`g.field[data-field="${field}"] :is(rect.block, .point-fg)`).length,
						named: svg.querySelectorAll(`g.field[data-field="${field}"] [data-id]`).length,
						real: svg.querySelector(`g.field[data-field="${field}"]`)?.classList.contains('real'),
						errors: svg.querySelectorAll('.integrity-error').length
					};
				}, field);

			for (const field of ['pill', 'light', 'weather']) {
				const { saved } = await openPlacement(page, field);
				const eye = page.locator(`${MODAL} .placement button.around`);
				assert.equal((await eye.innerText()).trim(), 'context', `${field} opens on context`);
				let seen = await picture(field);
				assert.deepEqual([seen.fields, seen.errors, seen.named], [[field], 0, 0], `${field}: the sample alone`);
				assert.ok(seen.own > 1, `${field}: a sample, ${seen.own} marks`);

				await turn(page, 'now');
				seen = await picture(field);
				// One state a field, each across the whole band; coffee's dot is no state.
				assert.deepEqual(seen.others, { mood: [0], work: [0] }, `${field}: every state now`);
				assert.ok(seen.own > 1 && seen.named === 0, `${field}: the sample over them`);

				await turn(page, '12 hours');
				seen = await picture(field);
				assert.deepEqual(seen.fields, ['coffee', field, 'mood', 'work'].sort(), `${field}: the last twelve hours`);
				assert.equal(seen.errors, 0, `${field}: no integrity marks`);
				assert.deepEqual([seen.own > 1, seen.named, seen.real], [true, 0, false], `${field}: none of its own, so the sample`);

				await turn(page, 'context');
				assert.deepEqual((await picture(field)).fields, [field], `${field}: back to the sample alone`);
				await leaveField(page);
				assert.equal(await saved, null);
			}

			// With an entry in the last twelve hours, that entry is what is placed.
			await app.run(['occur', 'pill', '-t', '-90']);
			const { saved } = await openPlacement(page, 'pill');
			assert.equal((await picture('pill')).named, 0, 'context: still the sample');
			await turn(page, '12 hours');
			const seen = await picture('pill');
			assert.deepEqual([seen.own, seen.named, seen.real], [1, 1, true], 'its own dot, outlined');
			// A drag on that real dot moves the lane, and the dot with it.
			const [, before] = await middle(page, 'pill');
			await drag(page, 'pill', 16);
			await page.waitForFunction(
				([sel, y]) => {
					const r = document.querySelector(sel)?.getBoundingClientRect();
					return r && Math.abs(r.y + r.height / 2 - y) < 1;
				},
				[marks('pill'), before + 16]
			);
			await done(page, saved);
			assert.notEqual((await spec(app, 'pill')).y, undefined, 'the lane it was dragged to');
		});
	});

	test('a block field has no size to set', async () => {
		await onSeam(async (app) => {
			await app.run(['track', 'work', '--verb', 'start']);
			const { saved } = await openPlacement(app.page, 'work');
			assert.equal(await app.page.locator(`${MODAL} .size`).count(), 0);
			await leaveField(app.page);
			assert.equal(await saved, null);
		});
	});
});
