# browser_wasi_shim 0.4.2, vendored

A WASI implementation in JavaScript, by bjorn3: what runs the `datum` CLI in the browser (`#lib/cli.ts`, `#lib/wasiFs.ts`).
MIT OR Apache-2.0, `LICENSE-MIT` and `LICENSE-APACHE` here. Upstream: https://github.com/bjorn3/browser_wasi_shim

Every `.js`, `.d.ts` and `LICENSE-*` here is byte for byte from the npm tarball of `@bjorn3/browser_wasi_shim@0.4.2`,
integrity `sha512-/iHkCVUG3VbcbmEHn5iIUpIrh7a7WPiwZ3sHy4HZKZzBdSadwdddYDZAII2zBvQYV0Lfi8naZngPCN7WPHI/hA==`: its `dist/*.js`
and `typings/*.d.ts` in one directory, where each `.js` finds its `.d.ts`.

The `.js` is minified. To read it, read upstream's `src/` at tag `v0.4.2`, commit
`4a55f2a519d0ddfa7e4609c42e0c9769c37c9ae8`: `swc src -d out` there, with its `.swcrc` and the `@swc/cli` 0.1.62 and
`@swc/core` 1.3.37 its lockfile pins, gives these files byte for byte.

Do not edit these files. A fix to the shim's behavior is a subclass in `#lib/wasiFs.ts`, or a new vendored version.
