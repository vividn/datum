#!/usr/bin/env node
import { configuredYargs, DatumYargsType } from "./input";
import chalk from "chalk";
import { displayDoc } from "./output";
import { PayloadError } from "./errors";
import dotenv from "dotenv";
import Nano from "nano";
import { parseData } from "./parseData";
import inferType from "./utils/inferType";
import { processTimeArgs } from "./timings";
import { assembleId } from "./ids";
import pass from "./utils/pass";

export async function main(args: DatumYargsType) {
  //TODO: put document type here
  // Get a timestamp as soon as possible

  if (args.env !== undefined) {
    dotenv.config({ path: args.env });
  }
  const env = process.env.NODE_ENV || "production";
  const defaultHost =
    env === "production" ? "localhost:5984" : "localhost:5983";
  const couchConfig = {
    username: args.username ?? process.env.COUCHDB_USER ?? "admin",
    password: args.password ?? process.env.COUCHDB_PASSWORD ?? "password",
    hostname: args.host ?? process.env.COUCHDB_HOSTNAME ?? defaultHost,
  };
  const nano = Nano(
    `http://${couchConfig.username}:${couchConfig.password}@${couchConfig.hostname}`
  );

  const {
    _: posArgs = [],
    field,
    comment,
    required,
    optional,
    remainder,
    stringRemainder,
    lenient,
    payload: payloadArg,
  } = args;

  const basePayload = payloadArg ? inferType(payloadArg) : {};
  if (typeof basePayload !== "object" || basePayload === null) {
    throw new PayloadError("base payload not a valid object");
  }

  const payload = parseData({
    posArgs,
    field,
    comment,
    required,
    optional,
    remainder,
    stringRemainder,
    lenient,
    payload: basePayload,
  });

  // Process timing/metadata
  const { noMetadata } = args;
  if (!noMetadata) {
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
    payload.meta.random = Math.random();
    payload.meta.humanId =
      Math.random().toString(36).slice(2) + Math.random().toString(36).slice(2);
  }

  const { idPart, idDelimiter = "__", partition = "%field" } = args;
  const { id: _id, structure: idStructure } = assembleId({
    idPart,
    delimiter: idDelimiter,
    partition,
    payload,
  });

  if (!noMetadata) {
    payload.meta.idStructure = idStructure;
  }

  const { db: dbName = "datum" } = args;

  // Create database if it doesn't exist
  await nano.db.create(dbName).catch(pass);

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
    displayDoc(doc, "EXISTS");
    return doc;
  } catch (err) {
    if (err.reason === "missing" || err.reason === "deleted") {
      // pass
    } else {
      console.log(err);
      throw err;
    }
  }

  await db.insert({ _id: _id, ...payload });
  const doc = await db.get(_id);
  displayDoc(doc, "CREATE");

  return doc;
}

if (require.main === module) {
  // Load command line arguments
  const args = configuredYargs.parse(process.argv.slice(2)) as DatumYargsType;
  main(args);
}
