// The spec modal's Placement tab (da-btyvj): drag a field's lane on
// today's real dayview band, optionally tied to a neighbour.
//
// Every assertion reads what was WRITTEN — `spec show`, `fields --json`,
// `dayview --show json` — and never the pixels a drag leaves behind: a
// driven drag sends its moves faster than frames are drawn, so a picture
// read after it can look exactly like «the feature is missing».

import test, { after, before, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp } from './support/app.mjs';
import { editedSpec, placeOf } from '../src/lib/track.ts';

const MODAL = 'dialog.spec';
const LANE = `${MODAL} .placement .lane`;
/** The band's day height in pixels (`H` in Placement.svelte): 32px is 0.2 of a day. */
const H = 160;
const round = (n) => Math.round(n * 1000) / 1000;

describe('the spec it saves', () => {
	const base = {
		field: 'coffee', verb: 'occur', color: null, unit: null, description: null,
		allowed_states: null, state_colors: {}, y: 0.3, height: 0.2
	};
	const edit = (place) => ({ verb: 'occur', color: '', unit: '', description: '', states: [], place });

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
});

describe('the Placement tab', () => {
	before(startApp);
	after(stopApp);
	const shared = sharedApp();

	const spec = async (app, field) => JSON.parse((await app.run(['spec', 'show', field])).stdout);
	const lane = async (app, field) =>
		JSON.parse((await app.run(['fields', '--json'])).stdout).find((f) => f.field === field);
	const setPlace = async (app, field, place) => {
		const r = await app.run(['spec', 'set', field, '--from-json', JSON.stringify({ ...(await spec(app, field)), ...place })]);
		assert.equal(r.code, 0, r.stderr);
	};

	/** Open `field`'s spec on Placement; resolves to `{ saved }`, the pending save — boxed, or awaiting this would await the save. */
	async function openPlacement(page, field) {
		const saved = page.evaluate((f) => window.datum.spec(f), field);
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
		await shared.solo(async (app) => {
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

	test('tied to a neighbour it writes the ratios, not y/height, and moves with its anchor', async () => {
		await shared.solo(async (app) => {
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
			await page.keyboard.press('Escape');
			assert.equal(await dismissed, null);
		});
	});
});
