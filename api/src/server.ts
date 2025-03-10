import express from 'express';
import cors from 'cors';
import { spawn } from 'child_process';
import path from 'path';
import fs from 'fs';
import { eventsRouter } from './routes/events';
import { getDb } from './db';
import { getConfig } from './config';
import { DateTime } from 'luxon'; // Add for better date handling

const app = express();
app.use(cors());
const port = process.env.PORT || 3001;

// Load config early to catch any issues
const config = getConfig();

// Temporary directory for tail output JSON files
const TAIL_DIR = path.join(__dirname, '../temp/tail');

// Ensure the directory exists
if (!fs.existsSync(TAIL_DIR)) {
  fs.mkdirSync(TAIL_DIR, { recursive: true });
}

// Ensure SVG directory exists
const SVG_DIR = path.join(process.env.HOME || '', '.datum', 'svgs');
fs.mkdirSync(SVG_DIR, { recursive: true });

// Mount the events router
app.use('/api', eventsRouter);

// Keep existing dayview endpoint
app.get('/api/dayview/:startDate/:endDate?', (req, res) => {
  console.log(req.params);
  const tomorrow = DateTime.now().plus({ days: 1 });
  const startDate = req.params.startDate;
  const endDate = req.params.endDate || tomorrow.toISODate();
  const width = parseInt(req.query.width as string) || 2000;

  const svgPath = path.join(SVG_DIR, `${startDate}${endDate ? `-${endDate}` : ''}-${width}.svg`);

  const args = ['dayview', '--start-date', startDate];
  if (endDate) {
    args.push('--end-date', endDate);
  }
  args.push('--width', width.toString(), '-o', svgPath);

  const process = spawn('datum', args);
  console.log(args);

  process.stderr.on('data', (data) => {
    console.error(`Error: ${data}`);
  });

  process.on('close', (code) => {
    if (code !== 0) {
      return res.status(500).send('Failed to generate dayview');
    }

    if (!fs.existsSync(svgPath)) {
      return res.status(404).send('SVG not generated');
    }

    const svgContent = fs.readFileSync(svgPath, 'utf8');
    res.contentType('image/svg+xml');
    res.send(svgContent);
  });
});

/**
 * GET /api/tail
 * Returns the most recent datum entries (similar to datum tail CLI command)
 */
app.get('/api/tail', async (req, res) => {
  console.log('Tail endpoint called with params:', req.query);

  const limit = parseInt(req.query.limit as string) || 10;
  const metric = req.query.metric as string || 'hybrid';
  const field = req.query.field as string || '';

  // Build arguments for datum tail command - use default output format
  const args = ['tail'];

  // Add the limit parameter
  args.push('-n', limit.toString());

  // Add metric if specified
  if (metric !== 'hybrid') {
    args.push('--metric', metric);
  }

  // Add field if specified
  if (field) {
    args.push(field);
  }

  console.log('Running command: datum', args.join(' '));

  try {
    const process = spawn('datum', args);

    let outputData = '';
    let errorOutput = '';

    process.stdout.on('data', (data) => {
      outputData += data.toString();
    });

    process.stderr.on('data', (data) => {
      errorOutput += data.toString();
      console.error(`Tail stderr: ${data}`);
    });

    // Use a Promise to wait for the process to complete
    await new Promise((resolve, reject) => {
      process.on('close', (code) => {
        console.log(`Tail process exited with code ${code}`);
        if (code !== 0) {
          reject(new Error(`Process exited with code ${code}: ${errorOutput}`));
        } else {
          resolve(null);
        }
      });
    });

    console.log('Raw output sample:', outputData.substring(0, 200));

    // Now parse the default output format
    // Split by lines and process each line to extract entry data
    const lines = outputData.split('\n')
      .filter(line => line.trim())
      // Skip header line and remove 'line' prefix
      .filter(line => !line.includes('time  field'))
      .map(line => line.replace(/^line\s+/, ''));

    // Create entries from the output
    const entries = [];
    let currentEntry = null;

    for (const line of lines) {
      // Detect if this is a header line (with date, ID, etc.)
      if (line.match(/^\d{4}-\d{2}-\d{2}/) || line.match(/^\s*\d{2}:\d{2}:\d{2}/)) {
        if (currentEntry) {
          entries.push(currentEntry);
        }

        // Start a new entry
        currentEntry = {
          id: '',
          timestamp: '',
          text: '',
          category: '',
          type: ''
        };

        // Parse the line into columns (handling multiple spaces as separators)
        const parts = line.trim().split(/\s+/);

        // Handle timestamp
        if (parts[0].includes(':')) {
          // Time-only format (e.g., "12:14:21+1")
          currentEntry.timestamp = DateTime.now().toFormat('yyyy-MM-dd') + ' ' + parts[0].replace('+1', '');
        } else {
          // Full date-time format (e.g., "2025-02-21 15:54:01+1")
          currentEntry.timestamp = `${parts[0]} ${parts[1].replace('+1', '')}`;
        }

        currentEntry.category = parts[2] || '';
        currentEntry.type = parts[3] || '';
        currentEntry.id = parts[4] || '';
      } else if (currentEntry) {
        // If not a header, then this is content for the current entry
        currentEntry.text += (currentEntry.text ? '\n' : '') + line;
      }
    }

    // Don't forget to add the last entry
    if (currentEntry) {
      entries.push(currentEntry);
    }
    console.log("Entries", entries);

    console.log(`Parsed ${entries.length} entries from command output`);

    // If we couldn't parse any entries, generate a dummy entry with the raw output for debugging
    if (entries.length === 0) {
      entries.push({
        id: `temp-${Date.now()}`,
        timestamp: new Date().toISOString(),
        text: 'Failed to parse entries from command output. Raw output:\n\n' + outputData,
        category: 'error',
        type: 'error'
      });
    }

    // Send the entries
    res.json(entries);

  } catch (error) {
    console.error('Error in tail endpoint:', error);
    res.status(500).json({
      error: 'Failed to generate tail data',
      details: error instanceof Error ? error.message : String(error)
    });
  }
});

// Test DB connection on startup
getDb().then(() => {
  console.log('Connected to database using config:', {
    host: config.host,
    db: config.db,
    user: config.user,
    // Don't log password
  });
}).catch(err => {
  console.error('Failed to connect to database:', err);
  process.exit(1); // Exit if we can't connect to the database
});

app.listen(port, () => {
  console.log(`Datum API running on port ${port}`);
});