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
  datum dayview -n 7       # Show the last week of data
  ```
  Generates a SVG timeline showing all tracked activities for a given day, with color-coding by category.

- **Activity Analysis**
  ```bash
  datum tail              # Show the last 10 datum data entered
  datum tail -n 50        # Show the last 50 entries
  datum tail project      # Show the last entries for the project field
  ```

- **Data Querying**
For more information see the [CouchDB](https://docs.couchdb.org/en/stable/ddocs/views/intro.html) and [PouchDB](https://pouchdb.com/guides/queries.html) documentation
  ```bash
  datum map <view_name>     # Show the key/value table for the given view
  datum map <view_name> <key> # show just the rows in the table starting with key
  datum map <view_name> <start_key> <end_key> # Show the rows between the two keys
  datum reduce <view_name>      # Run the specified reduce function
  datum reduce <view_name> -g 2 # Run the specifed reduce function with group level 2
  datum grep "search"     # Search through records
  ```

- ** Check for problems in the data
  ```bash
  datum check            # Check if there are repeated states or other problems in the data
  datum check --fix      # Fix automatically fixable errors
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

### Running the Web Interface

The web interface consists of two parts that need to be running simultaneously:

1. **API Server** (Terminal 1)
   ```bash
   cd projects/datum-api
   pnpm dev
   ```
   The API will be available at http://localhost:3001

2. **Frontend** (Terminal 2)
   ```bash
   cd projects/datum-frontend
   pnpm start
   ```
   The web interface will open automatically at http://localhost:3000

Both services will automatically reload when you make changes to the code.

### Development Tips
- The API server must be running for the frontend to work
- The frontend will proxy API requests to port 3001
- SVG files are cached in `~/.datum/svgs/`
- Use `pnpm run build` in either project directory to create production builds
