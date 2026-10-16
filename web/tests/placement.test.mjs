// The field form's Placement tab (da-btyvj): drag a field's lane on
// today's real dayview band, optionally tied to a neighbour.
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
import { FIELD_FORM as MODAL, cancelField, openField } from './support/field.mjs';

const LANE = `${MODAL} .placement .lane`;
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

	/** Open `field`'s spec on Placement; resolves to `{ saved }`, the pending save — boxed, or awaiting this would await the save. */
	async function openPlacement(page, field) {
		const { saved } = await openField(page, field);
		await page.locator(`${MODAL} [data-tab="placement"]`).click();
		await page.locator(`${MODAL} .placement .band svg`).waitFor();
		return { saved };
	}

	/** Press the middle of the lane, its bottom edge, or the middle of an occur dot's box (`dot`), and drag by `dy`. */
	async function drag(page, at, dy) {
		const box = await page.locator(at === 'dot' ? `${MODAL} .placement .dot` : LANE).boundingBox();
		const x = box.x + box.width / 2;
		const y = box.y + (at === 'bottom' ? box.height : box.height / 2);
		await page.mouse.move(x, y);
		await page.mouse.down();
		await page.mouse.move(x, y + dy / 2);
		await page.mouse.move(x, y + dy);
		await page.mouse.up();
	}

	async function done(page, saved) {
		await page.locator(`${MODAL} button.commit`).click();
		const result = await saved;
		assert.equal(result.code, 0, result.stderr);
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
			const box = await page.locator(LANE).boundingBox();
			assert.ok(Math.abs(box.y - band.y - 1 - hashed.y * H) < 1.5, `${box.y - band.y} vs ${hashed.y * H}`);

			const dir = hashed.y < 0.5 ? 1 : -1;
			await drag(page, 'middle', dir * 32);
			assert.doesNotMatch(await page.locator(`${MODAL} .readout`).innerText(), /nobody chose/);
			await done(page, saved);
			let shown = await spec(app, 'coffee');
			assert.equal(shown.y, round(hashed.y + dir * 0.2));
			assert.equal(shown.height, 0.12);

			({ saved } = await openPlacement(page, 'coffee'));
			await drag(page, 'bottom', 16);
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
			const box = await page.locator(LANE).boundingBox();
			assert.ok(Math.abs(box.y - band.y - 1 - resolved.y * H) < 1.5, `${box.y - band.y} vs ${resolved.y * H}`);
			assert.match(await page.locator(`${MODAL} .readout`).innerText(), /y 0\.44/);
			await cancelField(page);
			assert.equal(await saved, null);
		});
	});

	test('tied to a neighbour it writes the ratios, not y/height, and moves with its anchor', async () => {
		await onSeam(async (app) => {
			const { page } = app;
			await app.run(['track', 'mood', '--verb', 'switch']);
			await setPlace(app, 'mood', { y: 0.2, height: 0.25 });
			await app.run(['track', 'coffee', '--verb', 'start']);
			await setPlace(app, 'coffee', { y: 0.6, height: 0.1 });
			await app.run(['start', 'coffee', '-t', '-30']);

			const { saved } = await openPlacement(page, 'coffee');
			await page.locator(`${MODAL} .placement select`).selectOption('mood');
			// Tied where it stood: 0.6 is 1.6 mood-heights under mood's top.
			assert.match(await page.locator(`${MODAL} .readout`).innerText(), /below mood · ×0\.4/);
			await drag(page, 'middle', -24);
			await done(page, saved);

			const shown = await spec(app, 'coffee');
			assert.equal(shown.y, undefined);
			assert.equal(shown.height, undefined);
			// Up 24px is up 0.15 of the day, to one mood-height under mood's top.
			assert.deepEqual([shown.relative_to, shown.anchor_pos_ratio, shown.anchor_size_ratio], ['mood', 1, 0.4]);

			const drawnAt = async () => {
				const layout = JSON.parse((await app.run(['dayview', '--day-height', '100', '--show', 'json'])).stdout);
				return layout.days.at(-1).fields.find((f) => f.field === 'coffee').cross_offset;
			};
			assert.ok(Math.abs((await drawnAt()) - 45) < 1e-6, 'mood 0.2 + 0.25 × 1');
			await setPlace(app, 'mood', { y: 0.5 });
			assert.ok(Math.abs((await drawnAt()) - 75) < 1e-6, 'the anchor moved, and so did coffee');
			assert.equal((await lane(app, 'coffee')).y, 0.75);

			// Tied to a field with no spec, the resolver draws it free: the tab
			// says both — what the spec names, and where it is actually drawn.
			await setPlace(app, 'coffee', { relative_to: 'nobody' });
			const { saved: dismissed } = await openPlacement(page, 'coffee');
			assert.equal(await page.locator(`${MODAL} .placement select`).inputValue(), 'nobody');
			assert.match(await page.locator(`${MODAL} .readout`).innerText(), /tied to nobody, which has no spec/);
			await cancelField(page);
			assert.equal(await dismissed, null);
		});
	});

	// da-c1b6m: an occur field's box is drawn round its dot. Its body moves
	// the lane, a handle sizes the dot about its centre, and the CLI draws
	// the dot exactly as tall as the box — here, and in every view.
	test("an occur field's box moves and sizes its dot; lock aspect keeps its shape and reset makes it round again", async () => {
		await onSeam(async (app) => {
			const { page } = app;
			await app.run(['track', 'coffee', '--verb', 'occur']);
			await setPlace(app, 'coffee', { y: 0.3, height: 0.2 });
			// Not today: the band draws the day of its latest dot instead.
			await app.run(['occur', 'coffee', '-d', '-2', '-t', '1000']);

			let { saved } = await openPlacement(page, 'coffee');
			assert.match(await page.locator(`${MODAL} .size`).innerText(), /no dot today/);
			assert.equal(await page.locator(`${MODAL} input[type="range"]`).count(), 0, 'no sliders');
			assert.equal(await page.locator(LANE).count(), 0, 'the box is the dot, not the lane');
			const box = page.locator(`${MODAL} .placement .dot`);
			const dot = page.locator(`${MODAL} .band g.field[data-field="coffee"] .point-fg`);
			const near = (a, b, what) => assert.ok(Math.abs(a - b) < 1, `${what}: ${a} vs ${b}`);
			/** The box and the dot it is drawn round, each as its centre and size. */
			const read = async () => {
				const [b, d] = [await box.boundingBox(), await dot.boundingBox()];
				const at = (r) => ({ x: r.x + r.width / 2, y: r.y + r.height / 2, w: r.width, h: r.height });
				return { box: at(b), dot: at(d) };
			};
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
			async function pull(sx, sy, dx, dy) {
				const r = await page.locator(`${MODAL} .dot [data-handle="${sx},${sy}"]`).boundingBox();
				const [x, y] = [r.x + r.width / 2, r.y + r.height / 2];
				await page.mouse.move(x, y);
				await page.mouse.down();
				await page.mouse.move(x + dx / 2, y + dy / 2);
				await page.mouse.move(x + dx, y + dy);
				await page.mouse.up();
			}
			// The lane is 0.2 of the 160px day: a round dot 32px tall.
			await fits(32, 32, 'as saved');

			await pull(0, 1, 0, 8);
			await fits(32, 48, 'the bottom edge grows it about its centre, its width alone');

			await page.locator(`${MODAL} input[name="lock_aspect"]`).check();
			await pull(1, 0, 8, 0);
			await fits(48, 72, 'locked, the right edge grows both');

			const before = (await read()).box.y;
			await drag(page, 'dot', -16);
			near((await read()).box.y, before - 16, 'the body moves it up');

			await done(page, saved);
			let shown = await spec(app, 'coffee');
			assert.deepEqual([shown.y, shown.height], [0.2, 0.2], 'moving writes the lane; sizing leaves its height');
			assert.ok(Math.abs(shown.glyph_scale - 2.25) < 0.01, `scale ${shown.glyph_scale}`);
			assert.ok(Math.abs(shown.glyph_aspect - 2 / 3) < 0.01, `aspect ${shown.glyph_aspect}`);

			// Opened again, the CLI draws it at the saved size: what the box showed.
			({ saved } = await openPlacement(page, 'coffee'));
			await fits(48, 72, 'drawn by the CLI');
			await page.locator(`${MODAL} button.reset`).click();
			await fits(32, 32, 'reset');
			await done(page, saved);
			shown = await spec(app, 'coffee');
			assert.equal('glyph_scale' in shown || 'glyph_aspect' in shown, false, JSON.stringify(shown));
		});
	});

	test('a block field has no size to set', async () => {
		await onSeam(async (app) => {
			await app.run(['track', 'work', '--verb', 'start']);
			const { saved } = await openPlacement(app.page, 'work');
			assert.equal(await app.page.locator(`${MODAL} .size`).count(), 0);
			await cancelField(app.page);
			assert.equal(await saved, null);
		});
	});
});
