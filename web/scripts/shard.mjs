// Which test files THIS run takes — the arithmetic, alone, so a test can
// import it without spawning the suite (the shape `concurrency.mjs` has).
//
// The suite outgrew its runner before it outgrew any one file (da-buqa): 60
// files, ~26 minutes, against a 4-core runner's 30-minute cap, so main went
// red-or-cancelled for a day and every push deployed blind. Splitting big
// files does not help — each file is its own Chromium and its own
// `before(startApp)`, so the sum only ever rises. Splitting the RUN does.
//
// A shard is `N/M`: the files sorted, taken by `index % M === N - 1`. By file
// and never by test, for the same reason. Interleaving rather than slicing
// because neighbouring names run together (layout-*, fields-*) and a slice
// would hand one shard all of them.

/** Parse `N/M`. Returns undefined for undefined/empty; throws on nonsense. */
export function parseShard(spec) {
	if (spec === undefined || spec === '') return undefined;
	const m = /^(\d+)\/(\d+)$/.exec(String(spec).trim());
	const n = m && Number(m[1]);
	const total = m && Number(m[2]);
	if (!m || n < 1 || total < 1 || n > total) {
		throw new Error(`--shard wants N/M with 1 <= N <= M, not ${JSON.stringify(spec)}`);
	}
	return { n, total };
}

/** The files of shard N/M, in the order given. `undefined` shard = all of them. */
export function shardFiles(files, shard) {
	if (!shard) return files;
	return files.filter((_, i) => i % shard.total === shard.n - 1);
}
