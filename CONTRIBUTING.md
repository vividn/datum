# Contributing to Datum

## Development Setup

1. Prerequisites
   - Node.js (v18+)
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
  auth/         # Connecting to CouchDb and PouchDb 
  commands/     # CLI commands
  config/       # Parsing config options
  dayview/      # Generating the dayview svg output of data
  documentControl/ # Interface for writing/updating the JSON documents inside the database
  field/        # Spec and field management
  ids/          # Generating and managing ids of documents
  input/        # Parsing CLI options, and definitions of general arg parsers
  meta/         # Handling metadata of the documents 
  migrations/   # Tools for changing the data structure across many documents at once
  output/       # CLI formatting of data for printing to the command line
  state/        # Managing state of fields
  time/         # Time handling utilities
  undo/         # handling undoing of commands
  utils/        # General utils and abstractions
  views/        # built in couchdb views for interacting and analyzing data
```

## Release Process

1. Update version in `package.json`
2. Create a release PR
3. Tag release after merge
4. Publish to npm

## Questions?

Open an issue for questions or discussion topics.
