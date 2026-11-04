// Which datum a deployed site runs: its CLI module's own `datum --version`,
// asked through the served page's `window.datum` (#lib/console.ts), so what
// is read is the bytes the site serves and not the job's receipt (da-uzyih.2).
// Exits 1 unless the version names `expected-sha`.
//
// Usage: node scripts/served-version.mjs <url> <expected-sha>

import { chromium } from 'playwright';

const [url, want] = process.argv.slice(2);
if (!url || !want) throw new Error('usage: served-version.mjs <url> <expected-sha>');
const browser = await chromium.launch();
// No service worker: its first visit reloads the page for isolation, and a
// cached shell would be a previous deploy's.
const page = await (await browser.newContext({ serviceWorkers: 'block' })).newPage();
await page.goto(url, { waitUntil: 'load' });
await page.waitForFunction(() => 'datum' in window, null, { timeout: 120_000 });
const { stdout } = await page.evaluate(() => window.datum.runQuiet(['--version']));
await browser.close();
const sha = stdout.match(/\(([0-9a-f]{7,})\)/)?.[1];
console.log(`served: ${stdout.trim() || 'no version line'}`);
const ok = !!sha && want.startsWith(sha);
console.log(ok ? `✓ ${want.slice(0, 7)} is served` : `✗ ${want.slice(0, 7)} is not served`);
process.exit(ok ? 0 : 1);
