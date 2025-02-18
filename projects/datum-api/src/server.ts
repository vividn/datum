import express from 'express';
import { spawn } from 'child_process';
import fs from 'fs';
import path from 'path';
import cors from 'cors';

const app = express();
app.use(cors());
const port = process.env.PORT || 3001;

const SVG_DIR = path.join(process.env.HOME || '', '.datum', 'svgs');
fs.mkdirSync(SVG_DIR, { recursive: true });

app.get('/', (req, res) => {
  res.send('Datum API is running');
});

app.get('/api/dayview/:startDate', (req, res) => {
  const startDate = req.params.startDate;
  const endDate = req.query.endDate as string;
  const width = parseInt(req.query.width as string) || 2000;
  console.log('Start Date:', startDate, 'End Date:', endDate, 'Width:', width);
  const svgPath = path.join(SVG_DIR, `${startDate}${endDate ? `-${endDate}` : ''}-${width}.svg`);

  console.log('SVG path:', svgPath);

  const args = ['dayview', '--start-date', startDate];
  if (endDate) {
    args.push('--end-date', endDate);
  }
  args.push('--width', width.toString(), '-o', svgPath);

  const process = spawn('datum', args);

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

app.listen(port, () => {
  console.log(`Datum API running on port ${port}`);
});