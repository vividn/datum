#!/usr/bin/env node

async function main() {
  // Load command line arguments
  const { configuredYargs } = require("./input");
  const args = configuredYargs.parse(process.argv.slice(2));

  // Process timing
  const { processTimeArgs } = require("./timings");
  const { date, time, quick, yesterday, fullDay, timezone } = args;
  const timings = processTimeArgs({
    date,
    time,
    quick,
    yesterday,
    fullDay,
    timezone,
  });

  const { parseData } = require("./data");
  const { _: posArgs, extraKeys, lenient } = args;
  const payload = parseData({ posArgs, extraKeys, lenient });

  payload.meta = timings;

  const dbName = args.db;
  const nano = require("nano")("http://admin:password@localhost:5983");

  // Create database if it doesn't exist
  await nano.db.create(dbName).catch((err: any) => undefined);

  const db = await nano.use(dbName);

  const _id = new Date().toISOString();

  await db.insert({ _id: _id, ...payload });
  const doc = await db.get(_id);
  console.log(doc);
}

if (require.main === module) {
  main();
}

module.exports = main;
