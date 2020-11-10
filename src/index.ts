#!/usr/bin/env node

async function main() {
  // Get a timestamp as soon as possible 
  
  const env = process.env.NODE_ENV || 'production';

  // Load command line arguments
  const { configuredYargs } = require("./input");
  const args = configuredYargs.parse(process.argv.slice(2));

  if (args.env !== undefined) {
    require('dotenv').config({ path: args.env })
  }
  const defaultHost = env === "production" ? "localhost:5984" : "localhost:5983"
  const couchConfig = {
      username: args.username ?? process.env.COUCHDB_USERNAME ?? "admin",
      password: args.password ?? process.env.COUCHDB_PASSWORD ?? "password",
      hostname: args.host ?? process.env.COUCHDB_HOSTNAME ?? defaultHost
    };
  const nano = require("nano")(`http://${couchConfig.username}:${couchConfig.password}@${couchConfig.hostname}`);
  
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
  const { _: posArgs, field, extraKeys, lenient } = args;
  const payload = parseData({ posArgs, field, extraKeys, lenient });

  payload.meta = timings;

  const dbName = args.db;

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
