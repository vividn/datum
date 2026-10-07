// The worker's queue runs what is typed before the panes waiting, and the
// cheapest pane first (da-icmo1). vividn: «This whole app must feel very
// snappy». First in, first out had nowview's half second answer 12.9 s
// after a write, behind `stats`, and a typed `occur` 13 s after its Enter.
//
// The day strip's `dayview` is made slow inside the worker, where the queue
// measures it: the process busy-waits as it reads its argv, so the time is
// the run's own.
// Every request's arrival and answer is told to the page in order.
//
// And the requests a test is about wait together ON PURPOSE (vremya on
// #479): a hold keeps the one process running from an arrival `from` until
// `count` arrivals matching `until`, so the order they then run in is the
// queue's choice and not how fast a loaded box sent them. Held in
// `instantiate`, asynchronously, so the worker still hears what arrives.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { inWorker, startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP } from './support/layout.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const SLOW = { dayview: 800 };

const QUEUE = `
	const slow = ${JSON.stringify(SLOW)};
	const channel = new BroadcastChannel('datum-test-queue');
	let hold = null;
	let gate = null;
	new BroadcastChannel('datum-test-hold').onmessage = (e) => {
		hold = { ...e.data, armed: false, seen: 0 };
		channel.postMessage({ holding: true });
	};
	self.addEventListener('message', (e) => {
		if (!e.isTrusted || e.data?.op !== 'cli') return;
		const argv = e.data.argv.join(' ');
		if (hold && !hold.armed && new RegExp(hold.from).test(argv)) {
			hold.armed = true;
			gate = new Promise((resolve) => (hold.release = resolve));
		} else if (hold?.armed && new RegExp(hold.until).test(argv) && ++hold.seen >= hold.count) {
			// Released in the arrival's own dispatch, before the app's listener
			// has queued it: the worst moment, as a request arriving while jobs
			// run back to back. The queue must cross a task before it picks.
			hold.release();
			hold = null;
		}
		channel.postMessage({ arrived: e.data.id, argv, pane: e.data.pane });
	});
	const post = self.postMessage.bind(self);
	self.postMessage = (m) => {
		if (m && 'id' in m && !('notice' in m)) channel.postMessage({ answered: m.id, ran: !!m.cli });
		post(m);
	};
	const instantiate = WebAssembly.instantiate;
	WebAssembly.instantiate = async function (module, imports) {
		const wasi = imports?.wasi_snapshot_preview1;
		if (!wasi) return instantiate.apply(this, arguments);
		const held = gate;
		gate = null;
		// Bounded, so a hold that is never released fails the test rather than hangs it.
		if (held) await Promise.race([held, new Promise((resolve) => setTimeout(resolve, 10000))]);
		let instance;
		let size = 0;
		const sizes = wasi.args_sizes_get;
		const get = wasi.args_get;
		wasi.args_sizes_get = (count, at) => {
			const code = sizes(count, at);
			size = new DataView(instance.exports.memory.buffer).getUint32(at, true);
			return code;
		};
		wasi.args_get = (pointers, buffer) => {
			const code = get(pointers, buffer);
			const argv = new TextDecoder().decode(new Uint8Array(instance.exports.memory.buffer, buffer, size)).split('\\0');
			channel.postMessage({ started: argv.slice(1).join(' ') });
			const ms = slow[argv[1]];
			if (ms) for (const end = performance.now() + ms; performance.now() < end; );
			return code;
		};
		instance = await instantiate.call(this, module, imports);
		return instance;
	};
`;

/** Start hearing the worker; `heard()` is everything since, in order. */
async function listen(page) {
	await page.evaluate(() => {
		window.__queue = [];
		new BroadcastChannel('datum-test-queue').onmessage = (e) => window.__queue.push(e.data);
	});
	return {
		heard: () => page.evaluate(() => window.__queue),
		/** Hold the next process from an arrival matching `from` until `count` matching `until` have arrived. */
		hold: async (from, until, count) => {
			await page.evaluate((h) => new BroadcastChannel('datum-test-hold').postMessage(h), { from, until, count });
			await waitFor(page, (log) => log.some((e) => e.holding));
		}
	};
}

/** Each request heard, with where in the log it arrived and was answered. */
function requests(log) {
	const byId = new Map();
	log.forEach((event, at) => {
		if ('arrived' in event) byId.set(event.arrived, { ...event, id: event.arrived, in: at });
		else if (byId.has(event.answered)) Object.assign(byId.get(event.answered), { out: at, ran: event.ran });
	});
	return [...byId.values()];
}

const show = (reqs) => reqs.map((r) => `${r.in}→${r.out ?? '?'} ${r.argv}`).join('\n');
const dump = (log) => log.map((e, i) => `${i} ${JSON.stringify(e)}`).join('\n');

/** The default layout's panes: the day strip's bands, nowview and the tail. */
const PANE = /^(dayview|nowview|tail) /;

const waitFor = (page, ok) => until(() => page.evaluate(() => window.__queue), ok);

/** Every request heard once all of them are answered. */
const settled = async (ear) => requests(await until(ear.heard, (log) => requests(log).every((r) => r.out !== undefined)));

test('after a write, nowview answers before a slow dayview, though it was queued after it', async () => {
	await withApp(
		async (app) => {
			// The panes ran once as the page loaded: the queue has measured them.
			await app.idle();
			const ear = await listen(app.page);
			// The bar's first read runs while every pane's request arrives.
			await ear.hold('^fields --json', PANE.source, 4);

			await app.submit('switch mood good');
			await app.idle();
			const reqs = await settled(ear);
			const dayview = reqs.find((r) => /^dayview /.test(r.argv) && r.ran);
			const nowview = reqs.find((r) => /^nowview /.test(r.argv) && r.ran);
			assert.ok(dayview && nowview, `both panes refreshed:\n${show(reqs)}`);
			assert.ok(dayview.in < nowview.in, `dayview was queued first:\n${show(reqs)}`);
			assert.ok(nowview.in < dayview.out, `they waited together:\n${show(reqs)}`);
			assert.ok(nowview.out < dayview.out, `nowview answered first:\n${show(reqs)}`);
		},
		{ ...DESKTOP, prepare: inWorker(QUEUE) }
	);
});

test('a line typed while panes wait runs before them', async () => {
	await withApp(
		async (app) => {
			await app.idle();
			const ear = await listen(app.page);

			// The write sets every pane off, and the bar's first read runs until
			// the line, typed once they have all arrived, has arrived too.
			await ear.hold('^fields --json', '^occur tea', 1);
			await app.page.evaluate(() => void window.datum.run(['switch', 'mood', 'good']));
			await until(ear.heard, (log) => log.filter((e) => PANE.test(e.argv)).length >= 4);
			await app.submit('occur tea');
			await app.idle();
			const reqs = await settled(ear);
			const typed = reqs.find((r) => /^occur tea/.test(r.argv));
			assert.ok(typed, `the typed line ran:\n${show(reqs)}`);
			// Every pane was waiting when the line arrived: the held process was the bar's.
			const pending = reqs.filter((r) => PANE.test(r.argv) && r.ran && r.in < typed.in);
			assert.ok(pending.length >= 4 && pending.every((r) => r.out > typed.in), `the panes were waiting:\n${show(reqs)}`);
			const overtaken = pending.filter((r) => r.out > typed.out);
			assert.equal(overtaken.length, pending.length, `the line went ahead of every waiting pane:\n${dump(await ear.heard())}`);
		},
		{ ...DESKTOP, prepare: inWorker(QUEUE) }
	);
});
