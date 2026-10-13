// The field form's Placement tab (da-btyvj): drag a field's lane on
// today's real dayview band, optionally tied to a neighbour.
//
// Every assertion reads what was WRITTEN — `spec show`, `fields --json`,
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

	const spec = async (app, field) => JSON.parse((await app.run(['spec', 'show', field])).stdout);
	const lane = async (app, field) =>
		JSON.parse((await app.run(['fields', '--json'])).stdout).find((f) => f.field === field);
	const setPlace = async (app, field, place) => {
		const r = await app.run(['spec', 'set', field, '--from-json', JSON.stringify({ ...(await spec(app, field)), ...place })]);
		assert.equal(r.code, 0, r.stderr);
	};

	/** Open `field`'s spec on Placement; resolves to `{ saved }`, the pending save — boxed, or awaiting this would await the save. */
	async function openPlacement(page, field) {
		const { saved } = await openField(page, field);
		await page.locator(`${MODAL} [data-tab="placement"]`).click();
		await page.locator(`${MODAL} .placement .band svg`).waitFor();
		return { saved };
	}

	/** Press at `y` inside the lane (a fraction of its box, or a pixel offset past it) and drag by `dy`. */
	async function drag(page, at, dy) {
		const box = await page.locator(LANE).boundingBox();
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
			await app.run(['track', 'coffee', '--verb', 'occur']);
			await app.run(['occur', 'coffee', '-t', '-30']);
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
			await app.run(['track', 'coffee', '--verb', 'occur']);
			await setPlace(app, 'coffee', { y: 0.5 });
			await app.run(['occur', 'coffee', '-t', '-30']);
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
			await app.run(['track', 'coffee', '--verb', 'occur']);
			await setPlace(app, 'coffee', { y: 0.6, height: 0.1 });
			await app.run(['occur', 'coffee', '-t', '-30']);

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

	// da-c05v1: an occur field sizes its dot here, on the band, as it moves.
	test("an occur field's size slider scales its dot on the band and in the spec; aspect widens it alone", async () => {
		await onSeam(async (app) => {
			const { page } = app;
			await app.run(['track', 'coffee', '--verb', 'occur']);
			// Not today: the band draws the day of its latest dot instead.
			await app.run(['occur', 'coffee', '-d', '-2', '-t', '1000']);

			let { saved } = await openPlacement(page, 'coffee');
			assert.match(await page.locator(`${MODAL} .size`).innerText(), /no dot today/);
			const dot = page.locator(`${MODAL} .band g.field[data-field="coffee"] .point-fg`);
			const was = await dot.boundingBox();
			assert.ok(was && was.width > 0, 'the dot is drawn');

			/** Drag `name`'s slider to `at` of its width; resolves to the factor it now says. */
			async function slide(name, at) {
				const input = page.locator(`${MODAL} input[name="${name}"]`);
				const box = await input.boundingBox();
				const y = box.y + box.height / 2;
				await page.mouse.move(box.x + box.width * 0.4, y);
				await page.mouse.down();
				await page.mouse.move(box.x + box.width * ((0.4 + at) / 2), y);
				await page.mouse.move(box.x + box.width * at, y);
				await page.mouse.up();
				return Math.round(2 ** Number(await input.inputValue()) * 100) / 100;
			}
			const near = (a, b, what) => assert.ok(Math.abs(a / b - 1) < 0.03, `${what}: ${a} vs ${b}`);

			const scale = await slide('glyph_scale', 0.8);
			assert.ok(scale > 2, `dragged up to ×${scale}`);
			const big = await dot.boundingBox();
			near(big.width, was.width * scale, 'width scales');
			near(big.height, was.height * scale, 'height scales');

			const aspect = await slide('glyph_aspect', 0.15);
			assert.ok(aspect < 0.7, `dragged down to ${aspect}`);
			const narrow = await dot.boundingBox();
			near(narrow.width, big.width * aspect, 'aspect narrows it');
			near(narrow.height, big.height, 'and leaves its height');

			await done(page, saved);
			const shown = await spec(app, 'coffee');
			assert.deepEqual([shown.glyph_scale, shown.glyph_aspect], [scale, aspect]);

			// Opened again, the CLI draws it at the saved size: what the preview showed.
			({ saved } = await openPlacement(page, 'coffee'));
			const drawn = await dot.boundingBox();
			near(drawn.width, narrow.width, 'drawn width');
			near(drawn.height, narrow.height, 'drawn height');
			await cancelField(page);
			assert.equal(await saved, null);
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
