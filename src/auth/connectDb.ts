import { EitherPayload } from "../documentControl/DatumDocument";
import dotenv from "dotenv";
import { MainDatumArgs } from "../input/mainYargs";
import PouchDb from "pouchdb";

export function connectDb(
  args: MainDatumArgs
): PouchDB.Database<EitherPayload> {
  if (
    process.env.NODE_ENV?.includes("dev") ||
    process.env.NODE_ENV?.includes("test")
  ) {
    process.env.COUCHDB_USER = "admin";
    process.env.COUCHDB_PASSWORD = "password";
    process.env.COUCHDB_HOSTNAME = "localhost:5983";
  }
  if (args.env !== undefined) {
    dotenv.config({ path: args.env, override: true });
  }

  process.env.COUCHDB_HOSTNAME ??= "locahost:5984";

  const hostname = args.host ?? process.env.COUCHDB_HOSTNAME;
  if (!hostname) {
    throw new Error("No hostame set. Specify with --host or COUCHDB_HOSTNAME");
  }
  const { db: dbName = "datum", createDb } = args;

  const couchAuth = {
    username: args.username ?? process.env.COUCHDB_USER,
    password: args.password ?? process.env.COUCHDB_PASSWORD,
  };

  return new PouchDb(`http://${hostname}/${dbName}`, {
    skip_setup: !createDb,
    auth: couchAuth,
  });
}
