// A guide's demos (da-2fxiz, #lib/demo.ts). vividn, 2026-11-05: «Ideally
// there are even graphics and animations demonstrating various features. We
// should make sure that there is no drift in how these look compared to the
// app, so put some guards in place».
//
// The guards: every demo mounts its kind's own component from `KINDS`, not a
// copy — the scoped class Svelte gives a component's elements names the file
// it was compiled from, so the demo's widget is the one the app draws for the
// kind when that class is the app's — and every demo's script plays to its
// end on its sample database, each step finding what it is done at and every
// command it set off answering 0, with no error thrown. And a guide's page in
// the docs draws its demo as a frame of the demo's page, which plays.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP } from './support/layout.mjs';
import { resolveLibAsSource } from './support/libhook.mjs';

before(startApp);
after(stopApp);

let KINDS, DEMO;
before(async () => {
	resolveLibAsSource();
	({ KINDS } = await import('../src/lib/layout/registry.ts'));
	({ DEMO } = await import('../src/lib/guides.ts'));
});

/** Every demo, as [kind, name]. */
const demos = () => Object.entries(KINDS).flatMap(([kind, k]) => Object.keys(k.demos ?? {}).map((name) => [kind, name]));

/** The scoped Svelte classes of the elements a widget host holds, its own frame's left out. */
const scoped = (page, host) =>
	page.evaluate((host) => {
		const at = document.querySelector(host);
		const own = new Set([...(at?.querySelectorAll(':scope > section > .titlebar, :scope > section > .titlebar *') ?? [])]);
		return [...new Set([...(at?.querySelectorAll('*') ?? [])].filter((e) => !own.has(e)).flatMap((e) => [...e.classList].filter((c) => /^svelte-/.test(c))))];
	}, host);

/** `name` of `kind`'s demos on a page of its own beside `app`'s, played once or held still: how far it got, and what it threw. */
async function opened(app, kind, name, play) {
	const page = await app.page.context().newPage();
	const errors = [];
	page.on('pageerror', (e) => errors.push(String(e)));
	const origin = new URL(app.page.url()).origin;
	await page.goto(`${origin}/demo?${new URLSearchParams({ db: 'demo', kind, demo: name, ...(play && { play: '1' }) })}`);
	const demo = page.locator(`.demo:is([data-state="${play ? 'played' : 'still'}"], [data-state="failed"])`);
	await demo.waitFor({ timeout: 60_000 });
	return { page, errors, state: await demo.getAttribute('data-state'), failed: await demo.getAttribute('data-failed') };
}

describe('a guide’s demos', () => {
	test('every kind’s guide places each of its demos, and names none it lacks', () => {
		assert.ok(demos().length > 0, 'some kind has a demo');
		for (const [kind, k] of Object.entries(KINDS)) {
			const placed = [...k.guide.matchAll(DEMO)].map((m) => m[2]);
			assert.deepEqual([...placed].sort(), Object.keys(k.demos ?? {}).sort(), `${kind}'s guide places its demos, each once`);
		}
	});

	test('every demo mounts its kind’s own component, held still, and plays to its end on its sample database', async () => {
		// The wall's clock: a demo's taps log one after another, as a person's do.
		await withApp(
			async (app) => {
				for (const [kind, name] of demos()) {
					const what = `${kind}'s ${name}`;
					const still = await opened(app, kind, name, false);
					assert.deepEqual([still.state, still.failed], ['still', null], `${what} holds still where motion is reduced`);
					assert.equal(await still.page.locator('.finger').count(), 0, `${what} plays nothing`);
					const demo = await scoped(still.page, `.demo .host[data-widget="${kind}"]`);
					await still.page.close();
					// The app's own drawing of the kind, a tab of it alone, on what the demo starts from.
					for (const line of KINDS[kind].demos[name].seed) assert.equal((await app.page.evaluate((l) => window.datum.runLine(l), line)).code, 0, line);
					await app.page.evaluate((kind) => window.datum.shell.open(kind), kind);
					const host = `.host[data-widget="${kind}"]:not(.tab-body.behind .host)`;
					await app.page.locator(host).first().waitFor();
					await app.idle();
					const drawn = await scoped(app.page, host);
					assert.ok(demo.length > 0, `${what} drew its widget`);
					for (const c of demo) assert.ok(drawn.includes(c), `${what} draws ${c}, as the app's ${kind} does (${drawn})`);

					const played = await opened(app, kind, name, true);
					assert.deepEqual([played.state, played.failed], ['played', null], what);
					assert.deepEqual([...still.errors, ...played.errors], [], `${what} threw nothing`);
					await played.page.close();
				}
			},
			{ ...DESKTOP, clock: null, reducedMotion: 'reduce' }
		);
	});

	test('a guide’s page in the docs draws its demo, which plays, and the reader’s data stays as it was', async () => {
		const [kind, name] = demos()[0];
		await withApp(
			async (app) => {
				assert.equal((await app.runLine('moment tea -t -5')).code, 0);
				const before = await app.allEntries(200);
				await app.page.evaluate(() => window.datum.shell.open('docs'));
				const docs = app.page.locator('article.doc[data-doc="README.md"]');
				await docs.locator(`a[data-doc="widgets/${kind}.md"]`).first().click();
				const frame = app.page.locator(`article.doc[data-doc="widgets/${kind}.md"] iframe.demo`);
				await frame.waitFor();
				assert.match(await frame.getAttribute('src'), new RegExp(`/demo\\?db=demo&kind=${kind}&demo=${name}$`));
				const inside = app.page.frameLocator(`article.doc[data-doc="widgets/${kind}.md"] iframe.demo`);
				const played = inside.locator('.demo:is([data-state="played"], [data-state="failed"])');
				await played.waitFor({ timeout: 60_000 });
				assert.equal(await played.getAttribute('data-state'), 'played', `${await played.getAttribute('data-failed')}`);
				// What it logged reached neither the reader's database nor any file, nor the tab's record of its database.
				await app.idle();
				assert.deepEqual(await app.allEntries(200), before, "the reader's entries are unchanged");
				const files = await app.page.evaluate(async () => {
					const names = [];
					for await (const name of (await (await navigator.storage.getDirectory()).getDirectoryHandle('datum-cli')).keys()) names.push(name);
					return names;
				});
				assert.ok(!files.some((f) => f.startsWith('demo.')), `no file of the demo's database (${files})`);
				assert.notEqual(await app.page.evaluate(() => JSON.parse(sessionStorage.getItem('datum.db.tab') ?? 'null')?.db), 'demo');
			},
			{ ...DESKTOP, clock: null }
		);
	});
});
