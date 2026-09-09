// datum.day is client-side only, and this line is what makes that true
// everywhere rather than only in production.
//
// Production already serves an empty shell (adapter-static's fallback) and
// lets the browser draw everything: the data lives in the reader's own
// storage, so there is nothing a server could render. But with SvelteKit's
// default of `ssr = true`, the dev server and the preview server the tests
// run against still rendered every page in Node on each request — a
// document production never serves, produced by running page code in an
// environment it was never written for. Every `browser` guard in $lib
// exists for that render, and the one that was wrong hung CI for an
// afternoon (2026-09-09: a BroadcastChannel opened during the render, never
// closed, kept the test process alive after every test had passed).
//
// Off, no component code runs outside a browser: dev, preview, CI and the
// deployed site all serve the same shell, and the tests exercise exactly
// what deploys, first paint included.
export const ssr = false;
