# Datum

> A command-line personal analytics database for life tracking and data analysis

![Alpha Version](https://img.shields.io/badge/status-alpha-orange)

Datum helps you collect, track, and analyze personal data through an intuitive command-line interface. It supports multiple types of time-series data collection and arbitrary data storage.

## Features

### Data Collection

- **Point-in-time Events**
  ```bash
  datum occur alcohol type=beer
  datum occur vitamin -t 9:15am  # Flexible timestamp control
  ```

- **Duration Tracking**
  ```bash
  datum start sleep
  datum end sleep
  ```

- **State Management**
  ```bash
  datum switch project emails
  datum switch project meeting
  datum switch project end
  ```

- **General Data Storage**
  ```bash
  datum add person name=Nate city=Berlin
  ```

### Data Visualization

Datum provides multiple ways to visualize and analyze your data:

- **Daily Timeline View**
  ```bash
  datum dayview            # View today's tracked time blocks
  datum dayview -d 2024-02-01  # View specific date
  ```
  Generates a SVG timeline showing all tracked activities for a given day, with color-coding by category.

- **Activity Analysis**
  ```bash
  datum v1 sleep          # Timeline view of sleep records
  datum v1 project        # Project time allocation
  ```
  Shows patterns and trends for specific activities over time.

- **Data Querying**
  ```bash
  datum map field         # List all unique fields
  datum reduce field      # Count occurrences by field
  datum grep "search"     # Search through records
  ```

- **State Analysis**
  ```bash
  datum check            # View current state
  datum check -f project # Check specific field state
  ```

## Installation

### Package Managers
```bash
# pnpm (recommended)
pnpm install --global @vividn/datum

# npm
npm install -g @vividn/datum

# yarn
yarn global add @vividn/datum
```

### Development Setup
```bash
git clone https://github.com/your-repo/datum.git
cd datum
pnpm install
pnpm run system
```

## Database Configuration

Datum uses PouchDB by default for local storage. For remote synchronization or multi-device usage, you can connect to CouchDB:

1. Install CouchDB following the [official documentation](https://docs.couchdb.org/en/stable/)
2. Create a `datum` database and configure user permissions
3. Configure Datum via either:
   - Environment variables: `COUCHDB_USER` and `COUCHDB_PASSWORD`
   - Config file: Update `host:` and credentials
   - CLI: Use the `--host` flag

## Status

⚠️ Alpha Software Warning: Datum is under active development. Expect breaking changes and instability until version 2.1.0.

## License

[License Type] - See LICENSE file for details

## Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md) for development setup and guidelines.