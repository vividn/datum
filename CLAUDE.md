# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build/Test Commands
- Build: `pnpm build`
- Type check: `pnpm type`
- Lint: `pnpm lint`
- Format code: `pnpm fmt`
- Run all tests: `pnpm test`
- Run single test: `jest path/to/test.test.ts`
- Watch tests: `pnpm testwatch`

## Code Style Guidelines
- TypeScript with strict typing enabled
- Semi-colons required
- No default exports (`import/no-default-export`)
- Unused imports/variables not allowed (prefixed with `_` to ignore)
- Use ES2020 features and CommonJS modules
- No explicit `any` types allowed without good reason
- Use radix parameter for parseInt (`radix: "error"`)
- Always use strict equality (`eqeqeq: "error"`)
- Prefer named exports and imports
- Follow existing directory structure for new files
- Error handling should be explicit with proper typing