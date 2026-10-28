// The chart widget (da-xv7ti): one key of some fields' entries over time.
//
// vividn, 2026-10-19: «widgets (in general) to be able to display and analyze
// quantitative and qualititative time series data, so that we can see
// average weight over time of various different things plotted together».
//
// ONE flow on one app: two fields with a `weight` and a `color`, chosen in
// the widget's own menu; the number key draws a series a field and its
// means a day, and the categorical key its values counted a day, stacked
// in a bar a field.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { HELD_AT, seedLines, startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP, along, draft, menuButton, pickOption, widgetPane } from './support/layout.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const PAGE = JSON.stringify(along('column', widgetPane('repl', 'cli'), widgetPane('chart', 'chart')));

// The head names the key it draws, how, and the fields: checked for those, not its sentence.
async function assertHead(chart, key, facts) {
	assert.equal(await chart.locator('.head b').textContent(), key);
	const head = await chart.locator('.head').textContent();
	for (const fact of facts) assert.ok(head.includes(fact), `${fact} in ${head}`);
}

test('two fields with a number key draw both series and their means; a categorical key draws its counts', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			const chart = page.locator('section[data-pane="chart"] .chart');
			await seedLines(app, [
				'occur apple weight=150 color=red -d -2 -t 10:00',
				'occur apple weight=160 color=green -d -2 -t 11:00',
				'occur apple weight=170 color=red -d -1 -t 10:00',
				'occur pear weight=200 color=green -d -2 -t 10:00',
				'occur pear weight=210 color=green -d -1 -t 10:00',
				'occur pear weight=220 color=green -d -1 -t 11:00'
			]);
			// No field chosen yet: the widget says so, and draws no chart.
			await chart.locator('p.quiet').waitFor();
			assert.equal(await chart.locator('.head').count(), 0);

			// The fields: a set, each a switch that leaves the submenu open.
			await menuButton(page, 'chart').click();
			await page.getByRole('menuitem', { name: 'fields', exact: true }).click();
			for (const field of ['apple', 'pear']) {
				await page.getByRole('menuitemcheckbox', { name: field, exact: true }).click();
				await page.getByRole('menuitemcheckbox', { name: field, exact: true, checked: true }).waitFor();
			}
			await page.keyboard.press('Escape');
			await page.keyboard.press('Escape');
			await page.waitForFunction(() => !document.querySelector('[role="menu"]'));

			const series = () =>
				chart.locator('g.series').evaluateAll((gs) =>
					gs.map((g) => ({
						series: g.dataset.series,
						points: g.querySelectorAll('circle.point').length,
						means: g.querySelectorAll('rect.mean').length,
						line: g.querySelector('polyline.mean')?.getAttribute('points').split(' ').length ?? 0
					}))
				);
			// No key chosen: the first number key, which is `weight`.
			const both = [
				{ series: 'apple', points: 3, means: 2, line: 2 },
				{ series: 'pear', points: 3, means: 2, line: 2 }
			];
			await until(series, (s) => JSON.stringify(s) === JSON.stringify(both));
			await assertHead(chart, 'weight', ['mean', 'day', 'apple', 'pear']);

			// The key is offered from what the entries carry.
			await pickOption(page, 'chart', 'key', 'weight');
			await until(series, (s) => JSON.stringify(s) === JSON.stringify(both));

			await pickOption(page, 'chart', 'key', 'color');
			await until(() => chart.getAttribute('data-kind'), (k) => k === 'categorical');
			// A bar a field, side by side within each day, its values stacked.
			const stacks = await chart.locator('g.stack').evaluateAll((gs) =>
				gs.map((g) => [
					g.dataset.field,
					Object.fromEntries([...g.querySelectorAll('rect')].map((r) => [r.dataset.name, Number(r.dataset.count)]))
				])
			);
			assert.deepEqual(stacks, [
				['apple', { green: 1, red: 1 }],
				['pear', { green: 1 }],
				['apple', { red: 1 }],
				['pear', { green: 2 }]
			]);
			assert.deepEqual(await chart.locator('.legend li').allTextContents(), ['green', 'red']);
			await assertHead(chart, 'color', ['count', 'day']);

			// A start/stop field's state is a boolean, which reads on / off (#949).
			await seedLines(app, ['start walk -d -2 -t 10:00', 'stop walk -d -2 -t 11:00', 'start walk -d -1 -t 09:00']);
			await menuButton(page, 'chart').click();
			await page.getByRole('menuitem', { name: 'fields', exact: true }).click();
			for (const field of ['apple', 'pear', 'walk']) await page.getByRole('menuitemcheckbox', { name: field, exact: true }).click();
			await page.getByRole('menuitemcheckbox', { name: 'walk', exact: true, checked: true }).waitFor();
			await page.keyboard.press('Escape');
			await page.keyboard.press('Escape');
			await page.waitForFunction(() => !document.querySelector('[role="menu"]'));
			await pickOption(page, 'chart', 'key', 'state');
			await until(() => chart.locator('.legend li').allTextContents(), (l) => JSON.stringify(l) === '["off","on"]');
			const walk = await chart.locator('g.stack[data-field="walk"] rect').evaluateAll((rs) => rs.map((r) => `${r.dataset.name} ${r.dataset.count}`));
			assert.deepEqual(walk, ['off 1', 'on 1', 'on 1']);
		},
		{ ...DESKTOP, prepare: (context) => context.addInitScript(draft, PAGE) }
	);
});

