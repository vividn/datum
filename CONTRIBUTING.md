# Contributing to Datum

## Development Setup

1. Prerequisites
   - Node.js (v16+)
   - pnpm

2. Installation
```bash
git clone https://github.com/yourusername/datum.git
cd datum
pnpm install
pnpm run system
```

## Development Workflow

1. Create a feature branch
```bash
git checkout -b feature/your-feature
```

2. Run tests
```bash
pnpm test         # Run all tests
pnpm test:watch   # Run tests in watch mode
```

3. Build locally
```bash
pnpm build
```

## Code Guidelines

- Write TypeScript with strict type checking
- Add tests for new features
- Follow existing code style
- Use async/await for database operations
- Document public APIs

## Pull Request Process

1. Update relevant documentation
2. Add tests for new functionality
3. Ensure CI passes
4. Request review from maintainers

## Project Structure

```
src/
  commands/     # CLI commands
  views/        # Database views
  time/         # Time handling utilities
  state/        # State management
  projects/     # Project-specific features
```

## Release Process

1. Update version in `package.json`
2. Create a release PR
3. Tag release after merge

## Questions?

Open an issue for questions or discussion topics.