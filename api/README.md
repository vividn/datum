# Datum API

REST API service that generates SVG visualizations for the Datum time-tracking system.

## Quick Start

Install dependencies
```pnpm install```

Start development server with auto-reload
```pnpm dev```

The API will be available at http://localhost:3001

## Prerequisites

- Node.js and pnpm
- `datum` CLI tool in your PATH
- CouchDB/PouchDB configured for data storage

## API Endpoints

### GET /api/dayview/:startDate

Returns an SVG visualization of time-tracking data.

## Single day

`GET /api/dayview/2024-02-01`

Date range with custom width

`GET /api/dayview/2024-02-01?width=800`

Date range with custom width and end date

`GET /api/dayview/2024-02-01?width=800&endDate=2024-02-02`

#### Parameters
- `startDate`: YYYY-MM-DD format (required)
- `endDate`: YYYY-MM-DD format (optional)
- `width`: SVG width in pixels (optional, default: 2000)

#### Response
- Content-Type: `image/svg+xml`
- SVG visualization of tracked time blocks

## Development

SVG files are cached in `~/.datum/svgs/` with filenames in the format:
`{startDate}[-{endDate}]-{width}.svg`

## Configuration

Environment variables:
- `PORT`: Server port (default: 3001)