// More than one read's page (`ROWS`, 2000) in the window: the widget reads
// on with `tail --above` until it has reached back past the window's start,
// so no early bucket is drawn from part of its entries (vremya on #956).
test('a field with more entries in the window than one read holds is drawn whole', async () => {
	const COUNT = 2500;
	const at = (i) => new Date(HELD_AT - 3_600_000 - (COUNT - 1 - i) * 15 * 60_000).toISOString();
	const page = JSON.stringify(along('column', widgetPane('repl', 'cli'), widgetPane('chart', 'chart', { fields: ['heavy'] })));
	await withApp(
		async (app) => {
			// The header the CLI itself writes, so the schema version is its own.
			const backup = await app.run(['db', 'backup', '-']);
			assert.equal(backup.code, 0, backup.stderr);
			const header = JSON.parse(backup.stdout.split('\n')[0]);
			const lines = [JSON.stringify({ ...header, backup_time: at(COUNT - 1), source_db: 'seed', entry_count: COUNT })];
			for (let i = 0; i < COUNT; i++)
				lines.push(
					JSON.stringify({
						hid: 'heavy' + i.toString().padStart(7, '0'),
						slug: `heavy:${at(i)}`,
						content_hash: i.toString(16).padStart(64, '0'),
						field: 'heavy',
						verb: 'point',
						state: null,
						dur: 'MOMENT',
						following_state: null,
						data: { weight: i },
						occur_time: { utc: at(i), offset_minutes: 0, timezone: 'UTC' },
						create_time: at(i),
						modify_time: at(i),
						client_id: '01M281T64122KN04Z1VN626W9J',
						id_structure: '%occur_time%',
						raw_input: 'occur heavy',
						parser_version: 2
					})
				);
			const restored = await app.run(['db', 'restore', '-', '--merge'], lines.join('\n') + '\n');
			assert.equal(restored.code, 0, restored.stderr);
			await app.redraw();

			const days = new Set(Array.from({ length: COUNT }, (_, i) => at(i).slice(0, 10))).size;
			const drawn = () =>
				app.page
					.locator('section[data-pane="chart"] g.series[data-series="heavy"]')
					.evaluate((g) => [Number(g.dataset.points), Number(g.dataset.means)])
					.catch(() => null);
			await until(drawn, (d) => d?.[0] === COUNT);
			assert.deepEqual(await drawn(), [COUNT, days]);
		},
		{ ...DESKTOP, prepare: (context) => context.addInitScript(draft, page) }
	);
});
