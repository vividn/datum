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
- When adding new tests or documentation: any date examples should be on today's date as a sort of record of when it was added--purely for style
- When removing tests or chunks of code, don't leave a comment behind explaining that something was removed.
- Use `pnpm run eslint --fix --max-warnings 0` to format after making any changes
