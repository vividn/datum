import { EitherPayload } from "../documentControl/DatumDocument";
import dotenv from "dotenv";
import { MainDatumArgs } from "../input/mainYargs";
import PouchDb from "pouchdb";
import memoryAdapter from "pouchdb-adapter-memory";

PouchDb.plugin(memoryAdapter);

export function connectDb(
  args: MainDatumArgs
): PouchDB.Database<EitherPayload> {
  if (args.env !== undefined) {
    dotenv.config({ path: args.env, override: true });
  }
  const { db: dbName = "datum", createDb } = args;
  if (dbName instanceof PouchDb) {
    return dbName;
  }

  const adapter = args.adapter ?? process.env.POUCHDB_ADAPTER;
  const hostname = args.host ?? process.env.COUCHDB_HOST;

  const fullDatabaseName =
    adapter === "memory"
      ? dbName
      : !hostname
      ? dbName
      : hostname.at(-1) === "/"
      ? `${hostname}${dbName}`
      : `${hostname}/${dbName}`;

  const couchAuth = {
    username: args.username ?? process.env.COUCHDB_USER,
    password: args.password ?? process.env.COUCHDB_PASSWORD,
  };
  return new PouchDb(fullDatabaseName, {
    skip_setup: !createDb,
    auth: couchAuth,
    adapter,
  });
}
