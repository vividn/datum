// datum.day says the glossary's words (da-kjkpx; the glossary is AGENTS.md's
// and docs/concepts.md's, da-0u7p4). vividn, 2026-11-04: «we do all the user
// facing vocabulary and code first». So a fresh visit's every tab, the
// commands, the setup form and every lesson of the tutorial are read as a
// person reads them — the text drawn, and every title, label and placeholder
// — and none says an old word for a data concept, nor shows an old command
// as code. Where an alias is taught the page would say so; none is today.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { ready, startApp, stopApp, withApp } from './support/app.mjs';

before(startApp);
after(stopApp);

/** The glossary's old words for a data concept (AGENTS.md, «Vocabulary»): read anywhere. */
const OLD = /\b(verbs?|shapes?|occurs?|occurrences?|switch(?:es|ed|ing)?|blocks?|cards?|spans?)\b/i;
/** The old commands, which are English words too: wrong only as a command, at the start of code or of a title. */
const OLD_COMMAND = /^(occur|start|end|stop|switch|add|card|note)\b/;

/**
 * What a person reads on the page now, each with where it was. Not what the
 * CLI printed (`.raw`, RawLines.svelte): its words are the CLI's own sweep's
 * (da-gpz6m), and a layout's name it lists is stored data, renamed in phase 2.
 */
const read = (page) =>
	page.evaluate(() => {
		const said = [];
		const printed = [...document.querySelectorAll('.raw')];
		for (const el of printed) el.style.setProperty('display', 'none');
		const text = document.body.innerText;
		for (const el of printed) el.style.removeProperty('display');
		for (const line of text.split('\n')) if (line.trim()) said.push({ where: 'text', text: line.trim() });
		for (const el of document.querySelectorAll('[title], [aria-label], [placeholder]'))
			for (const name of ['title', 'aria-label', 'placeholder']) {
				const text = el.getAttribute(name);
				if (text) said.push({ where: name, text });
			}
		for (const el of document.querySelectorAll('code')) said.push({ where: 'code', text: el.textContent.trim() });
		return said;
	});

/** Every old word on the page, as `where: text`. */
async function oldWords(page, tab) {
	return (await read(page))
		.filter(({ where, text }) => OLD.test(text) || ((where === 'code' || where === 'title') && OLD_COMMAND.test(text)))
		.map(({ where, text }) => `${tab} ${where}: ${text}`);
}

const tabs = (page) => page.locator('.root-bar [role="tab"]');

/** Each root tab in front in turn, its old words. */
async function everyTab(page) {
	const found = [];
	const count = await tabs(page).count();
	for (let i = 0; i < count; i++) {
		const tab = tabs(page).nth(i);
		const name = await tab.innerText();
		await tab.click();
		await ready(page);
		found.push(...(await oldWords(page, name)));
	}
	return found;
}

test('a fresh datum.day, its commands and its tutorial say no old word', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			const found = await everyTab(page);

			// The commands, and their write submenu, where the log commands are.
			const bar = page.locator('.prompt-bar');
			await bar.locator('button.prompt').click();
			await bar.locator('button[data-menu="write"]').click();
			const written = await bar.locator('#bar-commands button[data-command]').evaluateAll((buttons) => buttons.map((b) => b.dataset.command));
			assert.deepEqual(written.slice(0, 7), ['moment', 'on', 'off', 'change', 'record', 'allday', 'remark'], 'the write submenu leads with the log commands');
			found.push(...(await oldWords(page, 'commands')));
			await bar.locator('button.prompt').click();

			await tabs(page).filter({ hasText: 'welcome' }).click();
			await Promise.all([page.waitForURL((url) => url.searchParams.get('db') === 'tutorial'), page.getByRole('button', { name: 'Tutorial', exact: true }).click()]);
			await ready(page);
			assert.deepEqual(
				await tabs(page).allInnerTexts(),
				['intro', 'Moments in a Café', 'Turn the TV off', 'Change of scenery', 'input-bar', 'tabs'],
				'the lessons, each game named by its scene (da-83y6w)'
			);
			found.push(...(await everyTab(page)));

			assert.deepEqual(found, []);
		},
		{ home: null }
	);
});

// The sweep can see what it is for: an old word put on the page is found.
test('the sweep finds an old word drawn, and an old command as code', async () => {
	await withApp(async ({ page }) => {
		await page.evaluate(() => {
			const p = document.body.appendChild(document.createElement('p'));
			p.innerHTML = 'a switch field <code>start sleep</code>';
		});
		const found = await oldWords(page, 'planted');
		assert.deepEqual(
			found.filter((f) => f.includes('start sleep')),
			['planted text: a switch field start sleep', 'planted code: start sleep']
		);
	});
});
