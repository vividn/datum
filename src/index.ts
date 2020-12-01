#!/usr/bin/env node
import { DatumYargsType } from "./input";
const chalk = require("chalk");

async function main(args: DatumYargsType) {
  //TODO: put document type here
  // Get a timestamp as soon as possible

  if (args.env !== undefined) {
    require("dotenv").config({ path: args.env });
  }
  const env = process.env.NODE_ENV || "production";
  const defaultHost =
    env === "production" ? "localhost:5984" : "localhost:5983";
  const couchConfig = {
    username: args.username ?? process.env.COUCHDB_USERNAME ?? "admin",
    password: args.password ?? process.env.COUCHDB_PASSWORD ?? "password",
    hostname: args.host ?? process.env.COUCHDB_HOSTNAME ?? defaultHost,
  };
  const nano = require("nano")(
    `http://${couchConfig.username}:${couchConfig.password}@${couchConfig.hostname}`
  );

  const { parseData } = require("./data");
  const {
    _: posArgs = [],
    field,
    comment,
    required,
    optional,
    remainder,
    lenient,
  } = args;
  const payload = parseData({
    posArgs,
    field,
    comment,
    required,
    optional,
    remainder,
    lenient,
  });

  // Process timing/metadata
  const { noMetadata } = args;
  if (!noMetadata) {
    const { processTimeArgs } = require("./timings");
    const {
      date,
      time,
      quick,
      yesterday,
      fullDay,
      noTimestamp,
      timezone,
    } = args;
    const timings = processTimeArgs({
      date,
      time,
      quick,
      yesterday,
      fullDay,
      noTimestamp,
      timezone,
    });
    payload.meta = timings;
  }

  const { assembleId } = require("./ids");
  const { idField, idDelimiter = "__", partition = "%field" } = args;
  const { id: _id, structure: idStructure } = assembleId({
    idField,
    delimiter: idDelimiter,
    partitionField: partition,
    payload,
  });

  const { db: dbName = "datum" } = args;

  // Create database if it doesn't exist
  await nano.db.create(dbName).catch((err: any) => undefined);

  const db = await nano.use(dbName);

  const { undo } = args;
  if (undo) {
    try {
      const doc = await db.get(_id);
      const { _rev } = doc;
      const resp = await db.destroy(_id, _rev);
      console.log(chalk.grey("DELETE: ") + chalk.red(_id));
      return doc;
    } catch (err) {
      console.log(err);
      return;
    }
  }

  try {
    const doc = await db.get(_id);
    console.log(chalk.grey("EXISTS: ") + chalk.yellow(doc["_id"]));
    console.log(doc);
    return doc;
  } catch (err) {
    if (err.reason === "missing" || err.reason === "deleted") {
    } else {
      console.log(err);
      throw err;
    }
  }

  await db.insert({ _id: _id, ...payload });
  const doc = await db.get(_id);
  console.log(chalk.grey("CREATE: " + chalk.green(doc["_id"])));
  console.log(doc);

  return doc;
}

if (require.main === module) {
  // Load command line arguments
  const { configuredYargs } = require("./input");
  const args = configuredYargs.parse(process.argv.slice(2));
  main(args);
}

module.exports = main;
