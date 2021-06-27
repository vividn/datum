#!/usr/bin/env node
import { configuredYargs, DatumYargsType } from "./input";
import chalk from "chalk";
import { BaseDataError } from "./errors";
import dotenv from "dotenv";
import Nano, { DocumentScope } from "nano";
import { parseData } from "./parseData";
import inferType from "./utils/inferType";
import { processTimeArgs } from "./timings";
import { assembleId, buildIdStructure, defaultIdComponents } from "./ids";
import pass from "./utils/pass";
import { GenericObject } from "./GenericObject";
import {
  DatumMetadata,
  DatumPayload, EitherDocument, EitherPayload,
} from "./documentControl/DatumDocument";
import newHumanId from "./meta/newHumanId";
import { defaults } from "./input/defaults";
import { showCreate, showExists } from "./output";

export async function main(
  args: DatumYargsType
): Promise<EitherDocument> {
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
  } = args;

  const baseData = args.baseData ? inferType(args.baseData) : {};
  if (typeof baseData !== "object" || baseData === null) {
    throw new BaseDataError("base data not a valid object");
  }

  const payloadData = parseData({
    posArgs,
    field,
    comment,
    required,
    optional,
    remainder,
    stringRemainder,
    lenient,
    baseData,
  });

  // Process timing/metadata
  const hasOccurTime = !args.noMetadata && !args.noTimestamp;
  const { defaultIdParts, defaultPartitionParts } = defaultIdComponents({
    data: payloadData,
    hasOccurTime,
  });

  const idStructure = buildIdStructure({
    idParts: args.idPart ?? defaultIdParts,
    delimiter: args.idDelimiter ?? defaults.idDelimiter,
    partition: args.partition ?? defaultPartitionParts,
  });

  const { noMetadata } = args;
  let meta: DatumMetadata | undefined = undefined;
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
    const metaTimings = processTimeArgs({
      date,
      time,
      quick,
      yesterday,
      fullDay,
      noTimestamp,
      timezone,
    });

    meta = {
      ...metaTimings,
      random: Math.random(),
      humanId: newHumanId(),
    };

    // don't include idStructure if it is just a raw string (i.e. has no field references in it)
    // that would be a waste of bits since _id then is exactly the same
    if (idStructure.match(/(?<!\\)%/)) {
      meta.idStructure = idStructure;
    }
  }

  const _id = assembleId({
    data: payloadData,
    meta: meta,
    idStructure: idStructure,
  });

  const { db: dbName = "datum" } = args;

  // Create database if it doesn't exist
  await nano.db.create(dbName).catch(pass);

  const db: DocumentScope<EitherPayload> = await nano.use(
    dbName
  );

  const { undo } = args;
  if (undo) {
    const doc = await db.get(_id);
    const { _rev } = doc;
    await db.destroy(_id, _rev);
    console.log(chalk.grey("DELETE: ") + chalk.red(_id));
    return doc;
  }

  try {
    const doc = await db.get(_id);
    showExists(doc, args.showAll);
    return doc;
  } catch (err) {
    if (err.reason === "missing" || err.reason === "deleted") {
      // pass
    } else {
      console.log(err);
      throw err;
    }
  }

  const payload =
    meta !== undefined
      ? { _id: _id, data: payloadData, meta: meta }
      : { _id: _id, ...payloadData };

  await db.insert(payload);
  const doc = await db.get(_id);
  showCreate(doc, args.showAll);

  return doc;
}

if (require.main === module) {
  // Load command line arguments
  const args = configuredYargs.parse(process.argv.slice(2)) as DatumYargsType;
  main(args).catch((err) => {
    console.error(err);
  });
}
