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